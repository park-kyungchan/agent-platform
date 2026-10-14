/** Portable reader projection, copied into the existing ontology-platform consumer.
 * No filesystem, dispatch, permission resolution, task store or model dependency.
 */
export const SEMANTIC_CONTEXT_OWNER = "ontology-platform" as const;
export const SEMANTIC_CONTEXT_NEEDS = [
  "purpose",
  "definitions",
  "dependencies",
  "authority",
  "evidence",
] as const;
export type SemanticContextNeed = (typeof SEMANTIC_CONTEXT_NEEDS)[number];
export type SemanticSourceName = "role" | "context";
export interface SemanticContextRequest {
  schema: "backend.common-context.request.v1";
  actor: { runtime: "native-codex" | "native-hermes"; profile: string };
  needs: SemanticContextNeed[];
  maxChars?: number;
  expectedSourcePins?: Partial<Record<SemanticSourceName, string>>;
  work?: {
    id: string;
    owner: string;
    domainOwner: string;
    contextRefs?: Array<{ id: string; locator: string }>;
  };
}
export interface SemanticSourceSnapshot {
  name: SemanticSourceName;
  path: string;
  sha256?: string;
  value?: unknown;
  /** The existing native reader rechecks bytes after selecting the records. */
  unchanged?: boolean;
}
export class SemanticContextError extends Error {
  constructor(readonly code: string) {
    super(code);
    this.name = "SemanticContextError";
  }
}
const fail = (code: string): never => {
  throw new SemanticContextError(code);
};
function object(value: unknown): asserts value is Record<string, any> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    fail("INVALID_COMMON_CONTEXT_REQUEST");
}
function only(value: Record<string, unknown>, keys: string[]) {
  if (Object.keys(value).some((key) => !keys.includes(key))) fail("UNKNOWN_COMMON_CONTEXT_FIELD");
}
function id(value: unknown): asserts value is string {
  if (typeof value !== "string" || !/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/u.test(value))
    fail("INVALID_CONTEXT_IDENTITY");
}
function text(value: unknown): asserts value is string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 1200 ||
    /[\x00-\x1f\x7f]/u.test(value)
  )
    fail("INVALID_CONTEXT_LOCATOR");
}
export function semanticContextRequest(input: unknown): SemanticContextRequest {
  object(input);
  only(input, ["schema", "actor", "needs", "maxChars", "expectedSourcePins", "work"]);
  if (input.schema !== "backend.common-context.request.v1") fail("INVALID_COMMON_CONTEXT_SCHEMA");
  object(input.actor);
  only(input.actor, ["runtime", "profile"]);
  id(input.actor.profile);
  if (
    !["native-codex", "native-hermes"].includes(input.actor.runtime) ||
    !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/u.test(input.actor.profile)
  )
    fail("INVALID_CONTEXT_ACTOR");
  if (
    !Array.isArray(input.needs) ||
    input.needs.length < 1 ||
    input.needs.length > 5 ||
    new Set(input.needs).size !== input.needs.length ||
    input.needs.some(
      (need: unknown) => !SEMANTIC_CONTEXT_NEEDS.includes(need as SemanticContextNeed),
    )
  )
    fail("INVALID_CONTEXT_SELECTION");
  if (
    input.maxChars !== undefined &&
    (!Number.isSafeInteger(input.maxChars) || input.maxChars < 1 || input.maxChars > 12000)
  )
    fail("INVALID_CONTEXT_BUDGET");
  if (input.expectedSourcePins !== undefined) {
    object(input.expectedSourcePins);
    only(input.expectedSourcePins, ["role", "context"]);
    for (const pin of Object.values(input.expectedSourcePins))
      if (typeof pin !== "string" || !/^[a-f0-9]{64}$/u.test(pin)) fail("INVALID_CONTEXT_PIN");
  }
  if (input.work !== undefined) {
    object(input.work);
    only(input.work, ["id", "owner", "domainOwner", "contextRefs"]);
    id(input.work.id);
    id(input.work.owner);
    id(input.work.domainOwner);
    if (input.work.contextRefs !== undefined) {
      if (!Array.isArray(input.work.contextRefs) || input.work.contextRefs.length > 16)
        fail("INVALID_WORK_CONTEXT_REFERENCES");
      const seen = new Set<string>();
      for (const reference of input.work.contextRefs) {
        object(reference);
        only(reference, ["id", "locator"]);
        id(reference.id);
        text(reference.locator);
        if (seen.has(reference.id)) fail("DUPLICATE_WORK_CONTEXT_REFERENCE");
        seen.add(reference.id);
      }
    }
  }
  return structuredClone(input) as SemanticContextRequest;
}

export function semanticContextSourceNames(input: unknown): SemanticSourceName[] {
  const request = semanticContextRequest(input);
  const names = new Set<SemanticSourceName>();
  for (const need of request.needs) {
    if (need === "dependencies") names.add("context");
    else if (need === "evidence") {
      names.add("role");
      names.add("context");
    } else names.add("role");
  }
  return (["role", "context"] as const).filter((name) => names.has(name));
}

type NativeRecord = Record<string, any> & { type: string; id: string };
type NativeSource = {
  schema: string;
  kind: string;
  id: string;
  profile: string;
  projectId: string;
  authority: string;
  observedAt: string;
  sourceRefs: string[];
  records: NativeRecord[];
};
function sourceIdentity(value: unknown, name: SemanticSourceName): value is NativeSource {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const v = value as NativeSource;
  return (
    v.schema === "backend.work-artifact.v4" &&
    v.kind === "native_context_binding" &&
    v.id === (name === "role" ? "ontology-steward-role" : "backend-project-context") &&
    v.profile === SEMANTIC_CONTEXT_OWNER &&
    typeof v.projectId === "string" &&
    v.authority === "none" &&
    typeof v.observedAt === "string" &&
    Array.isArray(v.sourceRefs) &&
    v.sourceRefs.every((ref) => typeof ref === "string") &&
    Array.isArray(v.records) &&
    v.records.every(
      (record) => record && typeof record.type === "string" && typeof record.id === "string",
    ) &&
    new Set(v.records.map((record) => record.id)).size === v.records.length
  );
}

/** Native ArtifactSchema remains the producer validator. This reader checks its
 * selected identity and does not relabel historical producers or accepted state.
 */
export function projectSemanticContext(
  input: unknown,
  snapshots: readonly SemanticSourceSnapshot[],
) {
  const request = semanticContextRequest(input),
    required = semanticContextSourceNames(request);
  const gaps: Array<{ source: SemanticSourceName; code: string }> = [];
  const sources: Array<{ snapshot: SemanticSourceSnapshot; value: NativeSource }> = [];
  for (const name of required) {
    const matching = snapshots.filter((snapshot) => snapshot.name === name);
    if (!matching.length) {
      gaps.push({ source: name, code: "SOURCE_MISSING" });
      continue;
    }
    if (matching.length !== 1) {
      gaps.push({ source: name, code: "SOURCE_AMBIGUOUS" });
      continue;
    }
    if (matching[0]!.value === undefined) {
      gaps.push({ source: name, code: "SOURCE_MISSING" });
      continue;
    }
    const snapshot = matching[0]!;
    if (typeof snapshot.sha256 !== "string" || !/^[a-f0-9]{64}$/u.test(snapshot.sha256)) {
      gaps.push({ source: name, code: "SOURCE_PIN_MISSING" });
      continue;
    }
    if (!sourceIdentity(snapshot.value, name)) {
      gaps.push({ source: name, code: "SOURCE_IDENTITY_CONFLICT" });
      continue;
    }
    if (snapshot.unchanged === false) {
      gaps.push({ source: name, code: "SOURCE_CHANGED_DURING_READ" });
      continue;
    }
    if (
      request.expectedSourcePins?.[name] !== undefined &&
      request.expectedSourcePins[name] !== snapshot.sha256
    ) {
      gaps.push({ source: name, code: "SOURCE_STALE" });
      continue;
    }
    sources.push({ snapshot, value: snapshot.value });
  }
  if (new Set(sources.map((source) => source.value.projectId)).size > 1)
    gaps.push({ source: "context", code: "SOURCE_PROJECT_CONFLICT" });
  const views: Array<{
    need: SemanticContextNeed;
    source: SemanticSourceName;
    recordId: string;
    sourceType: string;
    upperTerms: string[];
    value: unknown;
  }> = [];
  const byName = new Map(sources.map((source) => [source.snapshot.name, source.value]));
  function selected(
    name: SemanticSourceName,
    type: string,
    recordId: string,
  ): NativeRecord | undefined {
    const record = byName
      .get(name)
      ?.records.find((item) => item.type === type && item.id === recordId);
    if (!record && byName.has(name))
      gaps.push({ source: name, code: `RECORD_MISSING:${type}:${recordId}` });
    return record;
  }
  for (const need of request.needs) {
    if (need === "purpose") {
      const record = selected("role", "decision", "purpose");
      if (record)
        views.push({
          need,
          source: "role",
          recordId: record.id,
          sourceType: record.type,
          upperTerms: ["decision", "scope"],
          value: {
            topic: record.topic,
            status: record.status,
            rule: record.rule,
            sourceRefs: record.sourceRefs,
          },
        });
    } else if (need === "definitions") {
      const record = selected("role", "lead_protocol", "lead-protocol");
      if (record)
        views.push({
          need,
          source: "role",
          recordId: record.id,
          sourceType: record.type,
          upperTerms: ["authority", "scope", "version"],
          value: {
            protocolId: record.protocolId,
            revision: record.revision,
            semanticAccountability: record.accountability,
            dimensions: record.ontology,
            evidenceClasses: record.sdlc?.evidenceClasses,
            boundary:
              "The platform accountableLead owns common meaning; it does not replace the acting profile or current Work/domain owners.",
          },
        });
    } else if (need === "authority") {
      for (const recordId of ["scope", "autonomy", "evaluation"]) {
        const record = selected("role", "constraint", recordId);
        if (record)
          views.push({
            need,
            source: "role",
            recordId: record.id,
            sourceType: record.type,
            upperTerms: ["authority", "scope"],
            value: { category: record.category, rule: record.rule, sourceRefs: record.sourceRefs },
          });
      }
    } else if (need === "dependencies") {
      const source = byName.get("context");
      if (source)
        for (const record of source.records.filter(
          (record) => record.type === "link" && record.relation !== "history",
        ))
          views.push({
            need,
            source: "context",
            recordId: record.id,
            sourceType: record.type,
            upperTerms: ["reference"],
            value: {
              relation: record.relation,
              target: record.target,
              selectors: record.selectors,
              targetLoaded: false,
            },
          });
    } else {
      for (const name of required) {
        const source = byName.get(name);
        if (source)
          views.push({
            need,
            source: name,
            recordId: source.id,
            sourceType: "source-provenance",
            upperTerms: ["evidence", "reference"],
            value: {
              sourceRefs: source.sourceRefs,
              observedAt: source.observedAt,
              referencedBodiesLoaded: false,
            },
          });
      }
    }
  }
  const requiredChars = [...JSON.stringify(views)].length,
    maxChars = request.maxChars ?? 6000;
  const budgetExceeded = requiredChars > maxChars;
  const status = gaps.some((gap) => /CONFLICT|AMBIGUOUS|CHANGED/.test(gap.code))
    ? "CONTEXT_CONFLICT"
    : gaps.some((gap) => gap.code === "SOURCE_STALE")
      ? "CONTEXT_STALE"
      : gaps.length
        ? "CONTEXT_MISSING"
        : budgetExceeded
          ? "CONTEXT_BUDGET_EXCEEDED"
          : "CONTEXT_READY";
  return {
    schema: "backend.common-context.projection.v1" as const,
    semanticOwner: SEMANTIC_CONTEXT_OWNER,
    actor: request.actor,
    ...(request.work
      ? { work: request.work, workIdentity: "caller-declared-not-native-authenticated" as const }
      : {}),
    foundationReference: {
      package: "@palantirkc/ontology",
      version: "0.2.0",
      usage: "upper-vocabulary-compatibility-map",
      nativeProducerSchemasPreserved: true,
    },
    authority: "none" as const,
    executionReady: false as const,
    appliesEffects: false as const,
    needs: request.needs,
    status,
    selectionComplete: status === "CONTEXT_READY",
    gaps,
    budget: {
      maxChars,
      requiredChars,
      appliesTo: "selected-records-json-unicode-code-points",
      returnedChars: status === "CONTEXT_READY" ? requiredChars : 0,
    },
    records: status === "CONTEXT_READY" ? structuredClone(views) : [],
    sources: sources.map(({ snapshot, value }) => ({
      name: snapshot.name,
      path: snapshot.path,
      sha256: snapshot.sha256,
      sourceIdentity: {
        schema: value.schema,
        artifactId: value.id,
        profile: value.profile,
        projectId: value.projectId,
      },
      observedAt: value.observedAt,
      readback: snapshot.unchanged === true ? "unchanged-byte-pin" : "supplied-snapshot-only",
    })),
    limits: [
      "Context is source data, not a native permission grant, current capability assertion or AcceptanceDecision.",
      "Source producer identity stays ontology-platform; actor, Work owner and domain owner are separate.",
      "Selected records do not load linked targets, Work context references, or the complete Wiki corpus.",
      "The immutable upper Ontology pin is a compatibility reference; legacy producer schemas are not silently relabelled as package instances.",
      "A missing, stale, conflicting or oversized result returns no partial body; narrow the selection or refresh the owning source.",
    ],
  };
}
