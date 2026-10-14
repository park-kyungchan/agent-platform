/** Harness-owned audit/application contracts. Neither these records nor their
 * validators replace native identity, permissions, execution, or acceptance. */
export const UPPER_ONTOLOGY = { package: "@palantirkc/ontology", version: "0.2.0" } as const;
export const SEMANTIC_OWNER = "ontology-platform" as const;
export interface Pin {
  path: string;
  sha256: string;
}
export type IntentOrigin =
  | "AUTHORED_USER"
  | "NATIVE_INTERVIEW_ANSWER"
  | "PRIOR_CONTEXT"
  | "RELAY"
  | "COMPRESSION"
  | "ASSISTANT_INFERENCE"
  | "REMOTE_METADATA";
export interface IntentSource {
  id: string;
  origin: IntentOrigin;
  pin: Pin;
  selector: string;
  fragmentSha256: string;
  observedAt: string;
  /** Source producer classification, not a claim that a user-role row is authored. */
  authorRole: "user" | "assistant" | "system" | "unknown";
}
export interface IntentStatement {
  id: string;
  topic: string;
  statement: string;
  status: "CURRENT" | "SUPERSEDED" | "UNRESOLVED";
  sourceIds: string[];
  supersedes: string[];
  scope: { profiles: string[]; repositories: string[] };
}
export interface CurrentIntentRecord {
  schema: "hermes.current-intent.v1";
  id: string;
  programId: string;
  revision: number;
  recordedAt: string;
  semanticOwner: typeof SEMANTIC_OWNER;
  sources: IntentSource[];
  statements: IntentStatement[];
  repositoryResponsibilities: Array<{
    repositoryId: string;
    ownerProfile: string;
    domainOwner: string;
    statementId: string;
  }>;
  /** Preservation is a constraint on later native work, not proof of preservation. */
  preserveDataAndHistory: true;
}
export type NativeOwnerIdentity =
  | {
      runtime: "native-hermes";
      containerId: string;
      hostPid: number;
      localPid: number;
      startTicks: number;
      pidNamespace: string;
      sourceCommit: string;
    }
  | { runtime: "native-codex"; threadId: string };
export interface RepositoryBinding {
  repositoryId: string;
  root: string;
  evidenceScope: "metadata-and-selected-files" | "complete-source-tree";
  version:
    | { kind: "git-checkout"; commit: string; dirtyState: "clean" | "dirty"; dirtySnapshot?: Pin }
    | { kind: "source-archive"; sourceRevision: string | null; archiveManifest: Pin };
  evidence: Pin;
}
export interface DomainContextBinding {
  repositoryId: string;
  projection: Pin;
  expectedRevision: string;
  workUnit: string;
  workState: "in_progress" | "complete";
  relation: "current-work" | "reference";
  sourcePins: Array<{ id: string; path: string; sha256: string }>;
  /** Optional current owner files, separate from the captured producer result.
   * IDs/hashes must cover the complete declared sourcePins denominator. */
  liveSources?: Array<{ id: string; path: string; sha256: string }>;
}
export type CapabilityStage = "configured" | "exposed" | "authenticated" | "behaviorallyVerified";
export interface CapabilityClaim {
  name: string;
  stage: CapabilityStage;
  evidence: Pin;
  observedAt: string;
  validUntil: string;
}
export interface EffectScope {
  kind: string;
  target: string;
}
export interface WorkBinding {
  schema: "hermes.work-binding.v1";
  id: string;
  revision: number;
  programId: string;
  actor: { runtime: "native-hermes" | "native-codex"; profile: string };
  work: { id: string; owner: string; domainOwner: string };
  intent: Pin;
  intentId: string;
  intentRevision: number;
  statementIds: string[];
  repositories: RepositoryBinding[];
  acceptingOwner: string;
  domainContext?: DomainContextBinding;
}
export interface NativeObservation {
  schema: "hermes.native-observation.v1";
  binding: Pin;
  actor: WorkBinding["actor"];
  workId: string;
  observedAt: string;
  validUntil: string;
  evidence: Pin;
  owner: NativeOwnerIdentity;
  capabilities: CapabilityClaim[];
  deniedCapabilities: string[];
  effectScope: EffectScope[];
}
export type ReadPinned = ((pin: Pin) => Promise<Uint8Array>) & {
  resolvePath?: (path: string) => string;
};
export class OntologyContextError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(`${code}: ${message}`);
    this.name = "OntologyContextError";
  }
}
export function requireValue(value: unknown, code: string, message: string): asserts value {
  if (!value) throw new OntologyContextError(code, message);
}
export function object(value: unknown, label: string): asserts value is Record<string, any> {
  requireValue(
    value && typeof value === "object" && !Array.isArray(value),
    "INVALID_RECORD",
    label,
  );
}
export function exactKeys(value: Record<string, any>, allowed: readonly string[], label: string) {
  requireValue(
    Object.keys(value).every((k) => allowed.includes(k)),
    "UNKNOWN_FIELD",
    label,
  );
}
export function text(value: unknown, label: string): asserts value is string {
  requireValue(
    typeof value === "string" &&
      value.trim().length > 0 &&
      value.length <= 8000 &&
      !value.includes("\0"),
    "INVALID_TEXT",
    label,
  );
}
export function identity(value: unknown, label: string): asserts value is string {
  requireValue(
    typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9_.:@/-]{0,255}$/.test(value),
    "INVALID_ID",
    label,
  );
}
export function strings(value: unknown, label: string): asserts value is string[] {
  requireValue(
    Array.isArray(value) &&
      value.length <= 256 &&
      value.every((v) => typeof v === "string" && v.length > 0) &&
      new Set(value).size === value.length,
    "INVALID_LIST",
    label,
  );
}
export function timestamp(value: unknown, label: string): asserts value is string {
  requireValue(
    typeof value === "string" &&
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/.test(value) &&
      Number.isFinite(Date.parse(value)),
    "INVALID_TIME",
    label,
  );
  const canonical = value.includes(".")
    ? value.replace(/\.(\d{1,3})Z$/, (_, fraction: string) => "." + fraction.padEnd(3, "0") + "Z")
    : value.replace("Z", ".000Z");
  requireValue(new Date(value).toISOString() === canonical, "INVALID_TIME", label);
}
export function pin(value: unknown): asserts value is Pin {
  object(value, "pin");
  exactKeys(value, ["path", "sha256"], "pin");
  text(value.path, "pin.path");
  requireValue(
    typeof value.sha256 === "string" && /^[a-f0-9]{64}$/.test(value.sha256),
    "INVALID_PIN",
    value.path,
  );
}
export const hash = (raw: string | Uint8Array) =>
  new Bun.CryptoHasher("sha256").update(raw).digest("hex");
export function parsedJson(raw: Uint8Array, expected: Pin): unknown {
  pin(expected);
  requireValue(hash(raw) === expected.sha256, "SOURCE_PIN_CHANGED", expected.path);
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(raw));
}
