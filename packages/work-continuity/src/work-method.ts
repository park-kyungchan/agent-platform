import { createHash } from "node:crypto";
import type { LogicalIdentity } from "./logical-identity.ts";
import type { DependencyMilestone, SourceReference } from "./git-work/types.ts";
import type { WholeLens } from "./jev/contract.ts";

/** Source-owned facts only. None of these contracts authenticates a native caller. */
export type WorkMethodTask = Pick<LogicalIdentity, "domainId" | "projectId" | "workId" | "taskId">;
export interface WorkMethodPin {
  id: string;
  version: string;
  sha256: string;
}
export interface WorkMethodNativeIds {
  threadId: string | null;
  sessionId: string | null;
  taskId: string | null;
  runId: string | null;
  turnId: string | null;
  parentThreadId: string | null;
}
export interface WorkMethodActor {
  actorId: string;
  environmentId: string;
  runtime: "native-codex" | "native-hermes";
  profileId: string | null;
  paneId: string | null;
  terminalId: string | null;
  native: WorkMethodNativeIds;
}
export type WorkMethodRouteKind =
  | "start"
  | "task"
  | "delegate"
  | "followup"
  | "resume"
  | "refresh"
  | "pre-compact"
  | "post-compact";
export type WorkMethodMechanism = "owner-admission" | "native-enforced" | "instruction-advisory";
export interface WorkMethodRoute {
  id: string;
  kind: WorkMethodRouteKind;
  adapter: WorkMethodPin;
  supported: boolean;
  /** The adapter owns this limit. Required meaning must never be truncated to fit. */
  maxPacketBytes: number;
}
export interface WorkMethodStanding {
  schema: "common.agent-work-method.standing.v1";
  protocol: WorkMethodPin;
  purpose: string;
  acceptedWhy: string;
  authority: SourceReference;
  owners: WholeLens["owners"];
  routes: WorkMethodRoute[];
}
export interface WorkMethodRevision {
  id: string;
  revision: number | null;
  status: "admitted" | "proposed";
  source: SourceReference;
}
export interface WorkMethodPrerequisite {
  producer: WorkMethodTask;
  receiver: WorkMethodTask;
  milestone: DependencyMilestone;
  /** An exact, owner-observed producer result and matching receiver use are separate. */
  producerReceipt: SourceReference;
  receiverReceipt: SourceReference;
}
export interface WorkMethodOwnerTransition {
  from: WholeLens["owners"][number];
  to: WholeLens["owners"][number];
  logical: WorkMethodTask;
  authority: SourceReference;
}
export interface WorkMethodPacket {
  schema: "common.agent-work-method.scoped.v1";
  packetId: string;
  epochId: string;
  protocol: WorkMethodPin;
  adapter: WorkMethodPin;
  logical: WorkMethodTask;
  semantic: {
    issue: WorkMethodRevision | null;
    spec: WorkMethodRevision | null;
    plan: WorkMethodRevision | null;
  };
  task: { text: string; sha256: string; source: SourceReference };
  lens: WholeLens;
  authority: SourceReference;
  phase: "bootstrap" | "general";
  role: string;
  ownerTransitions: WorkMethodOwnerTransition[];
  native: WorkMethodNativeIds;
  references: {
    purpose: SourceReference[];
    decisions: SourceReference[];
    context: SourceReference[];
    memory: { status: "selected" | "none-applicable"; sources: SourceReference[]; reason: string };
  };
  source: {
    /** Repository access, distinct from writing the assigned bounded result receipt. */
    access: "write" | "read-only";
    repositoryId: string;
    baseCommit: string;
    headCommit: string;
    branch: string;
    worktree: string;
    ownedPaths: string[];
    resultPath: string;
  };
  fences: {
    completed: string[];
    consumed: string[];
    uncertain: string[];
    source: SourceReference;
    budget: SourceReference;
    permissions: SourceReference;
  };
  prerequisites: WorkMethodPrerequisite[];
}
export interface WorkMethodReceipt {
  schema: "common.agent-work-method.receipt.v1";
  id: string;
  observedAt: string;
  actor: WorkMethodActor;
  routeId: string;
  logical: WorkMethodTask;
  packetId: string;
  packetSha256: string;
  taskSha256: string;
  protocol: WorkMethodPin;
  adapter: WorkMethodPin;
  currentness: "current" | "stale" | "unknown";
  received: boolean;
  used: boolean;
  /** Owner observations, not flags inferred from source installation or a delivery ACK. */
  useEvidence: SourceReference[];
  authorityReloaded: boolean;
  admission: {
    ownerId: string;
    point: string;
    mechanism: WorkMethodMechanism;
    consumedBeforeEffects: boolean;
    evidence: SourceReference[];
  };
  transitionRefs: SourceReference[];
}
export interface WorkMethodSourceObservation {
  source: SourceReference;
  actualSha256: string | null;
  currentness: "current" | "stale" | "unknown";
}
export interface WorkMethodPrerequisiteObservation extends WorkMethodPrerequisite {
  currentness: "current" | "stale" | "unknown";
  producerSatisfied: boolean;
  receiverUsed: boolean;
}
export interface WorkMethodAdvice {
  status: "held" | "consumed";
  reason: string;
  actorId: string;
  eventId: string | null;
  packetSha256: string;
  taskSha256: string;
  /** Source revision of the actual native event, never a guessed parent Work. */
  sourceSha256: string | null;
  /** Facts from the existing budget owner; changed numbers grant no new allowance. */
  budgetSource: SourceReference;
  cumulativeLimitUsd: number | null;
  cumulativeUsageUsd: number | null;
  delivered: boolean;
  consumed: boolean;
  evidence: SourceReference[];
}
export interface WorkMethodConcurrentBinding {
  actorId: string;
  packetId: string;
  packetSha256: string;
  logical: WorkMethodTask;
  source: WorkMethodPacket["source"];
}
export interface WorkMethodEventBinding {
  eventId: string | null;
  actorId: string;
  packetSha256: string;
  taskSha256: string;
  source: SourceReference;
  native: WorkMethodNativeIds;
}
export interface WorkMethodAdmissionInput {
  standing: WorkMethodStanding;
  packet: WorkMethodPacket;
  receipt: WorkMethodReceipt;
  expectedActor: WorkMethodActor;
  routeId: string;
  currentness: {
    protocol: WorkMethodPin;
    adapter: WorkMethodPin;
    status: "current" | "stale" | "unknown";
  };
  /** Owner facts freshly checked at the supported boundary. No IO is performed here. */
  sources: WorkMethodSourceObservation[];
  prerequisites: WorkMethodPrerequisiteObservation[];
  concurrent: WorkMethodConcurrentBinding[];
  advice: WorkMethodAdvice;
  requiresJudgment: boolean;
  /** Independently supplied current native-event observation; unknown is null. */
  currentEvent: WorkMethodEventBinding | null;
  /** Bootstrap admission precedes coverage closure; general work requires its owner release. */
  release: WorkMethodRelease | null;
}
export interface WorkMethodValidation {
  valid: boolean;
  findings: string[];
}
export interface WorkMethodAdmission {
  status: "owner-eligible" | "held";
  reasons: string[];
  actorId: string | null;
  routeId: string | null;
  epochId: string | null;
  packetSha256: string | null;
  receiptId: string | null;
  receiptSha256: string | null;
  sourceObservationsSha256: string | null;
  mechanism: WorkMethodMechanism | null;
  adviceMode: "consumed" | "deterministic-held-advice";
  adviceHeldReasons: string[];
  /** Eligibility never replaces the native/source owner's effect-specific decision. */
  nativePermissionGranted: false;
  executionAuthorityGranted: false;
  retryAllowed: false;
  completionGranted: false;
}
export interface WorkMethodCoverageRow {
  id: string;
  environmentId: string;
  ownerId: string;
  routeId: string;
  actorId: string | null;
  required: boolean;
  state: "active" | "dormant" | "non-agent" | "unknown";
  protocol: WorkMethodPin;
  adapter: WorkMethodPin;
  classificationEvidence: SourceReference[];
  wake: {
    mechanism: WorkMethodMechanism;
    holdBeforeGeneralEffects: boolean;
    evidence: SourceReference[];
  } | null;
  receiptId: string | null;
  packetSha256: string | null;
}
export interface WorkMethodCoverageEpoch {
  schema: "common.agent-work-method.coverage.v1";
  epochId: string;
  ownerId: string;
  attestation: SourceReference;
  closedAt: string | null;
  changedBeforeRelease: boolean;
  rows: WorkMethodCoverageRow[];
}
export interface WorkMethodCoverage {
  status: "owner-eligible" | "held";
  epochId: string | null;
  epochSha256: string | null;
  observationsSha256: string | null;
  sourceObservationsSha256: string | null;
  reasons: string[];
  heldRowIds: string[];
  arbitraryAgentCoverage: false;
  nativePermissionGranted: false;
}
/** This bounded source receipt is written/read by the existing owner, never this library. */
export interface WorkMethodRelease {
  schema: "common.agent-work-method.release.v1";
  epochId: string;
  ownerId: string;
  source: SourceReference;
  coverage: WorkMethodCoverage;
  bootstrapClosed: boolean;
  currentness: "current" | "stale" | "unknown";
}

const digest = (value: string) => createHash("sha256").update(value).digest("hex");
/** Stable hashes isolate concurrent packets; hashing is not native authentication. */
function canonical(value: unknown): string {
  if (value === null || typeof value === "string" || typeof value === "boolean")
    return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object")
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`)
      .join(",")}}`;
  throw Error("NON_JSON_VALUE");
}
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw Error(message);
}
function object(value: unknown): asserts value is Record<string, unknown> {
  requireValue(value && typeof value === "object" && !Array.isArray(value), "INVALID_OBJECT");
}
function text(value: unknown): asserts value is string {
  requireValue(typeof value === "string" && value.trim().length > 0, "MISSING_TEXT");
}
function hash(value: unknown): asserts value is string {
  requireValue(typeof value === "string" && /^[a-f0-9]{64}$/.test(value), "INVALID_SHA256");
}
function texts(value: unknown): asserts value is string[] {
  requireValue(Array.isArray(value), "INVALID_LIST");
  for (const item of value) text(item);
  requireValue(new Set(value).size === value.length, "DUPLICATE_VALUE");
}
function list(value: unknown): asserts value is unknown[] {
  requireValue(Array.isArray(value), "INVALID_LIST");
}
function bool(value: unknown): void {
  requireValue(typeof value === "boolean", "INVALID_BOOLEAN");
}
function nullableText(value: unknown): void {
  if (value !== null) text(value);
}
function pin(value: unknown): asserts value is WorkMethodPin {
  object(value);
  text(value.id);
  requireValue(
    typeof value.version === "string" && /^\d+\.\d+\.\d+$/.test(value.version),
    "INVALID_VERSION",
  );
  hash(value.sha256);
}
function nativeIds(value: unknown): asserts value is WorkMethodNativeIds {
  object(value);
  for (const key of ["threadId", "sessionId", "taskId", "runId", "turnId", "parentThreadId"])
    nullableText(value[key]);
}
function logical(value: unknown): asserts value is WorkMethodTask {
  object(value);
  for (const key of ["domainId", "projectId", "workId", "taskId"]) text(value[key]);
}
function source(value: unknown): asserts value is SourceReference {
  object(value);
  if (value.kind === "native") {
    text(value.ownerId);
    text(value.locator);
    source(value.receipt);
    requireValue(value.receipt.kind !== "native", "NESTED_NATIVE_SOURCE");
    return;
  }
  requireValue(value.kind === "snapshot" || value.kind === "git", "INVALID_SOURCE_KIND");
  text(value.path);
  hash(value.sha256);
  if (value.kind === "git") {
    text(value.repositoryId);
    requireValue(
      typeof value.commit === "string" && /^[a-f0-9]{40,64}$/.test(value.commit),
      "INVALID_COMMIT",
    );
  }
  if (value.fragment !== undefined) text(value.fragment);
}
function sources(value: unknown, required = false): asserts value is SourceReference[] {
  list(value);
  if (required) requireValue(value.length > 0, "MISSING_SOURCE");
  for (const item of value) source(item);
  requireValue(new Set(value.map(canonical)).size === value.length, "DUPLICATE_SOURCE");
}
function sourceHash(value: SourceReference): string {
  return value.kind === "native" ? sourceHash(value.receipt) : value.sha256!;
}
function currentSource(
  source: SourceReference,
  observations: WorkMethodSourceObservation[],
): boolean {
  const matches = observations.filter((row) => canonical(row.source) === canonical(source));
  return (
    matches.length === 1 &&
    matches[0]!.currentness === "current" &&
    matches[0]!.actualSha256 === sourceHash(source)
  );
}
function sourceObservations(value: unknown): asserts value is WorkMethodSourceObservation[] {
  list(value);
  for (const row of value) {
    object(row);
    source(row.source);
    if (row.actualSha256 !== null) hash(row.actualSha256);
    requireValue(
      ["current", "stale", "unknown"].includes(String(row.currentness)),
      "INVALID_CURRENTNESS",
    );
  }
}
function uniqueSources(selected: SourceReference[]): SourceReference[] {
  return [...new Map(selected.map((row) => [canonical(row), row])).values()];
}
function actor(value: unknown): asserts value is WorkMethodActor {
  object(value);
  text(value.actorId);
  text(value.environmentId);
  requireValue(
    ["native-codex", "native-hermes"].includes(String(value.runtime)),
    "INVALID_RUNTIME",
  );
  for (const key of ["profileId", "paneId", "terminalId"]) nullableText(value[key]);
  nativeIds(value.native);
}
const routeKinds = [
  "start",
  "task",
  "delegate",
  "followup",
  "resume",
  "refresh",
  "pre-compact",
  "post-compact",
];
const mechanisms = ["owner-admission", "native-enforced", "instruction-advisory"];
const milestones = [
  "execution-and-verification",
  "shared-audit-delivery",
  "formal-task-completion",
  "acceptance",
];
function owner(value: unknown): asserts value is WholeLens["owners"][number] {
  object(value);
  requireValue(
    ["native", "source", "domain", "data"].includes(String(value.role)),
    "INVALID_OWNER_ROLE",
  );
  text(value.id);
}
function owners(value: unknown): void {
  list(value);
  for (const row of value) owner(row);
  requireValue(new Set(value.map(canonical)).size === value.length, "DUPLICATE_OWNER");
  requireValue(
    value.some((row) => (row as { role: string }).role === "native"),
    "MISSING_NATIVE_OWNER",
  );
  requireValue(
    value.some((row) => (row as { role: string }).role === "source"),
    "MISSING_SOURCE_OWNER",
  );
}
function assertStanding(value: unknown): asserts value is WorkMethodStanding {
  object(value);
  requireValue(value.schema === "common.agent-work-method.standing.v1", "INVALID_STANDING_SCHEMA");
  pin(value.protocol);
  requireValue(value.protocol.version.startsWith("1."), "UNSUPPORTED_PROTOCOL_VERSION");
  text(value.purpose);
  text(value.acceptedWhy);
  source(value.authority);
  owners(value.owners);
  list(value.routes);
  requireValue(value.routes.length > 0, "MISSING_ROUTES");
  const ids: string[] = [];
  for (const row of value.routes) {
    object(row);
    text(row.id);
    ids.push(row.id);
    requireValue(routeKinds.includes(String(row.kind)), "INVALID_ROUTE_KIND");
    pin(row.adapter);
    bool(row.supported);
    requireValue(
      Number.isSafeInteger(row.maxPacketBytes) && Number(row.maxPacketBytes) > 0,
      "INVALID_PACKET_BOUND",
    );
  }
  requireValue(new Set(ids).size === ids.length, "DUPLICATE_ROUTE");
}
function lens(value: unknown): asserts value is WholeLens {
  object(value);
  object(value.currentAuthority);
  text(value.currentAuthority.text);
  texts(value.currentAuthority.sourceRefs);
  requireValue(value.currentAuthority.sourceRefs.length > 0, "MISSING_AUTHORITY_SOURCE");
  text(value.wholeObjective);
  text(value.taskGoal);
  list(value.acceptedWhy);
  requireValue(value.acceptedWhy.length > 0, "MISSING_WHY");
  for (const row of value.acceptedWhy) {
    object(row);
    text(row.text);
    requireValue(
      ["current-user", "historical", "inference"].includes(String(row.attribution)),
      "INVALID_WHY_ATTRIBUTION",
    );
    texts(row.sourceRefs);
    requireValue(row.sourceRefs.length > 0, "MISSING_WHY_SOURCE");
  }
  requireValue(
    value.acceptedWhy.some((row) => (row as { attribution: string }).attribution !== "inference"),
    "ACCEPTED_WHY_REQUIRED",
  );
  list(value.acceptedDecisions);
  for (const row of value.acceptedDecisions) {
    object(row);
    text(row.id);
    text(row.revision);
    texts(row.sourceRefs);
    requireValue(row.sourceRefs.length > 0, "MISSING_DECISION_SOURCE");
  }
  owners(value.owners);
  list(value.dependencies);
  for (const row of value.dependencies) {
    object(row);
    text(row.id);
    nullableText(row.revision);
    requireValue(
      ["current", "stale", "unknown"].includes(String(row.currentness)),
      "INVALID_CURRENTNESS",
    );
  }
  for (const key of ["completedEffectRefs", "uncertainEffectRefs", "remainingAcceptance"])
    texts(value[key]);
}
function prerequisite(value: unknown): asserts value is WorkMethodPrerequisite {
  object(value);
  logical(value.producer);
  logical(value.receiver);
  requireValue(milestones.includes(String(value.milestone)), "INVALID_MILESTONE");
  source(value.producerReceipt);
  source(value.receiverReceipt);
}
function assertPacket(value: unknown): asserts value is WorkMethodPacket {
  object(value);
  requireValue(value.schema === "common.agent-work-method.scoped.v1", "INVALID_PACKET_SCHEMA");
  text(value.packetId);
  text(value.epochId);
  pin(value.protocol);
  pin(value.adapter);
  logical(value.logical);
  object(value.semantic);
  for (const key of ["issue", "spec", "plan"]) {
    const row = value.semantic[key];
    if (row === null) continue;
    object(row);
    text(row.id);
    requireValue(["admitted", "proposed"].includes(String(row.status)), "INVALID_SEMANTIC_STATUS");
    requireValue(
      row.revision === null || (Number.isSafeInteger(row.revision) && Number(row.revision) > 0),
      "INVALID_REVISION",
    );
    requireValue(
      row.status !== "admitted" || key === "issue" || row.revision !== null,
      "ADMITTED_REVISION_REQUIRED",
    );
    source(row.source);
  }
  object(value.task);
  text(value.task.text);
  hash(value.task.sha256);
  requireValue(digest(value.task.text) === value.task.sha256, "EXACT_TASK_HASH_MISMATCH");
  source(value.task.source);
  lens(value.lens);
  source(value.authority);
  requireValue(value.phase === "bootstrap" || value.phase === "general", "INVALID_PHASE");
  text(value.role);
  list(value.ownerTransitions);
  for (const row of value.ownerTransitions) {
    object(row);
    owner(row.from);
    owner(row.to);
    logical(row.logical);
    source(row.authority);
    requireValue(
      row.from.role === row.to.role && canonical(row.from) !== canonical(row.to),
      "INVALID_OWNER_TRANSITION",
    );
    requireValue(
      canonical(row.logical) === canonical(value.logical),
      "WRONG_OWNER_TRANSITION_TASK",
    );
  }
  requireValue(
    new Set(value.ownerTransitions.map(canonical)).size === value.ownerTransitions.length,
    "DUPLICATE_OWNER_TRANSITION",
  );
  nativeIds(value.native);
  object(value.references);
  sources(value.references.purpose, true);
  sources(value.references.decisions);
  if (value.lens.acceptedDecisions.length > 0) sources(value.references.decisions, true);
  sources(value.references.context, true);
  object(value.references.memory);
  requireValue(
    ["selected", "none-applicable"].includes(String(value.references.memory.status)),
    "INVALID_MEMORY_STATUS",
  );
  sources(value.references.memory.sources, value.references.memory.status === "selected");
  requireValue(
    value.references.memory.status !== "none-applicable" ||
      value.references.memory.sources.length === 0,
    "AMBIGUOUS_MEMORY_SELECTION",
  );
  text(value.references.memory.reason);
  object(value.source);
  requireValue(
    ["write", "read-only"].includes(String(value.source.access)),
    "INVALID_SOURCE_ACCESS",
  );
  for (const key of ["repositoryId", "branch", "worktree", "resultPath"]) text(value.source[key]);
  for (const key of ["baseCommit", "headCommit"])
    requireValue(
      typeof value.source[key] === "string" && /^[a-f0-9]{40,64}$/.test(value.source[key]),
      "INVALID_COMMIT",
    );
  texts(value.source.ownedPaths);
  requireValue(
    value.source.access !== "read-only" || value.source.ownedPaths.length === 0,
    "READ_ONLY_OWNED_PATHS",
  );
  for (const path of value.source.ownedPaths)
    requireValue(
      !path.startsWith("/") &&
        !path.includes("\\") &&
        !path.split("/").some((part) => ["", ".", ".."].includes(part)),
      "INVALID_OWNED_PATH",
    );
  object(value.fences);
  for (const key of ["completed", "consumed", "uncertain"]) texts(value.fences[key]);
  for (const key of ["source", "budget", "permissions"]) source(value.fences[key]);
  const fences = value.fences as unknown as WorkMethodPacket["fences"];
  requireValue(
    value.lens.completedEffectRefs.every((id) => fences.completed.includes(id)),
    "COMPLETED_FENCE_OMITTED",
  );
  requireValue(
    value.lens.uncertainEffectRefs.every((id) => fences.uncertain.includes(id)),
    "UNCERTAIN_FENCE_OMITTED",
  );
  list(value.prerequisites);
  for (const row of value.prerequisites) {
    prerequisite(row);
    requireValue(
      canonical(row.receiver) === canonical(value.logical),
      "WRONG_PREREQUISITE_RECEIVER",
    );
  }
  requireValue(
    new Set(value.prerequisites.map(canonical)).size === value.prerequisites.length,
    "DUPLICATE_PREREQUISITE",
  );
}
function assertReceipt(value: unknown): asserts value is WorkMethodReceipt {
  object(value);
  requireValue(value.schema === "common.agent-work-method.receipt.v1", "INVALID_RECEIPT_SCHEMA");
  text(value.id);
  text(value.observedAt);
  requireValue(Number.isFinite(Date.parse(value.observedAt)), "INVALID_OBSERVATION_TIME");
  actor(value.actor);
  text(value.routeId);
  logical(value.logical);
  text(value.packetId);
  hash(value.packetSha256);
  hash(value.taskSha256);
  pin(value.protocol);
  pin(value.adapter);
  requireValue(
    ["current", "stale", "unknown"].includes(String(value.currentness)),
    "INVALID_CURRENTNESS",
  );
  for (const key of ["received", "used", "authorityReloaded"]) bool(value[key]);
  sources(value.useEvidence, value.used === true);
  requireValue(value.used !== true || value.received === true, "USE_WITHOUT_RECEIVE");
  object(value.admission);
  text(value.admission.ownerId);
  text(value.admission.point);
  requireValue(mechanisms.includes(String(value.admission.mechanism)), "INVALID_MECHANISM");
  bool(value.admission.consumedBeforeEffects);
  sources(value.admission.evidence, true);
  sources(value.transitionRefs);
}
function validation(run: () => void): WorkMethodValidation {
  try {
    run();
    return { valid: true, findings: [] };
  } catch (error) {
    return { valid: false, findings: [error instanceof Error ? error.message : "INVALID_INPUT"] };
  }
}
export const validateWorkMethodStanding = (value: unknown): WorkMethodValidation =>
  validation(() => assertStanding(value));
export const validateWorkMethodPacket = (value: unknown): WorkMethodValidation =>
  validation(() => assertPacket(value));
export const validateWorkMethodReceipt = (value: unknown): WorkMethodValidation =>
  validation(() => assertReceipt(value));
export function workMethodPacketSha256(packet: WorkMethodPacket): string {
  assertPacket(packet);
  return digest(canonical(packet));
}
export function workMethodReceiptSha256(receipt: WorkMethodReceipt): string {
  assertReceipt(receipt);
  return digest(canonical(receipt));
}

/** Exact owner-supplied source observations, not live reads or source-derived permissions. */
export function workMethodRequiredSources(
  packet: WorkMethodPacket,
  standing?: WorkMethodStanding,
  receipt?: WorkMethodReceipt,
  release?: WorkMethodRelease | null,
): SourceReference[] {
  assertPacket(packet);
  if (standing) assertStanding(standing);
  if (receipt) assertReceipt(receipt);
  const selected = [
    ...(standing ? [standing.authority] : []),
    packet.authority,
    packet.task.source,
    ...Object.values(packet.semantic).flatMap((row) => (row ? [row.source] : [])),
    ...packet.references.purpose,
    ...packet.references.decisions,
    ...packet.references.context,
    ...packet.references.memory.sources,
    ...packet.ownerTransitions.map((row) => row.authority),
    packet.fences.source,
    packet.fences.budget,
    packet.fences.permissions,
    ...packet.prerequisites.flatMap((row) => [row.producerReceipt, row.receiverReceipt]),
    ...(receipt ? receiptEvidenceSources(receipt) : []),
    ...(release ? [release.source] : []),
  ];
  return uniqueSources(selected);
}
/** These are only mandatory for consuming advice, never for independent deterministic work. */
export function workMethodAdviceRequiredSources(
  advice: WorkMethodAdvice,
  event: WorkMethodEventBinding | null,
): SourceReference[] {
  sources(advice.evidence);
  if (event) source(event.source);
  return uniqueSources([...advice.evidence, ...(event ? [event.source] : [])]);
}
function receiptEvidenceSources(receipt: WorkMethodReceipt): SourceReference[] {
  sources(receipt.useEvidence, true);
  sources(receipt.admission.evidence, true);
  sources(receipt.transitionRefs);
  return uniqueSources([
    ...receipt.useEvidence,
    ...receipt.admission.evidence,
    ...receipt.transitionRefs,
  ]);
}
export function workMethodCoverageRequiredSources(
  epoch: WorkMethodCoverageEpoch,
  observations: readonly { receipt: WorkMethodReceipt }[] = [],
): SourceReference[] {
  source(epoch.attestation);
  list(epoch.rows);
  const selected = [epoch.attestation];
  for (const row of epoch.rows) {
    sources(row.classificationEvidence);
    selected.push(...row.classificationEvidence);
    if (row.wake) {
      sources(row.wake.evidence);
      selected.push(...row.wake.evidence);
    }
    if (row.required && row.state === "active")
      for (const { receipt } of observations.filter(
        (observation) => observation.receipt.id === row.receiptId,
      ))
        selected.push(...receiptEvidenceSources(receipt));
  }
  return uniqueSources(selected);
}
function pathsOverlap(a: string[], b: string[]): boolean {
  return a.some((left) =>
    b.some(
      (right) => left === right || left.startsWith(right + "/") || right.startsWith(left + "/"),
    ),
  );
}
export function decideWorkMethodAdmission(input: WorkMethodAdmissionInput): WorkMethodAdmission {
  const result: WorkMethodAdmission = {
    status: "held",
    reasons: [],
    actorId: null,
    routeId: null,
    epochId: null,
    packetSha256: null,
    receiptId: null,
    receiptSha256: null,
    sourceObservationsSha256: null,
    mechanism: null,
    adviceMode: "deterministic-held-advice",
    adviceHeldReasons: [],
    nativePermissionGranted: false,
    executionAuthorityGranted: false,
    retryAllowed: false,
    completionGranted: false,
  };
  const hold = (reason: string) => result.reasons.push(reason);
  try {
    object(input);
    assertStanding(input.standing);
    assertPacket(input.packet);
    assertReceipt(input.receipt);
    actor(input.expectedActor);
    text(input.routeId);
    bool(input.requiresJudgment);
    const { packet, receipt, standing } = input;
    const packetHash = workMethodPacketSha256(packet);
    Object.assign(result, {
      actorId: receipt.actor.actorId,
      routeId: input.routeId,
      epochId: packet.epochId,
      packetSha256: packetHash,
      receiptId: receipt.id,
      receiptSha256: workMethodReceiptSha256(receipt),
      mechanism: receipt.admission.mechanism,
    });
    const route = standing.routes.find((row) => row.id === input.routeId);
    object(input.currentness);
    pin(input.currentness.protocol);
    pin(input.currentness.adapter);
    if (
      input.currentness.status !== "current" ||
      canonical(input.currentness.protocol) !== canonical(standing.protocol) ||
      canonical(input.currentness.adapter) !== canonical(packet.adapter)
    )
      hold("PROJECTION_NOT_CURRENT");
    if (!route || !route.supported) hold("UNSUPPORTED_ROUTE");
    if (
      canonical(packet.protocol) !== canonical(standing.protocol) ||
      canonical(receipt.protocol) !== canonical(standing.protocol)
    )
      hold("PROTOCOL_MISMATCH");
    if (
      !route ||
      canonical(packet.adapter) !== canonical(route.adapter) ||
      canonical(receipt.adapter) !== canonical(route.adapter)
    )
      hold("ADAPTER_MISMATCH");
    if (route && Buffer.byteLength(canonical(packet), "utf8") > route.maxPacketBytes)
      hold("PACKET_EXCEEDS_ROUTE_BOUND");
    if (
      receipt.routeId !== input.routeId ||
      receipt.packetId !== packet.packetId ||
      receipt.packetSha256 !== packetHash ||
      receipt.taskSha256 !== packet.task.sha256 ||
      canonical(receipt.logical) !== canonical(packet.logical)
    )
      hold("RECEIPT_PACKET_BINDING_MISMATCH");
    if (canonical(receipt.actor) !== canonical(input.expectedActor)) hold("RECEIPT_ACTOR_MISMATCH");
    for (const key of [
      "threadId",
      "sessionId",
      "taskId",
      "runId",
      "turnId",
      "parentThreadId",
    ] as const)
      if (packet.native[key] !== null && packet.native[key] !== receipt.actor.native[key])
        hold("OBSERVED_NATIVE_ID_MISMATCH");
    if (receipt.currentness !== "current") hold("RECEIPT_NOT_CURRENT");
    if (!receipt.received || !receipt.used) hold("RECEIVE_USE_REQUIRED");
    if (receipt.admission.mechanism === "instruction-advisory") hold("ADVISORY_IS_NOT_ADMISSION");
    if (!receipt.admission.consumedBeforeEffects) hold("ADMISSION_NOT_CONSUMED");
    if (
      !packet.lens.owners.some(
        (row) => row.id === receipt.admission.ownerId && ["source", "native"].includes(row.role),
      )
    )
      hold("ADMISSION_OWNER_MISMATCH");
    for (const transition of packet.ownerTransitions) {
      if (
        !standing.owners.some((row) => canonical(row) === canonical(transition.from)) ||
        !packet.lens.owners.some((row) => canonical(row) === canonical(transition.to))
      )
        hold("OWNER_TRANSITION_SCOPE_MISMATCH");
    }
    for (const standingOwner of standing.owners) {
      if (
        !packet.lens.owners.some((row) => canonical(row) === canonical(standingOwner)) &&
        packet.ownerTransitions.filter((row) => canonical(row.from) === canonical(standingOwner))
          .length !== 1
      )
        hold("STANDING_OWNER_NOT_PRESERVED");
    }
    if (
      !standing.owners.some(
        (row) => row.id === receipt.admission.ownerId && ["source", "native"].includes(row.role),
      ) &&
      !packet.ownerTransitions.some(
        (row) =>
          row.to.id === receipt.admission.ownerId && ["source", "native"].includes(row.to.role),
      )
    )
      hold("ADMISSION_OWNER_TRANSITION_REQUIRED");
    if (
      route &&
      ["followup", "resume", "refresh", "post-compact"].includes(route.kind) &&
      (!receipt.authorityReloaded || receipt.transitionRefs.length === 0)
    )
      hold("RECOVERY_AUTHORITY_RELOAD_REQUIRED");
    if (packet.lens.dependencies.some((row) => row.currentness !== "current"))
      hold("DEPENDENCY_NOT_CURRENT");
    if (packet.phase === "general") {
      const release = input.release;
      if (!release) hold("GENERAL_WORK_RELEASE_REQUIRED");
      else {
        object(release);
        source(release.source);
        object(release.coverage);
        hash(release.coverage.epochSha256);
        hash(release.coverage.observationsSha256);
        hash(release.coverage.sourceObservationsSha256);
        if (
          release.schema !== "common.agent-work-method.release.v1" ||
          release.epochId !== packet.epochId ||
          release.coverage.epochId !== packet.epochId ||
          release.coverage.status !== "owner-eligible" ||
          release.coverage.reasons.length !== 0 ||
          release.coverage.heldRowIds.length !== 0 ||
          release.coverage.arbitraryAgentCoverage !== false ||
          release.coverage.nativePermissionGranted !== false ||
          release.bootstrapClosed !== true ||
          release.currentness !== "current" ||
          !packet.lens.owners.some(
            (row) => row.id === release.ownerId && ["source", "native"].includes(row.role),
          )
        )
          hold("GENERAL_WORK_RELEASE_NOT_CURRENT");
      }
    }
    sourceObservations(input.sources);
    for (const required of workMethodRequiredSources(
      packet,
      standing,
      receipt,
      packet.phase === "general" ? input.release : null,
    )) {
      if (!currentSource(required, input.sources)) hold("REQUIRED_SOURCE_NOT_CURRENT");
    }
    result.sourceObservationsSha256 = digest(canonical(input.sources));
    list(input.prerequisites);
    for (const required of packet.prerequisites) {
      const matches = input.prerequisites.filter(
        (row) =>
          canonical({
            producer: row.producer,
            receiver: row.receiver,
            milestone: row.milestone,
            producerReceipt: row.producerReceipt,
            receiverReceipt: row.receiverReceipt,
          }) === canonical(required),
      );
      if (
        matches.length !== 1 ||
        matches[0]!.currentness !== "current" ||
        matches[0]!.producerSatisfied !== true ||
        matches[0]!.receiverUsed !== true
      )
        hold("QUALIFIED_PREREQUISITE_NOT_CONSUMED");
    }
    list(input.concurrent);
    for (const binding of input.concurrent) {
      text(binding.actorId);
      hash(binding.packetSha256);
      logical(binding.logical);
      object(binding.source);
      requireValue(
        ["write", "read-only"].includes(String(binding.source.access)),
        "INVALID_SOURCE_ACCESS",
      );
      texts(binding.source.ownedPaths);
      requireValue(
        binding.source.access !== "read-only" || binding.source.ownedPaths.length === 0,
        "READ_ONLY_OWNED_PATHS",
      );
      text(binding.source.repositoryId);
      text(binding.source.branch);
      text(binding.source.worktree);
      const samePacket =
        binding.packetId === packet.packetId &&
        binding.packetSha256 === packetHash &&
        canonical(binding.logical) === canonical(packet.logical);
      const sameActor = binding.actorId === receipt.actor.actorId;
      const sameSource = canonical(binding.source) === canonical(packet.source);
      if (sameActor && (!samePacket || !sameSource)) hold("CONCURRENT_ACTOR_PACKET_CONFLICT");
      if (
        !(sameActor && samePacket && sameSource) &&
        binding.source.repositoryId === packet.source.repositoryId &&
        binding.source.access === "write" &&
        packet.source.access === "write" &&
        (binding.source.worktree === packet.source.worktree ||
          binding.source.branch === packet.source.branch)
      ) {
        hold("CONCURRENT_SOURCE_ISOLATION_REQUIRED");
        if (pathsOverlap(binding.source.ownedPaths, packet.source.ownedPaths))
          hold("CONCURRENT_SOURCE_PATH_CONFLICT");
      }
    }
    const advice = input.advice;
    object(advice);
    text(advice.reason);
    requireValue(advice.status === "held" || advice.status === "consumed", "INVALID_ADVICE_STATUS");
    source(advice.budgetSource);
    requireValue(
      advice.cumulativeLimitUsd === null ||
        (typeof advice.cumulativeLimitUsd === "number" &&
          Number.isFinite(advice.cumulativeLimitUsd) &&
          advice.cumulativeLimitUsd >= 0),
      "INVALID_CUMULATIVE_LIMIT",
    );
    requireValue(
      advice.cumulativeUsageUsd === null ||
        (typeof advice.cumulativeUsageUsd === "number" &&
          Number.isFinite(advice.cumulativeUsageUsd) &&
          advice.cumulativeUsageUsd >= 0),
      "INVALID_CUMULATIVE_USAGE",
    );
    sources(advice.evidence);
    const event = input.currentEvent;
    if (event) {
      source(event.source);
      nullableText(event.eventId);
      text(event.actorId);
      hash(event.packetSha256);
      hash(event.taskSha256);
      nativeIds(event.native);
    }
    const adviceReady =
      advice.status === "consumed" &&
      event !== null &&
      event !== undefined &&
      event.actorId === receipt.actor.actorId &&
      canonical(event.native) === canonical(receipt.actor.native) &&
      event.packetSha256 === packetHash &&
      event.taskSha256 === packet.task.sha256 &&
      event.eventId === advice.eventId &&
      sourceHash(event.source) === advice.sourceSha256 &&
      advice.actorId === receipt.actor.actorId &&
      typeof advice.eventId === "string" &&
      advice.eventId.trim().length > 0 &&
      advice.packetSha256 === packetHash &&
      advice.taskSha256 === packet.task.sha256 &&
      workMethodAdviceRequiredSources(advice, event).every((required) =>
        currentSource(required, input.sources),
      ) &&
      advice.cumulativeUsageUsd !== null &&
      advice.cumulativeLimitUsd !== null &&
      canonical(advice.budgetSource) === canonical(packet.fences.budget) &&
      advice.cumulativeUsageUsd <= advice.cumulativeLimitUsd &&
      advice.delivered === true &&
      advice.consumed === true &&
      advice.evidence.length > 0;
    if (adviceReady) result.adviceMode = "consumed";
    if (!adviceReady)
      result.adviceHeldReasons.push(
        advice.status === "held" ? advice.reason : "ADVICE_CONSUMPTION_BINDING_INVALID",
      );
    if (input.requiresJudgment && !adviceReady) hold("REQUIRED_JUDGMENT_HELD");
    if (result.reasons.length === 0) result.status = "owner-eligible";
  } catch (error) {
    hold(error instanceof Error ? error.message : "INVALID_INPUT");
  }
  result.reasons = [...new Set(result.reasons)];
  return result;
}

/** Finite owner-attested initial denominator. Later drift is evaluated on affected rows. */
export function evaluateWorkMethodCoverage(
  epoch: WorkMethodCoverageEpoch,
  observations: { receipt: WorkMethodReceipt; admission: WorkMethodAdmission }[],
  sourceEvidence: WorkMethodSourceObservation[],
): WorkMethodCoverage {
  const reasons: string[] = [],
    heldRowIds: string[] = [];
  let epochId: string | null = null,
    epochSha256: string | null = null,
    observationsSha256: string | null = null,
    sourceObservationsSha256: string | null = null;
  try {
    object(epoch);
    requireValue(
      epoch.schema === "common.agent-work-method.coverage.v1",
      "INVALID_COVERAGE_SCHEMA",
    );
    text(epoch.epochId);
    epochId = epoch.epochId;
    text(epoch.ownerId);
    source(epoch.attestation);
    sourceObservations(sourceEvidence);
    if (!currentSource(epoch.attestation, sourceEvidence))
      reasons.push("COVERAGE_ATTESTATION_NOT_CURRENT");
    if (!epoch.closedAt || !Number.isFinite(Date.parse(epoch.closedAt)))
      reasons.push("COVERAGE_EPOCH_OPEN");
    if (epoch.changedBeforeRelease !== false) reasons.push("COVERAGE_CHANGED_BEFORE_RELEASE");
    list(epoch.rows);
    requireValue(epoch.rows.length > 0, "EMPTY_COVERAGE_EPOCH");
    requireValue(
      new Set(epoch.rows.map((row) => row.id)).size === epoch.rows.length,
      "DUPLICATE_COVERAGE_ROW",
    );
    list(observations);
    for (const row of epoch.rows) {
      text(row.id);
      text(row.environmentId);
      text(row.ownerId);
      text(row.routeId);
      nullableText(row.actorId);
      bool(row.required);
      pin(row.protocol);
      pin(row.adapter);
      sources(row.classificationEvidence);
      requireValue(
        ["active", "dormant", "non-agent", "unknown"].includes(row.state),
        "INVALID_COVERAGE_STATE",
      );
      let held = row.classificationEvidence.some(
        (required) => !currentSource(required, sourceEvidence),
      );
      if (!row.required && row.classificationEvidence.length === 0) held = true;
      if (row.state === "unknown") held ||= row.required;
      if (row.state === "non-agent") held ||= row.classificationEvidence.length === 0;
      if (row.state === "dormant") {
        held ||=
          row.classificationEvidence.length === 0 ||
          !row.wake ||
          row.wake.mechanism === "instruction-advisory" ||
          !mechanisms.includes(row.wake.mechanism) ||
          !row.wake.holdBeforeGeneralEffects;
        if (row.wake) {
          sources(row.wake.evidence, true);
          held ||= row.wake.evidence.some((required) => !currentSource(required, sourceEvidence));
        }
      }
      if (row.state === "active" && row.required) {
        const matches = observations.filter(
          ({ receipt, admission }) =>
            receipt.id === row.receiptId &&
            admission.receiptId === receipt.id &&
            admission.receiptSha256 === workMethodReceiptSha256(receipt) &&
            admission.status === "owner-eligible" &&
            admission.epochId === epoch.epochId &&
            admission.actorId === row.actorId &&
            admission.routeId === row.routeId &&
            admission.packetSha256 === row.packetSha256 &&
            admission.mechanism === receipt.admission.mechanism &&
            receipt.actor.actorId === row.actorId &&
            receipt.actor.environmentId === row.environmentId &&
            receipt.routeId === row.routeId &&
            receipt.packetSha256 === row.packetSha256 &&
            receipt.admission.ownerId === row.ownerId &&
            receipt.currentness === "current" &&
            receipt.received &&
            receipt.used &&
            receipt.admission.consumedBeforeEffects &&
            receipt.admission.mechanism !== "instruction-advisory" &&
            canonical(receipt.protocol) === canonical(row.protocol) &&
            canonical(receipt.adapter) === canonical(row.adapter),
        );
        held ||= matches.length !== 1;
        if (matches.length === 1) {
          const receipt = matches[0]!.receipt;
          assertReceipt(receipt);
          held ||= receiptEvidenceSources(receipt).some(
            (required) => !currentSource(required, sourceEvidence),
          );
        }
      }
      if (held) heldRowIds.push(row.id);
    }
    epochSha256 = digest(canonical(epoch));
    observationsSha256 = digest(canonical(observations));
    sourceObservationsSha256 = digest(canonical(sourceEvidence));
  } catch (error) {
    reasons.push(error instanceof Error ? error.message : "INVALID_INPUT");
  }
  if (heldRowIds.length > 0) reasons.push("REQUIRED_COVERAGE_NOT_READY");
  return {
    status: reasons.length ? "held" : "owner-eligible",
    epochId,
    epochSha256,
    observationsSha256,
    sourceObservationsSha256,
    reasons,
    heldRowIds,
    arbitraryAgentCoverage: false,
    nativePermissionGranted: false,
  };
}
