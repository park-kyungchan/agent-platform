import { randomUUID } from "node:crypto";
import { createHash } from "node:crypto";
import { dirname, isAbsolute, resolve } from "node:path";
import { mkdir, open, readFile, realpath, rename, rm, lstat } from "node:fs/promises";

export const qualificationCaps = {
  maxRequests: 48,
  maxTokens: 2_000_000,
  maxDurationMs: 8 * 60 * 60 * 1000,
} as const;
export interface EvaluationCaps {
  maxRequests: number;
  maxTokens: number;
  maxDurationMs: number;
}
export interface EvaluationBudgetAuthorization {
  schema: "codex.evaluation-budget-authorization.v1";
  suiteId: string;
  projectId: string;
  caps: EvaluationCaps;
  envelope: { path: string; sha256: string };
  authority: { path: string; sha256: string };
}
export type EvaluationRequestKind = "turn" | "child" | "manual-compaction" | "observed-retry";
export interface TokenUsage {
  totalTokens: number;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  cacheWriteInputTokens?: number;
}
export interface UsageObservation {
  threadId: string;
  turnId: string;
  atUtc: string;
  cumulative: TokenUsage;
  evidenceRef: string;
  /** A parent native total cannot be added to child totals until this relation is evidenced. */
  childAggregation: "excludes-children" | "includes-children" | "unknown";
  includedChildren?: { threadId: string; cumulativeTokens: number }[];
}
export interface EvaluationRequest {
  id: string;
  kind: EvaluationRequestKind;
  reservationTokens: number;
  reservedAtUtc?: string;
  admissionClass?: "reserved-before-dispatch" | "observed-unreserved-effect";
  discoveredAtUtc?: string;
  observedEffectEvidenceRef?: string;
  plannedNativeChild?: true;
  plannedNativeChildTargetThreadId?: string;
  plannedNativeChildTargetPath?: string;
  nativeExecution?: "app-server-direct-child-followup";
  status: "reserved" | "dispatched" | "reconciled" | "cancelled" | "usage-unknown";
  parentRequestId?: string;
  threadId?: string;
  turnId?: string;
  dispatchedAtUtc?: string;
  reconciledAtUtc?: string;
  evidenceRef?: string;
  outcome?: "completed" | "interrupted" | "failed" | "rejected-before-execution";
  rejectedAtUtc?: string;
  rejectionRecordedAtUtc?: string;
  rejection?: NativeRequestRejectionResolution;
  nativeToolUseId?: string;
  hookInputSha256?: string;
  nativeParentThreadId?: string;
  dispatchEvidenceRef?: string;
  reconciliationEvidenceRef?: string;
}
export interface ThreadUsage {
  threadId: string;
  parentThreadId?: string;
  baseline: TokenUsage;
  last: UsageObservation | null;
  status: "known" | "unknown";
}
export interface NonDispatchResolution {
  class: "verified-non-dispatch";
  review: { path: string; sha256: string };
  protocol: { path: string; sha256: string };
  ledgerBeforeSha256: string;
}
export interface CompletedReadbackResolution {
  class: "verified-completed-native-readback";
  review: { path: string; sha256: string };
  ledgerBeforeSha256: string;
}
export interface CompletedReadbackReview {
  schema: "codex.evaluation.completed-readback-review.v1";
  status: "passed-scoped";
  verificationClass:
    "idle-only-readback-false-negative" | "reserved-to-reconciled-bookkeeping-false-negative";
  reviewerThreadId: string;
  suiteId: string;
  projectId: string;
  projectRoot: string;
  ledger: { path: string; sha256: string };
  requestId: string;
  unknownId: string;
  errorEvidence: { path: string; sha256: string };
  readback: { path: string; sha256: string };
  rollout: { path: string; sha256: string };
  snapshot?: { path: string; sha256: string };
  usageEvidence?: { path: string; sha256: string };
  native: {
    threadId: string;
    parentThreadId: string;
    sessionId: string;
    turnId: string;
    rootTurnId: string;
    cwd: string;
    model: "gpt-6.1-sol";
    effort: "high";
  };
  ownResponseCount: number;
  ownUsage: TokenUsage;
}
export interface OwnedNativeBoundaryResolution {
  class: "verified-owned-native-boundary";
  review: { path: string; sha256: string };
  protocol: { path: string; sha256: string };
  responseHistory: { path: string; sha256: string };
  readback: { path: string; sha256: string };
  ledgerBeforeSha256: string;
}
export interface NativeRequestRejectionResolution {
  class: "verified-native-request-rejection";
  requestId: string;
  review: { path: string; sha256: string };
  protocol: { path: string; sha256: string };
  responseHistory: { path: string; sha256: string };
  ledgerBeforeSha256: string;
}
export interface NativeRequestRejectionReview {
  schema: "codex.qualification.native-request-rejection-review.v1";
  status: "passed-scoped";
  verificationClass: "exact-owned-native-request-rejection";
  reviewer: string;
  caseId: string;
  workId: string;
  suiteId: string;
  ledger: {
    path: string;
    sha256: string;
    startedAtUtc: string;
    caps: EvaluationCaps;
    aggregateTokens: number;
    requestCount: number;
  };
  case: { path: string; sha256: string; checkpointPath: string; checkpointSha256: string };
  requestId: string;
  nativeRpcRequestId: number;
  nativeError: { code: -32600; message: string };
  protocol: { path: string; sha256: string };
  responseHistory: { path: string; sha256: string };
  priorResponseHistory: { path: string; sha256: string };
  ownedChildRollout: { path: string; sha256: string };
  sourcePins: { path: string; sha256: string }[];
  reconciliation: {
    allowedUnknowns: { id: string; reason: string; evidenceRef: string }[];
    physicalUnknownDeletionAllowed: false;
    suiteReplacementAllowed: false;
    closedSuiteReopenAllowed: false;
    countedAttemptRemovalAllowed: false;
    emittedCostChangeAllowed: false;
  };
  actualQualificationVerified: false;
}
export interface OwnedNativeBoundaryReview {
  schema: "codex.qualification.owned-native-boundary-review.v1";
  status: "passed-scoped";
  verificationClass: "exact-owned-native-boundary";
  reviewer: string;
  caseId: string;
  workId: string;
  suiteId: string;
  ledger: { path: string; sha256: string; startedAtUtc: string; caps: EvaluationCaps };
  case: { path: string; sha256: string; checkpointPath: string; checkpointSha256: string };
  protocol: { path: string; sha256: string };
  responseHistory: { path: string; sha256: string };
  readback: { path: string; sha256: string };
  sourcePins: { path: string; sha256: string }[];
  reconciliation: {
    allowedUnknowns: {
      id: string;
      reason: string;
      evidenceRef: string;
      observedEffectRequestId?: string;
    }[];
    physicalUnknownDeletionAllowed: false;
    suiteReplacementAllowed: false;
    closedSuiteReopenAllowed: false;
    requestsOrTokensRecyclingAllowed: false;
  };
  actualQualificationVerified: false;
}
export interface EvaluationLedger {
  schemaVersion: "1.0.0";
  suiteId: string;
  projectId: string;
  status: "active" | "closed";
  revision: number;
  caps: EvaluationCaps;
  budgetAuthorization?: EvaluationBudgetAuthorization;
  startedAtUtc: string;
  closedAtUtc?: string;
  requests: EvaluationRequest[];
  threads: ThreadUsage[];
  observations: UsageObservation[];
  unknowns: {
    id: string;
    reason: string;
    evidenceRef: string;
    resolvedAtUtc?: string;
    resolution?:
      | NonDispatchResolution
      | OwnedNativeBoundaryResolution
      | NativeRequestRejectionResolution
      | CompletedReadbackResolution;
  }[];
  nativeOwner: "codex";
  isScheduler: false;
  isProviderHardCap: false;
}
const zero = (): TokenUsage => ({
  totalTokens: 0,
  inputTokens: 0,
  cachedInputTokens: 0,
  outputTokens: 0,
  reasoningOutputTokens: 0,
  cacheWriteInputTokens: 0,
});
const clone = (ledger: EvaluationLedger) => structuredClone(ledger);
const time = (value: string) => {
  const result = Date.parse(value);
  if (!Number.isFinite(result)) throw new Error("Invalid ledger time");
  return result;
};
const natural = (value: number) => Number.isSafeInteger(value) && value >= 0;
const sameCaps = (a: EvaluationCaps, b: EvaluationCaps) =>
  a.maxRequests === b.maxRequests &&
  a.maxTokens === b.maxTokens &&
  a.maxDurationMs === b.maxDurationMs;
function validateBudget(
  ledger: Pick<EvaluationLedger, "caps" | "suiteId" | "projectId" | "budgetAuthorization">,
): void {
  const caps = ledger.caps,
    budget = ledger.budgetAuthorization;
  if (
    !caps ||
    ![caps.maxRequests, caps.maxTokens, caps.maxDurationMs].every(
      (value) => natural(value) && value > 0,
    )
  )
    throw new Error("Invalid native suite caps");
  if (!budget) {
    if (!sameCaps(caps, qualificationCaps))
      throw new Error("Legacy suite caps differ from the explicit accepted budget");
    return;
  }
  if (
    budget.schema !== "codex.evaluation-budget-authorization.v1" ||
    budget.suiteId !== ledger.suiteId ||
    budget.projectId !== ledger.projectId ||
    !budget.caps ||
    !sameCaps(caps, budget.caps)
  )
    throw new Error("Explicit suite budget identity or caps mismatch");
  for (const pin of [budget.envelope, budget.authority])
    if (
      !pin ||
      typeof pin.path !== "string" ||
      !isAbsolute(pin.path) ||
      typeof pin.sha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(pin.sha256)
    )
      throw new Error("Explicit suite budget requires exact envelope and authority pins");
}
async function verifyBudget(ledger: EvaluationLedger): Promise<void> {
  validateBudget(ledger);
  const budget = ledger.budgetAuthorization;
  if (!budget) return;
  const envelope = JSON.parse(
    (await pinnedNonDispatchBytes(budget.envelope, 1_048_576)).toString("utf8"),
  );
  if (
    envelope.schema !== "codex.workspace-native-suite-envelope.v1" ||
    envelope.suiteId !== ledger.suiteId ||
    envelope.projectId !== ledger.projectId ||
    !envelope.caps ||
    !sameCaps(ledger.caps, envelope.caps) ||
    typeof envelope.authorityRef !== "string" ||
    !envelope.authorityRef.trim()
  )
    throw new Error("Pinned native suite envelope identity or caps mismatch");
  // This verifies source custody only. Actual user authority is established by the owner,
  // never inferred from a generated envelope, pin or successful tool call.
  const authority = await pinnedNonDispatchBytes(budget.authority, 2_097_152);
  if (!authority.toString("utf8").trim()) throw new Error("Pinned suite authority source is empty");
}
function validateUsage(usage: TokenUsage): void {
  for (const value of Object.values(usage))
    if (!natural(value)) throw new Error("Native usage must contain nonnegative safe integers");
  if (usage.totalTokens !== usage.inputTokens + usage.outputTokens)
    throw new Error("Native total must equal input plus output; cached and reasoning are subsets");
  if (
    usage.cachedInputTokens > usage.inputTokens ||
    usage.reasoningOutputTokens > usage.outputTokens ||
    (usage.cacheWriteInputTokens ?? 0) > usage.inputTokens
  )
    throw new Error("Native usage subset exceeds its containing token count");
}
export function createEvaluationLedger(options: {
  suiteId: string;
  projectId: string;
  now?: Date;
}): EvaluationLedger {
  if (!options.suiteId || !options.projectId)
    throw new Error("A new explicitly authorized suite identity is required");
  return {
    schemaVersion: "1.0.0",
    suiteId: options.suiteId,
    projectId: options.projectId,
    status: "active",
    revision: 0,
    caps: qualificationCaps,
    startedAtUtc: (options.now ?? new Date()).toISOString(),
    requests: [],
    threads: [],
    observations: [],
    unknowns: [],
    nativeOwner: "codex",
    isScheduler: false,
    isProviderHardCap: false,
  };
}
/** Create a separate suite from an owner-selected, source-pinned budget; never renew a legacy suite. */
export async function createAuthorizedEvaluationLedger(options: {
  suiteId: string;
  projectId: string;
  caps: EvaluationCaps;
  envelopePin: { path: string; sha256: string };
  authorityPin: { path: string; sha256: string };
  now?: Date;
}): Promise<EvaluationLedger> {
  const ledger = createEvaluationLedger(options);
  ledger.caps = structuredClone(options.caps);
  ledger.budgetAuthorization = {
    schema: "codex.evaluation-budget-authorization.v1",
    suiteId: options.suiteId,
    projectId: options.projectId,
    caps: structuredClone(options.caps),
    envelope: structuredClone(options.envelopePin),
    authority: structuredClone(options.authorityPin),
  };
  await verifyBudget(ledger);
  return ledger;
}
export function aggregateTokens(ledger: EvaluationLedger): number {
  return ledger.threads.reduce((sum, thread) => {
    if (!thread.last) return sum;
    const inclusive = thread.last.cumulative.totalTokens - thread.baseline.totalTokens;
    const children =
      thread.last.childAggregation === "includes-children"
        ? (thread.last.includedChildren ?? []).reduce(
            (total, child) => total + child.cumulativeTokens,
            0,
          )
        : 0;
    return sum + Math.max(0, inclusive - children);
  }, 0);
}
export function ledgerSummary(ledger: EvaluationLedger) {
  // admittedRequests is the retained cap-accounting count, including discovered effects; it does not certify prior admission.
  return {
    admittedRequests: ledger.requests.length,
    preReservedRequests: ledger.requests.filter(
      (request) =>
        request.admissionClass !== "observed-unreserved-effect" &&
        request.kind !== "observed-retry",
    ).length,
    observedUnreservedRequests: ledger.requests.filter(
      (request) => request.admissionClass === "observed-unreserved-effect",
    ).length,
    rpcAttemptedRequests: ledger.requests.filter((request) => request.dispatchedAtUtc).length,
    rejectedBeforeExecutionRequests: ledger.requests.filter(
      (request) => request.rejection?.class === "verified-native-request-rejection",
    ).length,
    dispatchedRequests: ledger.requests.filter(
      (request) => request.dispatchedAtUtc && !request.rejection,
    ).length,
    aggregateTokens: aggregateTokens(ledger),
    reservedTokens: ledger.requests
      .filter((request) => ["reserved", "dispatched", "usage-unknown"].includes(request.status))
      .reduce((sum, request) => sum + request.reservationTokens, 0),
    pendingRequests: ledger.requests.filter((request) =>
      ["reserved", "dispatched", "usage-unknown"].includes(request.status),
    ).length,
    unknowns:
      ledger.unknowns.filter((unknown) => !unknown.resolvedAtUtc).length +
      ledger.threads.filter((thread) => thread.status === "unknown").length,
  };
}
export function dispatchGuard(
  ledger: EvaluationLedger,
  options: { now?: Date; requestedTokens?: number; nativeHooksTrusted?: boolean } = {},
): { allowed: boolean; reasons: string[] } {
  const now = options.now ?? new Date();
  const summary = ledgerSummary(ledger);
  const reasons: string[] = [];
  if (ledger.status !== "active") reasons.push("Suite is closed and cannot be reopened");
  try {
    validateBudget(ledger);
  } catch (error) {
    reasons.push(String(error));
  }
  if (
    now.getTime() < time(ledger.startedAtUtc) ||
    now.getTime() - time(ledger.startedAtUtc) >= ledger.caps.maxDurationMs
  )
    reasons.push("Suite duration limit or invalid clock");
  if (summary.admittedRequests >= ledger.caps.maxRequests)
    reasons.push("Native request limit reached");
  const tokens = options.requestedTokens ?? 0;
  if (!natural(tokens)) reasons.push("Invalid token reservation");
  if (
    summary.aggregateTokens + summary.reservedTokens + tokens > ledger.caps.maxTokens ||
    summary.aggregateTokens >= ledger.caps.maxTokens
  )
    reasons.push("Aggregate emitted plus reserved token limit reached");
  if (summary.unknowns) reasons.push("Usage or child aggregation is unresolved");
  if (options.nativeHooksTrusted === false)
    reasons.push("Native hooks are not behaviorally ready for qualification dispatch");
  return { allowed: reasons.length === 0, reasons };
}
export function reserveEvaluationRequest(
  ledger: EvaluationLedger,
  options: {
    kind: EvaluationRequestKind;
    reservationTokens: number;
    now?: Date;
    parentRequestId?: string;
    nativeHooksTrusted: boolean;
    id?: string;
  },
): { ledger: EvaluationLedger; request: EvaluationRequest } {
  const now = options.now ?? new Date();
  const guard = dispatchGuard(ledger, {
    now,
    requestedTokens: options.reservationTokens,
    nativeHooksTrusted: options.nativeHooksTrusted,
  });
  if (!guard.allowed) throw new Error(guard.reasons.join("; "));
  if (
    options.parentRequestId &&
    !ledger.requests.some((request) => request.id === options.parentRequestId)
  )
    throw new Error("Unknown parent native request");
  const result = clone(ledger);
  const id = options.id ?? randomUUID();
  if (result.requests.some((request) => request.id === id))
    throw new Error("Duplicate native request reservation");
  const request: EvaluationRequest = {
    id,
    kind: options.kind,
    reservationTokens: options.reservationTokens,
    reservedAtUtc: now.toISOString(),
    admissionClass: "reserved-before-dispatch",
    status: "reserved",
    ...(options.parentRequestId ? { parentRequestId: options.parentRequestId } : {}),
  };
  result.requests.push(request);
  return { ledger: result, request };
}
/** A finite controller may reserve its single planned native child before its parent invokes the tool. */
export function reservePlannedNativeChild(
  ledger: EvaluationLedger,
  options: {
    parentRequestId: string;
    nativeParentThreadId: string;
    reservationTokens: number;
    now?: Date;
    nativeHooksTrusted: boolean;
    id?: string;
    plannedNativeChildTargetThreadId?: string;
    plannedNativeChildTargetPath?: string;
  },
): { ledger: EvaluationLedger; request: EvaluationRequest } {
  const parent = ledger.requests.find((request) => request.id === options.parentRequestId);
  if (
    !parent?.dispatchedAtUtc ||
    parent.status !== "dispatched" ||
    parent.threadId !== options.nativeParentThreadId
  )
    throw new Error("Planned child requires its exact active admitted parent");
  if (
    ledger.requests.some(
      (request) =>
        request.plannedNativeChild &&
        request.parentRequestId === parent.id &&
        request.status === "reserved",
    )
  )
    throw new Error("Planned native child reservation already exists");
  if (
    (options.plannedNativeChildTargetThreadId === undefined) !==
      (options.plannedNativeChildTargetPath === undefined) ||
    (options.plannedNativeChildTargetThreadId !== undefined &&
      (!options.plannedNativeChildTargetPath?.startsWith("/") ||
        !ledger.threads.some(
          (thread) =>
            thread.threadId === options.plannedNativeChildTargetThreadId &&
            thread.parentThreadId === parent.threadId,
        )))
  )
    throw new Error(
      "Planned existing child requires its exact registered lineage and native agent path",
    );
  const result = reserveEvaluationRequest(ledger, { ...options, kind: "child" });
  Object.assign(result.request, {
    plannedNativeChild: true,
    nativeParentThreadId: options.nativeParentThreadId,
  });
  if (options.plannedNativeChildTargetThreadId)
    Object.assign(result.request, {
      plannedNativeChildTargetThreadId: options.plannedNativeChildTargetThreadId,
      plannedNativeChildTargetPath: options.plannedNativeChildTargetPath,
    });
  return result;
}
/** Correlate one actual tool invocation with a prior reservation; this makes no native call. */
export function attachPlannedNativeChildTool(
  ledger: EvaluationLedger,
  options: {
    requestId: string;
    nativeToolUseId: string;
    hookInputSha256?: string;
    evidenceRef: string;
  },
): EvaluationLedger {
  const result = clone(ledger),
    request = result.requests.find((row) => row.id === options.requestId);
  if (
    !request?.plannedNativeChild ||
    request.kind !== "child" ||
    request.status !== "reserved" ||
    !request.reservedAtUtc ||
    !options.nativeToolUseId ||
    !options.evidenceRef ||
    (options.hookInputSha256 !== undefined && !/^[a-f0-9]{64}$/.test(options.hookInputSha256))
  )
    throw new Error("Exact unused planned child reservation and invocation evidence required");
  if (request.nativeToolUseId && request.nativeToolUseId !== options.nativeToolUseId)
    throw new Error("Planned child is already attached to a different native tool");
  if (
    request.hookInputSha256 &&
    options.hookInputSha256 !== undefined &&
    request.hookInputSha256 !== options.hookInputSha256
  )
    throw new Error("Planned child arguments changed after attachment");
  if (
    result.requests.some(
      (row) => row.id !== request.id && row.nativeToolUseId === options.nativeToolUseId,
    )
  )
    throw new Error("Native tool invocation is already counted by another reservation");
  request.nativeToolUseId = options.nativeToolUseId;
  if (options.hookInputSha256) request.hookInputSha256 = options.hookInputSha256;
  if (!request.evidenceRef) request.evidenceRef = options.evidenceRef;
  return result;
}
export function markEvaluationDispatched(
  ledger: EvaluationLedger,
  requestId: string,
  options: { now?: Date; threadId: string; turnId?: string; evidenceRef: string },
): EvaluationLedger {
  validateBudget(ledger);
  const result = clone(ledger);
  const request = result.requests.find((request) => request.id === requestId);
  if (!request || request.status !== "reserved")
    throw new Error("Native dispatch requires an unused reservation");
  if (!options.threadId || !options.evidenceRef)
    throw new Error("Native dispatch identity and evidence are required");
  if (
    request.plannedNativeChildTargetThreadId &&
    request.plannedNativeChildTargetThreadId !== options.threadId
  )
    throw new Error("Actual child dispatch differs from its exact planned existing native child");
  const summary = ledgerSummary(result);
  if (
    result.status !== "active" ||
    (options.now ?? new Date()).getTime() - time(result.startedAtUtc) >=
      result.caps.maxDurationMs ||
    result.unknowns.some((unknown) => !unknown.resolvedAtUtc) ||
    result.threads.some((thread) => thread.status === "unknown") ||
    summary.aggregateTokens >= result.caps.maxTokens ||
    summary.aggregateTokens + summary.reservedTokens > result.caps.maxTokens ||
    result.requests.length > result.caps.maxRequests
  )
    throw new Error("Reservation cannot dispatch after a limit or unknown usage");
  Object.assign(request, {
    status: "dispatched",
    dispatchedAtUtc: (options.now ?? new Date()).toISOString(),
    threadId: options.threadId,
    ...(options.turnId ? { turnId: options.turnId } : {}),
    evidenceRef: options.evidenceRef,
    dispatchEvidenceRef: options.evidenceRef,
  });
  return result;
}
export function registerEvaluationThread(
  ledger: EvaluationLedger,
  options: { threadId: string; parentThreadId?: string; baseline?: TokenUsage },
): EvaluationLedger {
  const result = clone(ledger);
  if (result.threads.some((thread) => thread.threadId === options.threadId))
    throw new Error("Native thread already registered");
  if (
    options.parentThreadId &&
    !result.threads.some((thread) => thread.threadId === options.parentThreadId)
  )
    throw new Error("Unknown native parent thread");
  const baseline = options.baseline ?? zero();
  validateUsage(baseline);
  result.threads.push({
    threadId: options.threadId,
    ...(options.parentThreadId ? { parentThreadId: options.parentThreadId } : {}),
    baseline,
    last: null,
    status: "known",
  });
  return result;
}
export function observeEvaluationUsage(
  ledger: EvaluationLedger,
  observation: UsageObservation,
): EvaluationLedger {
  const result = clone(ledger);
  const thread = result.threads.find((thread) => thread.threadId === observation.threadId);
  if (!thread) throw new Error("Usage belongs to an unregistered suite thread");
  validateUsage(observation.cumulative);
  time(observation.atUtc);
  if (!observation.turnId || !observation.evidenceRef)
    throw new Error("Native usage identity and provenance are required");
  const previous = thread.last?.cumulative ?? thread.baseline;
  if (
    observation.cumulative.totalTokens < previous.totalTokens ||
    observation.cumulative.inputTokens < previous.inputTokens ||
    observation.cumulative.outputTokens < previous.outputTokens
  )
    throw new Error("Cumulative native usage moved backwards");
  if (thread.last && time(observation.atUtc) < time(thread.last.atUtc))
    throw new Error("Stale native usage observation");
  let known = observation.childAggregation !== "unknown";
  const included = observation.includedChildren ?? [];
  if (observation.childAggregation === "includes-children") {
    if (!observation.includedChildren) known = false;
    const seen = new Set<string>();
    for (const child of included) {
      const childThread = result.threads.find(
        (candidate) =>
          candidate.threadId === child.threadId && candidate.parentThreadId === thread.threadId,
      );
      if (!childThread || !natural(child.cumulativeTokens) || seen.has(child.threadId))
        throw new Error("Invalid or duplicate included child usage");
      if (
        child.cumulativeTokens >
        (childThread.last?.cumulative.totalTokens ?? childThread.baseline.totalTokens) -
          childThread.baseline.totalTokens
      )
        known = false;
      seen.add(child.threadId);
    }
    const nativeDelta = observation.cumulative.totalTokens - thread.baseline.totalTokens;
    if (included.reduce((sum, child) => sum + child.cumulativeTokens, 0) > nativeDelta)
      throw new Error("Included child usage exceeds parent native usage");
    if (
      result.threads.some(
        (child) => child.parentThreadId === thread.threadId && !seen.has(child.threadId),
      )
    )
      known = false;
  } else if (included.length)
    throw new Error("Included child totals contradict the native aggregation relation");
  thread.last = structuredClone(observation);
  thread.status = known ? "known" : "unknown";
  result.observations.push(structuredClone(observation));
  return result;
}
export function reconcileEvaluationRequest(
  ledger: EvaluationLedger,
  requestId: string,
  options: { now?: Date; evidenceRef: string; status: "completed" | "interrupted" | "failed" },
): EvaluationLedger {
  const result = clone(ledger);
  const request = result.requests.find((request) => request.id === requestId);
  if (!request || request.status !== "dispatched")
    throw new Error("Only an actual native dispatch can be reconciled");
  const thread = result.threads.find((thread) => thread.threadId === request.threadId);
  if (
    !thread?.last ||
    thread.status !== "known" ||
    (request.turnId && thread.last.turnId !== request.turnId) ||
    time(thread.last.atUtc) < time(request.dispatchedAtUtc!)
  ) {
    request.status = "usage-unknown";
    request.outcome = options.status;
    result.unknowns.push({
      id: requestId,
      reason: "Missing emitted cumulative usage for the dispatched native request",
      evidenceRef: options.evidenceRef,
    });
    return result;
  }
  Object.assign(request, {
    status: "reconciled",
    reconciledAtUtc: (options.now ?? new Date()).toISOString(),
    evidenceRef: options.evidenceRef,
    reconciliationEvidenceRef: options.evidenceRef,
    outcome: options.status,
  });
  return result;
}
export function resolveUnknownRequest(
  ledger: EvaluationLedger,
  requestId: string,
  options: { now?: Date; evidenceRef: string },
): EvaluationLedger {
  const result = clone(ledger);
  const request = result.requests.find((request) => request.id === requestId);
  const thread = result.threads.find((thread) => thread.threadId === request?.threadId);
  if (
    !request ||
    request.status !== "usage-unknown" ||
    !thread?.last ||
    thread.status !== "known" ||
    (request.turnId && thread.last.turnId !== request.turnId) ||
    time(thread.last.atUtc) < time(request.dispatchedAtUtc!)
  )
    throw new Error("Unknown request requires fresh emitted usage evidence");
  const atUtc = (options.now ?? new Date()).toISOString();
  Object.assign(request, {
    status: "reconciled",
    reconciledAtUtc: atUtc,
    evidenceRef: options.evidenceRef,
    reconciliationEvidenceRef: options.evidenceRef,
  });
  for (const unknown of result.unknowns.filter(
    (unknown) => unknown.id === requestId && !unknown.resolvedAtUtc,
  ))
    unknown.resolvedAtUtc = atUtc;
  return result;
}
export function cancelEvaluationReservation(
  ledger: EvaluationLedger,
  requestId: string,
): EvaluationLedger {
  const result = clone(ledger);
  const request = result.requests.find((request) => request.id === requestId);
  if (!request || request.status !== "reserved")
    throw new Error("Only undispatched reservations can be cancelled");
  request.status = "cancelled";
  return result; // Admitted request counts never recycle.
}
export function closeEvaluationLedger(
  ledger: EvaluationLedger,
  now = new Date(),
): EvaluationLedger {
  if (ledger.status !== "active") throw new Error("Suite is already closed");
  if (ledger.requests.some((request) => ["reserved", "dispatched"].includes(request.status)))
    throw new Error("Reconcile or cancel native dispatches before closing");
  const result = clone(ledger);
  result.status = "closed";
  result.closedAtUtc = now.toISOString();
  return result;
}

/** Internal retries are observed native effects, not pre-approved provider calls controlled by this ledger. */
export function recordObservedNativeRetry(
  ledger: EvaluationLedger,
  options: {
    parentRequestId: string;
    threadId: string;
    turnId?: string;
    evidenceRef: string;
    now?: Date;
    id?: string;
  },
): EvaluationLedger {
  const result = clone(ledger);
  if (
    !result.requests.some(
      (request) => request.id === options.parentRequestId && request.dispatchedAtUtc,
    )
  )
    throw new Error("Observed retry requires an actual parent dispatch");
  if (!options.threadId || !options.evidenceRef)
    throw new Error("Observed retry requires native identity and provenance");
  const id = options.id ?? randomUUID();
  if (result.requests.some((request) => request.id === id))
    throw new Error("Duplicate observed native retry");
  const atUtc = (options.now ?? new Date()).toISOString();
  result.requests.push({
    id,
    kind: "observed-retry",
    reservationTokens: 0,
    reservedAtUtc: atUtc,
    dispatchedAtUtc: atUtc,
    status: "dispatched",
    parentRequestId: options.parentRequestId,
    threadId: options.threadId,
    ...(options.turnId ? { turnId: options.turnId } : {}),
    evidenceRef: options.evidenceRef,
  });
  return result; // Overflow remains observable and prevents further dispatch; no provider retry policy is recreated.
}

/** Record a discovered actual child without inventing a prior reservation or authorizing further execution. */
export function recordObservedNativeChildEffect(
  ledger: EvaluationLedger,
  options: {
    parentRequestId: string;
    threadId: string;
    turnId: string;
    dispatchedAtUtc: string;
    discoveredAtUtc: string;
    evidenceRef: string;
    nativeToolUseId?: string;
    id?: string;
  },
): EvaluationLedger {
  const result = clone(ledger),
    parent = result.requests.find((request) => request.id === options.parentRequestId);
  if (
    result.status !== "active" ||
    !parent?.dispatchedAtUtc ||
    !parent.threadId ||
    !parent.dispatchEvidenceRef
  )
    throw new Error(
      "Observed child requires an actual admitted parent dispatch in its active suite",
    );
  if (
    !options.threadId ||
    options.threadId === parent.threadId ||
    !options.turnId ||
    !options.evidenceRef ||
    time(options.dispatchedAtUtc) < time(parent.dispatchedAtUtc) ||
    time(options.discoveredAtUtc) < time(options.dispatchedAtUtc) ||
    (parent.reconciledAtUtc && time(options.dispatchedAtUtc) > time(parent.reconciledAtUtc))
  )
    throw new Error("Observed child native identity, chronology or provenance is invalid");
  const id = options.id ?? randomUUID();
  if (
    result.requests.some(
      (request) =>
        request.id === id ||
        (request.threadId === options.threadId && request.turnId === options.turnId) ||
        (options.nativeToolUseId && request.nativeToolUseId === options.nativeToolUseId),
    )
  )
    throw new Error("Observed child effect is already admitted");
  result.requests.push({
    id,
    kind: "child",
    admissionClass: "observed-unreserved-effect",
    reservationTokens: 0,
    status: "dispatched",
    parentRequestId: parent.id,
    threadId: options.threadId,
    turnId: options.turnId,
    nativeParentThreadId: parent.threadId,
    dispatchedAtUtc: options.dispatchedAtUtc,
    discoveredAtUtc: options.discoveredAtUtc,
    observedEffectEvidenceRef: options.evidenceRef,
    dispatchEvidenceRef: options.evidenceRef,
    evidenceRef: options.evidenceRef,
    ...(options.nativeToolUseId ? { nativeToolUseId: options.nativeToolUseId } : {}),
  });
  return result; // Actual effects remain visible even after a cap or hold; this is not permission to dispatch.
}

export function validateEvaluationLedger(value: unknown): EvaluationLedger {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    throw new Error("Invalid native suite ledger");
  const ledger = value as EvaluationLedger;
  if (
    ledger.schemaVersion !== "1.0.0" ||
    !ledger.suiteId ||
    !ledger.projectId ||
    !["active", "closed"].includes(ledger.status) ||
    !natural(ledger.revision) ||
    ledger.nativeOwner !== "codex" ||
    ledger.isScheduler !== false ||
    ledger.isProviderHardCap !== false ||
    !Array.isArray(ledger.requests) ||
    !Array.isArray(ledger.threads) ||
    !Array.isArray(ledger.observations) ||
    !Array.isArray(ledger.unknowns)
  )
    throw new Error("Invalid native suite ledger shape or caps");
  validateBudget(ledger);
  time(ledger.startedAtUtc);
  if (ledger.closedAtUtc) time(ledger.closedAtUtc);
  const requestIds = new Set<string>();
  for (const request of ledger.requests) {
    if (
      !request.id ||
      requestIds.has(request.id) ||
      !["turn", "child", "manual-compaction", "observed-retry"].includes(request.kind) ||
      !["reserved", "dispatched", "reconciled", "cancelled", "usage-unknown"].includes(
        request.status,
      ) ||
      !natural(request.reservationTokens)
    )
      throw new Error("Invalid native request record");
    if (request.admissionClass === "observed-unreserved-effect") {
      if (
        request.kind !== "child" ||
        request.reservedAtUtc !== undefined ||
        request.reservationTokens !== 0 ||
        !request.dispatchedAtUtc ||
        !request.discoveredAtUtc ||
        !request.observedEffectEvidenceRef ||
        !request.parentRequestId ||
        !request.threadId ||
        !request.turnId ||
        !request.nativeParentThreadId ||
        !["dispatched", "usage-unknown", "reconciled"].includes(request.status) ||
        time(request.discoveredAtUtc) < time(request.dispatchedAtUtc)
      )
        throw new Error(
          "Observed unreserved child must preserve actual effects without fabricated reservation",
        );
    } else {
      if (
        request.admissionClass !== undefined &&
        request.admissionClass !== "reserved-before-dispatch"
      )
        throw new Error("Invalid native admission class");
      if (!request.reservedAtUtc)
        throw new Error("Admitted native reservation requires its original timestamp");
      time(request.reservedAtUtc);
    }
    if (request.dispatchedAtUtc) time(request.dispatchedAtUtc);
    if (request.reconciledAtUtc) time(request.reconciledAtUtc);
    if (
      (request.plannedNativeChildTargetThreadId === undefined) !==
        (request.plannedNativeChildTargetPath === undefined) ||
      (request.plannedNativeChildTargetThreadId !== undefined &&
        (!request.plannedNativeChild || !request.plannedNativeChildTargetPath?.startsWith("/")))
    )
      throw new Error("Invalid exact planned native child target");
    if (request.rejection || request.outcome === "rejected-before-execution") {
      if (
        request.rejection?.class !== "verified-native-request-rejection" ||
        request.rejection.requestId !== request.id ||
        request.status !== "cancelled" ||
        request.outcome !== "rejected-before-execution" ||
        !request.rejectedAtUtc ||
        !request.rejectionRecordedAtUtc ||
        request.turnId !== undefined ||
        request.reconciledAtUtc !== undefined ||
        request.kind !== "child" ||
        request.nativeExecution !== "app-server-direct-child-followup" ||
        time(request.rejectionRecordedAtUtc) < time(request.rejectedAtUtc)
      )
        throw new Error(
          "Native rejection must retain the counted RPC attempt without fabricating an executed turn",
        );
    }
    requestIds.add(request.id);
  }
  const threads = new Set<string>();
  for (const thread of ledger.threads) {
    if (
      !thread.threadId ||
      threads.has(thread.threadId) ||
      !["known", "unknown"].includes(thread.status)
    )
      throw new Error("Invalid native thread usage record");
    validateUsage(thread.baseline);
    if (thread.last) validateUsage(thread.last.cumulative);
    threads.add(thread.threadId);
  }
  return ledger;
}
export async function readEvaluationLedger(
  path: string,
): Promise<{ ledger: EvaluationLedger; sha256: string }> {
  if (!isAbsolute(path) || (await realpath(path)) !== path || (await lstat(path)).isSymbolicLink())
    throw new Error("Ledger path must be an exact canonical regular file");
  const bytes = await readFile(path);
  const ledger = validateEvaluationLedger(JSON.parse(bytes.toString("utf8")));
  await verifyBudget(ledger);
  return { ledger, sha256: createHash("sha256").update(bytes).digest("hex") };
}
async function assertLedgerTransition(
  before: EvaluationLedger,
  after: EvaluationLedger,
  beforeSha256: string,
  ledgerPath: string,
): Promise<void> {
  if (
    before.schemaVersion !== after.schemaVersion ||
    before.suiteId !== after.suiteId ||
    before.projectId !== after.projectId ||
    before.startedAtUtc !== after.startedAtUtc ||
    before.revision !== after.revision ||
    JSON.stringify(before.caps) !== JSON.stringify(after.caps)
  )
    throw new Error("Ledger identity, revision and caps cannot change through a transition");
  if (JSON.stringify(before.budgetAuthorization) !== JSON.stringify(after.budgetAuthorization))
    throw new Error("Native suite budget authorization cannot change through a transition");
  if (before.status === "closed")
    throw new Error("Closed native suite cannot be mutated or reopened");
  if (after.status === "closed" && !after.closedAtUtc)
    throw new Error("Closed suite requires its actual closing timestamp");
  if (before.closedAtUtc !== undefined && before.closedAtUtc !== after.closedAtUtc)
    throw new Error("Native ledger closing timestamp cannot change");
  const allowed: Record<EvaluationRequest["status"], EvaluationRequest["status"][]> = {
    reserved: ["reserved", "dispatched", "cancelled"],
    dispatched: ["dispatched", "reconciled", "usage-unknown", "cancelled"],
    "usage-unknown": ["usage-unknown", "reconciled", "cancelled"],
    reconciled: ["reconciled"],
    cancelled: ["cancelled"],
  };
  if (after.requests.length < before.requests.length)
    throw new Error("Admitted native requests are append-only and cannot recycle budget");
  for (let index = 0; index < before.requests.length; index++) {
    const old = before.requests[index]!;
    const current = after.requests[index];
    if (!current || !allowed[old.status].includes(current.status))
      throw new Error("Invalid native request status transition");
    for (const key of [
      "id",
      "kind",
      "reservationTokens",
      "reservedAtUtc",
      "parentRequestId",
      "admissionClass",
      "discoveredAtUtc",
      "observedEffectEvidenceRef",
      "plannedNativeChild",
      "plannedNativeChildTargetThreadId",
      "plannedNativeChildTargetPath",
    ] as const)
      if (old[key] !== current[key]) throw new Error(`Admitted native request ${key} is immutable`);
    for (const key of [
      "dispatchedAtUtc",
      "threadId",
      "turnId",
      "nativeToolUseId",
      "hookInputSha256",
      "nativeParentThreadId",
      "nativeExecution",
      "dispatchEvidenceRef",
      "outcome",
      "reconciledAtUtc",
      "reconciliationEvidenceRef",
    ] as const)
      if (old[key] !== undefined && old[key] !== current[key])
        throw new Error(`Observed native request ${key} is immutable`);
    if (
      ["reconciled", "cancelled"].includes(old.status) &&
      JSON.stringify(old) !== JSON.stringify(current)
    )
      throw new Error("Completed or cancelled native requests are immutable");
    if (current.status === "cancelled" && old.status !== "reserved" && old.status !== "cancelled") {
      if (!current.rejection)
        throw new Error(
          "An attempted native request can only be cancelled after an independently verified rejection",
        );
      const proof = await verifyNativeRequestRejectionReview(current.rejection.review, {
        before,
        beforeSha256,
        ledgerPath,
        resolution: current.rejection,
      });
      assertRejectedRequestResolutionOnly(before, after, proof.requestId, proof.rejectedAtUtc);
    }
  }
  if (after.threads.length < before.threads.length)
    throw new Error("Native suite thread lineage cannot be removed");
  for (let index = 0; index < before.threads.length; index++) {
    const old = before.threads[index]!;
    const current = after.threads[index];
    if (
      !current ||
      old.threadId !== current.threadId ||
      old.parentThreadId !== current.parentThreadId ||
      JSON.stringify(old.baseline) !== JSON.stringify(current.baseline)
    )
      throw new Error("Native suite thread identity and usage baseline are immutable");
    if (
      old.last &&
      (!current.last ||
        current.last.cumulative.totalTokens < old.last.cumulative.totalTokens ||
        current.last.cumulative.inputTokens < old.last.cumulative.inputTokens ||
        current.last.cumulative.outputTokens < old.last.cumulative.outputTokens ||
        time(current.last.atUtc) < time(old.last.atUtc))
    )
      throw new Error("Emitted cumulative native usage cannot move backwards");
  }
  if (
    after.observations.length < before.observations.length ||
    before.observations.some(
      (old, index) => JSON.stringify(old) !== JSON.stringify(after.observations[index]),
    )
  )
    throw new Error("Emitted usage observations are immutable append-only evidence");
  for (const thread of after.threads) {
    const latest = after.observations.findLast(
      (observation) => observation.threadId === thread.threadId,
    );
    if (JSON.stringify(latest ?? null) !== JSON.stringify(thread.last))
      throw new Error("Thread cumulative usage must match its latest preserved native observation");
    if (thread.last?.childAggregation === "unknown" && thread.status !== "unknown")
      throw new Error("Unknown child aggregation cannot be promoted to known");
  }
  if (after.unknowns.length < before.unknowns.length)
    throw new Error("Unknown native effects must be retained with explicit resolution");
  for (let index = 0; index < before.unknowns.length; index++) {
    const old = before.unknowns[index]!;
    const current = after.unknowns[index];
    if (
      !current ||
      old.id !== current.id ||
      old.reason !== current.reason ||
      old.evidenceRef !== current.evidenceRef ||
      (old.resolvedAtUtc !== undefined &&
        (old.resolvedAtUtc !== current.resolvedAtUtc ||
          JSON.stringify(old.resolution) !== JSON.stringify(current.resolution)))
    )
      throw new Error("Unknown native effect provenance is immutable");
    if (!old.resolvedAtUtc && current.resolvedAtUtc) {
      if (current.resolution?.class === "verified-completed-native-readback") {
        const proof = await verifyCompletedReadbackReview(current.resolution.review, {
          before,
          beforeSha256,
          ledgerPath,
          resolution: current.resolution,
        });
        if (proof.unknownId !== old.id || time(current.resolvedAtUtc) < time(proof.completedAtUtc))
          throw new Error("Readback resolution must follow the exact completed native effect");
        const unchanged = structuredClone(after),
          retained = unchanged.unknowns.find((row) => row.id === old.id)!;
        delete retained.resolvedAtUtc;
        delete retained.resolution;
        if (JSON.stringify(unchanged) !== JSON.stringify(before))
          throw new Error(
            "Readback resolution can only annotate its exact retained hold; requests, usage and budget cannot change",
          );
        continue;
      }
      if (current.resolution?.class === "verified-non-dispatch") {
        await verifyNonDispatchReview(current.resolution.review, {
          before,
          beforeSha256,
          ledgerPath,
          unknownId: old.id,
          resolution: current.resolution,
        });
        if (
          after.requests.length ||
          after.threads.length ||
          after.observations.length ||
          after.status !== "active" ||
          after.unknowns.length !== before.unknowns.length
        )
          throw new Error("Non-dispatch resolution cannot admit effects or change suite closure");
        continue;
      }
      if (current.resolution?.class === "verified-owned-native-boundary") {
        await verifyOwnedNativeBoundaryReview(current.resolution.review, {
          before,
          beforeSha256,
          ledgerPath,
          unknownId: old.id,
          resolution: current.resolution,
        });
        if (
          JSON.stringify(before.requests) !== JSON.stringify(after.requests) ||
          JSON.stringify(before.threads) !== JSON.stringify(after.threads) ||
          JSON.stringify(before.observations) !== JSON.stringify(after.observations) ||
          after.status !== "active" ||
          after.unknowns.length !== before.unknowns.length
        )
          throw new Error(
            "Owned native boundary resolution cannot change effects, costs, or suite closure",
          );
        continue;
      }
      if (current.resolution?.class === "verified-native-request-rejection") {
        const proof = await verifyNativeRequestRejectionReview(current.resolution.review, {
          before,
          beforeSha256,
          ledgerPath,
          unknownId: old.id,
          resolution: current.resolution,
        });
        assertRejectedRequestResolutionOnly(before, after, proof.requestId, proof.rejectedAtUtc);
        if (
          JSON.stringify(
            after.requests.find((request) => request.id === proof.requestId)?.rejection,
          ) !== JSON.stringify(current.resolution)
        )
          throw new Error("Rejected request and retained unknown proof must match");
        continue;
      }
      const request = after.requests.find((request) => request.id === current.id);
      const thread = after.threads.find((thread) => thread.threadId === request?.threadId);
      if (!request || request.status !== "reconciled" || !thread?.last || thread.status !== "known")
        throw new Error("Unknown native effect resolution requires reconciled actual usage");
    }
  }
}
/** Compare-and-swap owner updates only. This does not dispatch or schedule native execution. */
export async function updateEvaluationLedger(
  path: string,
  transform: (ledger: EvaluationLedger) => EvaluationLedger,
  options: { expectedSha256?: string } = {},
): Promise<{ ledger: EvaluationLedger; sha256: string }> {
  if (!isAbsolute(path)) throw new Error("Ledger path must be absolute");
  const lock = `${path}.lock`;
  await mkdir(lock, { mode: 0o700 });
  let temporary: string | undefined;
  try {
    const before = await readEvaluationLedger(path);
    if (before.ledger.status === "closed")
      throw new Error("Closed native suite cannot be mutated or reopened");
    if (options.expectedSha256 !== undefined && options.expectedSha256 !== before.sha256)
      throw new Error("Native ledger predecessor hash conflict");
    const after = validateEvaluationLedger(transform(structuredClone(before.ledger)));
    await verifyBudget(after);
    await assertLedgerTransition(before.ledger, after, before.sha256, path);
    after.revision = before.ledger.revision + 1;
    const bytes = `${JSON.stringify(after, null, 2)}\n`;
    temporary = `${path}.${randomUUID()}.tmp`;
    const metadata = await lstat(path);
    const file = await open(temporary, "wx", metadata.mode & 0o777);
    try {
      await file.writeFile(bytes);
      await file.sync();
    } finally {
      await file.close();
    }
    if ((await readEvaluationLedger(path)).sha256 !== before.sha256)
      throw new Error("Native ledger changed outside owner lock");
    await verifyBudget(after);
    if ((await realpath(dirname(path))) !== dirname(path))
      throw new Error("Unsafe native ledger directory");
    await rename(temporary, path);
    temporary = undefined;
    const directory = await open(dirname(path), "r");
    try {
      await directory.sync();
    } finally {
      await directory.close();
    }
    return { ledger: after, sha256: createHash("sha256").update(bytes).digest("hex") };
  } finally {
    if (temporary) await rm(temporary, { force: true });
    await rm(lock, { recursive: true });
  }
}

async function pinnedNonDispatchBytes(
  pin: { path: string; sha256: string },
  maxBytes: number,
): Promise<Buffer> {
  if (
    !pin ||
    !isAbsolute(pin.path) ||
    !/^[a-f0-9]{64}$/.test(pin.sha256) ||
    (await realpath(pin.path)) !== pin.path
  )
    throw new Error("Exact non-dispatch evidence pin required");
  const metadata = await lstat(pin.path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > maxBytes)
    throw new Error("Non-dispatch evidence exceeds its exact bounded file scope");
  const bytes = await readFile(pin.path);
  if (createHash("sha256").update(bytes).digest("hex") !== pin.sha256)
    throw new Error("Non-dispatch evidence pin drifted");
  return bytes;
}

async function verifyCompletedReadbackReview(
  reviewPin: { path: string; sha256: string },
  input: {
    before: EvaluationLedger;
    beforeSha256: string;
    ledgerPath: string;
    resolution?: CompletedReadbackResolution;
  },
) {
  const ledger = input.before;
  if (ledger.status !== "active" || !ledger.budgetAuthorization)
    throw new Error("Completed readback resolution requires a separately authorized new suite");
  const review: CompletedReadbackReview = JSON.parse(
    (await pinnedNonDispatchBytes(reviewPin, 1_048_576)).toString("utf8"),
  );
  const bookkeeping =
    review.verificationClass === "reserved-to-reconciled-bookkeeping-false-negative";
  if (
    review.schema !== "codex.evaluation.completed-readback-review.v1" ||
    review.status !== "passed-scoped" ||
    (!bookkeeping && review.verificationClass !== "idle-only-readback-false-negative") ||
    !review.reviewerThreadId ||
    review.suiteId !== ledger.suiteId ||
    review.projectId !== ledger.projectId ||
    review.ledger?.path !== input.ledgerPath ||
    review.ledger.sha256 !== input.beforeSha256 ||
    !isAbsolute(review.projectRoot)
  )
    throw new Error("Completed readback review does not match the exact new-suite predecessor");
  const request = ledger.requests.find((row) => row.id === review.requestId),
    unknown = ledger.unknowns.find((row) => row.id === review.unknownId);
  if (
    !request ||
    request.kind !== "turn" ||
    request.status !== "reserved" ||
    request.admissionClass !== "reserved-before-dispatch" ||
    !request.reservedAtUtc ||
    request.threadId ||
    request.dispatchedAtUtc ||
    !unknown ||
    unknown.resolvedAtUtc ||
    !unknown.id.startsWith(request.id + "-accounting-hold-") ||
    unknown.reason !==
      (bookkeeping
        ? "Error: Invalid native request status transition"
        : "Error: Exact fresh native terminal readback required") ||
    unknown.evidenceRef !== review.errorEvidence?.path
  )
    throw new Error(
      "Review is not the exact unused reservation and false bookkeeping/readback hold",
    );
  if (
    bookkeeping &&
    !ledger.unknowns.some(
      (row) =>
        row.id.startsWith(request.id + "-accounting-hold-") &&
        row.reason === "Error: Exact fresh native terminal readback required" &&
        row.resolvedAtUtc &&
        row.resolution?.class === "verified-completed-native-readback",
    )
  )
    throw new Error(
      "Bookkeeping resolution requires the retained previously reviewed readback hold",
    );
  const error = JSON.parse(
    (await pinnedNonDispatchBytes(review.errorEvidence, 1_048_576)).toString("utf8"),
  );
  const native = review.native;
  if (
    !native ||
    !native.threadId ||
    !native.turnId ||
    !native.sessionId ||
    !native.parentThreadId ||
    !native.rootTurnId ||
    !native.cwd ||
    native.model !== "gpt-6.1-sol" ||
    native.effort !== "high" ||
    review.reviewerThreadId === native.threadId ||
    error.schema !== "codex.native-accounting-hold.v1" ||
    error.caseId !== request.id ||
    error.threadId !== native.threadId ||
    error.reason !== unknown.reason ||
    typeof error.terminalPath !== "string" ||
    resolve(review.projectRoot, error.terminalPath) !== review.readback?.path
  )
    throw new Error("Original accounting error does not bind this native readback");
  const readback = JSON.parse(
    (await pinnedNonDispatchBytes(review.readback, 16_777_216)).toString("utf8"),
  );
  if (
    readback.thread?.id !== native.threadId ||
    readback.thread.status?.type !== "notLoaded" ||
    readback.page?.hasMore !== false ||
    !Array.isArray(readback.turns) ||
    readback.turns.length !== 1 ||
    readback.turns[0].id !== native.turnId ||
    readback.turns[0].status !== "completed"
  )
    throw new Error("Readback is not an exact completed unloaded native turn");
  const raw = (await pinnedNonDispatchBytes(review.rollout, 8_388_608)).toString("utf8");
  if (!raw.endsWith("\n"))
    throw new Error("Completed native rollout must end at a complete original line");
  const lines = raw.trim().split("\n");
  if (!lines.length || lines.length > 100_000)
    throw new Error("Completed native rollout exceeds proof bounds");
  const rows = lines.map((line) => JSON.parse(line)),
    header = rows[0];
  if (
    header.type !== "session_meta" ||
    header.payload.id !== native.threadId ||
    header.payload.cwd !== native.cwd ||
    header.payload.session_id !== native.sessionId ||
    header.payload.parent_thread_id !== native.parentThreadId
  )
    throw new Error("Completed native rollout identity or lineage mismatch");
  const contexts = rows.filter((row) => row.type === "turn_context"),
    starts = rows.filter((row) => row.type === "event_msg" && row.payload.type === "task_started"),
    ends = rows.filter((row) => row.type === "event_msg" && row.payload.type === "task_complete");
  if (
    contexts.length !== 1 ||
    contexts[0].payload.turn_id !== native.turnId ||
    contexts[0].payload.model !== native.model ||
    contexts[0].payload.effort !== native.effort ||
    starts.length !== 1 ||
    ends.length !== 1 ||
    starts[0].payload.turn_id !== native.turnId ||
    ends[0].payload.turn_id !== native.turnId ||
    starts[0].payload.root_turn_id !== native.rootTurnId ||
    ends[0].payload.root_turn_id !== native.rootTurnId ||
    time(header.payload.timestamp) < time(request.reservedAtUtc) ||
    time(starts[0].timestamp) < time(request.reservedAtUtc) ||
    time(contexts[0].timestamp) < time(starts[0].timestamp) ||
    time(contexts[0].timestamp) > time(ends[0].timestamp) ||
    time(ends[0].timestamp) < time(starts[0].timestamp) ||
    time(error.atUtc) < time(ends[0].timestamp)
  )
    throw new Error(
      "Completed native task/routing/chronology does not match the original reservation",
    );
  if (
    rows.some(
      (row) =>
        row.type === "response_item" &&
        row.payload.type === "function_call" &&
        /(spawn_agent|followup_task|send_input|resume_agent|create_thread|send_message_to_thread)$/.test(
          row.payload.name,
        ),
    ) ||
    rows.some(
      (row) =>
        row.type === "event_msg" &&
        (row.payload.willRetry === true ||
          row.payload.will_retry === true ||
          /retry|reconnect/i.test(row.payload.type) ||
          (["error", "warning"].includes(row.payload.type) &&
            /retry|reconnect/i.test(row.payload.message ?? ""))),
    )
  )
    throw new Error("Child or retry effects cannot be resolved as an idle-only readback error");
  const usage = zero(),
    responses = new Map<string, TokenUsage>();
  for (const row of rows.filter((row) => row.type === "token_usage_record")) {
    const p = row.payload,
      u = p.usage;
    if (
      p.thread_id !== native.threadId ||
      p.turn_id !== native.turnId ||
      p.session_id !== native.sessionId ||
      p.root_turn_id !== native.rootTurnId ||
      typeof p.response_id !== "string" ||
      !p.response_id ||
      time(row.timestamp) < time(request.reservedAtUtc) ||
      time(row.timestamp) > time(ends[0].timestamp)
    )
      throw new Error("Native own response lies outside its exact completed task");
    const value: TokenUsage = {
      totalTokens: u.total_tokens,
      inputTokens: u.input_tokens,
      outputTokens: u.output_tokens,
      cachedInputTokens: u.cached_input_tokens,
      reasoningOutputTokens: u.reasoning_output_tokens,
      cacheWriteInputTokens: u.cache_write_input_tokens ?? 0,
    };
    validateUsage(value);
    const prior = responses.get(p.response_id);
    if (prior && JSON.stringify(prior) !== JSON.stringify(value))
      throw new Error("Conflicting completed native response usage");
    if (!prior) {
      responses.set(p.response_id, value);
      for (const key of Object.keys(usage) as (keyof TokenUsage)[])
        usage[key] = (usage[key] ?? 0) + (value[key] ?? 0);
    }
  }
  validateUsage(review.ownUsage);
  if (
    !responses.size ||
    responses.size !== review.ownResponseCount ||
    !(Object.keys(usage) as (keyof TokenUsage)[]).every(
      (key) => usage[key] === (review.ownUsage[key] ?? 0),
    )
  )
    throw new Error("Reviewed native own-response totals differ from original usage");
  if (bookkeeping) {
    if (
      !review.snapshot ||
      !review.usageEvidence ||
      review.snapshot.sha256 !== review.rollout.sha256
    )
      throw new Error("Bookkeeping error requires exact immutable snapshot and usage proof");
    await pinnedNonDispatchBytes(review.snapshot, 8_388_608);
    const proof = JSON.parse(
      (await pinnedNonDispatchBytes(review.usageEvidence, 16_777_216)).toString("utf8"),
    );
    const total = proof.aggregate?.totals?.[native.threadId];
    if (
      proof.schema !== "codex.native-qualified-response-accounting.v1" ||
      proof.caseId !== request.id ||
      proof.threadId !== native.threadId ||
      proof.turnId !== native.turnId ||
      proof.observed?.source?.path !== review.rollout.path ||
      proof.observed.source.sha256 !== review.rollout.sha256 ||
      JSON.stringify(proof.observed?.snapshot) !== JSON.stringify(review.snapshot) ||
      proof.terminal?.sha256 !== review.readback.sha256 ||
      resolve(review.projectRoot, proof.terminal?.path ?? "") !== review.readback.path ||
      proof.aggregate?.uniqueResponses !== responses.size ||
      !total ||
      !(Object.keys(usage) as (keyof TokenUsage)[]).every(
        (key) => usage[key] === (total[key] ?? 0),
      ) ||
      !Array.isArray(proof.childCalls) ||
      proof.childCalls.length
    )
      throw new Error("Original failed accounting proof does not match exact native own usage");
  }
  if (
    input.resolution?.ledgerBeforeSha256 !== undefined &&
    input.resolution.ledgerBeforeSha256 !== input.beforeSha256
  )
    throw new Error("Completed readback resolution predecessor changed");
  return {
    unknownId: unknown.id,
    completedAtUtc: ends[0].timestamp as string,
    ownUsage: usage,
    ownResponseCount: responses.size,
  };
}

/** Observe proof only; no suite mutation, dispatch, cost reconciliation or acceptance. */
export async function verifyEvaluationCompletedReadbackReview(
  path: string,
  options: { reviewPin: { path: string; sha256: string } },
) {
  const before = await readEvaluationLedger(path);
  return {
    ...(await verifyCompletedReadbackReview(options.reviewPin, {
      before: before.ledger,
      beforeSha256: before.sha256,
      ledgerPath: path,
    })),
    ledgerSha256: before.sha256,
    actualQualificationVerified: false as const,
  };
}
/** Resolve only proved readback or reserved-to-completed bookkeeping errors; usage remains separate. */
export async function resolveEvaluationCompletedReadbackHold(
  path: string,
  options: { reviewPin: { path: string; sha256: string }; now?: Date },
) {
  const before = await readEvaluationLedger(path),
    proof = await verifyCompletedReadbackReview(options.reviewPin, {
      before: before.ledger,
      beforeSha256: before.sha256,
      ledgerPath: path,
    });
  return updateEvaluationLedger(
    path,
    (ledger) => {
      const unknown = ledger.unknowns.find((row) => row.id === proof.unknownId)!;
      unknown.resolvedAtUtc = (options.now ?? new Date()).toISOString();
      unknown.resolution = {
        class: "verified-completed-native-readback",
        review: options.reviewPin,
        ledgerBeforeSha256: before.sha256,
      };
      return ledger;
    },
    { expectedSha256: before.sha256 },
  );
}
async function verifyNonDispatchReview(
  reviewPin: { path: string; sha256: string },
  input: {
    before: EvaluationLedger;
    beforeSha256: string;
    ledgerPath: string;
    unknownId?: string;
    resolution?: NonDispatchResolution;
  },
): Promise<{ unknownId: string; protocol: { path: string; sha256: string } }> {
  const ledger = input.before;
  if (
    ledger.status !== "active" ||
    ledger.requests.length ||
    ledger.threads.length ||
    ledger.observations.length ||
    aggregateTokens(ledger) !== 0 ||
    ledgerSummary(ledger).reservedTokens !== 0
  )
    throw new Error(
      "Non-dispatch resolution requires an active suite with zero actual or admitted effects",
    );
  const review = JSON.parse((await pinnedNonDispatchBytes(reviewPin, 1_048_576)).toString("utf8"));
  if (
    review.schema !== "codex.qualification.non-dispatch-review.v1" ||
    review.status !== "passed-scoped" ||
    review.verificationClass !== "exact-owned-wire-non-dispatch" ||
    typeof review.reviewer !== "string" ||
    !review.reviewer ||
    review.suiteId !== ledger.suiteId ||
    review.ledger?.sha256 !== input.beforeSha256 ||
    review.ledger.startedAtUtc !== ledger.startedAtUtc ||
    review.ledger.status !== "active" ||
    ["requests", "threads", "observations", "aggregateTokens"].some(
      (key) => review.ledger[key] !== 0,
    )
  )
    throw new Error("Independent non-dispatch review does not match the exact suite predecessor");
  const nativeCase = JSON.parse(
    (
      await pinnedNonDispatchBytes(
        { path: review.case?.path, sha256: review.case?.sha256 },
        1_048_576,
      )
    ).toString("utf8"),
  );
  if (
    review.ledger.path !== input.ledgerPath ||
    nativeCase.ledgerPath !== input.ledgerPath ||
    nativeCase.projectId !== ledger.projectId ||
    nativeCase.caseId !== review.caseId ||
    nativeCase.workId !== review.workId ||
    nativeCase.checkpointPath !== review.case.checkpointPath ||
    nativeCase.initialCheckpointSha256 !== review.case.checkpointSha256 ||
    ledger.suiteId !== `codex-continuity-qualification-${nativeCase.caseId}`
  )
    throw new Error("Non-dispatch review is not scoped to the actual owning Case and ledger path");
  await pinnedNonDispatchBytes(
    { path: nativeCase.checkpointPath, sha256: nativeCase.initialCheckpointSha256 },
    2_097_152,
  );
  const allowed = review.reconciliation;
  const unknown = ledger.unknowns.find((row) => row.id === allowed?.allowedUnknownId);
  if (
    !unknown ||
    unknown.resolvedAtUtc ||
    !unknown.id.startsWith("qualification-hold-") ||
    (input.unknownId !== undefined && input.unknownId !== unknown.id) ||
    unknown.reason !== allowed.allowedExactReason ||
    unknown.evidenceRef !== allowed.allowedEvidenceRef ||
    allowed.physicalUnknownDeletionAllowed !== false ||
    allowed.closedSuiteReopenAllowed !== false ||
    allowed.suiteReplacementAllowed !== false ||
    allowed.requestsOrTokensRecyclingAllowed !== false ||
    allowed.startedAtUtcMustRemain !== ledger.startedAtUtc ||
    JSON.stringify(allowed.capsMustRemain) !== JSON.stringify(ledger.caps)
  )
    throw new Error("Non-dispatch review does not authorize this exact retained hold");
  const pins = review.sourcePins;
  if (!Array.isArray(pins) || !pins.length)
    throw new Error("Non-dispatch review requires original protocol and response-history pins");
  const protocolPin = pins.find((pin: { path: string }) => pin.path === unknown.evidenceRef);
  if (!protocolPin)
    throw new Error("Non-dispatch protocol locator does not match the held native evidence");
  const bytesByPath = new Map<string, Buffer>();
  for (const pin of pins)
    bytesByPath.set(
      pin.path,
      await pinnedNonDispatchBytes(
        pin,
        pin.path === protocolPin.path ? 64 * 1024 * 1024 : 1_048_576,
      ),
    );
  const rows = bytesByPath
    .get(protocolPin.path)!
    .toString("utf8")
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  if (!rows.length || rows.length > 100_000)
    throw new Error("Invalid non-dispatch protocol bounds");
  const outbound = rows.filter((row) => row.direction === "out");
  if (
    outbound.some(
      (row) =>
        !["initialize", "config/read", "hooks/list", "thread/start"].includes(row.message?.method),
    )
  )
    throw new Error("Native protocol contains a possible model or other dispatch effect");
  const starts = outbound.filter((row) => row.message.method === "thread/start");
  const started = starts[0];
  if (
    starts.length !== 1 ||
    started.message.params?.projectId !== ledger.projectId ||
    started.message.params?.cwd !== nativeCase.worktreeRoot
  )
    throw new Error("Non-dispatch review requires the exact rejected local project thread request");
  const reply = rows.find(
    (row) => row.direction === "in" && row.message?.id === started.message.id && row.message.error,
  );
  if (
    reply?.message.error?.code !== -32600 ||
    reply.message.error.message !== `project not found: ${ledger.projectId}` ||
    `Error: Native RPC ${JSON.stringify(reply.message.error)}` !== unknown.reason ||
    review.nativeObservation?.threadStartRequestId !== started.message.id ||
    JSON.stringify(review.nativeObservation.error) !== JSON.stringify(reply.message.error)
  )
    throw new Error("Actual native pre-thread project rejection does not match the reviewed hold");
  if (
    rows.some(
      (row) =>
        row.direction === "in" &&
        (row.message?.result?.thread?.id ||
          /^(?:thread\/|turn\/|rawResponse\/|hook\/|item\/)/.test(row.message?.method ?? "")),
    )
  )
    throw new Error("Native protocol contains a created thread, model, tool or hook effect");
  const histories = [...bytesByPath.entries()]
    .filter(([path]) => path.endsWith("/response-history.json"))
    .map(([, bytes]) => JSON.parse(bytes.toString("utf8")));
  if (
    histories.length !== 1 ||
    histories[0].caseId !== review.caseId ||
    !Array.isArray(histories[0].rawResponses) ||
    histories[0].rawResponses.length ||
    !Array.isArray(histories[0].chargedResponseKeys) ||
    histories[0].chargedResponseKeys.length ||
    histories[0].rawProtocol?.path !== protocolPin.path ||
    histories[0].rawProtocol?.sha256 !== protocolPin.sha256
  )
    throw new Error("Reviewed native response history does not establish zero model usage");
  if (
    input.resolution &&
    (input.resolution.ledgerBeforeSha256 !== input.beforeSha256 ||
      JSON.stringify(input.resolution.protocol) !== JSON.stringify(protocolPin))
  )
    throw new Error("Non-dispatch resolution proof changed from its reviewed pins");
  return { unknownId: unknown.id, protocol: protocolPin };
}

/** Resolve only an independently proved pre-thread rejection. This never recreates a suite or dispatches a native request. */
export async function resolveEvaluationNonDispatchHold(
  path: string,
  options: { reviewPin: { path: string; sha256: string }; now?: Date },
): Promise<{ ledger: EvaluationLedger; sha256: string }> {
  const before = await readEvaluationLedger(path);
  const proof = await verifyNonDispatchReview(options.reviewPin, {
    before: before.ledger,
    beforeSha256: before.sha256,
    ledgerPath: path,
  });
  return updateEvaluationLedger(
    path,
    (ledger) => {
      const unknown = ledger.unknowns.find((row) => row.id === proof.unknownId)!;
      unknown.resolvedAtUtc = (options.now ?? new Date()).toISOString();
      unknown.resolution = {
        class: "verified-non-dispatch",
        review: options.reviewPin,
        protocol: proof.protocol,
        ledgerBeforeSha256: before.sha256,
      };
      return ledger;
    },
    { expectedSha256: before.sha256 },
  );
}

const usageKeys = [
  "totalTokens",
  "inputTokens",
  "cachedInputTokens",
  "outputTokens",
  "reasoningOutputTokens",
  "cacheWriteInputTokens",
] as const;
const sameUsage = (a: TokenUsage, b: TokenUsage) =>
  usageKeys.every((key) => (a[key] ?? 0) === (b[key] ?? 0));
function nativeWireRows(bytes: Buffer): any[] {
  const lines = bytes.toString("utf8").trim().split("\n");
  if (!lines.length || lines.length > 100_000)
    throw new Error("Owned native protocol exceeds its bounded proof scope");
  return lines.map((line) => JSON.parse(line));
}
async function verifyOwnedNativeBoundaryReview(
  reviewPin: { path: string; sha256: string },
  input: {
    before: EvaluationLedger;
    beforeSha256: string;
    ledgerPath: string;
    unknownId?: string;
    resolution?: OwnedNativeBoundaryResolution;
  },
): Promise<{
  unknownIds: string[];
  protocol: { path: string; sha256: string };
  responseHistory: { path: string; sha256: string };
  readback: { path: string; sha256: string };
}> {
  const ledger = input.before,
    summary = ledgerSummary(ledger);
  if (
    ledger.status !== "active" ||
    !ledger.requests.length ||
    !ledger.threads.length ||
    summary.pendingRequests ||
    summary.reservedTokens ||
    ledger.requests.some(
      (request) =>
        request.status !== "reconciled" ||
        !request.threadId ||
        !request.turnId ||
        !request.dispatchedAtUtc ||
        !request.reconciledAtUtc ||
        !request.outcome,
    ) ||
    ledger.threads.some(
      (thread) =>
        thread.status !== "known" ||
        !thread.last ||
        thread.last.childAggregation !== "excludes-children",
    )
  )
    throw new Error(
      "Owned boundary requires every actual admitted effect reconciled with known own-thread usage",
    );
  const review: OwnedNativeBoundaryReview = JSON.parse(
    (await pinnedNonDispatchBytes(reviewPin, 1_048_576)).toString("utf8"),
  );
  if (
    review.schema !== "codex.qualification.owned-native-boundary-review.v1" ||
    review.status !== "passed-scoped" ||
    review.verificationClass !== "exact-owned-native-boundary" ||
    !review.reviewer ||
    review.actualQualificationVerified !== false ||
    review.suiteId !== ledger.suiteId ||
    review.ledger?.path !== input.ledgerPath ||
    review.ledger.sha256 !== input.beforeSha256 ||
    review.ledger.startedAtUtc !== ledger.startedAtUtc ||
    JSON.stringify(review.ledger.caps) !== JSON.stringify(ledger.caps)
  )
    throw new Error("Owned boundary review does not match the exact retained ledger predecessor");
  const nativeCase = JSON.parse(
    (await pinnedNonDispatchBytes(review.case, 1_048_576)).toString("utf8"),
  );
  if (
    nativeCase.caseId !== review.caseId ||
    nativeCase.workId !== review.workId ||
    nativeCase.projectId !== ledger.projectId ||
    nativeCase.ledgerPath !== input.ledgerPath ||
    nativeCase.checkpointPath !== review.case.checkpointPath ||
    ledger.suiteId !== `codex-continuity-qualification-${nativeCase.caseId}`
  )
    throw new Error("Owned boundary review is not scoped to its actual Case");
  await pinnedNonDispatchBytes(
    { path: review.case.checkpointPath, sha256: review.case.checkpointSha256 },
    2_097_152,
  );
  if (!Array.isArray(review.sourcePins) || !review.sourcePins.length)
    throw new Error("Owned boundary requires pinned original source observations");
  for (const pin of review.sourcePins) await pinnedNonDispatchBytes(pin, 2_097_152);
  const permissions = review.reconciliation,
    allowed = permissions?.allowedUnknowns;
  if (
    !Array.isArray(allowed) ||
    !allowed.length ||
    new Set(allowed.map((row) => row.id)).size !== allowed.length ||
    permissions.physicalUnknownDeletionAllowed !== false ||
    permissions.suiteReplacementAllowed !== false ||
    permissions.closedSuiteReopenAllowed !== false ||
    permissions.requestsOrTokensRecyclingAllowed !== false
  )
    throw new Error("Owned boundary review must narrowly retain the exact unknown rows and budget");
  for (const permission of allowed) {
    const unknown = ledger.unknowns.find((row) => row.id === permission.id);
    if (
      !unknown ||
      unknown.resolvedAtUtc ||
      unknown.reason !== permission.reason ||
      unknown.evidenceRef !== permission.evidenceRef
    )
      throw new Error("Owned boundary unknown provenance does not match the independent review");
    if (
      unknown.id.startsWith("qualification-hold-") &&
      unknown.reason === "Error: Native qualification stage deadline"
    ) {
      if (
        unknown.evidenceRef !== review.protocol.path ||
        permission.observedEffectRequestId !== undefined
      )
        throw new Error("Deadline boundary must reference the original owned protocol");
    } else if (unknown.id.startsWith("native-unreserved-child-")) {
      const effect = ledger.requests.find(
        (request) => request.id === permission.observedEffectRequestId,
      );
      if (
        !effect ||
        effect.admissionClass !== "observed-unreserved-effect" ||
        unknown.id !== `native-unreserved-child-${effect.threadId}` ||
        (effect.observedEffectEvidenceRef !== review.protocol.path &&
          !review.sourcePins.some((pin) => pin.path === effect.observedEffectEvidenceRef)) ||
        !review.sourcePins.some((pin) => pin.path === unknown.evidenceRef)
      )
        throw new Error(
          "Unreserved child gap requires the exact retained observed admission and original finding",
        );
    } else
      throw new Error(
        "Usage unknowns and unrelated holds cannot be cleared by an owned boundary review",
      );
  }
  if (input.unknownId !== undefined && !allowed.some((row) => row.id === input.unknownId))
    throw new Error("Resolution attempts an unknown outside its reviewed boundary");
  const rows = nativeWireRows(await pinnedNonDispatchBytes(review.protocol, 64 * 1024 * 1024));
  const history = JSON.parse(
    (await pinnedNonDispatchBytes(review.responseHistory, 4_194_304)).toString("utf8"),
  );
  if (
    history.caseId !== review.caseId ||
    !Array.isArray(history.rawResponses) ||
    !Array.isArray(history.chargedResponseKeys) ||
    JSON.stringify(history.rawProtocol) !== JSON.stringify(review.protocol)
  )
    throw new Error("Owned boundary response history must retain the exact original wire pin");
  const raw = new Map<
    string,
    {
      threadId: string;
      turnId: string;
      responseId: string;
      atUtc: string;
      callbackBeforeUtc: string;
      usage: TokenUsage;
    }
  >();
  for (const [index, row] of rows.entries()) {
    if (row.direction !== "in" || row.message?.method !== "rawResponse/completed") continue;
    const p = row.message.params;
    validateUsage(p.usage);
    time(row.atUtc);
    if (
      !p.threadId ||
      !p.turnId ||
      !p.responseId ||
      !ledger.requests.some(
        (request) => request.threadId === p.threadId && request.turnId === p.turnId,
      )
    )
      throw new Error("Original protocol contains unaccounted native response ownership");
    // NativeClient records onWire before the synchronous onEvent callback. Its callback clock must precede the next incoming wire event.
    const nextIncoming = rows.slice(index + 1).find((candidate) => candidate.direction === "in");
    if (!nextIncoming?.atUtc || time(nextIncoming.atUtc) < time(row.atUtc))
      throw new Error("Native callback observation has no monotonic original wire boundary");
    const value = {
        threadId: p.threadId,
        turnId: p.turnId,
        responseId: p.responseId,
        atUtc: row.atUtc,
        callbackBeforeUtc: nextIncoming.atUtc,
        usage: p.usage,
      },
      key = `${p.threadId}\0${p.responseId}`,
      previous = raw.get(key);
    if (previous && (previous.turnId !== value.turnId || !sameUsage(previous.usage, value.usage)))
      throw new Error("Duplicate native response key carries conflicting usage");
    if (!previous) raw.set(key, value);
  }
  const historyKeys = new Set(
    history.rawResponses.map((record: any) => `${record.threadId}\0${record.responseId}`),
  );
  if (
    !raw.size ||
    history.rawResponses.length !== raw.size ||
    historyKeys.size !== raw.size ||
    [...raw.keys()].some((key) => !historyKeys.has(key)) ||
    new Set(history.chargedResponseKeys).size !== raw.size ||
    history.chargedResponseKeys.length !== raw.size ||
    [...raw.keys()].some((key) => !history.chargedResponseKeys.includes(key))
  )
    throw new Error(
      "Every unique raw parent and child response must be preserved and charged exactly once",
    );
  for (const record of history.rawResponses) {
    const actual = raw.get(`${record.threadId}\0${record.responseId}`);
    if (
      !actual ||
      actual.turnId !== record.turnId ||
      time(record.atUtc) < time(actual.atUtc) ||
      time(record.atUtc) > time(actual.callbackBeforeUtc) ||
      !sameUsage(actual.usage, record.usage)
    )
      throw new Error(
        "Response history differs from original native response ownership, callback boundary or cost",
      );
  }
  const readback = JSON.parse(
    (await pinnedNonDispatchBytes(review.readback, 16_777_216)).toString("utf8"),
  );
  if (
    readback.schema !== "codex.qualification.owned-native-readback.v1" ||
    readback.modelCalls !== 0 ||
    !Array.isArray(readback.results) ||
    readback.results.length !== ledger.threads.length ||
    !Array.isArray(readback.protocol) ||
    readback.protocol.some(
      (row: any) =>
        row.direction === "out" && !["initialize", "thread/read"].includes(row.message?.method),
    )
  )
    throw new Error(
      "Owned liveness proof must use only exact read-only native thread observations",
    );
  for (const thread of ledger.threads) {
    const observed = readback.results.find(
      (value: any) => value.thread?.id === thread.threadId,
    )?.thread;
    if (
      !observed ||
      observed.cwd !== nativeCase.worktreeRoot ||
      !["notLoaded", "idle"].includes(observed.status?.type)
    )
      throw new Error("Every owned native thread must be observed without an active turn");
    const readRequest = readback.protocol.find(
      (row: any) =>
        row.direction === "out" &&
        row.message?.method === "thread/read" &&
        row.message.params?.threadId === thread.threadId &&
        row.message.params.includeTurns === true,
    );
    if (
      !readRequest ||
      !readback.protocol.some(
        (row: any) =>
          row.direction === "in" &&
          row.message?.id === readRequest.message.id &&
          JSON.stringify(row.message.result?.thread) === JSON.stringify(observed),
      )
    )
      throw new Error("Owned thread readback is not supported by its exact native RPC response");
    if (
      thread.parentThreadId &&
      observed.source?.subAgent?.thread_spawn?.parent_thread_id !== thread.parentThreadId
    )
      throw new Error("Native child readback lineage differs from its admitted parent");
    const totals = zero();
    for (const record of raw.values())
      if (record.threadId === thread.threadId)
        for (const key of usageKeys) totals[key] = (totals[key] ?? 0) + (record.usage[key] ?? 0);
    const delta = zero();
    for (const key of usageKeys)
      delta[key] = (thread.last!.cumulative[key] ?? 0) - (thread.baseline[key] ?? 0);
    if (
      !sameUsage(totals, delta) ||
      time(thread.last!.atUtc) <
        Math.max(
          ...[...raw.values()]
            .filter((record) => record.threadId === thread.threadId)
            .map((record) => time(record.atUtc)),
        )
    )
      throw new Error(
        "Ledger own-thread cumulative cost differs from all unique original raw responses",
      );
  }
  for (const request of ledger.requests) {
    const terminal = rows.find(
      (row) =>
        row.direction === "in" &&
        row.message?.method === "turn/completed" &&
        row.message.params?.threadId === request.threadId &&
        row.message.params.turn?.id === request.turnId,
    );
    if (
      !terminal ||
      terminal.message.params.turn.status !== request.outcome ||
      time(terminal.atUtc) < time(request.dispatchedAtUtc!) ||
      time(request.reconciledAtUtc!) < time(terminal.atUtc) ||
      time(readback.atUtc) < time(terminal.atUtc)
    )
      throw new Error(
        "Admitted native request lacks its exact terminal and later liveness boundary",
      );
    const observed = readback.results.find(
      (value: any) => value.thread.id === request.threadId,
    ).thread;
    if (
      !observed.turns?.some(
        (turn: any) => turn.id === request.turnId && turn.status === request.outcome,
      )
    )
      throw new Error("Native readback does not preserve the admitted turn terminal");
    if (
      [...raw.values()].some(
        (record) =>
          record.threadId === request.threadId &&
          record.turnId === request.turnId &&
          (time(record.atUtc) < time(request.dispatchedAtUtc!) ||
            time(record.atUtc) > time(terminal.atUtc)),
      )
    )
      throw new Error("Native response lies outside the actual admitted turn boundary");
    if (request.outcome === "interrupted") {
      const interrupt = rows.find(
        (row) =>
          row.direction === "out" &&
          row.message?.method === "turn/interrupt" &&
          row.message.params?.threadId === request.threadId &&
          row.message.params.turnId === request.turnId,
      );
      if (
        !interrupt ||
        !rows.some(
          (row) =>
            row.direction === "in" &&
            row.message?.id === interrupt.message.id &&
            row.message.result !== undefined &&
            time(row.atUtc) <= time(terminal.atUtc),
        )
      )
        throw new Error(
          "Interrupted boundary lacks the exact owned native interrupt acknowledgement",
        );
    }
    if (request.admissionClass === "observed-unreserved-effect") {
      const parent = ledger.requests.find((candidate) => candidate.id === request.parentRequestId);
      if (
        !parent ||
        !rows.some(
          (row) =>
            row.direction === "in" &&
            row.message?.method === "item/completed" &&
            row.message.params?.threadId === parent.threadId &&
            row.message.params.turnId === parent.turnId &&
            row.message.params.item?.type === "subAgentActivity" &&
            row.message.params.item.kind === "started" &&
            row.message.params.item.id === request.nativeToolUseId &&
            row.message.params.item.agentThreadId === request.threadId &&
            time(row.atUtc) >= time(request.dispatchedAtUtc!) &&
            time(row.atUtc) <= time(request.discoveredAtUtc!),
        )
      )
        throw new Error(
          "Observed unreserved child lacks actual source-owned native spawn correlation",
        );
      if (request.observedEffectEvidenceRef !== review.protocol.path) {
        const pin = review.sourcePins.find((pin) => pin.path === request.observedEffectEvidenceRef);
        const observedReview = JSON.parse(
            (await pinnedNonDispatchBytes(pin!, 2_097_152)).toString("utf8"),
          ),
          effect = observedReview.actualOwnedEvidence;
        if (
          observedReview.schema !==
            "codex.native-evaluation.observed-child-accounting-source-review.v1" ||
          observedReview.status !== "passed-scoped" ||
          observedReview.suiteId !== ledger.suiteId ||
          effect?.parentThreadId !== parent.threadId ||
          effect.parentRequestId !== parent.id ||
          effect.childThreadId !== request.threadId ||
          effect.childTurnId !== request.turnId ||
          effect.nativeToolUseId !== request.nativeToolUseId ||
          effect.invocationAtUtc !== request.dispatchedAtUtc ||
          !observedReview.evidencePins?.some(
            (pin: any) =>
              pin.path === review.protocol.path && pin.sha256 === review.protocol.sha256,
          )
        )
          throw new Error(
            "Observed child accounting provenance does not match its original native effect",
          );
      }
    }
  }
  const total = [...raw.values()].reduce((sum, record) => sum + record.usage.totalTokens, 0);
  if (total !== summary.aggregateTokens)
    throw new Error("Owned parent and child emitted costs are not retained in the suite total");
  if (
    input.resolution &&
    (input.resolution.ledgerBeforeSha256 !== input.beforeSha256 ||
      JSON.stringify(input.resolution.protocol) !== JSON.stringify(review.protocol) ||
      JSON.stringify(input.resolution.responseHistory) !== JSON.stringify(review.responseHistory) ||
      JSON.stringify(input.resolution.readback) !== JSON.stringify(review.readback))
  )
    throw new Error("Owned boundary resolution pins differ from their reviewed proof");
  return {
    unknownIds: allowed.map((row) => row.id),
    protocol: review.protocol,
    responseHistory: review.responseHistory,
    readback: review.readback,
  };
}

/** Read-only proof verification, useful before a separately authorized owner CAS. */
export async function verifyEvaluationOwnedNativeBoundaryReview(
  path: string,
  options: { reviewPin: { path: string; sha256: string } },
) {
  const before = await readEvaluationLedger(path);
  const proof = await verifyOwnedNativeBoundaryReview(options.reviewPin, {
    before: before.ledger,
    beforeSha256: before.sha256,
    ledgerPath: path,
  });
  return {
    ...proof,
    ledgerSha256: before.sha256,
    summary: ledgerSummary(before.ledger),
    qualificationVerified: false as const,
  };
}

/** Resolve only scoped deadline/accounting-gap holds after all actual native effects and costs are independently known. */
export async function resolveEvaluationOwnedNativeBoundaryHolds(
  path: string,
  options: { reviewPin: { path: string; sha256: string }; now?: Date },
): Promise<{ ledger: EvaluationLedger; sha256: string }> {
  const before = await readEvaluationLedger(path),
    proof = await verifyOwnedNativeBoundaryReview(options.reviewPin, {
      before: before.ledger,
      beforeSha256: before.sha256,
      ledgerPath: path,
    });
  return updateEvaluationLedger(
    path,
    (ledger) => {
      for (const unknown of ledger.unknowns.filter((row) => proof.unknownIds.includes(row.id))) {
        unknown.resolvedAtUtc = (options.now ?? new Date()).toISOString();
        unknown.resolution = {
          class: "verified-owned-native-boundary",
          review: options.reviewPin,
          protocol: proof.protocol,
          responseHistory: proof.responseHistory,
          readback: proof.readback,
          ledgerBeforeSha256: before.sha256,
        };
      }
      return ledger;
    },
    { expectedSha256: before.sha256 },
  );
}

function assertRejectedRequestResolutionOnly(
  before: EvaluationLedger,
  after: EvaluationLedger,
  requestId: string,
  nativeRejectedAtUtc: string,
): void {
  if (
    after.status !== "active" ||
    after.requests.length !== before.requests.length ||
    JSON.stringify(after.threads) !== JSON.stringify(before.threads) ||
    JSON.stringify(after.observations) !== JSON.stringify(before.observations) ||
    after.unknowns.length !== before.unknowns.length ||
    aggregateTokens(before) !== aggregateTokens(after)
  )
    throw new Error(
      "Native request rejection cannot change suite effects, costs, lineage or closure",
    );
  for (let index = 0; index < before.requests.length; index++) {
    const old = before.requests[index]!,
      current = after.requests[index]!;
    if (old.id !== requestId) {
      if (JSON.stringify(old) !== JSON.stringify(current))
        throw new Error("Native rejection cannot change any other counted request");
      continue;
    }
    const restored = structuredClone(current);
    restored.status = old.status;
    delete restored.outcome;
    delete restored.rejection;
    delete restored.rejectedAtUtc;
    delete restored.rejectionRecordedAtUtc;
    if (
      current.status !== "cancelled" ||
      current.outcome !== "rejected-before-execution" ||
      current.rejectedAtUtc !== nativeRejectedAtUtc ||
      JSON.stringify(restored) !== JSON.stringify(old)
    )
      throw new Error(
        "Rejected native attempt must retain all original admission and RPC attempt fields",
      );
  }
}
async function verifyNativeRequestRejectionReview(
  reviewPin: { path: string; sha256: string },
  input: {
    before: EvaluationLedger;
    beforeSha256: string;
    ledgerPath: string;
    unknownId?: string;
    resolution?: NativeRequestRejectionResolution;
  },
) {
  const ledger = input.before,
    review: NativeRequestRejectionReview = JSON.parse(
      (await pinnedNonDispatchBytes(reviewPin, 1_048_576)).toString("utf8"),
    );
  if (
    review.schema !== "codex.qualification.native-request-rejection-review.v1" ||
    review.status !== "passed-scoped" ||
    review.verificationClass !== "exact-owned-native-request-rejection" ||
    !review.reviewer ||
    review.actualQualificationVerified !== false ||
    ledger.status !== "active" ||
    review.suiteId !== ledger.suiteId ||
    review.ledger?.path !== input.ledgerPath ||
    review.ledger.sha256 !== input.beforeSha256 ||
    review.ledger.startedAtUtc !== ledger.startedAtUtc ||
    review.ledger.requestCount !== ledger.requests.length ||
    review.ledger.aggregateTokens !== aggregateTokens(ledger) ||
    JSON.stringify(review.ledger.caps) !== JSON.stringify(ledger.caps)
  )
    throw new Error("Native rejection review must match its exact counted ledger predecessor");
  const request = ledger.requests.find((row) => row.id === review.requestId),
    parent = ledger.requests.find((row) => row.id === request?.parentRequestId);
  if (
    !request ||
    !["dispatched", "usage-unknown"].includes(request.status) ||
    request.kind !== "child" ||
    request.nativeExecution !== "app-server-direct-child-followup" ||
    !request.reservedAtUtc ||
    !request.dispatchedAtUtc ||
    !request.threadId ||
    request.turnId !== undefined ||
    request.outcome !== undefined ||
    request.reconciledAtUtc !== undefined ||
    request.rejection !== undefined ||
    request.dispatchEvidenceRef !== review.protocol.path ||
    !parent?.threadId ||
    request.nativeParentThreadId !== parent.threadId ||
    ledger.requests.some(
      (row) => row.id !== request.id && !["reconciled", "cancelled"].includes(row.status),
    ) ||
    ledger.threads.some(
      (thread) =>
        thread.status !== "known" ||
        !thread.last ||
        time(thread.last.atUtc) >= time(request.dispatchedAtUtc!),
    )
  )
    throw new Error(
      "Native rejection resolution requires only one unexecuted direct-child RPC attempt with known prior costs",
    );
  const nativeCase = JSON.parse(
    (await pinnedNonDispatchBytes(review.case, 1_048_576)).toString("utf8"),
  );
  if (
    nativeCase.projectId !== ledger.projectId ||
    nativeCase.caseId !== review.caseId ||
    nativeCase.workId !== review.workId ||
    nativeCase.ledgerPath !== input.ledgerPath ||
    nativeCase.checkpointPath !== review.case.checkpointPath ||
    ledger.suiteId !== `codex-continuity-qualification-${nativeCase.caseId}`
  )
    throw new Error("Native rejection review is outside the exact owning Case");
  await pinnedNonDispatchBytes(
    { path: review.case.checkpointPath, sha256: review.case.checkpointSha256 },
    2_097_152,
  );
  if (!Array.isArray(review.sourcePins) || !review.sourcePins.length)
    throw new Error("Native rejection requires original source and result pins");
  for (const pin of review.sourcePins) await pinnedNonDispatchBytes(pin, 2_097_152);
  const permissions = review.reconciliation,
    allowed = permissions?.allowedUnknowns;
  if (
    !Array.isArray(allowed) ||
    allowed.length !== 3 ||
    new Set(allowed.map((row) => row.id)).size !== allowed.length ||
    permissions.physicalUnknownDeletionAllowed !== false ||
    permissions.suiteReplacementAllowed !== false ||
    permissions.closedSuiteReopenAllowed !== false ||
    permissions.countedAttemptRemovalAllowed !== false ||
    permissions.emittedCostChangeAllowed !== false
  )
    throw new Error("Native rejection may only resolve its three exact retained attempt holds");
  const expectedError = {
    code: -32600,
    message: "direct app-server input is not allowed for multi-agent v2 sub-agents",
  };
  if (JSON.stringify(review.nativeError) !== JSON.stringify(expectedError))
    throw new Error("Unsupported native rejection is not a pre-execution proof");
  let requestHold = false,
    fallbackHold = false,
    rpcHold = false;
  for (const permission of allowed) {
    const row = ledger.unknowns.find((row) => row.id === permission.id);
    if (
      !row ||
      row.resolvedAtUtc ||
      row.reason !== permission.reason ||
      row.evidenceRef !== permission.evidenceRef ||
      row.evidenceRef !== review.protocol.path
    )
      throw new Error("Native rejection hold provenance drifted");
    if (
      row.id === request.id &&
      row.reason === "Native execution outcome or emitted usage is unresolved"
    )
      requestHold = true;
    else if (
      row.id.startsWith("unknown-") &&
      row.reason === "Admitted child request does not have exactly one owned native turn"
    )
      fallbackHold = true;
    else if (
      row.id.startsWith("qualification-hold-") &&
      row.reason === `Error: Native RPC ${JSON.stringify(expectedError)}`
    )
      rpcHold = true;
    else
      throw new Error(
        "A true execution usage unknown or unrelated hold cannot use native rejection resolution",
      );
  }
  if (
    !requestHold ||
    !fallbackHold ||
    !rpcHold ||
    (input.unknownId !== undefined && !allowed.some((row) => row.id === input.unknownId))
  )
    throw new Error(
      "Native rejection must identify its exact request, empty fallback and rejected RPC holds",
    );
  const rows = nativeWireRows(await pinnedNonDispatchBytes(review.protocol, 64 * 1024 * 1024)),
    outbound = rows.filter((row) => row.direction === "out");
  if (
    outbound.some(
      (row) =>
        ![
          "initialize",
          "config/read",
          "hooks/list",
          "thread/resume",
          "thread/read",
          "turn/start",
        ].includes(row.message?.method),
    ) ||
    outbound.filter((row) => row.message?.method === "turn/start").length !== 1 ||
    rows.some(
      (row) =>
        row.direction === "in" &&
        /^(?:turn\/|rawResponse\/|rawResponseItem\/|thread\/tokenUsage\/|item\/)/.test(
          row.message?.method ?? "",
        ),
    )
  )
    throw new Error(
      "Native rejection protocol contains actual model, tool, usage or another possible dispatch effect",
    );
  const rpc = outbound.find((row) => row.message?.method === "turn/start"),
    error = rows.find(
      (row) =>
        row.direction === "in" &&
        row.message?.id === review.nativeRpcRequestId &&
        row.message.error,
    );
  if (
    !rpc ||
    rpc.message.id !== review.nativeRpcRequestId ||
    rpc.message.params?.threadId !== request.threadId ||
    rpc.message.params.cwd !== nativeCase.worktreeRoot ||
    time(rpc.atUtc) < time(request.dispatchedAtUtc) ||
    JSON.stringify(error?.message.error) !== JSON.stringify(expectedError) ||
    time(error.atUtc) < time(rpc.atUtc) ||
    rows.some(
      (row) =>
        row.direction === "in" &&
        row.message?.id === rpc.message.id &&
        row.message.result !== undefined,
    )
  )
    throw new Error("Exact native child turn/start rejection is not established by original wire");
  for (const row of outbound.filter((row) =>
    ["thread/resume", "thread/read"].includes(row.message?.method),
  ))
    if (
      !ledger.threads.some((thread) => thread.threadId === row.message.params?.threadId) ||
      (row.message.method === "thread/resume" && row.message.params.cwd !== nativeCase.worktreeRoot)
    )
      throw new Error("Rejected attempt includes native activity outside the exact owned threads");
  let childRead: any;
  for (const thread of ledger.threads) {
    const reads = rows.filter(
      (row) =>
        row.direction === "in" &&
        row.message?.result?.thread?.id === thread.threadId &&
        time(row.atUtc) > time(error.atUtc) &&
        outbound.some(
          (request) =>
            request.message?.id === row.message.id &&
            request.message.method === "thread/read" &&
            request.message.params?.includeTurns === true,
        ),
    );
    const read = reads.at(-1)?.message.result.thread;
    if (
      !read ||
      read.cwd !== nativeCase.worktreeRoot ||
      read.parentThreadId !== (thread.parentThreadId ?? null) ||
      !["idle", "notLoaded"].includes(read.status?.type) ||
      !Array.isArray(read.turns) ||
      read.turns.some(
        (turn: any) =>
          !["completed", "interrupted", "failed"].includes(turn.status) ||
          !ledger.requests.some(
            (row) =>
              row.id !== request.id &&
              row.threadId === thread.threadId &&
              row.turnId === turn.id &&
              row.outcome === turn.status,
          ),
      )
    )
      throw new Error(
        "Rejected request requires exact later idle readback without any new owned turn",
      );
    if (thread.threadId === request.threadId) childRead = read;
  }
  const history = JSON.parse(
      (await pinnedNonDispatchBytes(review.responseHistory, 4_194_304)).toString("utf8"),
    ),
    prior = JSON.parse(
      (await pinnedNonDispatchBytes(review.priorResponseHistory, 4_194_304)).toString("utf8"),
    );
  if (
    history.caseId !== review.caseId ||
    prior.caseId !== review.caseId ||
    JSON.stringify(history.rawProtocol) !== JSON.stringify(review.protocol) ||
    !Array.isArray(history.rawResponses) ||
    !Array.isArray(prior.rawResponses) ||
    !Array.isArray(history.chargedResponseKeys) ||
    !Array.isArray(prior.chargedResponseKeys)
  )
    throw new Error(
      "Rejected request response history must preserve its exact pre-attempt source usage",
    );
  const existingRecords = (records: any[]) => {
    const result = new Map<string, any>();
    for (const record of records) {
      validateUsage(record.usage);
      if (
        !record.threadId ||
        !record.responseId ||
        time(record.atUtc) >= time(request.dispatchedAtUtc!) ||
        !ledger.requests.some(
          (row) =>
            row.id !== request.id &&
            row.threadId === record.threadId &&
            row.turnId === record.turnId &&
            row.status === "reconciled",
        )
      )
        throw new Error(
          "Rejected request history contains unaccounted or post-attempt native usage",
        );
      const key = `${record.threadId}\0${record.responseId}`,
        old = result.get(key);
      if (old && (old.turnId !== record.turnId || !sameUsage(old.usage, record.usage)))
        throw new Error("Rejected request history contains conflicting old response records");
      if (!old) result.set(key, record);
    }
    return result;
  };
  const oldRecords = existingRecords(prior.rawResponses),
    currentRecords = existingRecords(history.rawResponses);
  if (
    !oldRecords.size ||
    oldRecords.size !== currentRecords.size ||
    history.chargedResponseKeys.length !== oldRecords.size ||
    prior.chargedResponseKeys.length !== oldRecords.size ||
    new Set(history.chargedResponseKeys).size !== oldRecords.size ||
    new Set(prior.chargedResponseKeys).size !== oldRecords.size ||
    [...oldRecords].some(
      ([key, old]) =>
        !history.chargedResponseKeys.includes(key) ||
        !prior.chargedResponseKeys.includes(key) ||
        !currentRecords.has(key) ||
        currentRecords.get(key).turnId !== old.turnId ||
        !sameUsage(currentRecords.get(key).usage, old.usage),
    )
  )
    throw new Error(
      "Rejected request must retain every prior charged response exactly once without new cost",
    );
  for (const thread of ledger.threads) {
    const total = zero();
    for (const record of currentRecords.values())
      if (record.threadId === thread.threadId)
        for (const key of usageKeys) total[key] = (total[key] ?? 0) + (record.usage[key] ?? 0);
    const delta = zero();
    for (const key of usageKeys)
      delta[key] = (thread.last!.cumulative[key] ?? 0) - (thread.baseline[key] ?? 0);
    if (!sameUsage(total, delta))
      throw new Error("Rejected request changed the retained actual emitted thread cost");
  }
  const rollout = nativeWireRows(await pinnedNonDispatchBytes(review.ownedChildRollout, 8_388_608)),
    header = rollout[0];
  if (
    header.type !== "session_meta" ||
    header.payload?.id !== request.threadId ||
    header.payload.cwd !== nativeCase.worktreeRoot ||
    header.payload.session_id !== childRead.sessionId ||
    header.payload.parent_thread_id !== parent.threadId
  )
    throw new Error("Rejected request lacks its exact owned child rollout identity");
  for (const row of rollout.filter((row) => row.type === "token_usage_record")) {
    if (
      row.payload?.thread_id !== request.threadId ||
      row.payload.session_id !== childRead.sessionId ||
      time(row.timestamp) >= time(request.dispatchedAtUtc)
    )
      throw new Error(
        "Owned child rollout contains new or foreign usage after the rejected RPC attempt",
      );
    const record = oldRecords.get(`${request.threadId}\0${row.payload.response_id}`),
      u = row.payload.usage;
    if (
      !record ||
      record.turnId !== row.payload.turn_id ||
      !sameUsage(record.usage, {
        totalTokens: u.total_tokens,
        inputTokens: u.input_tokens,
        cachedInputTokens: u.cached_input_tokens,
        outputTokens: u.output_tokens,
        reasoningOutputTokens: u.reasoning_output_tokens,
        cacheWriteInputTokens: u.cache_write_input_tokens ?? 0,
      })
    )
      throw new Error(
        "Rejected RPC child rollout differs from already preserved own-response usage",
      );
  }
  if (
    input.resolution &&
    (input.resolution.requestId !== request.id ||
      input.resolution.ledgerBeforeSha256 !== input.beforeSha256 ||
      JSON.stringify(input.resolution.protocol) !== JSON.stringify(review.protocol) ||
      JSON.stringify(input.resolution.responseHistory) !== JSON.stringify(review.responseHistory))
  )
    throw new Error("Native request rejection proof pins changed");
  return {
    requestId: request.id,
    unknownIds: allowed.map((row) => row.id),
    protocol: review.protocol,
    responseHistory: review.responseHistory,
    rejectedAtUtc: error.atUtc,
  };
}
export async function verifyEvaluationNativeRequestRejection(
  path: string,
  options: { reviewPin: { path: string; sha256: string } },
) {
  const before = await readEvaluationLedger(path),
    proof = await verifyNativeRequestRejectionReview(options.reviewPin, {
      before: before.ledger,
      beforeSha256: before.sha256,
      ledgerPath: path,
    });
  return {
    ...proof,
    ledgerSha256: before.sha256,
    summary: ledgerSummary(before.ledger),
    newExecutionCost: 0,
    qualificationVerified: false as const,
  };
}
/** Terminalize a counted, explicitly rejected native RPC attempt; no request count or actual cost is recycled. */
export async function resolveEvaluationNativeRequestRejection(
  path: string,
  options: { reviewPin: { path: string; sha256: string }; now?: Date },
): Promise<{ ledger: EvaluationLedger; sha256: string }> {
  const before = await readEvaluationLedger(path),
    proof = await verifyNativeRequestRejectionReview(options.reviewPin, {
      before: before.ledger,
      beforeSha256: before.sha256,
      ledgerPath: path,
    });
  const atUtc = (options.now ?? new Date()).toISOString();
  if (time(atUtc) < time(proof.rejectedAtUtc))
    throw new Error("Rejection resolution timestamp precedes actual native rejection");
  return updateEvaluationLedger(
    path,
    (ledger) => {
      const resolution: NativeRequestRejectionResolution = {
        class: "verified-native-request-rejection",
        requestId: proof.requestId,
        review: options.reviewPin,
        protocol: proof.protocol,
        responseHistory: proof.responseHistory,
        ledgerBeforeSha256: before.sha256,
      };
      Object.assign(
        ledger.requests.find((row) => row.id === proof.requestId)!,
        {
          status: "cancelled",
          outcome: "rejected-before-execution",
          rejectedAtUtc: proof.rejectedAtUtc,
          rejectionRecordedAtUtc: atUtc,
          rejection: resolution,
        },
      );
      for (const row of ledger.unknowns.filter((row) => proof.unknownIds.includes(row.id))) {
        row.resolvedAtUtc = atUtc;
        row.resolution = resolution;
      }
      return ledger;
    },
    { expectedSha256: before.sha256 },
  );
}
