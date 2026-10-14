import { collectFingerprints, fingerprint, normalizeFingerprints } from "../evals";
import type { ClaimProperties as PersistedClaimProperties } from "@palantirkc/ontology/domains/evidence";
import type { LinkRecord } from "@palantirkc/ontology/operational";
import { jsonSnapshot, newEvalObject, requireObjectProperties } from "../evals/types";
import type {
  EvalObject,
  Fingerprints,
  FoundryEvalService,
  Freshness,
  JsonObject,
  TemporalScope,
  VerificationClass,
  VerificationUnitProperties,
  Verdict,
} from "../evals/types";

export interface ClaimProperties extends PersistedClaimProperties {
  /** Explicit grouping for incompatible claims; no semantic equivalence is inferred from wording. */
  conflictsWith?: string[];
}

export interface FreshnessOptions {
  currentFingerprints?: Fingerprints;
  now?: string;
  temporal?: TemporalScope;
  freshnessHours?: number;
}

export interface ScopedResult {
  result: EvalObject;
  freshness: Freshness;
  applicableVerdict: Verdict;
}

export interface UnitVerification {
  unitId: string;
  verdict: Verdict;
  results: ScopedResult[];
  history: EvalObject[];
  diagnostics: string[];
}

export interface ClaimVerification {
  claim: EvalObject;
  verdict: Verdict;
  verificationClasses: VerificationClass[];
  freshness: "fresh" | "stale" | "unknown";
  checkedAt: string;
  units: UnitVerification[];
  conflicts: EvalObject[];
  resultConflicts: string[];
  decisions: EvalObject[];
  diagnostics: string[];
}

/** Applicability is recomputed. Historical verdicts and their fingerprints are never rewritten. */
export function assessFreshness(result: EvalObject, options: FreshnessOptions = {}): Freshness {
  const properties = result.properties;
  const now = options.now ?? new Date().toISOString();
  const temporal =
    options.temporal ??
    (properties.temporal === "fixed-revision" ? "fixed-revision" : "current-state");
  const checkedAt = now;
  const reasons: string[] = [];
  const changedFingerprints: string[] = [];
  const stored =
    properties.fingerprints !== null &&
    typeof properties.fingerprints === "object" &&
    !Array.isArray(properties.fingerprints)
      ? (properties.fingerprints as unknown as Fingerprints)
      : {};
  for (const [key, value] of Object.entries(options.currentFingerprints ?? {})) {
    if (stored[key] !== value) changedFingerprints.push(key);
  }
  if (changedFingerprints.length)
    reasons.push("Relevant fingerprints changed or were absent from the original verification");
  const verifiedAt =
    typeof properties.verifiedAt === "string" ? Date.parse(properties.verifiedAt) : NaN;
  const observedAt =
    typeof properties.observedAt === "string" ? Date.parse(properties.observedAt) : verifiedAt;
  const currentTime = Date.parse(now);
  const hours =
    options.freshnessHours ??
    (typeof properties.freshnessHours === "number" ? properties.freshnessHours : 24);
  const validTime =
    Number.isFinite(currentTime) &&
    Number.isFinite(verifiedAt) &&
    Number.isFinite(observedAt) &&
    verifiedAt <= currentTime &&
    observedAt <= currentTime;
  const validWindow = Number.isFinite(hours) && hours > 0;
  const expiresAt =
    temporal === "current-state" && validTime && validWindow
      ? new Date(Math.min(verifiedAt, observedAt) + hours * 3_600_000).toISOString()
      : null;
  if (!validTime)
    reasons.push("Verification time or current clock is missing, invalid, or in the future");
  if (temporal === "current-state" && !validWindow)
    reasons.push("Current-state freshness duration must be positive");
  const expired =
    temporal === "current-state" && expiresAt !== null && currentTime >= Date.parse(expiresAt);
  if (expired)
    reasons.push("Current-state verification needs reconfirmation after its freshness window");
  return {
    state:
      changedFingerprints.length || expired
        ? "stale"
        : !validTime || (temporal === "current-state" && !validWindow)
          ? "unknown"
          : "fresh",
    temporal,
    checkedAt,
    expiresAt,
    changedFingerprints,
    reasons,
  };
}

async function liveFingerprints(
  foundry: FoundryEvalService,
  result: EvalObject,
  unit?: EvalObject,
  provided: Fingerprints = {},
  temporal: TemporalScope = "current-state",
): Promise<Fingerprints> {
  const stored =
    result.properties.fingerprints !== null &&
    typeof result.properties.fingerprints === "object" &&
    !Array.isArray(result.properties.fingerprints)
      ? (result.properties.fingerprints as unknown as Fingerprints)
      : {};
  if (temporal === "fixed-revision") {
    // A historical assertion is scoped to its recorded source, evaluator and environment.
    // A changed requested unit or explicitly supplied revision is a different applicability check.
    const current: Fingerprints = { ...stored };
    if (unit) {
      const unitProperties = requireObjectProperties<VerificationUnitProperties>(
        unit,
        "VerificationUnit",
      );
      current[`object:${unit.id}`] = fingerprint(unit);
      current[`snapshot:${unit.id}`] = fingerprint({
        inputSnapshot: unitProperties.inputSnapshot,
        expectedPredicate: unitProperties.expectedPredicate,
      });
      Object.assign(current, unitProperties.fingerprints ?? {});
    }
    return { ...current, ...normalizeFingerprints(provided) };
  }
  const current: Fingerprints = normalizeFingerprints(provided);
  for (const key of Object.keys(stored)) {
    if (key.startsWith("object:")) {
      const object = await foundry.getObject(key.slice("object:".length));
      current[key] = object ? fingerprint(object) : fingerprint({ missingObject: key });
    } else if (key.startsWith("source:")) {
      const source = await foundry.getObject(key.slice("source:".length));
      current[key] = source
        ? fingerprint(source.properties)
        : fingerprint({ missingSource: key.slice("source:".length) });
    } else if (key.startsWith("target:")) {
      const suite = await foundry.getObject(key.slice("target:".length));
      current[key] = suite
        ? fingerprint(suite.properties.target)
        : fingerprint({ missingTarget: key });
    } else if (key === "runtime:bun")
      current[key] = fingerprint({ runtime: "bun", version: Bun.version });
  }
  if (unit) {
    const unitFingerprints = await collectFingerprints(foundry, unit, undefined, [], provided);
    for (const [key, value] of Object.entries(unitFingerprints)) current[key] = value;
  }
  return current;
}

function aggregate(verdicts: Verdict[]): Verdict {
  if (!verdicts.length) return "insufficient";
  if (verdicts.includes("refuted")) return "refuted";
  return verdicts.every((verdict) => verdict === "supported") ? "supported" : "insufficient";
}

function verdictOf(result: EvalObject): Verdict {
  const properties = result.properties;
  if (properties.status !== "completed") return "insufficient";
  // Imported malformed legacy records cannot elevate a model trace by changing its label.
  if (properties.nativeResult && properties.verificationClass === "deterministic-verified")
    return "insufficient";
  if (properties.verdict === "supported" || properties.verdict === "refuted")
    return properties.verdict;
  return "insufficient";
}

/** Read-only canonical claim query: scopes, freshness, conflicts and decisions stay visible. */
export async function verifyClaim(
  foundry: FoundryEvalService,
  claimId: string,
  options: FreshnessOptions = {},
): Promise<ClaimVerification> {
  const claim = await foundry.getObject(claimId);
  if (!claim) throw new Error(`Missing Claim: ${claimId}`);
  const properties = requireObjectProperties<ClaimProperties>(claim, "Claim");
  const now = options.now ?? new Date().toISOString();
  const diagnostics: string[] = [];
  const allResults = await foundry.queryObjects({
    projectId: claim.projectId,
    objectType: "CaseResult",
    includeShared: true,
  });
  const committedOrder = new Map(
    await Promise.all(
      allResults.map(
        async (result) =>
          [
            result.id,
            foundry.store?.history
              ? ((await foundry.store.history(result.id)).at(-1)?.sequence ?? 0)
              : 0,
          ] as const,
      ),
    ),
  );
  const units: UnitVerification[] = [];
  for (const unitId of properties.verificationUnitIds) {
    const unitObject = await foundry.getObject(unitId);
    const history = allResults
      .filter((result) => result.properties.unitRef === unitId)
      .sort(
        (a, b) =>
          (committedOrder.get(b.id) ?? 0) - (committedOrder.get(a.id) ?? 0) ||
          String(b.properties.recordedAt ?? b.properties.verifiedAt ?? b.createdAt).localeCompare(
            String(a.properties.recordedAt ?? a.properties.verifiedAt ?? a.createdAt),
          ) ||
          b.createdAt.localeCompare(a.createdAt),
      );
    const latestRunByEvaluator = new Map<string, string>();
    for (const result of history) {
      const evaluator = String(result.properties.evaluatorRef ?? "unrecorded");
      if (!latestRunByEvaluator.has(evaluator))
        latestRunByEvaluator.set(evaluator, String(result.properties.runRef ?? result.id));
    }
    const currentResults = history.filter(
      (result) =>
        latestRunByEvaluator.get(String(result.properties.evaluatorRef ?? "unrecorded")) ===
        String(result.properties.runRef ?? result.id),
    );
    const results: ScopedResult[] = [];
    const unitDiagnostics: string[] = [];
    if (!unitObject || unitObject.objectType !== "VerificationUnit")
      unitDiagnostics.push("VerificationUnit is missing");
    if (!currentResults.length) unitDiagnostics.push("No scoped verification result is available");
    const unitProperties =
      unitObject?.objectType === "VerificationUnit"
        ? requireObjectProperties<VerificationUnitProperties>(unitObject, "VerificationUnit")
        : undefined;
    const temporal =
      options.temporal ?? properties.temporal ?? unitProperties?.temporal ?? "current-state";
    for (const result of currentResults) {
      const currentFingerprints = await liveFingerprints(
        foundry,
        result,
        unitObject,
        options.currentFingerprints,
        temporal,
      );
      currentFingerprints[`claim:${claim.id}`] = fingerprint(claim);
      const freshness = assessFreshness(result, {
        now,
        temporal,
        freshnessHours:
          options.freshnessHours ?? unitProperties?.freshnessHours ?? properties.freshnessHours,
        currentFingerprints,
      });
      results.push({
        result,
        freshness,
        applicableVerdict:
          freshness.state === "fresh" && !!unitObject ? verdictOf(result) : "insufficient",
      });
    }
    units.push({
      unitId,
      verdict: aggregate(results.map((result) => result.applicableVerdict)),
      results,
      history,
      diagnostics: unitDiagnostics,
    });
  }
  const allClaims = await foundry.queryObjects({
    projectId: claim.projectId,
    objectType: "Claim",
    includeShared: true,
  });
  const conflicts = allClaims.filter(
    (other) =>
      other.id !== claim.id &&
      (properties.conflictsWith?.includes(other.id) ||
        (properties.conflictKey !== undefined &&
          other.properties.conflictKey === properties.conflictKey) ||
        (Array.isArray(other.properties.conflictsWith) &&
          other.properties.conflictsWith.includes(claim.id))),
  );
  const resultConflicts = units
    .filter((unit) => {
      const verdicts = unit.results.map((result) => result.applicableVerdict);
      return verdicts.includes("supported") && verdicts.includes("refuted");
    })
    .map((unit) => unit.unitId);
  if (conflicts.length || resultConflicts.length)
    diagnostics.push("Conflicting claims or scoped results coexist; no acceptance is inferred");
  const decisions = (
    await foundry.queryObjects({
      projectId: claim.projectId,
      objectType: "Decision",
      includeShared: true,
    })
  ).filter((decision) => decision.properties.subjectRef === claim.id);
  const scopedResults = units.flatMap((unit) => unit.results);
  const verificationClasses = [
    ...new Set(
      scopedResults
        .map((result) => result.result.properties.verificationClass)
        .filter(
          (value): value is VerificationClass =>
            value === "deterministic-verified" ||
            value === "source-supported" ||
            value === "model-judged" ||
            value === "human-reviewed",
        ),
    ),
  ];
  const freshness =
    !scopedResults.length || scopedResults.some((result) => result.freshness.state === "unknown")
      ? "unknown"
      : scopedResults.some((result) => result.freshness.state === "stale")
        ? "stale"
        : "fresh";
  return {
    claim,
    verdict: aggregate(units.map((unit) => unit.verdict)),
    verificationClasses,
    freshness,
    checkedAt: now,
    units,
    conflicts,
    resultConflicts,
    decisions,
    diagnostics,
  };
}

export interface AcceptanceInput {
  status: "accepted" | "rejected" | "superseded";
  rationale: string;
  verificationRefs: string[];
  authority: { actorId: string; scope: string };
  choice: JsonObject;
}

/** Explicit policy application is an independent Decision, never a side effect of evaluation. */
export async function recordAcceptance(
  foundry: FoundryEvalService,
  claimId: string,
  input: AcceptanceInput,
): Promise<EvalObject> {
  const claim = await foundry.getObject(claimId);
  if (!claim || claim.objectType !== "Claim") throw new Error(`Missing Claim: ${claimId}`);
  if (!input.rationale.trim() || !input.authority.actorId.trim() || !input.authority.scope.trim())
    throw new Error("Acceptance requires a rationale and explicit scoped authority");
  if (input.status === "accepted" && !input.verificationRefs.length)
    throw new Error("Accepted decision requires explicit verification references");
  const units = claim.properties.verificationUnitIds;
  for (const reference of input.verificationRefs) {
    const result = await foundry.getObject(reference);
    if (
      !result ||
      result.objectType !== "CaseResult" ||
      !Array.isArray(units) ||
      !units.includes(result.properties.unitRef ?? null)
    )
      throw new Error(
        `Acceptance verification reference does not belong to this claim: ${reference}`,
      );
  }
  const decision = newEvalObject("Decision", claim.projectId, {
    subjectRef: claim.id,
    ...(jsonSnapshot(input) as JsonObject),
  });
  const links: LinkRecord[] = [
    ["decision-claim", claim.id],
    ...[...new Set(input.verificationRefs)].map((id) => ["decision-result", id]),
  ].map(([linkType, to]) => ({
    schemaVersion: "1.0.0",
    kind: "link",
    id: crypto.randomUUID(),
    linkType: linkType!,
    linkTypeVersion: "1.0.0",
    version: 1,
    from: decision.id,
    to: to!,
    projectId: claim.projectId,
    scope: "project",
    createdAt: decision.createdAt,
  }));
  await foundry.putObjects([decision], links);
  return decision;
}
