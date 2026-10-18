import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Foundry, createObject as createNativeObject } from "./owner-policy-fixture";
import { evaluateUnit, fingerprint, runSuite } from "../src/modules/evals";
import type {
  EvalObject,
  EvaluatorProperties,
  JsonObject,
  VerificationUnitProperties,
} from "../src/modules/evals";
import { assessFreshness, recordAcceptance, verifyClaim } from "../src/modules/claims";
import { compareImprovement } from "../src/modules/evals/improvement";
import {
  importNativeCodexResult,
  parseNativeCodexResult,
  runNativeCodex,
} from "../src/adapters/native-codex";

const NOW = "2026-10-06T03:00:00.000Z";
// Synthetic clock only; production creation and chronology validation remain unchanged.
function createObject(...args: Parameters<typeof createNativeObject>) {
  return { ...createNativeObject(...args), createdAt: NOW, updatedAt: NOW };
}
let directory: string;
let foundry: Foundry;

beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), "foundry-evals-"));
  foundry = await Foundry.open(join(directory, "data"));
});
afterEach(async () => {
  foundry.close();
  await rm(directory, { recursive: true, force: true });
});

const unitProperties = (
  overrides: Partial<VerificationUnitProperties> = {},
): VerificationUnitProperties => ({
  objective: "The snapshot contains the expected marker",
  scope: "One pinned snapshot value",
  inputSnapshot: { value: "expected marker" },
  expectedPredicate: { operator: "contains", path: "value", value: "expected" },
  method: "deterministic",
  ...overrides,
});

async function fixture(
  options: {
    unit?: Partial<VerificationUnitProperties>;
    evaluator?: EvaluatorProperties;
    target?: JsonObject;
    suite?: JsonObject;
    source?: EvalObject;
  } = {},
) {
  const project = createObject(
    "Project",
    "pilot",
    { name: "Eval acceptance pilot", domainId: "research-foundation" },
    { id: "pilot" },
  );
  const unit = createObject(
    "VerificationUnit",
    "pilot",
    JSON.parse(JSON.stringify(unitProperties(options.unit))),
  );
  const evaluator = createObject(
    "Evaluator",
    "pilot",
    JSON.parse(
      JSON.stringify(
        options.evaluator ?? {
          method: "deterministic",
          objectiveVersion: "1.2.0",
          thresholdVersion: "2.0.0",
        },
      ),
    ),
  );
  const dataset = createObject("EvalDataset", "pilot", {
    name: "Atomic cases",
    caseRefs: [unit.id],
    datasetVersion: "1.0.0",
  });
  const suite = createObject("EvalSuite", "pilot", {
    datasetRef: dataset.id,
    target: options.target ?? { kind: "snapshot" },
    evaluatorRefs: [evaluator.id],
    iterations: 1,
    ...options.suite,
  });
  const claim = createObject("Claim", "pilot", {
    statement: "The scoped snapshot contains the marker",
    verificationUnitIds: [unit.id],
    temporal: "current-state",
  });
  await foundry.putObjects([
    project,
    ...(options.source ? [options.source] : []),
    unit,
    evaluator,
    dataset,
    suite,
    claim,
  ]);
  return { project, unit, evaluator, dataset, suite, claim };
}

function nativeTrace(verdict = "supported", tokens = 8) {
  return (
    [
      { type: "thread.started", thread_id: "fixture-thread" },
      { type: "turn.started" },
      {
        type: "item.completed",
        item: { id: "message", type: "agent_message", text: JSON.stringify({ verdict }) },
      },
      {
        type: "turn.completed",
        usage: { input_tokens: tokens - 3, cached_input_tokens: 2, output_tokens: 3 },
      },
    ]
      .map((event) => JSON.stringify(event))
      .join("\n") + "\n"
  );
}

describe("Atomic evaluation and applicability", () => {
  test("nonempty extraction and expected-content failure remain separate metrics", () => {
    const unit = unitProperties({ inputSnapshot: { value: "nonempty different content" } });
    const result = evaluateUnit(unit, unit.inputSnapshot, {
      method: "deterministic",
      objectiveVersion: "1.2.0",
      thresholdVersion: "2.0.0",
    });
    expect(result.verdict).toBe("refuted");
    expect(result.metrics[0]?.passed).toBe(true);
    expect(result.metrics[1]?.passed).toBe(false);
    expect(result.metrics[1]?.objectiveVersion).toBe("1.2.0");
    expect(result.metrics[1]?.thresholdVersion).toBe("2.0.0");
  });

  test("suite records versions and lineage, and acceptance stays separate", async () => {
    const { suite, claim, unit } = await fixture();
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.run.properties.status).toBe("completed");
    expect(result.caseResults[0]?.properties.verificationClass).toBe("deterministic-verified");
    expect(result.run.properties.datasetVersion).toBe("1");
    const verified = await verifyClaim(foundry, claim.id, { now: NOW });
    expect(verified.verdict).toBe("supported");
    expect(verified.decisions).toHaveLength(0);
    expect(verified.units[0]?.history).toHaveLength(1);
    const graph = await foundry.getProvenance(claim.id);
    expect(graph.objects.some((object) => object.id === unit.id)).toBe(true);
    expect(graph.objects.some((object) => object.id === suite.id)).toBe(true);
    const decision = await recordAcceptance(foundry, claim.id, {
      status: "accepted",
      rationale: "Apply this narrow result to the pilot",
      verificationRefs: [result.caseResults[0]!.id],
      authority: { actorId: "local-owner", scope: "pilot" },
      choice: { apply: true },
    });
    expect(decision.properties.status).toBe("accepted");
    expect((await verifyClaim(foundry, claim.id, { now: NOW })).decisions).toHaveLength(1);
    expect((await foundry.store.history(result.run.id)).map((version) => version.version)).toEqual([
      1, 2,
    ]);
  });

  test.each([
    {
      label: "unit overrides the effective suite evaluator",
      unit: { objectiveVersion: "unit-objective", thresholdVersion: "unit-threshold" },
      evaluator: {
        method: "deterministic",
        objectiveVersion: "evaluator-objective",
        thresholdVersion: "evaluator-threshold",
      },
      suite: { objectiveVersion: "suite-objective", thresholdVersion: "suite-threshold" },
      expected: ["unit-objective", "unit-threshold"],
    },
    {
      label: "suite overrides evaluator",
      unit: {},
      evaluator: {
        method: "deterministic",
        objectiveVersion: "evaluator-objective",
        thresholdVersion: "evaluator-threshold",
      },
      suite: { objectiveVersion: "suite-objective", thresholdVersion: "suite-threshold" },
      expected: ["suite-objective", "suite-threshold"],
    },
    {
      label: "evaluator supplies versions",
      unit: {},
      evaluator: {
        method: "deterministic",
        objectiveVersion: "evaluator-objective",
        thresholdVersion: "evaluator-threshold",
      },
      suite: {},
      expected: ["evaluator-objective", "evaluator-threshold"],
    },
    {
      label: "default versions apply",
      unit: {},
      evaluator: { method: "deterministic" },
      suite: {},
      expected: ["1.0.0", "1.0.0"],
    },
  ] satisfies {
    label: string;
    unit: Partial<VerificationUnitProperties>;
    evaluator: EvaluatorProperties;
    suite: JsonObject;
    expected: [string, string];
  }[])("runSuite results retain comparison-compatible versions: $label", async (options) => {
    const stored = await foundry.store.putBlob("independently fixed expected marker");
    const source = createObject("SourceCapture", "pilot", {
      url: "https://example.test/independent-expectation",
      retrievedAt: NOW,
      outcome: "success",
      artifacts: [{ kind: "extracted-text", ...stored }],
      metadata: {},
    });
    const { suite, unit, evaluator, dataset } = await fixture({
      unit: { ...options.unit, sourceRefs: [source.id] },
      evaluator: options.evaluator,
      suite: options.suite as JsonObject,
      source,
    });
    const baseline = await runSuite(foundry, suite.id, { now: NOW });
    const candidate = await runSuite(foundry, suite.id, { now: NOW });
    for (const output of [baseline, candidate]) {
      expect(output.run.properties.status).toBe("completed");
      const result = output.caseResults[0]!;
      expect(result.properties.objectiveVersion).toBe(options.expected[0]);
      expect(result.properties.thresholdVersion).toBe(options.expected[1]);
      expect(result.properties.evidenceRefs).toEqual([source.id]);
      const persisted = await foundry.getObject(result.id);
      expect(persisted?.properties.objectiveVersion).toBe(options.expected[0]);
      expect(persisted?.properties.thresholdVersion).toBe(options.expected[1]);
      const metrics = result.properties.metrics as JsonObject[];
      expect(
        metrics.every(
          (metric) =>
            metric.objectiveVersion === result.properties.objectiveVersion &&
            metric.thresholdVersion === result.properties.thresholdVersion,
        ),
      ).toBe(true);
    }
    const bound = (ref: string, value: unknown, version = "1") => ({
      ref,
      version,
      sha256: fingerprint(value),
    });
    const target = bound(suite.id, suite.properties.target);
    const datasetPin = bound(dataset.id, dataset, String(dataset.version));
    const evaluatorPin = bound(evaluator.id, evaluator, String(evaluator.version));
    const expectation = bound(unit.id, unit.properties.expectedPredicate);
    const observation = (output: typeof baseline) => ({
      target,
      dataset: datasetPin,
      evaluator: evaluatorPin,
      expectedBehavior: expectation,
      contextSha256: fingerprint(unit.properties.inputSnapshot),
      environmentSha256: fingerprint("deterministic fixture environment"),
      results: output.caseResults.map((result) => ({
        id: result.id,
        properties: result.properties,
        measurements: {
          expectedPredicate: (result.properties.metrics as JsonObject[]).find(
            (metric) => metric.name === "expectedPredicate",
          )!.value,
        },
      })),
    });
    const comparison = compareImprovement({
      protocol: {
        schema: "common.constrained-improvement.v1",
        work: {
          projectId: "pilot",
          workId: "producer-compatibility",
          workRef: bound("work.json", "fixture-work"),
          specRef: bound("spec@1.json", "fixture-spec"),
          planRef: bound("plan@1.json", "fixture-plan"),
        },
        target: { baseline: target, candidate: target },
        goal: {
          metric: "expectedPredicate",
          unit: "predicate score",
          direction: "increase",
          minimumDelta: 1,
        },
        validation: {
          dataset: datasetPin,
          evaluator: evaluatorPin,
          expectedBehavior: expectation,
          independenceEvidence: bound(source.id, source),
          objectiveVersion: options.expected[0],
          thresholdVersion: options.expected[1],
          caseIds: [unit.id],
          iterations: 1,
        },
        allowedChanges: ["fixture.ts"],
        limits: { maxCaseResults: 2, maxDurationMs: 10000, maxTotalTokens: 1 },
        decision: { ownerId: "fixture-reviewer", reviewRequired: true },
      },
      baseline: observation(baseline),
      candidate: observation(candidate),
      changedPaths: [],
      usage: { durationMs: 0, totalTokens: 0 },
    });
    // Identical real runs demonstrate producer/consumer compatibility, not a measured optimization.
    expect(comparison.status).toBe("defer");
    expect(comparison.findings).toEqual(["PRACTICAL_GOAL_NOT_ESTABLISHED"]);
    expect(comparison.comparison).toMatchObject({ baseline: 1, candidate: 1, improvement: 0 });
    expect(comparison.evidenceRefs).toEqual([source.id]);
  });

  test("current-state expiration and fingerprint change preserve prior verdict history", async () => {
    const { suite, claim, unit } = await fixture({
      unit: { fingerprints: { code: fingerprint("revision-a") } },
    });
    await runSuite(foundry, suite.id, { now: NOW });
    expect(
      (await verifyClaim(foundry, claim.id, { now: "2026-10-07T03:00:00.000Z" })).freshness,
    ).toBe("stale");
    const changed = await verifyClaim(foundry, claim.id, {
      now: NOW,
      currentFingerprints: { code: "revision-b" },
    });
    expect(changed.verdict).toBe("insufficient");
    expect(changed.units[0]?.results[0]?.freshness.changedFingerprints).toContain("code");
    await foundry.putObjects([
      {
        ...unit,
        version: 2,
        updatedAt: NOW,
        properties: { ...unit.properties, inputSnapshot: { value: "changed" } },
      },
    ]);
    const updated = await verifyClaim(foundry, claim.id, { now: NOW });
    expect(updated.verdict).toBe("insufficient");
    expect(updated.units[0]?.history[0]?.properties.verdict).toBe("supported");
  });

  test("changing a claim statement cannot reuse the original scoped result as current support", async () => {
    const { suite, claim } = await fixture();
    await runSuite(foundry, suite.id, { now: NOW });
    await foundry.putObjects([
      {
        ...claim,
        version: 2,
        properties: {
          ...claim.properties,
          statement: "A different assertion with the same unit reference",
        },
      },
    ]);
    const verification = await verifyClaim(foundry, claim.id, { now: NOW });
    expect(verification.verdict).toBe("insufficient");
    expect(verification.units[0]?.results[0]?.freshness.changedFingerprints).toContain(
      `claim:${claim.id}`,
    );
    expect(verification.units[0]?.history[0]?.properties.verdict).toBe("supported");
  });

  test("fixed-revision results do not expire, and source observation time limits current-state applicability", () => {
    const result = createObject("CaseResult", "pilot", {
      runRef: "run",
      unitRef: "unit",
      verdict: "supported",
      verificationClass: "source-supported",
      verifiedAt: NOW,
      observedAt: "2026-10-04T03:00:00.000Z",
      fingerprints: { source: "same" },
    });
    expect(assessFreshness(result, { now: NOW }).state).toBe("stale");
    expect(
      assessFreshness(result, {
        now: "2030-10-06T03:00:00.000Z",
        temporal: "fixed-revision",
        currentFingerprints: { source: "same" },
      }).state,
    ).toBe("fresh");
  });

  test("conflicting claims coexist in the canonical claim query", async () => {
    const { suite, claim, unit } = await fixture();
    await foundry.putObjects([
      { ...claim, version: 2, properties: { ...claim.properties, conflictKey: "marker" } },
      createObject("Claim", "pilot", {
        statement: "The snapshot does not contain the marker",
        verificationUnitIds: [unit.id],
        temporal: "current-state",
        conflictKey: "marker",
      }),
    ]);
    await runSuite(foundry, suite.id, { now: NOW });
    const verified = await verifyClaim(foundry, claim.id, { now: NOW });
    expect(verified.conflicts).toHaveLength(1);
    expect(verified.diagnostics.join(" ")).toContain("Conflicting");
  });

  test("source-supported evaluation requires successful pinned captured bytes", async () => {
    const stored = await foundry.store.putBlob("expected marker from source");
    const source = createObject("SourceCapture", "pilot", {
      url: "https://example.test/",
      retrievedAt: NOW,
      outcome: "success",
      artifacts: [{ kind: "extracted-text", ...stored }],
      metadata: {},
    });
    const { suite, claim } = await fixture({
      source,
      evaluator: { method: "source-supported" },
      unit: {
        sourceRefs: [source.id],
        inputSnapshot: { artifactSha256: stored.sha256, value: "spoofed snapshot" },
      },
    });
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.caseResults[0]?.properties.verdict).toBe("supported");
    expect(result.caseResults[0]?.properties.verificationClass).toBe("source-supported");
    expect((result.caseResults[0]?.properties.inputSnapshot as JsonObject).value).toBe(
      "expected marker from source",
    );
    await foundry.putObjects([
      {
        ...source,
        version: 2,
        updatedAt: NOW,
        properties: { ...source.properties, outcome: "failure" },
      },
    ]);
    const verification = await verifyClaim(foundry, claim.id, { now: NOW });
    expect(verification.freshness).toBe("stale");
    expect(verification.units[0]?.history[0]?.properties.verdict).toBe("supported");
    const failed = await runSuite(foundry, suite.id, { now: NOW });
    expect(failed.caseResults[0]?.properties.verdict).toBe("insufficient");
    expect(failed.caseResults[0]?.properties.status).toBe("error");
  });

  test("fixed-revision claims retain pinned source support after live source changes", async () => {
    const stored = await foundry.store.putBlob("expected marker at captured revision");
    const source = createObject("SourceCapture", "pilot", {
      url: "https://example.test/fixed",
      retrievedAt: NOW,
      outcome: "success",
      artifacts: [{ kind: "extracted-text", ...stored }],
      metadata: {},
    });
    const { suite, claim } = await fixture({
      source,
      evaluator: { method: "source-supported" },
      unit: {
        sourceRefs: [source.id],
        inputSnapshot: { artifactSha256: stored.sha256 },
        temporal: "fixed-revision",
      },
    });
    await foundry.putObjects([
      { ...claim, version: 2, properties: { ...claim.properties, temporal: "fixed-revision" } },
    ]);
    await runSuite(foundry, suite.id, { now: NOW });
    await foundry.putObjects([
      { ...source, version: 2, properties: { ...source.properties, outcome: "failure" } },
    ]);
    const historical = await verifyClaim(foundry, claim.id, { now: "2030-10-06T03:00:00.000Z" });
    expect(historical.verdict).toBe("supported");
    expect(historical.freshness).toBe("fresh");
    const current = await verifyClaim(foundry, claim.id, { now: NOW, temporal: "current-state" });
    expect(current.verdict).toBe("insufficient");
    expect(current.freshness).toBe("stale");
  });

  test("a model judgment cannot become deterministic verification", async () => {
    const { suite, claim } = await fixture({
      evaluator: { method: "model-judged" },
      unit: {
        inputSnapshot: {
          judgment: { verdict: "supported", verificationClass: "deterministic-verified" },
        },
        expectedPredicate: { operator: "equals", path: "judgment.verdict", value: "supported" },
      },
    });
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.caseResults[0]?.properties.verificationClass).toBe("model-judged");
    expect((await verifyClaim(foundry, claim.id, { now: NOW })).verificationClasses).toEqual([
      "model-judged",
    ]);
    expect(
      (await foundry.queryObjects({ projectId: "pilot", objectType: "Decision" })).length,
    ).toBe(0);
  });

  test("failed final batch preserves diagnostic case snapshots and records a failed run", async () => {
    const { suite } = await fixture();
    const original = foundry.putObjects.bind(foundry);
    foundry.putObjects = async (objects, links) => {
      if (
        objects.some(
          (object) => object.objectType === "EvalRun" && object.properties.status === "completed",
        )
      )
        throw new Error("Injected final-batch failure");
      return original(objects, links);
    };
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.run.properties.status).toBe("failed");
    expect(result.caseResults).toHaveLength(0);
    expect(result.run.properties.diagnostics).toEqual([
      "Final eval batch could not be committed: Error: Injected final-batch failure",
    ]);
    expect(
      (result.run.properties.diagnosticCaseResults as JsonObject[])[0]?.properties,
    ).toBeDefined();
    expect(
      await foundry.queryObjects({ projectId: "pilot", objectType: "CaseResult" }),
    ).toHaveLength(0);
  });

  test("local Action simulation captures proposed effects without changing operation objects", async () => {
    const { suite, claim } = await fixture({
      evaluator: { method: "local-action-simulation" },
      unit: { expectedPredicate: { operator: "equals", path: "status", value: "simulated" } },
    });
    const replacement = {
      ...claim,
      version: 2,
      updatedAt: NOW,
      properties: { ...claim.properties, statement: "Proposed replacement" },
    };
    const changedSuite = {
      ...suite,
      version: 2,
      properties: {
        ...suite.properties,
        target: {
          kind: "local-action",
          action: {
            actionType: "putObject",
            projectId: "pilot",
            idempotencyKey: "simulate-change",
            expectedVersions: { [claim.id]: 1 },
            input: { object: JSON.parse(JSON.stringify(replacement)) },
          },
        },
      },
    };
    await foundry.putObjects([changedSuite]);
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.caseResults[0]?.properties.diagnostics).toEqual([]);
    expect(result.caseResults[0]?.properties.verdict).toBe("supported");
    expect((await foundry.getObject(claim.id))?.version).toBe(1);
    expect((await foundry.getObject(claim.id))?.properties.statement).toBe(
      claim.properties.statement,
    );
  });
});

describe("Native Codex adapter and suite budget", () => {
  test("imports raw events, usage and structured output without model execution", async () => {
    const path = join(directory, "old-trace.jsonl");
    await writeFile(path, nativeTrace());
    const result = await importNativeCodexResult({ jsonlPath: path });
    expect(result.status).toBe("completed");
    expect(result.rawStdout).toBe(await readFile(path, "utf8"));
    expect(result.events).toHaveLength(4);
    expect(result.usage.totalTokens).toBe(8);
    expect(result.usage.cachedInputTokens).toBe(2);
    expect(result.structuredOutput).toEqual({ verdict: "supported" });
  });

  test("malformed trace, evaluator failures and schema failures remain insufficient execution evidence", () => {
    expect(parseNativeCodexResult({ stdout: "invalid\n" }).status).toBe("failed");
    expect(
      parseNativeCodexResult({ stdout: nativeTrace() + JSON.stringify({ type: "turn.failed" }) })
        .status,
    ).toBe("failed");
    expect(
      parseNativeCodexResult({
        stdout: nativeTrace(),
        outputSchema: { type: "object", required: ["missing"] },
      }).status,
    ).toBe("failed");
  });

  test("native execution is refused without an explicit suite budget", async () => {
    let invoked = 0;
    const { suite } = await fixture({
      target: { kind: "native-codex", cwd: directory, prompt: "Judge the scoped snapshot" },
      evaluator: { method: "native-codex" },
    });
    const result = await runSuite(foundry, suite.id, {
      now: NOW,
      nativeRunner: async () => {
        invoked++;
        return parseNativeCodexResult({ stdout: nativeTrace() });
      },
    });
    expect(invoked).toBe(0);
    expect(result.caseResults[0]?.properties.verdict).toBe("insufficient");
    expect(result.caseResults[0]?.properties.status).toBe("error");
  });

  test("suite budget limits native runs, preserves traces, and prevents promotion", async () => {
    let invoked = 0;
    const { suite, claim } = await fixture({
      target: { kind: "native-codex", cwd: directory, prompt: "Judge the scoped snapshot" },
      evaluator: { method: "native-codex" },
      suite: { iterations: 2, budget: { maxRuns: 1, maxDurationMs: 10_000, maxTotalTokens: 100 } },
    });
    const result = await runSuite(foundry, suite.id, {
      now: NOW,
      nativeRunner: async () => {
        invoked++;
        return parseNativeCodexResult({ stdout: nativeTrace() });
      },
    });
    expect(invoked).toBe(1);
    expect(result.run.properties.status).toBe("budget-exceeded");
    expect(result.caseResults[0]?.properties.verificationClass).toBe("model-judged");
    expect((result.caseResults[0]?.properties.nativeResult as JsonObject).rawStdout).toBe(
      nativeTrace(),
    );
    expect(result.caseResults[1]?.properties.verdict).toBe("insufficient");
    expect((await verifyClaim(foundry, claim.id, { now: NOW })).verdict).toBe("insufficient");
  });

  test("native import remains model-judged even with a deterministic evaluator", async () => {
    const { suite } = await fixture({
      target: { kind: "native-import", stdout: nativeTrace() },
      unit: { expectedPredicate: { operator: "equals", path: "verdict", value: "supported" } },
    });
    const result = await runSuite(foundry, suite.id, { now: NOW });
    expect(result.caseResults[0]?.properties.verificationClass).toBe("model-judged");
  });

  test("suite independently rejects a native result whose observed usage exceeds its budget", async () => {
    const { suite } = await fixture({
      target: { kind: "native-codex", cwd: directory, prompt: "Scoped judgment" },
      evaluator: { method: "native-codex" },
      suite: { budget: { maxRuns: 1, maxDurationMs: 10_000, maxTotalTokens: 10 } },
    });
    const result = await runSuite(foundry, suite.id, {
      now: NOW,
      nativeRunner: async () => parseNativeCodexResult({ stdout: nativeTrace("supported", 1000) }),
    });
    expect(result.run.properties.status).toBe("budget-exceeded");
    expect(result.caseResults[0]?.properties.verdict).toBe("insufficient");
    expect(result.caseResults[0]?.properties.suiteBudgetExceeded).toBe(true);
    expect((result.caseResults[0]?.properties.nativeResult as JsonObject).status).toBe("completed");
  });

  test("fake native subprocess preserves on-disk artifacts and selected profile", async () => {
    const fixturePath = join(directory, "fake-codex.ts");
    await writeFile(
      fixturePath,
      `const args = process.argv; const index = args.indexOf('--output-last-message'); await Bun.write(args[index + 1]!, JSON.stringify({ verdict: 'supported' })); process.stdout.write(${JSON.stringify(nativeTrace())}); process.stderr.write('fixture diagnostic');`,
    );
    const result = await runNativeCodex({
      prompt: "Scoped fixture",
      cwd: directory,
      profile: "selected-test",
      budget: { maxRuns: 1, maxDurationMs: 3000, maxTotalTokens: 100 },
      runDirectory: join(directory, "native-run"),
      executable: [process.execPath, fixturePath],
      outputSchema: {
        type: "object",
        properties: { verdict: { type: "string" } },
        required: ["verdict"],
        additionalProperties: false,
      },
    });
    expect(result.status).toBe("completed");
    expect(result.rawStderr).toBe("fixture diagnostic");
    expect(result.invocation?.command).toContain("selected-test");
    expect(result.invocation?.command).toContain("read-only");
    expect(await readFile(join(directory, "native-run/events.jsonl"), "utf8")).toBe(nativeTrace());
    expect(
      JSON.parse(await readFile(join(directory, "native-run/result.json"), "utf8")).usage
        .totalTokens,
    ).toBe(8);
  });

  test("wall-clock and observable token overrun terminate fake native subprocesses", async () => {
    const fixturePath = join(directory, "slow-codex.ts");
    await writeFile(
      fixturePath,
      `process.stdout.write(${JSON.stringify(nativeTrace("supported", 1000))}); await Bun.sleep(10_000);`,
    );
    const result = await runNativeCodex({
      prompt: "Scoped fixture",
      cwd: directory,
      budget: { maxRuns: 1, maxDurationMs: 1000, maxTotalTokens: 10 },
      runDirectory: join(directory, "overrun"),
      executable: [process.execPath, fixturePath],
    });
    expect(result.status).toBe("budget-exceeded");
    expect(result.events.length).toBeGreaterThan(0);
    expect(result.diagnostics.join(" ")).toContain("not a provider hard token cap");
    await writeFile(
      fixturePath,
      "process.stdout.write(JSON.stringify({type:'turn.started'})+'\\n'); await Bun.sleep(10_000);",
    );
    const timeout = await runNativeCodex({
      prompt: "Scoped fixture",
      cwd: directory,
      budget: { maxRuns: 1, maxDurationMs: 150, maxTotalTokens: 100 },
      runDirectory: join(directory, "timeout"),
      executable: [process.execPath, fixturePath],
    });
    expect(timeout.status).toBe("budget-exceeded");
    expect(timeout.elapsedMs).toBeLessThan(2000);
  });
});
