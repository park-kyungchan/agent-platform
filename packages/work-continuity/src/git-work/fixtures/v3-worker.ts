import { readFile } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { mock } from "bun:test";
import { fixtureOwner } from "./v3-owner.ts";
import * as childProcess from "node:child_process";
import type { ExecFileException, ExecFileOptionsWithStringEncoding } from "node:child_process";

// Deliberate collision injection is confined to this disposable test process.
const input = JSON.parse(await readFile(process.argv[2]!, "utf8"));
if (input.forcedSyntheticId) mock.module("node:crypto", () => ({ createHash, randomUUID: () => input.forcedSyntheticId }));
if (input.syntheticLostCasAcknowledgement) {
  const original = { ...childProcess };
  // Fault only the callback after the real Git CAS succeeds. This disposable
  // worker neither requires executable tmpfs shims nor changes production APIs.
  mock.module("node:child_process", () => ({ ...original, execFile: (file: string, args: string[], options: ExecFileOptionsWithStringEncoding, callback: (error: ExecFileException | null, stdout: string, stderr: string) => void) => original.execFile(file, args, options, (error, stdout, stderr) => {
    const lost = !error && args.includes("update-ref") ? Object.assign(Error("synthetic successful CAS acknowledgement lost"), { code: 1 }) : error;
    callback(lost, stdout, stderr);
  }) }));
}
const { allocateGitWorkIdentity } = await import("../allocation.ts");
try {
  const result = await allocateGitWorkIdentity(input.request, input.target, fixtureOwner(input.target.repositoryRoot));
  console.log(JSON.stringify(result));
} catch (error) {
  console.log(JSON.stringify({ error: error instanceof Error ? error.message : "SYNTHETIC_UNKNOWN" }));
  process.exitCode = 1;
}
void randomUUID;
