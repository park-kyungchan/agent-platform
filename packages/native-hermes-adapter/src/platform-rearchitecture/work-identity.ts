/** Explicit domain extensions over Ontology0.2.0; these are not upper ArtifactEnvelope kinds. */
import { createHash } from "node:crypto";
import type {
  WorkIssueRecord,
  WorkSpecRecord,
  WorkPlanBinding,
} from "./work-identity-contracts.ts";
export interface WorkIdentityDocuments {
  issue: unknown;
  spec: unknown;
  planBinding: unknown;
  plan: unknown;
  decisions: unknown;
  authorization: unknown;
}
export interface IdentityPurposePin {
  role: "plan" | "decisions" | "authorization";
  originPath: string;
  sha256: string;
}
const canonical = (v: unknown): string =>
  JSON.stringify(v, (_k, x) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(
          Object.keys(x)
            .sort()
            .map((k) => [k, x[k]]),
        )
      : x,
  );
function check(ok: unknown, message: string): asserts ok {
  if (!ok) throw Error("Work identity: " + message);
}
function record(x: unknown): asserts x is Record<string, any> {
  check(x && typeof x === "object" && !Array.isArray(x), "source record required");
}
function same(a: unknown, b: unknown, message: string) {
  check(canonical(a) === canonical(b), message);
}
export function validateWorkIdentityDocuments(
  documents: WorkIdentityDocuments,
  expected: { projectId: string; workId: string; pins: IdentityPurposePin[] },
) {
  const { issue, spec, planBinding: binding, plan, decisions, authorization } = documents;
  record(issue);
  record(spec);
  record(binding);
  record(plan);
  record(decisions);
  record(authorization);
  const entries = [
    [issue, "issue", "hermes.work-issue.v1"],
    [spec, "spec", "hermes.work-spec.v1"],
    [binding, "plan", "hermes.work-plan-binding.v1"],
  ] as const;
  for (const [d, kind, schema] of entries) {
    check(d.kind === kind && d.schema === schema, "exact Work-domain schema/kind required");
    check(
      d.projectId === expected.projectId && d.ontologyVersion === "0.2.0",
      "same source owner/Ontology pin required",
    );
    check(
      d.recordClass === "current-source-projection-of-approved-scope-not-new-user-acceptance" &&
        typeof d.createdAt === "string" &&
        /^\d{4}-\d\d-\d\dT.*Z$/.test(d.createdAt) &&
        Number.isFinite(Date.parse(d.createdAt)),
      "current source projection provenance required",
    );
    check(
      typeof d.id === "string" && d.id.length > 0 && !/^[tp]_/.test(d.id),
      "explicit source-owned identity, never native Task/Project ID",
    );
    record(d.payload);
  }
  const ids = { workId: expected.workId, issueId: issue.id, specId: spec.id, planId: binding.id };
  for (const d of [issue, spec, binding])
    for (const [key, value] of Object.entries(ids))
      check(d.payload[key] === value, "Issue/Spec/Plan cross-join changed: " + key);
  check(
    plan.workId === expected.workId && plan.status === "implementation-authorized",
    "exact current approved Plan Work required",
  );
  check(
    spec.payload.revision === 1 && binding.payload.revision === 1,
    "explicit current domain revision1 required",
  );
  check(
    issue.payload.historicalAttributionInvented === false &&
      Array.isArray(issue.payload.nativeTaskIds) &&
      issue.payload.nativeTaskIds.length === 0 &&
      Array.isArray(issue.payload.nativeSessionIds) &&
      issue.payload.nativeSessionIds.length === 0,
    "initial source identity cannot invent native Task/session attribution",
  );
  check(
    spec.payload.implementationResultsAccepted === false &&
      spec.payload.nativePermissionGranted === false,
    "source Spec grants no acceptance or native permission",
  );
  check(
    expected.pins.length === 3 && new Set(expected.pins.map((p) => p.role)).size === 3,
    "three exact purpose proof pins required",
  );
  const source = expected.pins.find((p) => p.role === "plan")!,
    auth = expected.pins.find((p) => p.role === "authorization")!;
  same(
    binding.payload.source,
    { path: source.originPath, sha256: source.sha256 },
    "Plan binding must pin actual original adopted Plan",
  );
  const authorizingDecision = { path: auth.originPath, sha256: auth.sha256 };
  for (const d of [issue, spec, binding])
    same(
      d.payload.authorizingDecision,
      authorizingDecision,
      "same exact authorizing decision required",
    );
  const purposePins = expected.pins.map((p) => ({ path: p.originPath, sha256: p.sha256 }));
  for (const d of [issue, spec]) {
    check(
      Array.isArray(d.payload.sourcePins) && d.payload.sourcePins.length === 3,
      "exact source pin denominator required",
    );
    same(
      [...d.payload.sourcePins].sort((a, b) => a.path.localeCompare(b.path)),
      [...purposePins].sort((a, b) => a.path.localeCompare(b.path)),
      "Issue/Spec purpose pins changed",
    );
  }
  check(
    authorization.kind === "decision" &&
      authorization.projectId === expected.projectId &&
      authorization.payload?.status === "accepted" &&
      authorization.payload?.details?.programId === expected.workId &&
      authorization.payload?.details?.priorCompletedWorkReopened === false &&
      authorization.payload?.details?.implementationResultsAccepted === false,
    "actual accepted implementation decision required; no result acceptance inferred",
  );
  check(
    Array.isArray(decisions.decisions) && decisions.decisions.some((d: any) => d.id === "D9"),
    "actual D9 source required",
  );
  check(Array.isArray(plan.stages), "approved stages required");
  same(
    spec.payload.requirements,
    plan.stages.map((s: any) => ({
      id: spec.id + ":" + s.id,
      stageId: s.id,
      outcome: s.outcome,
      dependsOn: s.dependencies,
    })),
    "Spec requirements must be exact approved stage projection",
  );
  const sectionKeys = [
    "architecture",
    "scope",
    "budget",
    "validation",
    "effects",
    "sourcePromotion",
    "git",
    "runtimeSetup",
  ];
  same(
    spec.payload.acceptedPlanSections,
    Object.fromEntries(sectionKeys.map((k) => [k, plan[k]])),
    "Spec scope/criteria must equal the existing adopted Plan sections",
  );
  return {
    ...ids,
    specRevision: 1,
    planRevision: 1,
    issue: issue as unknown as WorkIssueRecord,
    spec: spec as unknown as WorkSpecRecord,
    planBinding: binding as unknown as WorkPlanBinding,
    purposeProofDigest: createHash("sha256").update(canonical(expected.pins)).digest("hex"),
  };
}

/** Bounded current source reader; records/pins are not native actor or effect authorization. */
export async function readCurrentEngineeringIdentity(ownerRoot: string) {
  const { resolve } = await import("node:path"),
    { readExactFile } = await import("./work-reader");
  if (!ownerRoot || resolve(ownerRoot) !== ownerRoot)
    throw Error("EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED");
  const owner = ownerRoot,
    paths = {
      issue: resolve(owner, "work/hermes-platform-rearchitecture/issue.json"),
      spec: resolve(owner, "work/hermes-platform-rearchitecture/spec.json"),
      planBinding: resolve(owner, "work/hermes-platform-rearchitecture/plan-binding.json"),
      plan: resolve(owner, "work/hermes-platform-rearchitecture/plan.json"),
      decisions: resolve(owner, "work/hermes-platform-rearchitecture/interview.json"),
      authorization: resolve(owner, "decisions/platform-rearchitecture-implementation.json"),
    };
  const bytes: Record<string, Buffer> = {},
    docs: Record<string, unknown> = {},
    pins: IdentityPurposePin[] = [];
  for (const [role, path] of Object.entries(paths)) {
    const raw = await readExactFile(path, 262144);
    bytes[role] = raw;
    docs[role] = JSON.parse(raw.toString());
    if (["plan", "decisions", "authorization"].includes(role))
      pins.push({
        role: role as IdentityPurposePin["role"],
        originPath: path,
        sha256: createHash("sha256").update(raw).digest("hex"),
      });
  }
  const result = validateWorkIdentityDocuments(docs as unknown as WorkIdentityDocuments, {
    projectId: "hermes-harness-engineering",
    workId: "hermes-platform-rearchitecture",
    pins,
  });
  for (const [role, path] of Object.entries(paths))
    await readExactFile(path, 262144, createHash("sha256").update(bytes[role]!).digest("hex"));
  return {
    ...result,
    sourcePins: Object.entries(paths).map(([role, path]) => ({
      role,
      path,
      sha256: createHash("sha256").update(bytes[role]!).digest("hex"),
    })),
    nativeActor: null,
    nativeSessionSelection: null,
    authority: "none",
    executionReady: false,
  };
}
