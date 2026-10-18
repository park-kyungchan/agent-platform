import type {
  QualityComparisonRequest,
  QualityCriterion,
} from "../../src/modules/evals/quality-improvement";

export const qualityPin = (ref: string, version = "1") => ({
  ref,
  version,
  sha256: "a".repeat(64),
});
/** Independent synthetic expectations; does not attest native authorship or exposure. */
export function qualityFixture(
  kind: "behavior" | "detection" = "behavior",
): QualityComparisonRequest {
  const criteria: QualityCriterion[] = [
    {
      id: "purpose-preserved",
      caseId: "frozen-complex-case",
      evaluator: qualityPin("independent-evaluator"),
      objectiveVersion: "objective-1",
      thresholdVersion: "threshold-1",
      verificationClass: "deterministic-verified",
      kind,
      ...(kind === "detection"
        ? { fixedDefect: qualityPin("synthetic-fixed-defect"), validControlIds: ["valid-control"] }
        : {}),
    },
    {
      id: "valid-control",
      caseId: "frozen-valid-control",
      evaluator: qualityPin("independent-evaluator"),
      objectiveVersion: "objective-1",
      thresholdVersion: "threshold-1",
      verificationClass: "deterministic-verified",
      kind: "control",
    },
  ];
  const protocol: QualityComparisonRequest["protocol"] = {
    schema: "common.quality-improvement.v1",
    work: {
      projectId: "fixture",
      workId: "quality",
      workRef: qualityPin("work"),
      specRef: qualityPin("spec@3"),
      planRef: qualityPin("plan@3"),
    },
    target: { baseline: qualityPin("baseline"), candidate: qualityPin("candidate") },
    goal: "Preserve the consumer's complete purpose and detect the declared defect",
    validation: {
      dataset: qualityPin("dataset"),
      expectedBehavior: qualityPin("fixed-expected"),
      independenceEvidence: qualityPin("independent-review"),
      inputSha256: "b".repeat(64),
      contextSha256: "c".repeat(64),
      environmentSha256: "d".repeat(64),
      consumers: [
        { id: "codex", version: "source-1" },
        { id: "hermes", version: "source-1" },
      ],
      iterations: 1,
      criteria,
    },
    allowedChanges: ["packages/evidence-foundry/", "scripts/harness.ts"],
    limits: { maxCaseResults: 8, maxDurationMs: 10000, maxTotalTokens: 10000 },
    decision: { ownerId: "lead", reviewRequired: true },
  };
  const observation = (arm: "baseline" | "candidate"): QualityComparisonRequest["baseline"] => ({
    target: protocol.target[arm],
    dataset: protocol.validation.dataset,
    expectedBehavior: protocol.validation.expectedBehavior,
    inputSha256: protocol.validation.inputSha256,
    contextSha256: protocol.validation.contextSha256,
    environmentSha256: protocol.validation.environmentSha256,
    eligibility: { state: "eligible", evidenceRefs: [`${arm}:eligibility-source`] },
    results: criteria.flatMap((criterion) =>
      protocol.validation.consumers.map((consumer) => {
        const passed = arm === "candidate" || criterion.kind === "control";
        return {
          id: `${arm}:${criterion.id}:${consumer.id}`,
          criterionId: criterion.id,
          consumerId: consumer.id,
          consumerVersion: consumer.version,
          output: qualityPin(`${arm}:output:${criterion.id}:${consumer.id}`),
          provenance: qualityPin(`${arm}:provenance`),
          exposure: qualityPin(`${arm}:exposure`),
          evidenceScope: "source" as const,
          properties: {
            runRef: `${arm}:run`,
            unitRef: criterion.caseId,
            iteration: 1,
            evaluatorRef: criterion.evaluator.ref,
            evaluatorVersion: criterion.evaluator.version,
            objectiveVersion: criterion.objectiveVersion,
            thresholdVersion: criterion.thresholdVersion,
            verificationClass: criterion.verificationClass,
            verdict: passed ? "supported" : "refuted",
            status: "completed",
            passed,
            evidenceRefs: [`${arm}:result-evidence`],
          },
        };
      }),
    ),
  });
  return {
    protocol,
    baseline: observation("baseline"),
    candidate: observation("candidate"),
    changedPaths: ["scripts/harness.ts"],
    usage: { durationMs: 100, totalTokens: 0 },
    boundaries: {
      authority: "preserved",
      effects: "preserved",
      evidenceRefs: ["source-only-conduct"],
    },
  };
}
