import type { CaseResultProperties } from "@palantirkc/ontology/domains/evidence";
import type { ImprovementPin, ImprovementProtocol } from "./improvement";

export interface QualityCriterion {
  id: string;
  caseId: string;
  evaluator: ImprovementPin;
  objectiveVersion: string;
  thresholdVersion: string;
  verificationClass: CaseResultProperties["verificationClass"];
  kind: "behavior" | "detection" | "control";
  /** Independently fixed defect, including whether it is synthetic or observed. */
  fixedDefect?: ImprovementPin;
  validControlIds?: string[];
}
export interface QualityProtocol {
  schema: "common.quality-improvement.v1";
  work: ImprovementProtocol["work"];
  target: ImprovementProtocol["target"];
  goal: string;
  validation: {
    dataset: ImprovementPin;
    expectedBehavior: ImprovementPin;
    independenceEvidence: ImprovementPin;
    inputSha256: string;
    contextSha256: string;
    environmentSha256: string;
    consumers: { id: string; version: string }[];
    iterations: number;
    /** All criteria are mandatory; no compensating total or implicit exclusions. */
    criteria: QualityCriterion[];
  };
  allowedChanges: string[];
  limits: ImprovementProtocol["limits"];
  decision: ImprovementProtocol["decision"];
}
export interface QualityCaseResult {
  id: string;
  criterionId: string;
  consumerId: string;
  consumerVersion: string;
  output: ImprovementPin;
  provenance: ImprovementPin;
  exposure: ImprovementPin;
  evidenceScope: "source" | "adapter" | "native-observation";
  properties: CaseResultProperties;
}
export interface QualityObservation {
  target: ImprovementPin;
  dataset: ImprovementPin;
  expectedBehavior: ImprovementPin;
  inputSha256: string;
  contextSha256: string;
  environmentSha256: string;
  eligibility: { state: "eligible" | "ineligible" | "unknown"; evidenceRefs: string[] };
  results: QualityCaseResult[];
}
export interface QualityComparisonRequest {
  protocol: QualityProtocol;
  baseline: QualityObservation;
  candidate: QualityObservation;
  changedPaths: string[];
  usage: { durationMs: number | null; totalTokens: number | null };
  boundaries: {
    authority: "preserved" | "breached" | "unknown";
    effects: "preserved" | "breached" | "unknown";
    evidenceRefs: string[];
  };
}
type State = "satisfied" | "violated" | "insufficient";
export interface QualityTransition {
  criterionId: string;
  caseId: string;
  consumerId: string;
  iteration: number;
  kind: QualityCriterion["kind"];
  baseline: State;
  candidate: State;
  transition:
    "behavior-gain" | "verification-detection-gain" | "preserved" | "regression" | "insufficient";
  evidence: {
    baseline: QualityCaseResult | null;
    candidate: QualityCaseResult | null;
  };
}
export interface QualityAssessment {
  status: "ready-for-comparison" | "ready-for-review" | "repair" | "defer" | "stop";
  outcome:
    | "behavior-gain"
    | "verification-detection-gain"
    | "mixed-gain"
    | "preserved"
    | "regression"
    | "insufficient";
  findings: string[];
  evidenceRefs: string[];
  transitions: QualityTransition[];
  authority: "none";
  nativePermissionGranted: false;
  sourceAuthenticityVerified: false;
  actualExposureAuthenticated: false;
  acceptanceEstablished: false;
}

type Row = Record<string, unknown>;
const CLASSES = ["deterministic-verified", "source-supported", "model-judged", "human-reviewed"];
const MAX_RESULTS = 1000;
function fail(code: string): never {
  throw Error(code);
}
function record(value: unknown): Row {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("INVALID_OBJECT");
  return value as Row;
}
function row(value: unknown, required: string[], optional: string[] = []): Row {
  const r = record(value);
  if (
    required.some((key) => !Object.hasOwn(r, key)) ||
    Object.keys(r).some((key) => !required.includes(key) && !optional.includes(key))
  )
    fail("UNKNOWN_OR_MISSING_FIELD");
  return r;
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
function integer(value: unknown, max = Number.MAX_SAFE_INTEGER): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) < 1 || (value as number) > max)
    fail("INVALID_POSITIVE_LIMIT");
}
function measurement(value: unknown): void {
  if (value !== null && (typeof value !== "number" || !Number.isFinite(value) || value < 0))
    fail("INVALID_USAGE");
}
function list(value: unknown, empty = false): asserts value is unknown[] {
  if (!Array.isArray(value) || (!empty && !value.length) || value.length > MAX_RESULTS)
    fail("INVALID_LIST");
}
function strings(value: unknown, empty = false): asserts value is string[] {
  list(value, empty);
  value.forEach(text);
  if (new Set(value).size !== value.length) fail("DUPLICATE_LIST_VALUE");
}
function digest(value: unknown): void {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/.test(value)) fail("INVALID_SHA256");
}
function pin(value: unknown): void {
  const p = row(value, ["ref", "version", "sha256"]);
  text(p.ref);
  text(p.version);
  digest(p.sha256);
}
function path(value: unknown): asserts value is string {
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
/** Reject non-JSON values, cycles, accessors and excessive input before reading contracts. */
function boundedJson(input: unknown): void {
  let nodes = 0,
    bytes = 0;
  const seen = new Set<object>();
  function visit(value: unknown, depth: number): void {
    if (++nodes > 60000 || depth > 24) fail("JSON_INPUT_LIMIT");
    if (value === null || typeof value === "boolean") return;
    if (typeof value === "number") {
      if (!Number.isFinite(value)) fail("INVALID_JSON_NUMBER");
      return;
    }
    if (typeof value === "string") {
      bytes += value.length * 3;
      if (bytes > 1_048_576) fail("JSON_INPUT_LIMIT");
      return;
    }
    if (!value || typeof value !== "object" || seen.has(value)) fail("INVALID_JSON_VALUE");
    const array = Array.isArray(value);
    if (
      Object.getPrototypeOf(value) !== (array ? Array.prototype : Object.prototype) &&
      Object.getPrototypeOf(value) !== null
    )
      fail("INVALID_JSON_OBJECT");
    if (
      Reflect.ownKeys(value).some(
        (key) =>
          typeof key !== "string" ||
          (!(array && key === "length") &&
            !Object.getOwnPropertyDescriptor(value, key)?.enumerable),
      )
    )
      fail("INVALID_JSON_OBJECT");
    const keys = Object.keys(value);
    if (keys.length > (array ? MAX_RESULTS : 128)) fail("JSON_INPUT_LIMIT");
    seen.add(value);
    for (const key of keys) {
      bytes += key.length * 3;
      if (bytes > 1_048_576) fail("JSON_INPUT_LIMIT");
      const d = Object.getOwnPropertyDescriptor(value, key)!;
      if (!Object.hasOwn(d, "value")) fail("INVALID_JSON_ACCESSOR");
      visit(d.value, depth + 1);
    }
    if (array && keys.length !== value.length) fail("INVALID_JSON_ARRAY");
    seen.delete(value);
  }
  visit(input, 0);
}
function protocol(input: unknown): QualityProtocol {
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
  if (p.schema !== "common.quality-improvement.v1") fail("INVALID_PROTOCOL_SCHEMA");
  const w = row(p.work, ["projectId", "workId", "workRef", "specRef", "planRef"]);
  text(w.projectId);
  text(w.workId);
  [w.workRef, w.specRef, w.planRef].forEach(pin);
  const target = row(p.target, ["baseline", "candidate"]);
  pin(target.baseline);
  pin(target.candidate);
  text(p.goal);
  const v = row(p.validation, [
    "dataset",
    "expectedBehavior",
    "independenceEvidence",
    "inputSha256",
    "contextSha256",
    "environmentSha256",
    "consumers",
    "iterations",
    "criteria",
  ]);
  [v.dataset, v.expectedBehavior, v.independenceEvidence].forEach(pin);
  [v.inputSha256, v.contextSha256, v.environmentSha256].forEach(digest);
  integer(v.iterations, 20);
  list(v.consumers);
  list(v.criteria);
  const consumerIds = new Set<string>();
  for (const value of v.consumers) {
    const c = row(value, ["id", "version"]);
    text(c.id);
    text(c.version);
    if (consumerIds.has(c.id)) fail("DUPLICATE_CONSUMER");
    consumerIds.add(c.id);
  }
  const criteria = v.criteria as QualityCriterion[];
  const ids = new Set<string>();
  for (const value of criteria) {
    const c = row(
      value,
      [
        "id",
        "caseId",
        "evaluator",
        "objectiveVersion",
        "thresholdVersion",
        "verificationClass",
        "kind",
      ],
      ["fixedDefect", "validControlIds"],
    );
    [c.id, c.caseId, c.objectiveVersion, c.thresholdVersion].forEach(text);
    pin(c.evaluator);
    if (
      !CLASSES.includes(String(c.verificationClass)) ||
      !["behavior", "detection", "control"].includes(String(c.kind))
    )
      fail("INVALID_CRITERION");
    if (ids.has(c.id as string)) fail("DUPLICATE_CRITERION");
    ids.add(c.id as string);
    if (c.kind === "detection") {
      pin(c.fixedDefect);
      strings(c.validControlIds);
    } else if (c.fixedDefect !== undefined || c.validControlIds !== undefined)
      fail("UNEXPECTED_DETECTION_FIELDS");
  }
  for (const c of criteria)
    if (
      c.kind === "detection" &&
      c.validControlIds!.some(
        (id) => !criteria.some((control) => control.id === id && control.kind === "control"),
      )
    )
      fail("INVALID_DETECTION_CONTROL");
  strings(p.allowedChanges);
  p.allowedChanges.forEach(path);
  const limits = row(p.limits, ["maxCaseResults", "maxDurationMs", "maxTotalTokens"]);
  integer(limits.maxCaseResults, MAX_RESULTS);
  integer(limits.maxDurationMs);
  integer(limits.maxTotalTokens);
  const d = row(p.decision, ["ownerId", "reviewRequired"]);
  text(d.ownerId);
  if (d.reviewRequired !== true) fail("INDEPENDENT_REVIEW_REQUIRED");
  if (criteria.length * v.consumers.length * v.iterations * 2 > limits.maxCaseResults)
    fail("PLANNED_COVERAGE_EXCEEDS_LIMIT");
  return input as QualityProtocol;
}
function observation(input: unknown): QualityObservation {
  const o = row(input, [
    "target",
    "dataset",
    "expectedBehavior",
    "inputSha256",
    "contextSha256",
    "environmentSha256",
    "eligibility",
    "results",
  ]);
  [o.target, o.dataset, o.expectedBehavior].forEach(pin);
  [o.inputSha256, o.contextSha256, o.environmentSha256].forEach(digest);
  const eligibility = row(o.eligibility, ["state", "evidenceRefs"]);
  if (!["eligible", "ineligible", "unknown"].includes(String(eligibility.state)))
    fail("INVALID_ELIGIBILITY");
  strings(eligibility.evidenceRefs, true);
  list(o.results, true);
  for (const value of o.results) {
    const r = row(value, [
      "id",
      "criterionId",
      "consumerId",
      "consumerVersion",
      "output",
      "provenance",
      "exposure",
      "evidenceScope",
      "properties",
    ]);
    [r.id, r.criterionId, r.consumerId, r.consumerVersion].forEach(text);
    [r.output, r.provenance, r.exposure].forEach(pin);
    if (!["source", "adapter", "native-observation"].includes(String(r.evidenceScope)))
      fail("INVALID_EVIDENCE_SCOPE");
    // CaseResultProperties deliberately permits producer-owned JSON extensions (snapshots, usage).
    const p = record(r.properties);
    [p.runRef, p.unitRef].forEach(text);
    if (
      !["supported", "refuted", "insufficient"].includes(String(p.verdict)) ||
      !CLASSES.includes(String(p.verificationClass))
    )
      fail("INVALID_EVALUATION");
    if (
      p.status !== undefined &&
      !["completed", "error", "budget-exceeded"].includes(String(p.status))
    )
      fail("INVALID_EVALUATION_STATUS");
    if (p.passed !== undefined && p.passed !== null && typeof p.passed !== "boolean")
      fail("INVALID_EVALUATION_PASS");
    if (p.iteration !== undefined) integer(p.iteration, 20);
    for (const key of ["evaluatorRef", "evaluatorVersion", "objectiveVersion", "thresholdVersion"])
      if (p[key] !== undefined) text(p[key]);
    if (p.evidenceRefs !== undefined) strings(p.evidenceRefs, true);
    if (p.metrics !== undefined) {
      list(p.metrics, true);
      for (const metric of p.metrics) {
        const m = row(metric, [
          "name",
          "value",
          "objectiveVersion",
          "thresholdVersion",
          "threshold",
          "passed",
        ]);
        [m.name, m.objectiveVersion, m.thresholdVersion].forEach(text);
        if (m.passed !== null && typeof m.passed !== "boolean") fail("INVALID_METRIC");
      }
    }
  }
  return input as QualityObservation;
}
const samePin = (a: ImprovementPin, b: ImprovementPin) =>
  a.ref === b.ref && a.version === b.version && a.sha256 === b.sha256;
function assessment(
  status: QualityAssessment["status"],
  findings: string[] = [],
  transitions: QualityTransition[] = [],
  outcome: QualityAssessment["outcome"] = "insufficient",
  evidenceRefs: string[] = [],
): QualityAssessment {
  return {
    status,
    outcome,
    findings: [...new Set(findings)],
    evidenceRefs: [...new Set(evidenceRefs)],
    transitions,
    authority: "none",
    nativePermissionGranted: false,
    sourceAuthenticityVerified: false,
    actualExposureAuthenticated: false,
    acceptanceEstablished: false,
  };
}
function errorCode(error: unknown): string {
  return error instanceof Error ? error.message : "INVALID_INPUT";
}

/** Pure validation of declarations. No source read, native admission, execution or acceptance. */
export function preflightQualityImprovement(input: unknown): QualityAssessment {
  try {
    boundedJson(input);
    protocol(input);
    return assessment("ready-for-comparison");
  } catch (error) {
    const code = errorCode(error);
    return assessment(code === "PLANNED_COVERAGE_EXCEEDS_LIMIT" ? "stop" : "defer", [code]);
  }
}

/** Read only own data properties when salvaging explicit stops from malformed input. */
function own(input: unknown, key: string): unknown {
  if (!input || typeof input !== "object") return undefined;
  const d = Object.getOwnPropertyDescriptor(input, key);
  return d && Object.hasOwn(d, "value") ? d.value : undefined;
}
function knownStops(input: unknown): string[] {
  const stops: string[] = [],
    b = own(input, "boundaries");
  for (const key of ["authority", "effects"])
    if (own(b, key) === "breached") stops.push(`${key.toUpperCase()}_BOUNDARY_BREACHED`);
  // Each independently valid limit/scope can stop even if other observations are malformed.
  const p = own(input, "protocol"),
    limits = own(p, "limits"),
    usage = own(input, "usage");
  for (const [observed, limit] of [
    ["durationMs", "maxDurationMs"],
    ["totalTokens", "maxTotalTokens"],
  ] as const) {
    const value = own(usage, observed),
      max = own(limits, limit);
    if (
      typeof max === "number" &&
      Number.isSafeInteger(max) &&
      max > 0 &&
      typeof value === "number" &&
      Number.isFinite(value) &&
      value > max
    )
      stops.push("OBSERVED_LIMIT_EXCEEDED");
  }
  const allowed = own(p, "allowedChanges"),
    changed = own(input, "changedPaths");
  try {
    boundedJson(allowed);
    boundedJson(changed);
    strings(allowed);
    allowed.forEach(path);
    strings(changed, true);
    changed.forEach(path);
    if (changed.some((c) => !allowed.some((a) => (a.endsWith("/") ? c.startsWith(a) : c === a))))
      stops.push("CHANGE_OUTSIDE_ALLOWED_SCOPE");
  } catch {
    /* Unknown scope cannot establish a breach. */
  }
  const max = own(limits, "maxCaseResults"),
    a = own(own(input, "baseline"), "results"),
    c = own(own(input, "candidate"), "results");
  if (
    typeof max === "number" &&
    Number.isSafeInteger(max) &&
    max > 0 &&
    (Array.isArray(a) ? a.length : 0) + (Array.isArray(c) ? c.length : 0) > max
  )
    stops.push("OBSERVED_LIMIT_EXCEEDED");
  return stops;
}
function state(result: QualityCaseResult | undefined): State {
  const p = result?.properties;
  if (!p || p.status !== "completed") return "insufficient";
  if (p.verdict === "supported" && p.passed === true) return "satisfied";
  if (p.verdict === "refuted" && p.passed === false) return "violated";
  return "insufficient";
}
function effectiveVersionsMatch(p: CaseResultProperties, c: QualityCriterion): boolean {
  if (p.objectiveVersion !== c.objectiveVersion || p.thresholdVersion !== c.thresholdVersion)
    return false;
  if (
    p.metrics?.some(
      (m) => m.objectiveVersion !== c.objectiveVersion || m.thresholdVersion !== c.thresholdVersion,
    )
  )
    return false;
  // Unit overrides are authoritative. Suite overrides are captured on EvalRun,
  // not evaluatorSnapshot; the producer effective fields and metrics bind those.
  if (p.unitSnapshot !== undefined) {
    const unit = record(record(p.unitSnapshot).properties);
    if (
      (unit.objectiveVersion !== undefined && unit.objectiveVersion !== c.objectiveVersion) ||
      (unit.thresholdVersion !== undefined && unit.thresholdVersion !== c.thresholdVersion)
    )
      return false;
  }
  return true;
}

/** Compares captured evidence only. Explicit breaches take precedence over malformed observations. */
export function compareQualityImprovement(input: unknown): QualityAssessment {
  const stops = knownStops(input);
  try {
    boundedJson(input);
    const r = row(input, [
      "protocol",
      "baseline",
      "candidate",
      "changedPaths",
      "usage",
      "boundaries",
    ]);
    const p = protocol(r.protocol),
      v = p.validation;
    strings(r.changedPaths, true);
    r.changedPaths.forEach(path);
    const usage = row(r.usage, ["durationMs", "totalTokens"]);
    measurement(usage.durationMs);
    measurement(usage.totalTokens);
    if (usage.totalTokens !== null && !Number.isSafeInteger(usage.totalTokens))
      fail("INVALID_TOKEN_COUNT");
    const b = row(r.boundaries, ["authority", "effects", "evidenceRefs"]);
    for (const key of ["authority", "effects"])
      if (!["preserved", "breached", "unknown"].includes(String(b[key]))) fail("INVALID_BOUNDARY");
    strings(b.evidenceRefs, true);
    const baseline = observation(r.baseline),
      candidate = observation(r.candidate);
    const holds: string[] = [],
      evidence = [...b.evidenceRefs, v.independenceEvidence.ref];
    if (usage.durationMs === null || usage.totalTokens === null)
      holds.push("USAGE_EVIDENCE_MISSING");
    if (b.authority !== "preserved" || b.effects !== "preserved" || !b.evidenceRefs.length)
      holds.push("BOUNDARY_EVIDENCE_UNKNOWN");
    const maps: Map<string, QualityCaseResult>[] = [];
    const globalIds = new Set<string>(),
      baselineRuns = new Set(baseline.results.map((x) => x.properties.runRef));
    for (const [arm, o] of [
      ["baseline", baseline],
      ["candidate", candidate],
    ] as const) {
      const map = new Map<string, QualityCaseResult>();
      maps.push(map);
      if (
        !samePin(o.target, p.target[arm]) ||
        !samePin(o.dataset, v.dataset) ||
        !samePin(o.expectedBehavior, v.expectedBehavior)
      )
        holds.push(`${arm}:VERSION_OR_PIN_MISMATCH`);
      if (
        o.inputSha256 !== v.inputSha256 ||
        o.contextSha256 !== v.contextSha256 ||
        o.environmentSha256 !== v.environmentSha256
      )
        holds.push(`${arm}:FROZEN_INPUT_OR_EXPOSURE_MISMATCH`);
      if (o.eligibility.state !== "eligible" || !o.eligibility.evidenceRefs.length)
        holds.push(`${arm}:ELIGIBILITY_INSUFFICIENT`);
      evidence.push(...o.eligibility.evidenceRefs, o.target.ref);
      for (const result of o.results) {
        const properties = result.properties,
          c = v.criteria.find((x) => x.id === result.criterionId),
          consumer = v.consumers.find((x) => x.id === result.consumerId);
        const key = JSON.stringify([result.criterionId, result.consumerId, properties.iteration]);
        if (map.has(key) || globalIds.has(result.id)) holds.push(`${arm}:DUPLICATE_CASE_RESULT`);
        globalIds.add(result.id);
        map.set(key, result);
        if (arm === "candidate" && baselineRuns.has(properties.runRef))
          holds.push("BASELINE_CANDIDATE_RUN_REUSED");
        if (
          !c ||
          !consumer ||
          consumer.version !== result.consumerVersion ||
          properties.unitRef !== c.caseId ||
          !properties.iteration ||
          properties.iteration > v.iterations
        ) {
          holds.push(`${arm}:CASE_CRITERION_CONSUMER_ITERATION_MISMATCH`);
          continue;
        }
        if (
          properties.evaluatorRef !== c.evaluator.ref ||
          properties.evaluatorVersion !== c.evaluator.version ||
          properties.verificationClass !== c.verificationClass ||
          !effectiveVersionsMatch(properties, c)
        )
          holds.push(`${arm}:EVALUATOR_OR_EFFECTIVE_VERSION_MISMATCH`);
        if (!properties.evidenceRefs?.length) holds.push(`${arm}:RESULT_EVIDENCE_MISSING`);
        evidence.push(
          ...(properties.evidenceRefs ?? []),
          result.output.ref,
          result.provenance.ref,
          result.exposure.ref,
        );
      }
    }
    const transitions: QualityTransition[] = [];
    for (const c of v.criteria)
      for (const consumer of v.consumers)
        for (let iteration = 1; iteration <= v.iterations; iteration++) {
          const key = JSON.stringify([c.id, consumer.id, iteration]),
            before = maps[0]!.get(key),
            after = maps[1]!.get(key);
          const bs = state(before),
            cs = state(after);
          if (!before || !after) holds.push("REQUIRED_RESULT_MISSING");
          // Source fixtures cannot be promoted into native consumption by switching evidence class.
          if (before && after && before.evidenceScope !== after.evidenceScope)
            holds.push("EVIDENCE_SCOPE_MISMATCH");
          let transition: QualityTransition["transition"] =
            bs === cs && bs !== "insufficient"
              ? "preserved"
              : bs === "satisfied" && cs === "violated"
                ? "regression"
                : "insufficient";
          if (bs === "violated" && cs === "satisfied") {
            if (c.kind === "behavior") transition = "behavior-gain";
            if (c.kind === "detection") {
              const validControls = c.validControlIds!.every((id) => {
                const k = JSON.stringify([id, consumer.id, iteration]);
                return (
                  state(maps[0]!.get(k)) === "satisfied" && state(maps[1]!.get(k)) === "satisfied"
                );
              });
              if (validControls) {
                transition = "verification-detection-gain";
                evidence.push(c.fixedDefect!.ref);
              }
            }
          }
          transitions.push({
            criterionId: c.id,
            caseId: c.caseId,
            consumerId: consumer.id,
            iteration,
            kind: c.kind,
            baseline: bs,
            candidate: cs,
            transition,
            evidence: { baseline: before ?? null, candidate: after ?? null },
          });
        }
    if (stops.length) return assessment("stop", stops, transitions, "insufficient", evidence);
    // Unbound evidence must never label a failure a regression or a success a gain.
    if (holds.length)
      return assessment(
        "defer",
        holds,
        transitions.map((t) => ({ ...t, transition: "insufficient" })),
        "insufficient",
        evidence,
      );
    if (transitions.some((t) => t.candidate === "violated"))
      return assessment(
        "repair",
        ["MANDATORY_CANDIDATE_CRITERION_FAILED"],
        transitions,
        transitions.some((t) => t.transition === "regression") ? "regression" : "insufficient",
        evidence,
      );
    if (
      transitions.some(
        (t) =>
          t.candidate !== "satisfied" ||
          t.baseline === "insufficient" ||
          t.transition === "insufficient",
      )
    )
      return assessment(
        "defer",
        ["QUALITY_EVIDENCE_INSUFFICIENT"],
        transitions,
        "insufficient",
        evidence,
      );
    const behavior = transitions.some((t) => t.transition === "behavior-gain"),
      detection = transitions.some((t) => t.transition === "verification-detection-gain");
    const outcome =
      behavior && detection
        ? "mixed-gain"
        : behavior
          ? "behavior-gain"
          : detection
            ? "verification-detection-gain"
            : "preserved";
    return assessment(
      behavior || detection ? "ready-for-review" : "defer",
      behavior || detection ? [] : ["QUALITY_PRESERVED_NO_DEMONSTRATED_GAIN"],
      transitions,
      outcome,
      evidence,
    );
  } catch (error) {
    const code = errorCode(error);
    return assessment(
      stops.length || code === "PLANNED_COVERAGE_EXCEEDS_LIMIT" ? "stop" : "defer",
      [...stops, code],
    );
  }
}
