import { readFile } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { mock } from "bun:test";
import { fixtureOwner } from "./v3-owner.ts";

// Deliberate collision injection is confined to this disposable test process.
const input = JSON.parse(await readFile(process.argv[2]!, "utf8"));
if (input.forcedSyntheticId) mock.module("node:crypto", () => ({ createHash, randomUUID: () => input.forcedSyntheticId }));
const { allocateGitWorkIdentity } = await import("../allocation.ts");
try {
  const result = await allocateGitWorkIdentity(input.request, input.target, fixtureOwner(input.target.repositoryRoot));
  console.log(JSON.stringify(result));
} catch (error) {
  console.log(JSON.stringify({ error: error instanceof Error ? error.message : "SYNTHETIC_UNKNOWN" }));
  process.exitCode = 1;
}
void randomUUID;
