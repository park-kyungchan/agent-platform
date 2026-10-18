import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { decodeCompactGitWorkContext } from "@palantirkc/work-continuity/git-work";
import { evaluateUnit } from "../../../packages/evidence-foundry/src/modules/evals/runner.ts";
import { fixture, nextAction } from "./fixtures/work.ts";
import { codex, hermes } from "./fixtures/consumers.ts";

// Required integrated contract join: a missing canonical module must fail, never skip.
const apiPath = resolve(
  import.meta.dir,
  "../../../packages/evidence-foundry/src/modules/evals/quality-improvement.ts",
);
interface QualityApi {
  preflightQualityImprovement(input: unknown): { status: string };
  compareQualityImprovement(input: unknown): {
    status: string;
    outcome: string;
    authority: string;
    nativePermissionGranted: boolean;
    acceptanceEstablished: boolean;
  };
}
const hash = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const pin = (ref: string, bytes: Uint8Array | string) => ({
  ref,
  version: "1",
  sha256: hash(bytes),
});

test("PT-QC-01 quality API join: real adapter outputs and evaluateUnit results classify a SYNTHETIC weak-oracle calibration", async () => {
  const api = (await import(apiPath)) as QualityApi;
  const started = performance.now();
  const f = await fixture();
  try {
    const c = await codex(f.root, f.oid),
      codexResult = await c.read("SessionStart");
    expect(codexResult.status).toBe("context-injected");
    const text = JSON.parse(codexResult.stdout).hookSpecificOutput.additionalContext as string;
    const codexWork = JSON.parse(text.slice(text.indexOf("\n") + 1)).selectedWork;
    const hermesResult = await hermes(f.root, f.oid).read();
    expect(hermesResult.status).toBe("source-qualified");
    if (!hermesResult.context) throw Error("Complete Hermes output required");
    const outputs = [
      { consumerId: "codex", value: codexWork },
      { consumerId: "hermes", value: hermesResult.context.work },
    ];
    const frozenBytes = await readFile(
      resolve(import.meta.dir, "fixtures/independent-expectations.json"),
    );
    const treatment = {
      baseline: pin("synthetic:fit-only-oracle", "status === source-qualified && length <= 12000"),
      candidate: pin("synthetic:exact-next-action-oracle", nextAction),
    };
    const evaluator = pin("evaluateUnit:deterministic-boolean", "observed must equal true");
    const criteria = [
      {
        id: "valid",
        caseId: "valid-control",
        kind: "control",
        evaluator,
        objectiveVersion: "1",
        thresholdVersion: "1",
        verificationClass: "deterministic-verified",
      },
      {
        id: "detect",
        caseId: "dropped-next-action",
        kind: "detection",
        evaluator,
        objectiveVersion: "1",
        thresholdVersion: "1",
        verificationClass: "deterministic-verified",
        fixedDefect: pin(
          "synthetic:drop-next-action",
          "Replace authored next action with Inspect patch",
        ),
        validControlIds: ["valid"],
      },
    ];
    const bytes = JSON.stringify(outputs),
      sharedHash = hash(bytes);
    const protocol = {
      schema: "common.quality-improvement.v1",
      work: {
        projectId: "agent-platform",
        workId: "ponytail-harness-evaluation-20261018",
        workRef: pin("fictional:work", f.oid),
        specRef: pin("fictional:spec", "1"),
        planRef: pin("fictional:plan", "1"),
      },
      target: treatment,
      goal: "Calibrate detection of synthetic meaning loss while both real adapter valid controls remain satisfied",
      validation: {
        dataset: pin("fixture:adapter-outputs", bytes),
        expectedBehavior: pin("fixture:independent-expectations", frozenBytes),
        independenceEvidence: pin("PT-10:pre-candidate-oracle-freeze", frozenBytes),
        inputSha256: sharedHash,
        contextSha256: sharedHash,
        environmentSha256: hash(`Bun:${Bun.version}`),
        consumers: outputs.map(({ consumerId }) => ({ id: consumerId, version: "v2" })),
        iterations: 1,
        criteria,
      },
      allowedChanges: ["ci/quality/harness-scenarios/"],
      limits: { maxCaseResults: 8, maxDurationMs: 30000, maxTotalTokens: 0 },
      decision: { ownerId: "accountable-lead", reviewRequired: true },
    };
    const observation = (arm: "baseline" | "candidate") => ({
      target: treatment[arm],
      dataset: protocol.validation.dataset,
      expectedBehavior: protocol.validation.expectedBehavior,
      inputSha256: sharedHash,
      contextSha256: sharedHash,
      environmentSha256: protocol.validation.environmentSha256,
      eligibility: { state: "eligible", evidenceRefs: ["local:ordinary-engineering-only"] },
      results: outputs.flatMap(({ consumerId, value }) =>
        criteria.map((criterion) => {
          const damaged = structuredClone(value);
          if (criterion.kind === "detection")
            damaged.strings[damaged.strings.indexOf(nextAction)] = "Inspect patch";
          const exact = decodeCompactGitWorkContext(damaged).task?.nextAction?.value === nextAction;
          const weak =
            damaged.status === "source-qualified" && JSON.stringify(damaged).length <= 12000;
          const observed =
            criterion.kind === "control"
              ? arm === "baseline"
                ? weak
                : exact
              : !(arm === "baseline" ? weak : exact);
          const evaluated = evaluateUnit(
            {
              objective: "Detect the fixed defect or preserve the valid control",
              scope: "Synthetic calibration on actual source adapter output",
              inputSnapshot: { observed },
              expectedPredicate: { operator: "equals", path: "observed", value: true },
              method: "deterministic",
              objectiveVersion: "1",
              thresholdVersion: "1",
            },
            { observed },
            { method: "deterministic", objectiveVersion: "1", thresholdVersion: "1" },
          );
          return {
            id: `${arm}:${consumerId}:${criterion.id}`,
            criterionId: criterion.id,
            consumerId,
            consumerVersion: "v2",
            output: pin(
              `local:${arm}:${consumerId}:${criterion.id}:output`,
              JSON.stringify(damaged),
            ),
            provenance: pin("local:source-adapter-fixture", f.oid),
            exposure: pin(`local:${consumerId}:exposure`, bytes),
            evidenceScope: "adapter",
            properties: {
              runRef: `local:${arm}`,
              unitRef: criterion.caseId,
              iteration: 1,
              verdict: evaluated.verdict,
              verificationClass: evaluated.verificationClass,
              status: evaluated.status,
              passed: evaluated.passed,
              metrics: evaluated.metrics,
              diagnostics: evaluated.diagnostics,
              evaluatorRef: evaluator.ref,
              evaluatorVersion: "1",
              objectiveVersion: "1",
              thresholdVersion: "1",
              evidenceRefs: [`local:${arm}:${consumerId}:${criterion.id}`],
            },
          };
        }),
      ),
    });
    expect(api.preflightQualityImprovement(protocol).status).toBe("ready-for-comparison");
    const result = api.compareQualityImprovement({
      protocol,
      baseline: observation("baseline"),
      candidate: observation("candidate"),
      changedPaths: ["ci/quality/harness-scenarios/quality-api-join.test.ts"],
      usage: { durationMs: Math.ceil(performance.now() - started), totalTokens: 0 },
      boundaries: {
        authority: "preserved",
        effects: "preserved",
        evidenceRefs: ["local:no-native-effects"],
      },
    });
    expect(result).toMatchObject({
      status: "ready-for-review",
      outcome: "verification-detection-gain",
      authority: "none",
      nativePermissionGranted: false,
      acceptanceEstablished: false,
    });
    // This is a deliberately weakened synthetic comparator, never evidence that the prior production oracle missed a defect.
  } finally {
    await f.cleanup();
  }
}, 30000);
