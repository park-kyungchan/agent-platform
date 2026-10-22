import { expect, test } from "bun:test";
import { join, resolve } from "node:path";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { requiredSemanticCases, sha256 } from "./policy";
import { fixedProcess } from "./process";
import { semanticRequest, superviseSemanticCase, type SemanticCase } from "./semantic";
const source = resolve(import.meta.dir, "../../..");
async function run(c: SemanticCase, root = source) {
  const result = await fixedProcess(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      join(import.meta.dir, "fixtures/behavior-call-driver.ts"),
      join(root, c.module),
      root,
    ],
    {
      cwd: "/tmp",
      stdin: Buffer.from(JSON.stringify(semanticRequest(c))),
      timeoutMs: 120000,
      maxBytes: 128000,
    },
  );
  return {
    stdout: result.stdout,
    observation: {
      exitCode: result.code,
      timedOut: result.timedOut,
      oomKilled: false,
      stdoutSha256: sha256(result.stdout),
    },
  };
}
test("fixed scenario calls produce actual Git/store/physical-IO/owner/Memory outcomes for external P assertions", async () => {
  for (const id of [
    "git-source-positive",
    "service-policy-positive",
    "handoff-partial-publication",
    "owner-v6",
    "memory-local-positive",
  ]) {
    const c = requiredSemanticCases.find((c) => c.id === "scenario-" + id)!;
    await superviseSemanticCase(
      c,
      () => run(c),
      () => {},
    );
  }
}, 180000);
test("a real hostile source import exits before Git fixture calls; scenario host rejects its C01 source verdict", async () => {
  const root = await mkdtemp(join(tmpdir(), "s04-scenario-hostile-"));
  try {
    const path = join(root, "packages/work-continuity/src/git-work/v3.ts");
    await mkdir(join(root, "packages/work-continuity/src/git-work"), { recursive: true });
    await writeFile(path, await readFile(join(import.meta.dir, "fixtures/source-forged-exit.ts")));
    const c = requiredSemanticCases.find((c) => c.id === "scenario-git-source-positive")!;
    const observed = await run(c, root);
    expect(observed.observation.exitCode).toBe(0);
    expect(JSON.parse(observed.stdout.toString()).schema).toBe("agent-platform.source-ci.v1");
    let recorded = 0;
    await expect(
      superviseSemanticCase(
        c,
        async () => observed,
        () => {
          recorded++;
        },
      ),
    ).rejects.toThrow("REQUIRED_SEMANTIC_CASE_FAILED");
    expect(recorded).toBe(1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
