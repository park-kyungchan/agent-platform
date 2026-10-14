import {
  admitEvaluationRun,
  normalizeEvaluationUsage,
  reconcileEvaluationRun,
  validateEvaluationBudget,
  type EvaluationBudgetLedger,
  type EvaluationBudgetSummary,
  type EvaluationRunKind,
  type EvaluationRunStatus,
  type EvaluationTokenUsage,
} from "../adoption/evaluation-budget.ts";
import {
  exact,
  record,
  same,
  text,
  type LunaExecutionScope,
  type ModelSelection,
} from "./contracts.ts";
import { stableJson } from "./bot-correlation.ts";
import { CONTROLLED_MODEL_POLICY, validateLunaScope, validateModelNecessity } from "./policy.ts";

/** Counts all emitted input and output tokens with provider-native inclusive usage semantics. */
export const NATIVE_SUITE_LIMITS = Object.freeze({
  maxRuns: 48,
  maxAggregateTokens: 2_000_000,
  maxDurationSeconds: 8 * 60 * 60,
  includesChildRuns: true,
} as const);
export interface RequestAdmission {
  admissionId: string;
  taskId: string;
  profileId: string;
  kind: EvaluationRunKind;
  parentAdmissionId?: string;
  reservedInputTokens: number;
  reservedOutputTokens: number;
  selection: ModelSelection;
  lunaScope?: LunaExecutionScope;
  /** Alias resolution is a helper, never an entitlement/context-window proof. */
  expectedWireModel: "gpt-6.1-sol" | "gpt-6-luna";
  deterministicUtilityConsidered: true;
  requiresModelReason: string;
}
export interface NativeRequestObservation {
  admissionId: string;
  atUtc: string;
  nativeRequestId: string;
  nativeRunId: string;
  taskId: string;
  profileId: string;
  status: Exclude<EvaluationRunStatus, "admitted">;
  /** Missing/partial telemetry must never be treated as zero consumption. */
  usageCompleteness: "complete" | "partial" | "unknown";
  usage: EvaluationTokenUsage | null;
  effectiveWireModel: string | null;
  effectiveProvider: string | null;
  effectiveEffort: string | null;
  sourceRef: string;
}
export interface NativeEvaluationSuite {
  suiteId: string;
  workId: string;
  ownerId: string;
  closedPredecessorSuiteIds: string[];
  ledger: EvaluationBudgetLedger;
  requests: RequestAdmission[];
  requestObservations: NativeRequestObservation[];
  policyViolations: string[];
}
function validateAdmission(request: RequestAdmission): void {
  const r = record(request, "admission");
  exact(
    r,
    [
      "admissionId",
      "taskId",
      "profileId",
      "kind",
      "parentAdmissionId",
      "reservedInputTokens",
      "reservedOutputTokens",
      "selection",
      "lunaScope",
      "expectedWireModel",
      "deterministicUtilityConsidered",
      "requiresModelReason",
    ],
    "admission",
  );
  for (const key of ["admissionId", "taskId", "profileId"]) text(r[key], key);
  if (request.parentAdmissionId !== undefined) text(request.parentAdmissionId, "parentAdmissionId");
  validateModelNecessity({
    deterministicUtilityConsidered: request.deterministicUtilityConsidered,
    requiresModelReason: request.requiresModelReason,
  });
  const selection = record(request.selection, "selection");
  exact(selection, ["model", "provider", "effort"], "selection");
  const expected =
    request.selection.model === CONTROLLED_MODEL_POLICY.primary.model
      ? CONTROLLED_MODEL_POLICY.primary
      : CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker;
  for (const key of ["model", "provider", "effort"] as const)
    same(selection[key], expected[key], key);
  if (expected.model === "gpt-6-luna-900k") validateLunaScope(request.lunaScope);
  else if (request.lunaScope !== undefined)
    throw new Error("Primary budget admission cannot carry Luna policy overrides");
  same(
    request.expectedWireModel,
    expected.model === "gpt-6.1-sol-900k" ? "gpt-6.1-sol" : "gpt-6-luna",
    "expectedWireModel",
  );
  if (request.kind !== "primary" && !request.parentAdmissionId)
    throw new Error("Every child, retry and auxiliary request requires its admitted lineage");
}
function validateNativeObservation(value: NativeRequestObservation): void {
  const o = record(value, "requestObservation");
  exact(
    o,
    [
      "admissionId",
      "atUtc",
      "nativeRequestId",
      "nativeRunId",
      "taskId",
      "profileId",
      "status",
      "usageCompleteness",
      "usage",
      "effectiveWireModel",
      "effectiveProvider",
      "effectiveEffort",
      "sourceRef",
    ],
    "requestObservation",
  );
  for (const key of [
    "admissionId",
    "nativeRequestId",
    "nativeRunId",
    "taskId",
    "profileId",
    "sourceRef",
  ])
    text(o[key], key);
  if (!["running", "completed", "failed", "cancelled", "uncertain"].includes(value.status))
    throw new Error("Invalid native request status");
  if (!["complete", "partial", "unknown"].includes(value.usageCompleteness))
    throw new Error("Explicit usage completeness required");
  for (const key of ["effectiveWireModel", "effectiveProvider", "effectiveEffort"] as const)
    if (value[key] !== null) text(value[key], key);
  if (value.usage !== null) normalizeEvaluationUsage(value.usage);
  if (value.usageCompleteness === "complete" && value.usage === null)
    throw new Error("Complete usage requires counters");
}
export function validateNativeEvaluationSuite(
  suite: NativeEvaluationSuite,
  nowUtc: string,
): EvaluationBudgetSummary {
  const s = record(suite, "suite");
  exact(
    s,
    [
      "suiteId",
      "workId",
      "ownerId",
      "closedPredecessorSuiteIds",
      "ledger",
      "requests",
      "requestObservations",
      "policyViolations",
    ],
    "suite",
  );
  for (const key of ["suiteId", "workId", "ownerId"]) text(s[key], key);
  if (
    !Array.isArray(suite.closedPredecessorSuiteIds) ||
    !Array.isArray(suite.requests) ||
    !Array.isArray(suite.requestObservations) ||
    !Array.isArray(suite.policyViolations)
  )
    throw new Error("Suite histories required");
  for (const id of suite.closedPredecessorSuiteIds) text(id, "closedPredecessorSuiteId");
  if (suite.closedPredecessorSuiteIds.includes(suite.suiteId))
    throw new Error("Closed predecessor budget cannot be reopened");
  const summary = validateEvaluationBudget(NATIVE_SUITE_LIMITS, suite.ledger, nowUtc);
  if (suite.requests.length !== suite.ledger.runs.length)
    throw new Error("Every model request must have one budget admission");
  for (let index = 0; index < suite.requests.length; index++) {
    const request = suite.requests[index]!,
      run = suite.ledger.runs[index]!;
    validateAdmission(request);
    same(request.admissionId, run.runId, "request admission");
    same(request.kind, run.kind, "request kind");
    same(request.parentAdmissionId, run.parentRunId, "request parent");
    same(request.reservedInputTokens, run.reservedInputTokens, "reservedInputTokens");
    same(request.reservedOutputTokens, run.reservedOutputTokens, "reservedOutputTokens");
  }
  for (const run of suite.ledger.runs) {
    const observations = suite.requestObservations.filter((o) => o.admissionId === run.runId);
    if (observations.length !== run.observations.length)
      throw new Error("Native receipt and accounting histories differ");
    let usage: EvaluationTokenUsage | null = null;
    for (let index = 0; index < observations.length; index++) {
      const native = observations[index]!,
        accounting = run.observations[index]!;
      if (native.usage !== null) usage = native.usage;
      same(native.atUtc, accounting.atUtc, "request observation time");
      if (stableJson(usage) !== stableJson(accounting.observedUsage))
        throw new Error("Native usage differs from accounting observation");
    }
  }
  const nativeIdentities = new Map<string, string>();
  for (const observation of suite.requestObservations) {
    validateNativeObservation(observation);
    const request = suite.requests.find((r) => r.admissionId === observation.admissionId);
    if (!request) throw new Error("Observation refers to unadmitted model request");
    for (const [key, actual, expected] of [
      ["taskId", observation.taskId, request.taskId],
      ["profileId", observation.profileId, request.profileId],
      ["effectiveWireModel", observation.effectiveWireModel, request.expectedWireModel],
      ["effectiveProvider", observation.effectiveProvider, request.selection.provider],
      ["effectiveEffort", observation.effectiveEffort, request.selection.effort],
    ] as const) {
      if (
        actual !== null &&
        actual !== expected &&
        !suite.policyViolations.includes(`${request.admissionId}: ${key} mismatch`)
      )
        throw new Error("Persisted native policy violation missing from ledger: " + key);
    }
    const first = suite.requestObservations.find((o) => o.admissionId === observation.admissionId)!;
    same(first.nativeRequestId, observation.nativeRequestId, "Persisted nativeRequestId");
    same(first.nativeRunId, observation.nativeRunId, "Persisted nativeRunId");
    const key = JSON.stringify([observation.effectiveProvider, observation.nativeRequestId]);
    const existing = nativeIdentities.get(key);
    if (existing && existing !== observation.admissionId)
      throw new Error("Native model request counted against multiple reservations");
    nativeIdentities.set(key, observation.admissionId);
  }
  for (const violation of suite.policyViolations) text(violation, "policyViolation");
  const unknownAdmissionIds = new Set(
    suite.ledger.runs.filter((r) => r.usageState === "unknown").map((r) => r.runId),
  );
  for (const request of suite.requests) {
    const latest = suite.requestObservations
      .filter((o) => o.admissionId === request.admissionId)
      .at(-1);
    if (
      latest &&
      (latest.usageCompleteness !== "complete" ||
        latest.effectiveWireModel === null ||
        latest.effectiveProvider === null ||
        latest.effectiveEffort === null)
    )
      unknownAdmissionIds.add(request.admissionId);
  }
  return {
    ...summary,
    unknownUsageRuns: unknownAdmissionIds.size,
    admissionBlockedReasons: [
      ...new Set([
        ...summary.admissionBlockedReasons,
        ...(unknownAdmissionIds.size ? ["unknown-usage" as const] : []),
      ]),
    ],
  };
}
export function createNativeEvaluationSuite(
  suiteId: string,
  workId: string,
  ownerId: string,
  closedPredecessorSuiteIds: string[],
  nowUtc: string,
): NativeEvaluationSuite {
  const suite: NativeEvaluationSuite = {
    suiteId,
    workId,
    ownerId,
    closedPredecessorSuiteIds: [...closedPredecessorSuiteIds],
    ledger: { startedAtUtc: null, finishedAtUtc: null, runs: [] },
    requests: [],
    requestObservations: [],
    policyViolations: [],
  };
  validateNativeEvaluationSuite(suite, nowUtc);
  return suite;
}
/** Persist the resulting owner ledger with CAS before dispatching any request. */
export function reserveNativeModelRequest(
  suite: NativeEvaluationSuite,
  request: RequestAdmission,
  nowUtc: string,
): NativeEvaluationSuite {
  const summary = validateNativeEvaluationSuite(suite, nowUtc);
  validateAdmission(request);
  if (suite.policyViolations.length)
    throw new Error("Additional dispatch blocked by unresolved native policy violation");
  if (summary.admissionBlockedReasons.length)
    throw new Error("Evaluation admission blocked: " + summary.admissionBlockedReasons.join(", "));
  const ledger = admitEvaluationRun(
    NATIVE_SUITE_LIMITS,
    suite.ledger,
    {
      runId: request.admissionId,
      kind: request.kind,
      ...(request.parentAdmissionId ? { parentRunId: request.parentAdmissionId } : {}),
      reservedInputTokens: request.reservedInputTokens,
      reservedOutputTokens: request.reservedOutputTokens,
    },
    nowUtc,
  );
  const next = structuredClone({ ...suite, ledger, requests: [...suite.requests, request] });
  validateNativeEvaluationSuite(next, nowUtc);
  return next;
}
/** Always retains emitted usage, including overrun or misrouted execution; never dispatches/retries. */
export function reconcileNativeModelRequest(
  suite: NativeEvaluationSuite,
  observation: NativeRequestObservation,
  nowUtc: string,
): NativeEvaluationSuite {
  validateNativeEvaluationSuite(suite, nowUtc);
  validateNativeObservation(observation);
  const request = suite.requests.find((r) => r.admissionId === observation.admissionId);
  if (!request) throw new Error("Cannot reconcile unreserved model request");
  same(observation.atUtc, nowUtc, "observation.atUtc");
  const prior = suite.requestObservations.filter((o) => o.admissionId === observation.admissionId);
  if (prior.length && stableJson(prior.at(-1)) === stableJson(observation))
    return structuredClone(suite);
  for (const previous of prior) {
    same(previous.nativeRequestId, observation.nativeRequestId, "nativeRequestId");
    same(previous.nativeRunId, observation.nativeRunId, "nativeRunId");
  }
  const violations = [...suite.policyViolations];
  for (const [key, actual, expected] of [
    ["taskId", observation.taskId, request.taskId],
    ["profileId", observation.profileId, request.profileId],
    ["effectiveWireModel", observation.effectiveWireModel, request.expectedWireModel],
    ["effectiveProvider", observation.effectiveProvider, request.selection.provider],
    ["effectiveEffort", observation.effectiveEffort, request.selection.effort],
  ] as const) {
    if (actual !== null && actual !== expected)
      violations.push(`${request.admissionId}: ${key} ${actual === null ? "unknown" : "mismatch"}`);
  }
  const complete = observation.usageCompleteness === "complete";
  // Partial counters still preserve observed consumption; unknown state blocks new admissions.
  const previousStatus = suite.ledger.runs.find((r) => r.runId === request.admissionId)!.status;
  const previousTerminal = ["completed", "failed", "cancelled"].includes(previousStatus);
  const status = complete
    ? observation.status
    : previousTerminal
      ? (previousStatus as Exclude<EvaluationRunStatus, "admitted">)
      : "uncertain";
  const ledger = reconcileEvaluationRun(
    NATIVE_SUITE_LIMITS,
    suite.ledger,
    { runId: request.admissionId, status, usage: observation.usage },
    nowUtc,
  );
  const next = structuredClone({
    ...suite,
    ledger,
    policyViolations: [...new Set(violations)],
    requestObservations: [...suite.requestObservations, observation],
  });
  validateNativeEvaluationSuite(next, nowUtc);
  return next;
}
