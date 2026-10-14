/**
 * Pure admission/accounting over a caller-owned ledger. No dispatch, clocks, storage or retries.
 * Persist each returned ledger under one writer/lock before dispatch. Reservations are admission
 * estimates, not a provider hard cap: emitted usage can exceed them and must still be recorded.
 */
export interface EvaluationBudgetPolicy {
  maxRuns: number;
  maxAggregateTokens: number;
  maxDurationSeconds: number;
  includesChildRuns: true;
}

export type EvaluationRunKind = "primary" | "child" | "retry" | "auxiliary";
export type EvaluationRunStatus =
  "admitted" | "running" | "completed" | "failed" | "cancelled" | "uncertain";
export type EvaluationUsageState = "pending" | "reported" | "unknown";

/** Cumulative usage for this run only; exclude separately recorded child/retry/auxiliary runs. */
export interface EvaluationTokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  inputAccounting: "includes-cache" | "excludes-cache";
}

export interface EvaluationRunObservation {
  atUtc: string;
  status: Exclude<EvaluationRunStatus, "admitted">;
  usageState: EvaluationUsageState;
  observedUsage: EvaluationTokenUsage | null;
}

export interface EvaluationRunRecord {
  runId: string;
  kind: EvaluationRunKind;
  parentRunId?: string;
  admittedAtUtc: string;
  lastObservedAtUtc: string;
  reservedInputTokens: number;
  reservedOutputTokens: number;
  status: EvaluationRunStatus;
  usageState: EvaluationUsageState;
  observedUsage: EvaluationTokenUsage | null;
  observations: EvaluationRunObservation[];
}

export interface EvaluationBudgetTotals {
  admittedRuns: number;
  completedRuns: number;
  erroredRuns: number;
  unknownUsageRuns: number;
  aggregateTokens: number;
  /** Remaining reservation beyond observed tokens, not the original reservation counted twice. */
  reservedTokens: number;
}

export interface EvaluationBudgetLedger {
  startedAtUtc: string | null;
  finishedAtUtc: string | null;
  lastObservedAtUtc?: string | null;
  runs: EvaluationRunRecord[];
  /** A projection only. Validation and admission recompute totals from records. */
  totals?: EvaluationBudgetTotals;
  [metadata: string]: unknown;
}

export interface EvaluationAdmission {
  runId: string;
  kind: EvaluationRunKind;
  parentRunId?: string;
  reservedInputTokens: number;
  reservedOutputTokens: number;
}

export interface EvaluationReconciliation {
  runId: string;
  status: Exclude<EvaluationRunStatus, "admitted">;
  /** Null means no complete new usage observation, never zero consumption. */
  usage: EvaluationTokenUsage | null;
}

export interface EvaluationBudgetSummary extends EvaluationBudgetTotals {
  elapsedSeconds: number;
  deadlineAtUtc: string | null;
  accountedTokens: number;
  remainingRuns: number;
  remainingTokens: number;
  admissionBlockedReasons: (
    "suite-finished" | "run-cap" | "token-cap" | "deadline" | "unknown-usage"
  )[];
  overrun: { runs: boolean; tokens: boolean; duration: boolean };
}

const terminal = (status: EvaluationRunStatus) =>
  ["completed", "failed", "cancelled"].includes(status);
function count(value: unknown, label: string): asserts value is number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
    throw new Error(`Invalid nonnegative safe integer: ${label}`);
}
function sum(...values: number[]): number {
  const result = values.reduce((a, b) => a + b, 0);
  count(result, "aggregate counter");
  return result;
}
function timestamp(value: unknown, label: string): number {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/.test(value)
  )
    throw new Error(`Explicit UTC timestamp required: ${label}`);
  const parsed = Date.parse(value);
  const normalized = value.includes(".") ? value : value.replace("Z", ".000Z");
  if (!Number.isFinite(parsed) || new Date(parsed).toISOString() !== normalized)
    throw new Error(`Invalid timestamp: ${label}`);
  return parsed;
}
function identity(value: unknown, label: string): asserts value is string {
  if (typeof value !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,255}$/.test(value))
    throw new Error(`Invalid identifier: ${label}`);
}
function status(value: unknown): asserts value is EvaluationRunStatus {
  if (
    typeof value !== "string" ||
    !["admitted", "running", "completed", "failed", "cancelled", "uncertain"].includes(value)
  )
    throw new Error("Invalid run status");
}
function state(value: unknown): asserts value is EvaluationUsageState {
  if (typeof value !== "string" || !["pending", "reported", "unknown"].includes(value))
    throw new Error("Invalid usage state");
}

/** Cache categories are additive only when the caller explicitly reports exclusive input. */
export function normalizeEvaluationUsage(usage: EvaluationTokenUsage): {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
} {
  if (!usage || typeof usage !== "object") throw new Error("Usage object required");
  count(usage.inputTokens, "inputTokens");
  count(usage.outputTokens, "outputTokens");
  for (const key of ["cacheReadTokens", "cacheWriteTokens"] as const)
    if (usage[key] !== undefined) count(usage[key], key);
  if (!["includes-cache", "excludes-cache"].includes(usage.inputAccounting))
    throw new Error("Explicit cache accounting mode required");
  const cache = sum(usage.cacheReadTokens ?? 0, usage.cacheWriteTokens ?? 0);
  if (usage.inputAccounting === "includes-cache" && cache > usage.inputTokens)
    throw new Error("Cache categories exceed inclusive input tokens");
  const inputTokens =
    usage.inputAccounting === "includes-cache" ? usage.inputTokens : sum(usage.inputTokens, cache);
  return {
    inputTokens,
    outputTokens: usage.outputTokens,
    totalTokens: sum(inputTokens, usage.outputTokens),
  };
}

function validateObservation(
  previous: { status: EvaluationRunStatus; observedUsage: EvaluationTokenUsage | null },
  observation: EvaluationRunObservation,
): void {
  status(observation.status);
  state(observation.usageState);
  if (observation.status === ("admitted" as EvaluationRunStatus))
    throw new Error("Cannot return to admitted state");
  if (terminal(previous.status) && previous.status !== observation.status)
    throw new Error("Terminal status cannot be rewritten");
  if (observation.observedUsage !== null) {
    const current = normalizeEvaluationUsage(observation.observedUsage);
    if (previous.observedUsage !== null) {
      const before = normalizeEvaluationUsage(previous.observedUsage);
      if (current.inputTokens < before.inputTokens || current.outputTokens < before.outputTokens)
        throw new Error("Cumulative token usage cannot decrease");
    }
  } else if (previous.observedUsage !== null)
    throw new Error("Previously observed usage cannot be discarded");
  if (observation.usageState === "reported" && observation.observedUsage === null)
    throw new Error("Reported usage requires counters");
  if (observation.status === "uncertain" && observation.usageState !== "unknown")
    throw new Error("Uncertain execution must retain unknown usage");
  if (terminal(observation.status) && observation.usageState === "pending")
    throw new Error("Terminal usage must be reported or unknown");
}

/** Validates shape/history and derives limits. Overruns remain visible instead of losing receipts. */
export function validateEvaluationBudget(
  policy: EvaluationBudgetPolicy,
  ledger: EvaluationBudgetLedger,
  nowUtc: string,
): EvaluationBudgetSummary {
  for (const key of ["maxRuns", "maxAggregateTokens", "maxDurationSeconds"] as const) {
    count(policy?.[key], key);
    if (policy[key] === 0) throw new Error(`Budget limit must be positive: ${key}`);
  }
  if (policy.includesChildRuns !== true)
    throw new Error("All child, retry and auxiliary runs must be counted");
  const now = timestamp(nowUtc, "nowUtc");
  if (!ledger || !Array.isArray(ledger.runs)) throw new Error("Ledger runs required");
  if (
    ledger.startedAtUtc === null &&
    (ledger.runs.length || ledger.finishedAtUtc !== null || ledger.lastObservedAtUtc != null)
  )
    throw new Error("Started suite cannot reset its clock");
  const started =
    ledger.startedAtUtc === null ? null : timestamp(ledger.startedAtUtc, "startedAtUtc");
  const lastObserved =
    ledger.lastObservedAtUtc == null
      ? started
      : timestamp(ledger.lastObservedAtUtc, "lastObservedAtUtc");
  if ((started !== null && now < started) || (lastObserved !== null && now < lastObserved))
    throw new Error("Elapsed time cannot move backwards");
  if (ledger.runs.length && ledger.lastObservedAtUtc == null)
    throw new Error("Active history requires a monotonic ledger timestamp");
  if (started !== null && lastObserved! < started)
    throw new Error("Ledger observation precedes suite start");
  if (ledger.finishedAtUtc !== null) {
    const finished = timestamp(ledger.finishedAtUtc, "finishedAtUtc");
    if (
      started === null ||
      finished < started ||
      finished > now ||
      ledger.runs.some((run) => !terminal(run.status))
    )
      throw new Error("Invalid suite finish");
  }
  if (ledger.totals !== undefined) {
    if (!ledger.totals || typeof ledger.totals !== "object")
      throw new Error("Invalid projected totals");
    for (const [key, value] of Object.entries(ledger.totals)) count(value, `totals.${key}`);
  }
  const seen = new Map<string, EvaluationRunRecord>();
  let previousAdmission = started;
  let aggregateTokens = 0,
    reservedTokens = 0,
    completedRuns = 0,
    erroredRuns = 0,
    unknownUsageRuns = 0;
  for (const run of ledger.runs) {
    identity(run.runId, "runId");
    if (seen.has(run.runId)) throw new Error("Duplicate run ID");
    if (!["primary", "child", "retry", "auxiliary"].includes(run.kind))
      throw new Error("Invalid run kind");
    if (run.kind === "child" || run.kind === "retry") {
      if (!run.parentRunId || !seen.has(run.parentRunId))
        throw new Error("Child/retry requires a previously admitted parent");
    } else if (run.parentRunId !== undefined && !seen.has(run.parentRunId))
      throw new Error("Unknown parent run");
    const admitted = timestamp(run.admittedAtUtc, "admittedAtUtc");
    const observed = timestamp(run.lastObservedAtUtc, "run.lastObservedAtUtc");
    if (
      started === null ||
      admitted < started ||
      admitted > now ||
      observed < admitted ||
      observed > lastObserved!
    )
      throw new Error("Run timestamp outside recorded history");
    if (previousAdmission !== null && admitted < previousAdmission)
      throw new Error("Admission history cannot move backwards");
    previousAdmission = admitted;
    count(run.reservedInputTokens, "reservedInputTokens");
    count(run.reservedOutputTokens, "reservedOutputTokens");
    const reservation = sum(run.reservedInputTokens, run.reservedOutputTokens);
    if (!reservation) throw new Error("A positive token reservation is required");
    status(run.status);
    state(run.usageState);
    if (!Array.isArray(run.observations)) throw new Error("Reconciliation history required");
    let previous: { status: EvaluationRunStatus; observedUsage: EvaluationTokenUsage | null } = {
      status: "admitted",
      observedUsage: null,
    };
    let previousTime = admitted;
    for (const observation of run.observations) {
      const time = timestamp(observation.atUtc, "observation.atUtc");
      if (time < previousTime || time > observed)
        throw new Error("Observation time cannot move backwards");
      validateObservation(previous, observation);
      previous = observation;
      previousTime = time;
    }
    const latest = run.observations.at(-1);
    if (
      latest
        ? latest.status !== run.status ||
          latest.usageState !== run.usageState ||
          JSON.stringify(latest.observedUsage) !== JSON.stringify(run.observedUsage) ||
          timestamp(latest.atUtc, "latest.atUtc") !== observed
        : run.status !== "admitted" ||
          run.usageState !== "pending" ||
          run.observedUsage !== null ||
          observed !== admitted
    )
      throw new Error("Run projection differs from preserved observation history");
    if (ledger.finishedAtUtc !== null) {
      const firstTerminal = run.observations.find((observation) => terminal(observation.status));
      if (
        !firstTerminal ||
        timestamp(firstTerminal.atUtc, "firstTerminal.atUtc") >
          timestamp(ledger.finishedAtUtc, "finishedAtUtc")
      ) {
        throw new Error("Suite finish cannot precede an execution terminal observation");
      }
    }
    const actual =
      run.observedUsage === null ? 0 : normalizeEvaluationUsage(run.observedUsage).totalTokens;
    const outstanding =
      terminal(run.status) && run.usageState === "reported" ? 0 : Math.max(0, reservation - actual);
    aggregateTokens = sum(aggregateTokens, actual);
    reservedTokens = sum(reservedTokens, outstanding);
    if (run.status === "completed") completedRuns++;
    if (run.status === "failed") erroredRuns++;
    if (run.usageState === "unknown") unknownUsageRuns++;
    seen.set(run.runId, run);
  }
  const accountedTokens = sum(aggregateTokens, reservedTokens);
  const ended =
    ledger.finishedAtUtc === null ? now : timestamp(ledger.finishedAtUtc, "finishedAtUtc");
  const elapsedSeconds = started === null ? 0 : (ended - started) / 1000;
  const reasons: EvaluationBudgetSummary["admissionBlockedReasons"] = [];
  if (ledger.finishedAtUtc !== null) reasons.push("suite-finished");
  if (ledger.runs.length >= policy.maxRuns) reasons.push("run-cap");
  if (accountedTokens >= policy.maxAggregateTokens) reasons.push("token-cap");
  if (elapsedSeconds >= policy.maxDurationSeconds) reasons.push("deadline");
  if (unknownUsageRuns) reasons.push("unknown-usage");
  return {
    admittedRuns: ledger.runs.length,
    completedRuns,
    erroredRuns,
    unknownUsageRuns,
    aggregateTokens,
    reservedTokens,
    accountedTokens,
    elapsedSeconds,
    deadlineAtUtc:
      started === null ? null : new Date(started + policy.maxDurationSeconds * 1000).toISOString(),
    remainingRuns: Math.max(0, policy.maxRuns - ledger.runs.length),
    remainingTokens: Math.max(0, policy.maxAggregateTokens - accountedTokens),
    admissionBlockedReasons: reasons,
    overrun: {
      runs: ledger.runs.length > policy.maxRuns,
      tokens: accountedTokens > policy.maxAggregateTokens,
      duration: elapsedSeconds > policy.maxDurationSeconds,
    },
  };
}

function projectedLedger(
  policy: EvaluationBudgetPolicy,
  ledger: EvaluationBudgetLedger,
  nowUtc: string,
): EvaluationBudgetLedger {
  const summary = validateEvaluationBudget(policy, ledger, nowUtc);
  const {
    admittedRuns,
    completedRuns,
    erroredRuns,
    unknownUsageRuns,
    aggregateTokens,
    reservedTokens,
  } = summary;
  return {
    ...ledger,
    totals: {
      admittedRuns,
      completedRuns,
      erroredRuns,
      unknownUsageRuns,
      aggregateTokens,
      reservedTokens,
    },
  };
}

/** Admission permanently consumes one run slot, including cancelled reservations and retries. */
export function admitEvaluationRun(
  policy: EvaluationBudgetPolicy,
  ledger: EvaluationBudgetLedger,
  request: EvaluationAdmission,
  nowUtc: string,
): EvaluationBudgetLedger {
  const summary = validateEvaluationBudget(policy, ledger, nowUtc);
  identity(request.runId, "runId");
  if (ledger.runs.some((run) => run.runId === request.runId))
    throw new Error("Run ID cannot be reused, including terminal records");
  count(request.reservedInputTokens, "reservedInputTokens");
  count(request.reservedOutputTokens, "reservedOutputTokens");
  const reserved = sum(request.reservedInputTokens, request.reservedOutputTokens);
  if (!reserved) throw new Error("A positive token reservation is required");
  if (summary.admissionBlockedReasons.length)
    throw new Error("Evaluation admission blocked: " + summary.admissionBlockedReasons.join(", "));
  if (reserved > summary.remainingTokens)
    throw new Error("Token reservation would exceed the suite cap");
  const copy = structuredClone(ledger);
  const record: EvaluationRunRecord = {
    ...request,
    admittedAtUtc: nowUtc,
    lastObservedAtUtc: nowUtc,
    status: "admitted",
    usageState: "pending",
    observedUsage: null,
    observations: [],
  };
  return projectedLedger(
    policy,
    {
      ...copy,
      startedAtUtc: ledger.startedAtUtc ?? nowUtc,
      lastObservedAtUtc: nowUtc,
      runs: [...copy.runs, record],
    },
    nowUtc,
  );
}

/** Final/late usage is always accounted even after a limit; it never grants another dispatch. */
export function reconcileEvaluationRun(
  policy: EvaluationBudgetPolicy,
  ledger: EvaluationBudgetLedger,
  update: EvaluationReconciliation,
  nowUtc: string,
): EvaluationBudgetLedger {
  validateEvaluationBudget(policy, ledger, nowUtc);
  const index = ledger.runs.findIndex((run) => run.runId === update.runId);
  if (index < 0) throw new Error("Cannot reconcile an unadmitted run");
  status(update.status);
  if (update.status === ("admitted" as EvaluationRunStatus))
    throw new Error("Cannot return to admitted state");
  if (update.usage !== null) normalizeEvaluationUsage(update.usage);
  const copy = structuredClone(ledger),
    run = copy.runs[index]!;
  const usageState: EvaluationUsageState =
    update.status === "uncertain"
      ? "unknown"
      : update.usage !== null
        ? "reported"
        : terminal(update.status) || run.usageState === "unknown"
          ? "unknown"
          : run.usageState;
  const observation: EvaluationRunObservation = {
    atUtc: nowUtc,
    status: update.status,
    usageState,
    observedUsage: update.usage === null ? run.observedUsage : structuredClone(update.usage),
  };
  validateObservation(run, observation);
  copy.runs[index] = {
    ...run,
    status: observation.status,
    usageState: observation.usageState,
    observedUsage: observation.observedUsage,
    lastObservedAtUtc: nowUtc,
    observations: [...run.observations, observation],
  };
  return projectedLedger(policy, { ...copy, lastObservedAtUtc: nowUtc }, nowUtc);
}
