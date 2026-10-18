import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ImprovementProtocol } from "../packages/evidence-foundry/src/modules/evals/improvement";

const directories: string[] = [];
afterEach(async () => {
  await Promise.all(
    directories.splice(0).map((path) => rm(path, { recursive: true, force: true })),
  );
});
async function invoke(command: string, input: unknown) {
  const directory = await mkdtemp(join(tmpdir(), "harness-improvement-"));
  directories.push(directory);
  const request = join(directory, "request.json");
  await writeFile(request, JSON.stringify(input));
  const process = Bun.spawn(
    [Bun.which("bun")!, "--no-env-file", import.meta.dir + "/harness.ts", command, request],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);
  return { stdout, stderr, exitCode };
}

test("CLI preflight returns a non-authorizing plan and comparison holds missing outcomes", async () => {
  const pin = (ref: string) => ({ ref, version: "1", sha256: "a".repeat(64) });
  // Synthetic source declarations, never a real evaluation grant or observation.
  const protocol: ImprovementProtocol = {
    schema: "common.constrained-improvement.v1",
    work: {
      projectId: "fixture",
      workId: "fixture",
      workRef: pin("work"),
      specRef: pin("spec"),
      planRef: pin("plan"),
    },
    target: { baseline: pin("baseline"), candidate: pin("candidate") },
    goal: { metric: "elapsedMs", unit: "ms", direction: "decrease", minimumDelta: 10 },
    validation: {
      dataset: pin("dataset"),
      evaluator: pin("evaluator"),
      expectedBehavior: pin("expected"),
      independenceEvidence: pin("review"),
      objectiveVersion: "1",
      thresholdVersion: "1",
      caseIds: ["ordinary"],
      iterations: 1,
    },
    allowedChanges: ["fixture/"],
    limits: { maxCaseResults: 2, maxDurationMs: 1000, maxTotalTokens: 1000 },
    decision: { ownerId: "fixture-owner", reviewRequired: true },
  };
  const preflight = await invoke("improvement-preflight", protocol);
  expect(preflight.exitCode).toBe(0);
  expect(preflight.stderr).toBe("");
  expect(JSON.parse(preflight.stdout)).toMatchObject({
    status: "ready-for-comparison",
    authority: "none",
    nativePermissionGranted: false,
    acceptanceEstablished: false,
  });
  const observation = {
    target: protocol.target.baseline,
    dataset: protocol.validation.dataset,
    evaluator: protocol.validation.evaluator,
    expectedBehavior: protocol.validation.expectedBehavior,
    contextSha256: "b".repeat(64),
    environmentSha256: "c".repeat(64),
    results: [],
  };
  const comparison = await invoke("improvement-compare", {
    protocol,
    baseline: observation,
    candidate: { ...observation, target: protocol.target.candidate },
    changedPaths: [],
    usage: { durationMs: null, totalTokens: null },
  });
  expect(comparison.exitCode).toBe(1);
  expect(JSON.parse(comparison.stdout)).toMatchObject({
    status: "defer",
    acceptanceEstablished: false,
  });
  expect(JSON.parse(comparison.stdout).findings).toContain("candidate:REQUIRED_COVERAGE_MISSING");
});

test("CLI refuses oversized improvement requests before parsing or evaluation", async () => {
  const result = await invoke("improvement-preflight", { ignored: "x".repeat(1_048_576) });
  expect(result.exitCode).toBe(1);
  expect(result.stdout).toBe("");
  expect(result.stderr.trim()).toBe("Request exceeds the bounded input size");
});
