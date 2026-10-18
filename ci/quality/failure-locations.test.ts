import { expect, test } from "bun:test";
import { metadataOnlyCommandOutput, sourceTestFailureLocations } from "./run";
import { readQualityScope } from "./scope";
import { resolve } from "node:path";

test("actual CI-owned test scope supplies diagnostic locations without promoting tests to package source", async () => {
  const root = resolve(import.meta.dir, "../..");
  const scope = await readQualityScope(root);
  const path = "ci/quality/failure-locations.test.ts";
  expect(scope.testFiles).not.toContain(path);
  expect(scope.auxiliaryTypescript).toContain(path);
  expect(
    sourceTestFailureLocations(`at ${resolve(root, path)}:42:7`, root, scope.auxiliaryTypescript),
  ).toEqual([{ path, line: 42, column: 7 }]);
});

test("failed fixture diagnostics retain only bounded known source locations", () => {
  const stderr =
    "SYNTHETIC-PRIVATE-RESULT expected another value\n" +
    "at <anonymous> (/fixture/repo/ci/check.test.ts:42:7)\n" +
    "at /tmp/private-original.test.ts:1:2\n" +
    "at <anonymous> (/fixture/repo/ci/check.test.ts:42:7)\n";
  const failureLocations = sourceTestFailureLocations(stderr, "/fixture/repo", [
    "ci/check.test.ts",
  ]);
  expect(failureLocations).toEqual([{ path: "ci/check.test.ts", line: 42, column: 7 }]);
  const safe = metadataOnlyCommandOutput({
    name: "quality-negative-fixtures",
    exitCode: 1,
    stdout: "private",
    stderr,
    failureLocations,
  });
  const encoded = JSON.stringify(safe);
  expect(encoded).not.toContain("SYNTHETIC-PRIVATE-RESULT");
  expect(encoded).not.toContain("/tmp/private-original");
  expect(encoded).not.toContain('"stderr":');
  expect(encoded).toContain('"stderrSha256":');
  expect(encoded).toContain('"failureLocations":');
});

test("unknown, outside, malformed and excessive diagnostic input stays unqualified", () => {
  expect(
    sourceTestFailureLocations("at /private.test.ts:1:1", "/fixture/repo", [
      "../private.test.ts",
      "/private.test.ts",
    ]),
  ).toEqual([]);
  expect(
    sourceTestFailureLocations("at /fixture/repo/ci/check.test.ts:0:1", "/fixture/repo", [
      "ci/check.test.ts",
    ]),
  ).toEqual([]);
  expect(
    sourceTestFailureLocations("x".repeat(1_048_577), "/fixture/repo", ["ci/check.test.ts"]),
  ).toEqual([]);
  const many = Array.from(
    { length: 30 },
    (_, i) => `at /fixture/repo/ci/check.test.ts:${i + 1}:1`,
  ).join("\n");
  expect(sourceTestFailureLocations(many, "/fixture/repo", ["ci/check.test.ts"])).toHaveLength(16);
});
