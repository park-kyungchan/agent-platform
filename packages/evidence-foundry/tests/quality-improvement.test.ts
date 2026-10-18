import { describe, expect, test } from "bun:test";
import {
  compareQualityImprovement,
  preflightQualityImprovement,
  type QualityComparisonRequest,
} from "../src/modules/evals/quality-improvement";
import { qualityFixture, qualityPin } from "./fixtures/quality";
import {
  runSuite,
  newEvalObject,
  type FoundryEvalService,
  type EvalObject,
  type JsonObject,
} from "../src/modules/evals";
import type { CaseResultProperties } from "@palantirkc/ontology/domains/evidence";

const setPass = (r: QualityComparisonRequest["baseline"]["results"][number], passed: boolean) =>
  Object.assign(r.properties, { passed, verdict: passed ? "supported" : "refuted" });

describe("quality-first captured evidence", () => {
  test("offline zero-token budgets allow zero usage and stop any token use even with malformed observations", () => {
    const r = qualityFixture();
    r.protocol.limits.maxTotalTokens = 0;
    expect(preflightQualityImprovement(r.protocol).status).toBe("ready-for-comparison");
    expect(compareQualityImprovement(r).status).toBe("ready-for-review");
    r.usage.totalTokens = 1;
    expect(compareQualityImprovement(r)).toMatchObject({
      status: "stop",
      findings: ["OBSERVED_LIMIT_EXCEEDED"],
    });
    expect(compareQualityImprovement({ ...r, candidate: null }).status).toBe("stop");
  });
  test("baseline failure becomes a behavior gain; all mandatory consumer controls pass", () => {
    const r = qualityFixture(),
      before = structuredClone(r);
    expect(preflightQualityImprovement(r.protocol).status).toBe("ready-for-comparison");
    const result = compareQualityImprovement(r);
    expect(result).toMatchObject({
      status: "ready-for-review",
      outcome: "behavior-gain",
      authority: "none",
      nativePermissionGranted: false,
      acceptanceEstablished: false,
      sourceAuthenticityVerified: false,
      actualExposureAuthenticated: false,
    });
    expect(result.transitions.map((t) => t.transition)).toEqual([
      "behavior-gain",
      "behavior-gain",
      "preserved",
      "preserved",
    ]);
    expect(result.transitions[0]!.evidence.baseline!.properties.verificationClass).toBe(
      "deterministic-verified",
    );
    expect(r).toEqual(before);
  });
  test("verification gain requires fixed defect, baseline miss, candidate detection and both controls", () => {
    const r = qualityFixture("detection");
    expect(compareQualityImprovement(r)).toMatchObject({
      status: "ready-for-review",
      outcome: "verification-detection-gain",
    });
    setPass(r.baseline.results[2]!, false);
    expect(compareQualityImprovement(r).status).toBe("defer");
    r.protocol.validation.criteria[0]!.fixedDefect = undefined;
    expect(preflightQualityImprovement(r.protocol).status).toBe("defer");
  });
  test("passing equivalence and stronger evidence alone are preserved, never a gain", () => {
    const r = qualityFixture();
    r.baseline.results.forEach((x) => setPass(x, true));
    expect(compareQualityImprovement(r)).toMatchObject({ status: "defer", outcome: "preserved" });
    r.candidate.results[0]!.evidenceScope = "native-observation";
    expect(compareQualityImprovement(r)).toMatchObject({
      status: "defer",
      outcome: "insufficient",
    });
  });
  test.each(["baseline-pass", "both-fail"])(
    "mandatory candidate failures need repair: %s",
    (variant) => {
      const r = qualityFixture();
      if (variant === "baseline-pass") r.baseline.results.forEach((x) => setPass(x, true));
      setPass(r.candidate.results[0]!, false);
      const result = compareQualityImprovement(r);
      expect(result).toMatchObject({
        status: "repair",
        outcome: variant === "baseline-pass" ? "regression" : "insufficient",
      });
      expect(result.transitions[0]).toMatchObject({
        baseline: variant === "baseline-pass" ? "satisfied" : "violated",
        candidate: "violated",
      });
    },
  );
  test("no compensation for one failed control by other consumer gains or efficiency", () => {
    const r = qualityFixture();
    setPass(r.candidate.results[3]!, false);
    r.usage = { durationMs: 0, totalTokens: 0 };
    expect(compareQualityImprovement(r).status).toBe("repair");
  });
  const unknowns: [string, (r: QualityComparisonRequest) => void][] = [
    [
      "missing result",
      (r) => {
        r.candidate.results.pop();
      },
    ],
    [
      "duplicate result",
      (r) => {
        r.candidate.results[1] = structuredClone(r.candidate.results[0]!);
      },
    ],
    [
      "reused result ID",
      (r) => {
        r.candidate.results[0]!.id = r.baseline.results[0]!.id;
      },
    ],
    [
      "reused run",
      (r) => {
        r.candidate.results[0]!.properties.runRef = r.baseline.results[0]!.properties.runRef;
      },
    ],
    [
      "baseline target pin",
      (r) => {
        r.baseline.target = qualityPin("wrong");
      },
    ],
    [
      "candidate source digest",
      (r) => {
        r.candidate.target = { ...r.candidate.target, sha256: "e".repeat(64) };
      },
    ],
    [
      "dataset version",
      (r) => {
        r.candidate.dataset = qualityPin("dataset", "2");
      },
    ],
    [
      "expected behavior",
      (r) => {
        r.candidate.expectedBehavior = qualityPin("other");
      },
    ],
    [
      "input",
      (r) => {
        r.candidate.inputSha256 = "e".repeat(64);
      },
    ],
    [
      "context exposure",
      (r) => {
        r.candidate.contextSha256 = "e".repeat(64);
      },
    ],
    [
      "environment",
      (r) => {
        r.candidate.environmentSha256 = "e".repeat(64);
      },
    ],
    [
      "eligibility unknown",
      (r) => {
        r.baseline.eligibility.state = "unknown";
      },
    ],
    [
      "eligibility ineligible",
      (r) => {
        r.candidate.eligibility.state = "ineligible";
      },
    ],
    [
      "provenance absent",
      (r) => {
        r.candidate.results[0]!.provenance = null as never;
      },
    ],
    [
      "exposure absent",
      (r) => {
        r.candidate.results[0]!.exposure = null as never;
      },
    ],
    [
      "consumer",
      (r) => {
        r.candidate.results[0]!.consumerId = "other";
      },
    ],
    [
      "consumer version",
      (r) => {
        r.candidate.results[0]!.consumerVersion = "other";
      },
    ],
    [
      "criterion",
      (r) => {
        r.candidate.results[0]!.criterionId = "other";
      },
    ],
    [
      "case",
      (r) => {
        r.candidate.results[0]!.properties.unitRef = "other";
      },
    ],
    [
      "iteration",
      (r) => {
        r.candidate.results[0]!.properties.iteration = 2;
      },
    ],
    [
      "evaluator version",
      (r) => {
        r.candidate.results[0]!.properties.evaluatorVersion = "2";
      },
    ],
    [
      "objective version",
      (r) => {
        r.candidate.results[0]!.properties.objectiveVersion = "2";
      },
    ],
    [
      "threshold version",
      (r) => {
        r.candidate.results[0]!.properties.thresholdVersion = "2";
      },
    ],
    [
      "evidence class",
      (r) => {
        r.candidate.results[0]!.properties.verificationClass = "model-judged";
      },
    ],
    [
      "missing evidence",
      (r) => {
        r.candidate.results[0]!.properties.evidenceRefs = [];
      },
    ],
    [
      "unknown usage",
      (r) => {
        r.usage.durationMs = null;
      },
    ],
    [
      "unknown authority",
      (r) => {
        r.boundaries.authority = "unknown";
      },
    ],
    [
      "unknown effects",
      (r) => {
        r.boundaries.effects = "unknown";
      },
    ],
    [
      "target error",
      (r) => {
        r.candidate.results[0]!.properties.status = "error";
      },
    ],
    [
      "insufficient baseline",
      (r) => {
        Object.assign(r.baseline.results[0]!.properties, { passed: null, verdict: "insufficient" });
      },
    ],
    [
      "inconsistent verdict",
      (r) => {
        r.candidate.results[0]!.properties.verdict = "refuted";
      },
    ],
  ];
  test.each(unknowns)(
    "unknown or mismatched %s defers without attributed gains",
    (_label, mutate) => {
      const r = qualityFixture();
      mutate(r);
      const result = compareQualityImprovement(r);
      expect(result).toMatchObject({ status: "defer", outcome: "insufficient" });
    },
  );
  test.each(["authority", "effects", "scope", "duration", "tokens", "count"])(
    "known %s stop survives malformed observations and envelope",
    (kind) => {
      const r = qualityFixture();
      if (kind === "authority" || kind === "effects") r.boundaries[kind] = "breached";
      if (kind === "scope") r.changedPaths = ["other/package.ts"];
      if (kind === "duration") r.usage.durationMs = 10001;
      if (kind === "tokens") r.usage.totalTokens = 10001;
      if (kind === "count") r.baseline.results.push(...r.baseline.results);
      expect(compareQualityImprovement({ ...r, candidate: {} }).status).toBe(
        kind === "count" ? "defer" : "stop",
      );
      expect(compareQualityImprovement({ ...r, unknown: true }).status).toBe("stop");
    },
  );
  test("preflight bounds full criterion x consumer x iteration coverage", () => {
    const r = qualityFixture();
    r.protocol.limits.maxCaseResults = 7;
    expect(preflightQualityImprovement(r.protocol).status).toBe("stop");
  });
  test.each(
    [
      null,
      [],
      {},
      { schema: "other" },
      { ...qualityFixture().protocol, nativePermissionGranted: true },
    ].map((value) => [value] as const),
  )("strict bounded preflight refuses malformed input", (value) => {
    expect(preflightQualityImprovement(value).status).toBe("defer");
  });
  test("rejects deep, cyclic, oversized, non-JSON and accessor input without executing accessors", () => {
    const cycle: Record<string, unknown> = {};
    cycle.self = cycle;
    const accessor = Object.defineProperty({}, "protocol", {
      enumerable: true,
      get() {
        throw Error("EXECUTED");
      },
    });
    const hiddenAccessor = Object.defineProperty({}, "goal", {
      get() {
        throw Error("EXECUTED");
      },
    });
    const arrayAccessor = Object.defineProperty([], "forEach", {
      get() {
        throw Error("EXECUTED");
      },
    });
    for (const input of [
      hiddenAccessor,
      { protocol: { allowedChanges: arrayAccessor }, changedPaths: [] },
      cycle,
      accessor,
      { x: NaN },
      { x: new Date() },
      { x: undefined },
      { x: "x".repeat(1_048_576) },
    ])
      expect(compareQualityImprovement(input).status).toBe("defer");
  });
});

// Existing runSuite producer integration: real captured results, no model/native target or store.
// inputSnapshot holds captured target output; the frozen task input is unchanged.
// This verifies producer compatibility, not an observed native consumer repair.
test.each([
  {
    label: "unit override",
    unit: { objectiveVersion: "unit", thresholdVersion: "unit" },
    suite: { objectiveVersion: "suite", thresholdVersion: "suite" },
    expected: "unit",
  },
  {
    label: "suite override",
    unit: {},
    suite: { objectiveVersion: "suite", thresholdVersion: "suite" },
    expected: "suite",
  },
  { label: "evaluator fallback", unit: {}, suite: {}, expected: "evaluator" },
  { label: "default fallback", unit: {}, suite: {}, expected: "1.0.0" },
])("actual runSuite producer results compare with $label versions", async (options) => {
  const r = qualityFixture();
  r.protocol.validation.consumers = [{ id: "codex", version: "source-1" }];
  r.protocol.validation.criteria = [r.protocol.validation.criteria[0]!];
  const criterion = r.protocol.validation.criteria[0]!;
  criterion.objectiveVersion = criterion.thresholdVersion = options.expected;
  criterion.evaluator = qualityPin("evaluator");
  const objects = new Map<string, EvalObject>();
  const service: FoundryEvalService = {
    getObject: (id) => objects.get(id),
    queryObjects: () => [],
    putObjects: async (items) => {
      for (const item of items) objects.set(item.id, item);
    },
    runAction: async () => {
      throw Error("Unexpected action");
    },
  };
  const evaluator = newEvalObject(
    "Evaluator",
    "fixture",
    {
      method: "deterministic",
      ...(options.label === "default fallback"
        ? {}
        : { objectiveVersion: "evaluator", thresholdVersion: "evaluator" }),
    },
    { id: "evaluator" },
  );
  const unit = newEvalObject(
    "VerificationUnit",
    "fixture",
    {
      objective: "Full purpose retained",
      scope: "Synthetic public-contract consumer",
      method: "deterministic",
      inputSnapshot: { value: false },
      expectedPredicate: { operator: "equals", path: "value", value: true },
      sourceRefs: ["independently-fixed-control"],
      ...(options.unit as JsonObject),
    },
    { id: criterion.caseId },
  );
  const dataset = newEvalObject(
    "EvalDataset",
    "fixture",
    { name: "Frozen quality cases", caseRefs: [unit.id], datasetVersion: "1" },
    { id: "dataset" },
  );
  const suite = newEvalObject(
    "EvalSuite",
    "fixture",
    {
      datasetRef: dataset.id,
      evaluatorRefs: [evaluator.id],
      target: { kind: "snapshot" },
      iterations: 1,
      ...(options.suite as JsonObject),
    },
    { id: "suite" },
  );
  await service.putObjects([evaluator, unit, dataset, suite]);
  const baseline = await runSuite(service, suite.id);
  objects.set(unit.id, {
    ...unit,
    properties: { ...unit.properties, inputSnapshot: { value: true } },
  });
  const candidate = await runSuite(service, suite.id);
  for (const [arm, output] of [
    ["baseline", baseline],
    ["candidate", candidate],
  ] as const) {
    expect(output.run.properties.status).toBe("completed");
    expect(output.caseResults).toHaveLength(1);
    const result = output.caseResults[0]!;
    const envelope = r[arm].results[0]!;
    r[arm].results = [
      {
        ...envelope,
        id: result.id,
        properties: result.properties as unknown as CaseResultProperties,
      },
    ];
  }
  const result = compareQualityImprovement(r);
  expect(result).toMatchObject({ status: "ready-for-review", outcome: "behavior-gain" });
  const metric = r.candidate.results[0]!.properties.metrics![0]!;
  metric.objectiveVersion = "stale";
  expect(compareQualityImprovement(r).status).toBe("defer");
});
