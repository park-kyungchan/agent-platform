import { operationalTimeouts } from "../operational-timeouts";
import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fixedProcess } from "./process";
import { sha256 } from "./policy";
import {
  superviseSourceAssertions,
  type ObservedSourceWorkload,
  type SharedProcessSourceRun,
  sourceAssertionHoldCode,
} from "./source-completion";

async function exercise(candidate: string) {
  const scratch = await mkdtemp(join(tmpdir(), "s04-source-control-flow-"));
  try {
    const witness = join(scratch, "assertion-finished.txt");
    const records: ObservedSourceWorkload<SharedProcessSourceRun>[] = [];
    let gateFailure: string | null = null;
    try {
      await superviseSourceAssertions(
        async () => {
          const run = await fixedProcess(
            [
              process.execPath,
              "--no-env-file",
              "--no-install",
              join(import.meta.dir, "fixtures/source-assertion-driver.ts"),
              join(import.meta.dir, "fixtures", candidate),
              witness,
            ],
            {
              cwd: scratch,
              timeoutMs: operationalTimeouts.scenarioCommandMs,
              maxBytes: 16384,
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
        },
        (observed) => {
          records.push(observed);
        },
      );
    } catch (error) {
      gateFailure = error instanceof Error ? error.message : "unknown";
    }
    const observed = records[0];
    if (!observed) throw Error("Source fixture did not reach the production observation gate");
    let assertionFinished = false;
    try {
      assertionFinished = (await readFile(witness, "utf8")) === "P assertion finished\n";
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    // This witness observes the fixture's control flow only. It is accessible to the child
    // and deliberately does not authenticate assertions or grant source eligibility.
    return { ...observed, assertionFinished, gateFailure };
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
}

test("actual hostile candidate emits a complete-looking source verdict and exits before P asserts", async () => {
  const observed = await exercise("source-forged-exit.ts");
  expect(observed.result.observation.exitCode).toBe(0);
  const claimed = JSON.parse(Buffer.from(observed.result.stdout).toString());
  expect(claimed.schema).toBe("agent-platform.source-ci.v1");
  expect(claimed.status).toBe("pass");
  expect(claimed.canonicalTestFiles).toBe(1000000);
  expect(claimed.qualityTestFiles).toBe(1000000);
  expect(claimed.commands).toEqual([{ exitCode: 0 }]);
  expect(observed.assertionFinished).toBe(false);
  expect(observed.evidence.workload.state).toBe("exit-zero-only");
  expect(observed.evidence.assertionsAuthenticated).toBe(false);
  expect(observed.evidence.state).toBe("held");
  expect(observed.evidence.reason).toBe(sourceAssertionHoldCode);
  expect(observed.gateFailure).toBe(null);
});

test("actual positive implementation reaches P assertion but shared-process assertion proof stays unauthenticated", async () => {
  const observed = await exercise("source-positive.ts");
  expect(observed.result.observation.exitCode).toBe(0);
  expect(observed.assertionFinished).toBe(true);
  expect(observed.evidence.workload.state).toBe("exit-zero-only");
  expect(observed.evidence.assertionsAuthenticated).toBe(false);
  expect(observed.evidence.state).toBe("held");
  expect(observed.gateFailure).toBe(null);
});

test("actual failing P assertion retains failure separately from its unqualified boundary", async () => {
  const observed = await exercise("source-negative.ts");
  expect(observed.result.observation.exitCode).not.toBe(0);
  expect(observed.assertionFinished).toBe(false);
  expect(observed.evidence.workload.state).toBe("failed");
  expect(observed.gateFailure).toBe("TRUSTED_CI_SOURCE_WORKLOAD_FAILED");
  expect(observed.evidence.workload.exitCode).toBe(observed.result.observation.exitCode);
  expect(observed.evidence.assertionsAuthenticated).toBe(false);
  expect(observed.evidence.state).toBe("held");
});
