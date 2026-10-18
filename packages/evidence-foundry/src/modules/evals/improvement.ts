import type { CaseResultProperties } from "@palantirkc/ontology/domains/evidence";
import type { Evaluation } from "./types";

export interface ImprovementPin {
  ref: string;
  version: string;
  sha256: string;
}
export interface ImprovementProtocol {
  schema: "common.constrained-improvement.v1";
  work: {
    projectId: string;
    workId: string;
    workRef: ImprovementPin;
    specRef: ImprovementPin;
    planRef: ImprovementPin;
  };
  target: { baseline: ImprovementPin; candidate: ImprovementPin };
  /** minimumDelta is an absolute improvement in unit, never a percentage. */
  goal: { metric: string; unit: string; direction: "increase" | "decrease"; minimumDelta: number };
  validation: {
    dataset: ImprovementPin;
    evaluator: ImprovementPin;
    expectedBehavior: ImprovementPin;
    independenceEvidence: ImprovementPin;
    objectiveVersion: string;
    thresholdVersion: string;
    caseIds: string[];
    iterations: number;
  };
  /** Exact repository-relative paths, or directory prefixes ending in '/'. */
  allowedChanges: string[];
  limits: { maxCaseResults: number; maxDurationMs: number; maxTotalTokens: number };
  decision: { ownerId: string; reviewRequired: true };
}
export interface ImprovementCaseResult {
  id: string;
  properties: CaseResultProperties;
  measurements: Record<string, number | null>;
}
export interface ImprovementObservation {
  target: ImprovementPin;
  dataset: ImprovementPin;
  evaluator: ImprovementPin;
  expectedBehavior: ImprovementPin;
  /** Shared task context excluding the deliberately changed target treatment. */
  contextSha256: string;
  environmentSha256: string;
  results: ImprovementCaseResult[];
  /** Declared observed cost, in the same caller-selected currency for both arms. */
  cost?: number | null;
}
export interface ImprovementComparisonRequest {
  protocol: ImprovementProtocol;
  baseline: ImprovementObservation;
  candidate: ImprovementObservation;
  changedPaths: string[];
  usage: { durationMs: number | null; totalTokens: number | null };
}
export interface ImprovementAssessment {
  status: "ready-for-comparison" | "ready-for-review" | "defer" | "repair" | "stop";
  findings: string[];
  evidenceRefs: string[];
  authority: "none";
  nativePermissionGranted: false;
  sourceAuthenticityVerified: false;
  acceptanceEstablished: false;
  comparison?: {
    metric: string;
    unit: string;
    aggregation: "paired-arithmetic-mean";
    baseline: number | null;
    candidate: number | null;
    improvement: number | null;
    minimumDelta: number;
    cost: { baseline: number | null; candidate: number | null };
  };
}

type Row = Record<string, unknown>;
const MAX_RESULTS = 1000;
function fail(code: string): never {
  throw Error(code);
}
function row(value: unknown, required: string[], optional: string[] = []): Row {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("INVALID_OBJECT");
  const result = value as Row;
  if (
    required.some((key) => !Object.hasOwn(result, key)) ||
    Object.keys(result).some((key) => !required.includes(key) && !optional.includes(key))
  )
    fail("UNKNOWN_OR_MISSING_FIELD");
  return result;
}
function text(value: unknown): asserts value is string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 2000 ||
    /[\u0000-\u001f\u007f]/u.test(value)
  )
    fail("INVALID_TEXT");
}
function finite(
  value: unknown,
  nullable = false,
  nonnegative = true,
): asserts value is number | null {
  if (
    !(nullable && value === null) &&
    (typeof value !== "number" || !Number.isFinite(value) || (nonnegative && value < 0))
  )
    fail("INVALID_NONNEGATIVE_MEASUREMENT");
}
function integer(value: unknown, max = Number.MAX_SAFE_INTEGER): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) < 1 || (value as number) > max)
    fail("INVALID_POSITIVE_LIMIT");
}
function strings(value: unknown): asserts value is string[] {
  if (!Array.isArray(value) || !value.length || value.length > MAX_RESULTS) fail("INVALID_LIST");
  value.forEach(text);
  if (new Set(value).size !== value.length) fail("DUPLICATE_LIST_VALUE");
}
function digest(value: unknown): void {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/.test(value)) fail("INVALID_SHA256");
}
function pin(value: unknown): void {
  const item = row(value, ["ref", "version", "sha256"]);
  text(item.ref);
  text(item.version);
  digest(item.sha256);
}
function path(value: unknown): void {
  text(value);
  if (
    value.startsWith("/") ||
    /^[A-Za-z]:/.test(value) ||
    value.includes("\\") ||
    /[*?\[\]]/.test(value) ||
    value
      .replace(/\/$/, "")
      .split("/")
      .some((part) => !part || part === "." || part === "..")
  )
    fail("INVALID_CHANGE_PATH");
}
function protocol(input: unknown): ImprovementProtocol {
  const p = row(input, [
    "schema",
    "work",
    "target",
    "goal",
    "validation",
    "allowedChanges",
    "limits",
    "decision",
  ]);
  if (p.schema !== "common.constrained-improvement.v1") fail("INVALID_PROTOCOL_SCHEMA");
  const work = row(p.work, ["projectId", "workId", "workRef", "specRef", "planRef"]);
  text(work.projectId);
  text(work.workId);
  [work.workRef, work.specRef, work.planRef].forEach(pin);
  const target = row(p.target, ["baseline", "candidate"]);
  pin(target.baseline);
  pin(target.candidate);
  const goal = row(p.goal, ["metric", "unit", "direction", "minimumDelta"]);
  text(goal.metric);
  text(goal.unit);
  finite(goal.minimumDelta);
  if (goal.minimumDelta === 0 || !["increase", "decrease"].includes(String(goal.direction)))
    fail("INVALID_PRACTICAL_GOAL");
  const validation = row(p.validation, [
    "dataset",
    "evaluator",
    "expectedBehavior",
    "independenceEvidence",
    "objectiveVersion",
    "thresholdVersion",
    "caseIds",
    "iterations",
  ]);
  [
    validation.dataset,
    validation.evaluator,
    validation.expectedBehavior,
    validation.independenceEvidence,
  ].forEach(pin);
  text(validation.objectiveVersion);
  text(validation.thresholdVersion);
  strings(validation.caseIds);
  integer(validation.iterations, 20);
  strings(p.allowedChanges);
  p.allowedChanges.forEach(path);
  const limits = row(p.limits, ["maxCaseResults", "maxDurationMs", "maxTotalTokens"]);
  integer(limits.maxCaseResults, MAX_RESULTS);
  integer(limits.maxDurationMs);
  integer(limits.maxTotalTokens);
  if (validation.caseIds.length * validation.iterations * 2 > limits.maxCaseResults)
    fail("PLANNED_COVERAGE_EXCEEDS_LIMIT");
  const decision = row(p.decision, ["ownerId", "reviewRequired"]);
  text(decision.ownerId);
  if (decision.reviewRequired !== true) fail("INDEPENDENT_REVIEW_REQUIRED");
  return input as ImprovementProtocol;
}
function observation(input: unknown): ImprovementObservation {
  const o = row(
    input,
    [
      "target",
      "dataset",
      "evaluator",
      "expectedBehavior",
      "contextSha256",
      "environmentSha256",
      "results",
    ],
    ["cost"],
  );
  [o.target, o.dataset, o.evaluator, o.expectedBehavior].forEach(pin);
  digest(o.contextSha256);
  digest(o.environmentSha256);
  if (Object.hasOwn(o, "cost")) finite(o.cost, true);
  if (!Array.isArray(o.results) || o.results.length > MAX_RESULTS) fail("RESULT_INPUT_LIMIT");
  for (const result of o.results) {
    const item = row(result, ["id", "properties", "measurements"]);
    text(item.id);
    // Existing CaseResult properties remain their domain contract; no new persisted entity.
    const p = item.properties;
    if (!p || typeof p !== "object" || Array.isArray(p)) fail("INVALID_CASE_RESULT");
    const properties = p as Row;
    text(properties.unitRef);
    text(properties.runRef);
    if (
      !["supported", "refuted", "insufficient"].includes(String(properties.verdict)) ||
      !["deterministic-verified", "source-supported", "model-judged", "human-reviewed"].includes(
        String(properties.verificationClass),
      )
    )
      fail("INVALID_EVALUATION");
    if (
      properties.status !== undefined &&
      !["completed", "error", "budget-exceeded"].includes(String(properties.status))
    )
      fail("INVALID_EVALUATION_STATUS");
    if (
      properties.passed !== undefined &&
      properties.passed !== null &&
      typeof properties.passed !== "boolean"
    )
      fail("INVALID_EVALUATION_PASS");
    if (properties.iteration !== undefined) integer(properties.iteration, 20);
    for (const key of ["evaluatorRef", "evaluatorVersion", "objectiveVersion", "thresholdVersion"])
      if (properties[key] !== undefined) text(properties[key]);
    if (properties.evidenceRefs !== undefined) strings(properties.evidenceRefs);
    if (
      !item.measurements ||
      typeof item.measurements !== "object" ||
      Array.isArray(item.measurements) ||
      Object.keys(item.measurements).length > 32
    )
      fail("INVALID_MEASUREMENTS");
    for (const [key, value] of Object.entries(item.measurements)) {
      text(key);
      finite(value, true, false);
    }
  }
  return input as ImprovementObservation;
}
function assessment(
  status: ImprovementAssessment["status"],
  findings: string[] = [],
): ImprovementAssessment {
  return {
    status,
    findings: [...new Set(findings)],
    evidenceRefs: [],
    authority: "none",
    nativePermissionGranted: false,
    sourceAuthenticityVerified: false,
    acceptanceEstablished: false,
  };
}
const samePin = (a: ImprovementPin, b: ImprovementPin) =>
  a.ref === b.ref && a.version === b.version && a.sha256 === b.sha256;

/** Validates declared scope only. Pins and independence claims are not authenticated. */
export function preflightImprovement(input: unknown): ImprovementAssessment {
  try {
    protocol(input);
    return assessment("ready-for-comparison");
  } catch (error) {
    const code = error instanceof Error ? error.message : "INVALID_PROTOCOL";
    return assessment(code === "PLANNED_COVERAGE_EXCEEDS_LIMIT" ? "stop" : "defer", [code]);
  }
}

/** Compare existing observations only; never dispatch, persist, accept, merge or activate. */
export function compareImprovement(input: unknown): ImprovementAssessment {
  const stops: string[] = [];
  try {
    const request = row(input, ["protocol", "baseline", "candidate", "changedPaths", "usage"]);
    const p = protocol(request.protocol);
    if (!Array.isArray(request.changedPaths) || request.changedPaths.length > MAX_RESULTS)
      fail("INVALID_CHANGED_PATHS");
    request.changedPaths.forEach(path);
    if (new Set(request.changedPaths).size !== request.changedPaths.length)
      fail("DUPLICATE_CHANGE_PATH");
    const usage = row(request.usage, ["durationMs", "totalTokens"]);
    finite(usage.durationMs, true);
    finite(usage.totalTokens, true);
    if (usage.totalTokens !== null && !Number.isSafeInteger(usage.totalTokens))
      fail("INVALID_TOKEN_COUNT");
    const holds: string[] = [],
      regressions: string[] = [],
      evidence = new Set<string>();
    if (
      request.changedPaths.some(
        (changed) =>
          !p.allowedChanges.some((allowed) =>
            allowed.endsWith("/") ? (changed as string).startsWith(allowed) : changed === allowed,
          ),
      )
    )
      stops.push("CHANGE_OUTSIDE_ALLOWED_SCOPE");
    if (
      (usage.durationMs !== null && usage.durationMs > p.limits.maxDurationMs) ||
      (usage.totalTokens !== null && usage.totalTokens > p.limits.maxTotalTokens)
    )
      stops.push("OBSERVED_LIMIT_EXCEEDED");
    const baseline = observation(request.baseline),
      candidate = observation(request.candidate);
    if (baseline.results.length + candidate.results.length > p.limits.maxCaseResults)
      stops.push("OBSERVED_LIMIT_EXCEEDED");
    if (usage.durationMs === null || usage.totalTokens === null)
      holds.push("USAGE_LIMIT_EVIDENCE_MISSING");
    if (
      baseline.contextSha256 !== candidate.contextSha256 ||
      baseline.environmentSha256 !== candidate.environmentSha256
    )
      holds.push("CONTEXT_OR_ENVIRONMENT_NOT_COMPARABLE");
    const baselineIds = new Set(baseline.results.map((result) => result.id));
    const baselineRuns = new Set(baseline.results.map((result) => result.properties.runRef));
    if (
      candidate.results.some(
        (result) => baselineIds.has(result.id) || baselineRuns.has(result.properties.runRef),
      )
    )
      holds.push("BASELINE_CANDIDATE_RESULT_OR_RUN_REUSED");
    const means: (number | null)[] = [];
    for (const [arm, o] of [
      ["baseline", baseline],
      ["candidate", candidate],
    ] as const) {
      const matchingArm =
        samePin(o.target, p.target[arm]) &&
        samePin(o.dataset, p.validation.dataset) &&
        samePin(o.evaluator, p.validation.evaluator) &&
        samePin(o.expectedBehavior, p.validation.expectedBehavior);
      if (!matchingArm) holds.push(`${arm}:VERSION_OR_PIN_MISMATCH`);
      const seen = new Set<string>(),
        ids = new Set<string>(),
        values: number[] = [];
      for (const result of o.results) {
        const r = result.properties;
        const evaluation = r as Pick<
          Evaluation,
          "verdict" | "verificationClass" | "status" | "passed"
        >;
        const key = JSON.stringify([r.unitRef, r.iteration]);
        if (seen.has(key) || ids.has(result.id)) holds.push(`${arm}:DUPLICATE_CASE_RESULT`);
        seen.add(key);
        ids.add(result.id);
        if (
          !p.validation.caseIds.includes(r.unitRef) ||
          !r.iteration ||
          r.iteration > p.validation.iterations
        )
          holds.push(`${arm}:UNEXPECTED_CASE_OR_ITERATION`);
        if (
          r.evaluatorRef !== p.validation.evaluator.ref ||
          r.evaluatorVersion !== p.validation.evaluator.version ||
          r.objectiveVersion !== p.validation.objectiveVersion ||
          r.thresholdVersion !== p.validation.thresholdVersion
        )
          holds.push(`${arm}:EVALUATOR_VERSION_MISMATCH`);
        if (!r.evidenceRefs?.length) holds.push(`${arm}:RESULT_EVIDENCE_MISSING`);
        r.evidenceRefs?.forEach((ref) => evidence.add(ref));
        const applicable =
          matchingArm &&
          p.validation.caseIds.includes(r.unitRef) &&
          !!r.iteration &&
          r.iteration <= p.validation.iterations &&
          r.evaluatorRef === p.validation.evaluator.ref &&
          r.evaluatorVersion === p.validation.evaluator.version &&
          r.objectiveVersion === p.validation.objectiveVersion &&
          r.thresholdVersion === p.validation.thresholdVersion &&
          !!r.evidenceRefs?.length;
        if (
          applicable &&
          evaluation.status === "completed" &&
          (evaluation.verdict === "refuted" || evaluation.passed === false)
        )
          (arm === "candidate" ? regressions : holds).push(`${arm}:REQUIRED_QUALITY_FAILED`);
        else if (
          evaluation.status !== "completed" ||
          evaluation.verdict !== "supported" ||
          evaluation.passed !== true
        )
          holds.push(`${arm}:QUALITY_EVIDENCE_INSUFFICIENT`);
        const value = Object.hasOwn(result.measurements, p.goal.metric)
          ? result.measurements[p.goal.metric]
          : null;
        if (typeof value !== "number") holds.push(`${arm}:GOAL_MEASUREMENT_UNKNOWN`);
        else values.push(value);
      }
      for (const caseId of p.validation.caseIds)
        for (let iteration = 1; iteration <= p.validation.iterations; iteration++)
          if (!seen.has(JSON.stringify([caseId, iteration])))
            holds.push(`${arm}:REQUIRED_COVERAGE_MISSING`);
      // Incremental mean avoids overflowing a sum of individually finite observations.
      const mean = values.reduce(
        (value, next, index) => value * (index / (index + 1)) + next / (index + 1),
        0,
      );
      means.push(
        values.length === p.validation.caseIds.length * p.validation.iterations &&
          Number.isFinite(mean)
          ? mean
          : null,
      );
    }
    const [before, after] = means as [number | null, number | null];
    const delta =
      before === null || after === null
        ? null
        : p.goal.direction === "decrease"
          ? before - after
          : after - before;
    if (delta === null || !Number.isFinite(delta) || delta < p.goal.minimumDelta)
      holds.push("PRACTICAL_GOAL_NOT_ESTABLISHED");
    const result = assessment(
      stops.length
        ? "stop"
        : regressions.length
          ? "repair"
          : holds.length
            ? "defer"
            : "ready-for-review",
      [...stops, ...regressions, ...holds],
    );
    result.evidenceRefs = [...evidence];
    result.comparison = {
      metric: p.goal.metric,
      unit: p.goal.unit,
      aggregation: "paired-arithmetic-mean",
      baseline: before,
      candidate: after,
      improvement: delta !== null && Number.isFinite(delta) ? delta : null,
      minimumDelta: p.goal.minimumDelta,
      cost: { baseline: baseline.cost ?? null, candidate: candidate.cost ?? null },
    };
    return result;
  } catch (error) {
    const code = error instanceof Error ? error.message : "INVALID_COMPARISON";
    return assessment(
      stops.length || code === "PLANNED_COVERAGE_EXCEEDS_LIMIT" ? "stop" : "defer",
      [...stops, code],
    );
  }
}
