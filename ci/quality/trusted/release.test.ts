import { expect, test } from "bun:test";
import { readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { readQualityScope, testFile } from "../scope";
import { buildPolicy, readPolicyMinimum } from "./release";
import { git, scratch, commit } from "./scenarios/support";

const source = resolve(import.meta.dir, "../../..");
const newlyAdmitted = [
  "coordination/source-adapters.test.ts",
  "coordination/source-closure.test.ts",
  "coordination/source-history.test.ts",
  "coordination/source-output.test.ts",
  "coordination/source-tooling.test.ts",
  "coordination/source.test.ts",
  "memory/native-use.test.ts",
].map((path) => "packages/work-continuity/src/" + path);

test("actual current P minimum and clean exact buildPolicy admit every full source test; a missing coverage row refuses both before workers", async () => {
  const minimum = await readPolicyMinimum(source),
    canonical = await readQualityScope(source);
  expect(minimum.packageTests).toEqual(canonical.testFiles);
  expect(minimum.qualityTests).toEqual(
    canonical.lintFiles.filter((p) => p.startsWith("ci/") && testFile(p)),
  );
  expect(minimum.supportFiles).toEqual(canonical.testSupportFiles);
  for (const path of newlyAdmitted) expect(minimum.packageTests).toContain(path);
  const head = await git(source, ["rev-parse", "HEAD"]);
  await scratch("policy-bootstrap", async (root) => {
    // Fresh private clone of the exact current committed source, including every admitted test.
    // No package manager, candidate config or native store is consulted by this metadata bootstrap.
    const policyRoot = join(root, "policy");
    await git(root, ["clone", "--no-hardlinks", "--no-checkout", source, policyRoot]);
    await git(policyRoot, ["checkout", "--detach", head]);
    const p = await buildPolicy(policyRoot, head);
    expect(p.release).toBe(head);
    expect(p.legacyMinimum).toEqual(minimum);
    const coveragePath = join(policyRoot, "ci/quality/trusted/coverage.json");
    const coverage = JSON.parse(await readFile(coveragePath, "utf8")) as {
      legacyTests: { path: string }[];
    };
    const removed = newlyAdmitted[5]!;
    expect(coverage.legacyTests.filter((row) => row.path === removed).length).toBe(1);
    coverage.legacyTests = coverage.legacyTests.filter((row) => row.path !== removed);
    await writeFile(coveragePath, JSON.stringify(coverage, null, 2) + "\n");
    const incompleteHead = await commit(policyRoot);
    // The actual tests/source/selection remain present. Only one required coverage association is missing.
    const after = await readQualityScope(policyRoot);
    expect(after.testFiles).toEqual(canonical.testFiles);
    expect(after.lintFiles).toEqual(canonical.lintFiles);
    await expect(readPolicyMinimum(policyRoot)).rejects.toThrow(
      "EVERY_P_MINIMUM_REQUIRES_EXPLICIT_COVERAGE",
    );
    await expect(buildPolicy(policyRoot, incompleteHead)).rejects.toThrow(
      "EVERY_P_MINIMUM_REQUIRES_EXPLICIT_COVERAGE",
    );
  });
}, 600_000);
