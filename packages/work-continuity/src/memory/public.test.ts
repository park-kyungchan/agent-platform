import { test, expect } from "bun:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runHarness } from "../../../../scripts/harness.ts";
import { nativeFixture, memory } from "./fixtures/native-reader.ts";

test("fresh explicit public Memory package imports have one production implementation without eager legacy dependency changes", async () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const script = `import {strict as assert} from 'node:assert';import * as memory from '@palantirkc/work-continuity/memory/v1';import * as legacy from '@palantirkc/work-continuity/git-work';const direct=await import(${JSON.stringify(join(root, "packages/work-continuity/src/memory/query.ts"))});assert.equal(memory.withMemoryQuery,direct.withMemoryQuery);assert.equal(memory.memoryClaimKey,direct.memoryClaimKey);assert.equal('memoryClaimKey' in legacy,false);assert.equal('withMemoryQuery' in legacy,false);assert.equal(typeof memory.decodeMemory,'function');console.log('memory-public-route-pass');`;
  const child = Bun.spawn(
    [process.execPath, "--no-env-file", "--no-install", "--no-macros", "--no-addons", "-e", script],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  const out = await new Response(child.stdout).text(),
    err = await new Response(child.stderr).text();
  expect(await child.exited, err).toBe(0);
  expect(out.trim()).toBe("memory-public-route-pass");
});
test("harness Memory query consumes only native supplied current reader; JSON cannot load a reader or native permission", async () => {
  const root = await mkdtemp(join(tmpdir(), "memory-cli-synthetic-")),
    path = join(root, "query.json");
  try {
    await writeFile(
      path,
      JSON.stringify({
        schema: "common.memory-query.v1",
        purpose: "synthetic public owner route",
        query: "언어",
        mode: "current",
      }),
    );
    await expect(runHarness(["memory-query", path])).rejects.toThrow(
      "CURRENT_NATIVE_READER_REQUIRED",
    );
    const f = nativeFixture([memory("user-pref", "언어 한국어", { kind: "preference" })]);
    const result = (await runHarness(["memory-query", path], undefined, undefined, f.owner)) as {
      schema: string;
      hits: unknown[];
      nativePermissionGranted: boolean;
    };
    expect(result.schema).toBe("common.memory-query-result.v1");
    expect(result.hits).toHaveLength(1);
    expect(result.nativePermissionGranted).toBe(false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
