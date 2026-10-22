import { expect, test } from "bun:test";
import { resolve, join } from "node:path";
import { scenarioModule } from "./scenario-adapters";
import { readPolicyMinimum } from "./release";
import { fixture } from "./fixtures/execution-report";
import { fixedProcess } from "./process";
import { requiredSemanticCases, sha256, trustedDigest, qualifyTrustedExecution } from "./policy";
import { observeSourceWorkload } from "./source-completion";
import { buildEligibilityCheck } from "./eligibility";
import {
  semanticRequest,
  semanticRequestDigest,
  superviseSemanticCase,
  type SemanticCase,
} from "./semantic";

const root = resolve(import.meta.dir, "../../..");
async function execute(c: SemanticCase, path = join(root, c.module)) {
  const run = await fixedProcess(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      join(import.meta.dir, "fixtures/behavior-call-driver.ts"),
      path,
    ],
    {
      cwd: "/tmp",
      stdin: Buffer.from(JSON.stringify(semanticRequest(c))),
      timeoutMs: c.module === scenarioModule ? 120000 : 15000,
      maxBytes: 128000,
    },
  );
  return {
    stdout: run.stdout,
    observation: {
      exitCode: run.code,
      timedOut: run.timedOut,
      oomKilled: false,
      stdoutSha256: sha256(run.stdout),
    },
  };
}
const control: SemanticCase = {
  id: "real-control-flow",
  family: "boundary",
  callers: ["counterexample"],
  module: "ci/quality/trusted/fixtures/source-positive.ts",
  exportName: "answer",
  args: [],
  expected: { kind: "return", value: 42 },
};

test("real synchronous and asynchronous behavior pass outside the candidate process", async () => {
  for (const file of ["source-positive.ts", "source-async.ts"]) {
    let recorded = 0;
    await superviseSemanticCase(
      control,
      () => execute(control, join(import.meta.dir, "fixtures", file)),
      () => {
        recorded++;
      },
    );
    expect(recorded).toBe(1);
  }
});
test("real C01 import-exit forgery, wrong return and actual throw fail host comparisons", async () => {
  for (const file of ["source-forged-exit.ts", "source-negative.ts", "source-throw.ts"]) {
    const result = await execute(control, join(import.meta.dir, "fixtures", file));
    // C01 still exits zero with complete-looking legacy JSON. None replaces API behavior.
    if (file === "source-forged-exit.ts") {
      expect(result.observation.exitCode).toBe(0);
      expect(JSON.parse(Buffer.from(result.stdout).toString()).status).toBe("pass");
    }
    let recorded = 0;
    await expect(
      superviseSemanticCase(
        control,
        async () => result,
        () => {
          recorded++;
        },
      ),
    ).rejects.toThrow("REQUIRED_SEMANTIC_CASE_FAILED");
    expect(recorded).toBe(1);
  }
});
test("exit, timeout, OOM, missing export and import failure cannot stand for an expected API throw", async () => {
  const result = await execute(control);
  for (const change of [{ exitCode: 7 }, { timedOut: true }, { oomKilled: true }])
    await expect(
      superviseSemanticCase(
        control,
        async () => ({ ...result, observation: { ...result.observation, ...change } }),
        () => {},
      ),
    ).rejects.toThrow();
  for (const c of [
    { ...control, exportName: "missing" },
    { ...control, module: "ci/quality/trusted/fixtures/absent.ts" },
  ])
    await expect(
      superviseSemanticCase(
        c,
        () => execute(c),
        () => {},
      ),
    ).rejects.toThrow();
});

test("actual complete production catalogue reaches aggregate and pure issuer, retaining legacy limits", async () => {
  const { p, e } = fixture();
  // Subject/container/provider metadata here are fixtures, not actual Git/Docker/provider observations.
  // Every behavior outcome below comes from the real selected implementation in a separate process.
  const metadata = e.probes;
  e.probes = [];
  for (const subject of [e.target.authored, e.target.tested]) {
    for (const c of requiredSemanticCases) {
      await superviseSemanticCase(
        c,
        () => execute(c),
        (result, actual) => {
          const base = metadata.find((x) => x.id === c.id && x.subject.commit === subject.commit)!;
          e.probes.push({
            ...base,
            subject,
            inputSha256: semanticRequestDigest(c),
            actual,
            observation: {
              ...base.observation,
              ...result.observation,
              inputSha256: trustedDigest(subject),
            },
          });
        },
      );
    }
  }
  qualifyTrustedExecution(p, e, Date.now());
  const check = buildEligibilityCheck(
    p,
    e,
    { reportSha256: "a".repeat(64), externalId: "b".repeat(64) },
    Date.now(),
  );
  expect(check.conclusion).toBe("success");
  expect(check.output.summary).toContain("bounded-p-semantic-v1");
  expect(check.output.summary).toContain("unauthenticated");
  const c = requiredSemanticCases[0]!;
  const forged = await execute(c, join(import.meta.dir, "fixtures/source-forged-exit.ts"));
  const legacy = await observeSourceWorkload(async () => forged);
  expect(legacy.evidence.workload.state).toBe("exit-zero-only");
  await expect(
    superviseSemanticCase(
      c,
      async () => forged,
      (result, actual) => {
        e.probes[0]!.actual = actual;
        e.probes[0]!.observation = { ...e.probes[0]!.observation, ...result.observation };
      },
    ),
  ).rejects.toThrow("REQUIRED_SEMANTIC_CASE_FAILED");
  expect(() => qualifyTrustedExecution(p, e, Date.now())).toThrow();
  expect(() =>
    buildEligibilityCheck(
      p,
      e,
      { reportSha256: "a".repeat(64), externalId: "b".repeat(64) },
      Date.now(),
    ),
  ).toThrow();
}, 600000);

test("the immutable P coverage map accounts for the actual full legacy denominator", async () => {
  const minimum = await readPolicyMinimum(root);
  expect(minimum.packageTests.length).toBeGreaterThan(0);
  expect(minimum.qualityTests).toContain("ci/quality/trusted/semantic.test.ts");
  expect(minimum.supportFiles.length).toBeGreaterThan(0);
});
