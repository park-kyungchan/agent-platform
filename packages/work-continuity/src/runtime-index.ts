import { assertLogicalIdentity, type LogicalIdentity } from "./logical-identity.ts";

export interface NativeReferenceScope {
  runtime: "native-hermes" | "native-codex";
  profileId: string | null;
  sessionId: string | null;
  agentId: string | null;
  nativeProjectId: string | null;
  nativeTaskId: string | null;
  nativeRunId: string | null;
  logical: LogicalIdentity;
}
export interface RuntimeIndexEntry extends NativeReferenceScope {
  observation: {
    class: "declared" | "observed" | "authenticated" | "behaviorally-verified";
    observedAtUtc: string | null;
    evidenceRefs: string[];
    currentness: "current" | "stale" | "unknown";
  };
  sourceOnly: true;
}
export interface RuntimeIndex {
  schema: "native-harness.runtime-index.v1";
  entries: RuntimeIndexEntry[];
}
type Row = Record<string, unknown>;
const referenceKeys = [
  "runtime",
  "profileId",
  "sessionId",
  "agentId",
  "nativeProjectId",
  "nativeTaskId",
  "nativeRunId",
  "logical",
];
const logicalKeys = [
  "domainId",
  "projectId",
  "workId",
  "issueId",
  "specId",
  "specRevision",
  "planId",
  "planRevision",
  "taskId",
];
function row(value: unknown, keys: string[]): Row {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("INVALID_RUNTIME_INDEX");
  const result = value as Row;
  if (Object.keys(result).length !== keys.length || keys.some((key) => !Object.hasOwn(result, key)))
    throw Error("INVALID_RUNTIME_INDEX_FIELDS");
  return result;
}
function literal(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.trim() !== value ||
    !value ||
    value.length > 2048 ||
    /[\u0000-\u001f]/u.test(value)
  )
    throw Error("INVALID_NATIVE_REFERENCE");
  return value;
}
function optional(value: unknown): string | null {
  return value === null ? null : literal(value);
}
function logical(value: unknown): LogicalIdentity {
  const data = row(value, logicalKeys);
  const result: LogicalIdentity = {
    domainId: literal(data.domainId),
    projectId: literal(data.projectId),
    workId: literal(data.workId),
    issueId: literal(data.issueId),
    specId: literal(data.specId),
    specRevision: data.specRevision as number,
    planId: literal(data.planId),
    planRevision: data.planRevision as number,
    taskId: literal(data.taskId),
  };
  assertLogicalIdentity(result);
  return result;
}
export function validateNativeReferenceScope(value: unknown): NativeReferenceScope {
  const data = row(value, referenceKeys);
  if (data.runtime !== "native-hermes" && data.runtime !== "native-codex")
    throw Error("INVALID_NATIVE_RUNTIME_REFERENCE");
  return {
    runtime: data.runtime,
    profileId: optional(data.profileId),
    sessionId: optional(data.sessionId),
    agentId: optional(data.agentId),
    nativeProjectId: optional(data.nativeProjectId),
    nativeTaskId: optional(data.nativeTaskId),
    nativeRunId: optional(data.nativeRunId),
    logical: logical(data.logical),
  };
}
/** Bounded audit references only; this never contacts, authenticates or mirrors a native owner. */
export function validateRuntimeIndex(value: unknown): RuntimeIndex {
  const data = row(value, ["schema", "entries"]);
  if (
    data.schema !== "native-harness.runtime-index.v1" ||
    !Array.isArray(data.entries) ||
    data.entries.length > 256
  )
    throw Error("INVALID_RUNTIME_INDEX");
  const entries = data.entries.map((value): RuntimeIndexEntry => {
    const data = row(value, [...referenceKeys, "observation", "sourceOnly"]);
    const scope = validateNativeReferenceScope(
      Object.fromEntries(referenceKeys.map((key) => [key, data[key]])),
    );
    const observation = row(data.observation, [
      "class",
      "observedAtUtc",
      "evidenceRefs",
      "currentness",
    ]);
    if (
      typeof observation.class !== "string" ||
      !["declared", "observed", "authenticated", "behaviorally-verified"].includes(
        observation.class,
      ) ||
      typeof observation.currentness !== "string" ||
      !["current", "stale", "unknown"].includes(observation.currentness) ||
      data.sourceOnly !== true ||
      !Array.isArray(observation.evidenceRefs) ||
      observation.evidenceRefs.length > 32
    )
      throw Error("INVALID_RUNTIME_OBSERVATION");
    const evidenceRefs = observation.evidenceRefs.map(literal);
    const observedAtUtc = optional(observation.observedAtUtc);
    if (
      observedAtUtc !== null &&
      (!/^\d{4}-\d\d-\d\dT.*Z$/u.test(observedAtUtc) || !Number.isFinite(Date.parse(observedAtUtc)))
    )
      throw Error("INVALID_RUNTIME_OBSERVATION_TIME");
    if (observation.class !== "declared" && (!observedAtUtc || !evidenceRefs.length))
      throw Error("NATIVE_OBSERVATION_EVIDENCE_REQUIRED");
    return {
      ...scope,
      observation: {
        class: observation.class as RuntimeIndexEntry["observation"]["class"],
        observedAtUtc,
        evidenceRefs,
        currentness: observation.currentness as RuntimeIndexEntry["observation"]["currentness"],
      },
      sourceOnly: true,
    };
  });
  return { schema: "native-harness.runtime-index.v1", entries };
}
export interface RuntimeReferenceSelection {
  status: "matched" | "held";
  entry: RuntimeIndexEntry | null;
  holdReasons: string[];
  verificationClass: "source-reference-only";
  nativeAuthorshipVerified: false;
  authority: "none";
  nativePermissionGranted: false;
}
export function selectRuntimeReference(
  index: unknown,
  selected: unknown,
): RuntimeReferenceSelection {
  const held = (reason: string): RuntimeReferenceSelection => ({
    status: "held",
    entry: null,
    holdReasons: [reason],
    verificationClass: "source-reference-only",
    nativeAuthorshipVerified: false,
    authority: "none",
    nativePermissionGranted: false,
  });
  try {
    const scope = validateNativeReferenceScope(selected),
      validated = validateRuntimeIndex(index);
    if (!scope.sessionId || (scope.runtime === "native-hermes" && !scope.profileId))
      return held("EXPLICIT_NATIVE_PROFILE_SESSION_REQUIRED");
    const entries = validated.entries.filter((entry) =>
      referenceKeys.every((key) =>
        key === "logical"
          ? JSON.stringify(entry.logical) === JSON.stringify(scope.logical)
          : entry[key as keyof NativeReferenceScope] === scope[key as keyof NativeReferenceScope],
      ),
    );
    if (entries.length !== 1)
      return held(
        entries.length ? "AMBIGUOUS_NATIVE_WORK_REFERENCE" : "NATIVE_WORK_REFERENCE_NOT_SELECTED",
      );
    const entry = entries[0]!;
    if (entry.observation.currentness !== "current")
      return held("NATIVE_REFERENCE_CURRENTNESS_" + entry.observation.currentness.toUpperCase());
    return {
      status: "matched",
      entry,
      holdReasons: [],
      verificationClass: "source-reference-only",
      nativeAuthorshipVerified: false,
      authority: "none",
      nativePermissionGranted: false,
    };
  } catch {
    return held("INVALID_RUNTIME_REFERENCE_SELECTION");
  }
}
