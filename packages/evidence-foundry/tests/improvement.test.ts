import { describe, expect, test } from "bun:test";
import {
  compareImprovement,
  preflightImprovement,
  type ImprovementComparisonRequest,
  type ImprovementPin,
} from "../src/modules/evals/improvement";

const pin = (ref: string): ImprovementPin => ({ ref, version: "1", sha256: "a".repeat(64) });
function fixture(): ImprovementComparisonRequest {
  const protocol: ImprovementComparisonRequest["protocol"] = {
    schema: "common.constrained-improvement.v1",
    work: {
      projectId: "common",
      workId: "context-efficiency",
      workRef: pin("work.json"),
      specRef: pin("spec@1.json"),
      planRef: pin("plan@1.json"),
    },
    target: { baseline: pin("baseline.ts"), candidate: pin("candidate.ts") },
    goal: { metric: "durationMs", unit: "ms", direction: "decrease", minimumDelta: 10 },
    validation: {
      dataset: pin("cases.json"),
      evaluator: pin("evaluator.ts"),
      expectedBehavior: pin("expected.json"),
      independenceEvidence: pin("independent-expectation-review.json"),
      objectiveVersion: "1",
      thresholdVersion: "1",
      caseIds: ["ordinary", "recovery"],
      iterations: 2,
    },
    allowedChanges: ["packages/work-continuity/", "scripts/harness.ts"],
    limits: { maxCaseResults: 8, maxDurationMs: 10000, maxTotalTokens: 10000 },
    decision: { ownerId: "accountable-lead", reviewRequired: true },
  };
  const observation = (arm: "baseline" | "candidate") => ({
    target: protocol.target[arm],
    dataset: protocol.validation.dataset,
    evaluator: protocol.validation.evaluator,
    expectedBehavior: protocol.validation.expectedBehavior,
    contextSha256: "b".repeat(64),
    environmentSha256: "c".repeat(64),
    results: protocol.validation.caseIds.flatMap((unitRef) =>
      [1, 2].map((iteration) => ({
        id: `${arm}:${unitRef}:${iteration}`,
        properties: {
          runRef: `${arm}-run`,
          unitRef,
          iteration,
          verdict: "supported" as const,
          verificationClass: "deterministic-verified" as const,
          status: "completed" as const,
          passed: true,
          evaluatorRef: protocol.validation.evaluator.ref,
          evaluatorVersion: "1",
          objectiveVersion: "1",
          thresholdVersion: "1",
          evidenceRefs: [`${arm}-evidence:${unitRef}:${iteration}`],
        },
        measurements: { durationMs: arm === "baseline" ? 100 : 80 },
      })),
    ),
  });
  return {
    protocol,
    baseline: observation("baseline"),
    candidate: observation("candidate"),
    changedPaths: ["packages/work-continuity/src/context/context.ts"],
    usage: { durationMs: 1000, totalTokens: 0 },
  };
}

describe("constrained improvement from existing evidence", () => {
  test("declared protocol preflights; complete paired evidence reaches review only", () => {
    const request = fixture(),
      before = structuredClone(request);
    expect(preflightImprovement(request.protocol).status).toBe("ready-for-comparison");
    const result = compareImprovement(request);
    expect(result.status).toBe("ready-for-review");
    expect(result.comparison).toMatchObject({
      baseline: 100,
      candidate: 80,
      improvement: 20,
      minimumDelta: 10,
      unit: "ms",
      cost: { baseline: null, candidate: null },
    });
    expect(result.evidenceRefs).toHaveLength(8);
    expect(result).toMatchObject({
      authority: "none",
      nativePermissionGranted: false,
      sourceAuthenticityVerified: false,
      acceptanceEstablished: false,
    });
    expect(request).toEqual(before);
  });

  test.each([
    [
      "missing baseline",
      (r: ImprovementComparisonRequest) => {
        r.baseline.results.pop();
      },
    ],
    [
      "missing candidate",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results.pop();
      },
    ],
    [
      "duplicate pair",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[1] = structuredClone(r.candidate.results[0]!);
      },
    ],
    [
      "unexpected case",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.unitRef = "extra";
      },
    ],
    [
      "unexpected iteration",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.iteration = 3;
      },
    ],
    [
      "target version",
      (r: ImprovementComparisonRequest) => {
        r.candidate.target = { ...r.candidate.target, version: "2" };
      },
    ],
    [
      "dataset version",
      (r: ImprovementComparisonRequest) => {
        r.candidate.dataset = { ...r.candidate.dataset, version: "2" };
      },
    ],
    [
      "evaluator version",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.evaluatorVersion = "2";
      },
    ],
    [
      "objective version",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.objectiveVersion = "2";
      },
    ],
    [
      "threshold version",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.thresholdVersion = "2";
      },
    ],
    [
      "unknown metric",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.measurements = {};
      },
    ],
    [
      "null metric",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.measurements.durationMs = null;
      },
    ],
    [
      "unexposed context",
      (r: ImprovementComparisonRequest) => {
        r.candidate.contextSha256 = "d".repeat(64);
      },
    ],
    [
      "different environment",
      (r: ImprovementComparisonRequest) => {
        r.candidate.environmentSha256 = "d".repeat(64);
      },
    ],
    [
      "missing evidence",
      (r: ImprovementComparisonRequest) => {
        delete r.candidate.results[0]!.properties.evidenceRefs;
      },
    ],
    [
      "reused result ID",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.id = r.baseline.results[0]!.id;
      },
    ],
    [
      "reused run",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results[0]!.properties.runRef = r.baseline.results[0]!.properties.runRef;
      },
    ],
    [
      "unknown usage",
      (r: ImprovementComparisonRequest) => {
        r.usage.totalTokens = null;
      },
    ],
    [
      "goal below practical threshold",
      (r: ImprovementComparisonRequest) => {
        r.protocol.goal.minimumDelta = 21;
      },
    ],
    [
      "baseline quality failure",
      (r: ImprovementComparisonRequest) => {
        r.baseline.results[0]!.properties.passed = false;
        r.baseline.results[0]!.properties.verdict = "refuted";
      },
    ],
  ])("defers %s instead of reporting a demonstrated improvement", (_name, mutate) => {
    const request = fixture();
    mutate(request);
    expect(compareImprovement(request).status).toBe("defer");
  });

  test.each(["complete baseline", "incomplete baseline", "failed baseline"])(
    "candidate failure requires repair with %s",
    (variant) => {
      const request = fixture();
      if (variant === "incomplete baseline") request.baseline.results.pop();
      if (variant === "failed baseline") request.baseline.results[0]!.properties.passed = false;
      request.candidate.results[0]!.properties.passed = false;
      request.candidate.results[0]!.properties.verdict = "refuted";
      request.candidate.results.forEach((result) => {
        result.measurements.durationMs = 1;
      });
      expect(compareImprovement(request).status).toBe("repair");
    },
  );

  test.each(["error", "budget-exceeded"] as const)("a %s result remains insufficient", (status) => {
    const request = fixture();
    Object.assign(request.candidate.results[0]!.properties, {
      status,
      verdict: "insufficient",
      passed: null,
    });
    expect(compareImprovement(request).status).toBe("defer");
  });

  test.each([
    [
      "scope",
      (r: ImprovementComparisonRequest) => {
        r.changedPaths = ["packages/work-continuity-sibling/index.ts"];
      },
    ],
    [
      "time",
      (r: ImprovementComparisonRequest) => {
        r.usage.durationMs = 10001;
      },
    ],
    [
      "tokens",
      (r: ImprovementComparisonRequest) => {
        r.usage.totalTokens = 10001;
      },
    ],
    [
      "result count",
      (r: ImprovementComparisonRequest) => {
        r.candidate.results.push(structuredClone(r.candidate.results[0]!));
      },
    ],
  ])("stops for %s before quality or goal recommendations", (_name, mutate) => {
    const request = fixture();
    mutate(request);
    request.candidate.results[0]!.properties.passed = false;
    expect(compareImprovement(request).status).toBe("stop");
  });

  test("supports increase and preserves separately observed optional costs", () => {
    const request = fixture();
    request.protocol.goal.direction = "increase";
    request.candidate.results.forEach((result) => {
      result.measurements.durationMs = 120;
    });
    request.baseline.cost = 3;
    const result = compareImprovement(request);
    expect(result.status).toBe("ready-for-review");
    expect(result.comparison?.cost).toEqual({ baseline: 3, candidate: null });
  });

  test.each([NaN, Infinity, -Infinity])("invalid numeric observations cannot pass: %s", (value) => {
    const request = fixture();
    request.candidate.results[0]!.measurements.durationMs = value;
    expect(compareImprovement(request).status).toBe("defer");
  });

  test("signed domain scores remain valid while cost and usage stay nonnegative", () => {
    const request = fixture();
    request.protocol.goal.direction = "increase";
    request.protocol.goal.minimumDelta = 1;
    request.baseline.results.forEach((result) => {
      result.measurements.durationMs = -10;
    });
    request.candidate.results.forEach((result) => {
      result.measurements.durationMs = -5;
    });
    expect(compareImprovement(request).status).toBe("ready-for-review");
    request.candidate.cost = -1;
    expect(compareImprovement(request).status).toBe("defer");
  });
  test("unrelated failed evaluation is insufficient to establish required candidate regression", () => {
    const request = fixture();
    Object.assign(request.candidate.results[0]!.properties, {
      evaluatorVersion: "other",
      verdict: "refuted",
      passed: false,
    });
    expect(compareImprovement(request).status).toBe("defer");
  });

  test("finite observations do not overflow while calculating their mean", () => {
    const request = fixture();
    request.baseline.results.forEach((result) => {
      result.measurements.durationMs = Number.MAX_VALUE;
    });
    request.candidate.results.forEach((result) => {
      result.measurements.durationMs = Number.MAX_VALUE / 2;
    });
    const result = compareImprovement(request);
    expect(result.status).toBe("ready-for-review");
    expect(Number.isFinite(result.comparison?.baseline)).toBe(true);
  });

  test.each([
    "../escape.ts",
    "/absolute.ts",
    "C:/absolute.ts",
    "packages/**",
    "packages/../escape.ts",
    "packages\\file.ts",
  ])("rejects noncanonical declared change path %s", (value) => {
    const request = fixture();
    request.changedPaths = [value];
    expect(compareImprovement(request).status).toBe("defer");
  });

  test.each(
    [null, [], {}, { schema: "unknown" }, { ...fixture().protocol, authority: "all" }].map(
      (input) => [input] as const,
    ),
  )("malformed preflight is held", (input) => {
    expect(preflightImprovement(input).status).toBe("defer");
  });
  test("declared coverage beyond a predeclared limit stops before comparison", () => {
    const request = fixture();
    request.protocol.limits.maxCaseResults = 7;
    expect(preflightImprovement(request.protocol).status).toBe("stop");
    expect(compareImprovement(request).status).toBe("stop");
  });

  test.each(["scope", "usage"])(
    "malformed results preserve an independently observed %s stop",
    (boundary) => {
      const request = fixture();
      if (boundary === "scope") request.changedPaths = ["outside/owned-scope.ts"];
      else request.usage.durationMs = 10001;
      const result = compareImprovement({ ...request, candidate: {} });
      expect(result.status).toBe("stop");
      expect(result.findings).toContain(
        boundary === "scope" ? "CHANGE_OUTSIDE_ALLOWED_SCOPE" : "OBSERVED_LIMIT_EXCEEDED",
      );
    },
  );
});
