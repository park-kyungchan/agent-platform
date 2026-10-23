import { parseDriverDiagnostic } from "./trusted/driver-failure";
import { command } from "./trusted/policy";
import { expect, test } from "bun:test";
import { runSourceCommand } from "../source-check";
import { sourceTestFailureDiagnostics } from "./native-tools";
import { ProcessOutputLimitError } from "./trusted/process";
import { metadataOnlyCommandOutput, sourceTestFailureLocations } from "./run";
import { readQualityScope } from "./scope";
import { resolve } from "node:path";

test("actual CI-owned test scope supplies diagnostic locations without promoting tests to package source", async () => {
  const root = resolve(import.meta.dir, "../..");
  // Only TypeScript admission is under test; finite owner QA intentionally has no source Git root.
  // The full quality caller separately reads and verifies its actual tracked Markdown.
  const scope = await readQualityScope(root, undefined, []);
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
  for (const input of [
    "    at <anonymous> (/private/fixture/repo/ci/check.test.ts:42:7)",
    "Example diagnostic says /fixture/repo/ci/check.test.ts:42:7 is relevant",
  ])
    expect(sourceTestFailureLocations(input, "/fixture/repo", ["ci/check.test.ts"])).toEqual([]);
  expect(
    sourceTestFailureLocations(
      "\u001b[2m    at <anonymous> (/fixture/repo/ci/check.test.ts:42:7)\u001b[0m",
      "/fixture/repo",
      ["ci/check.test.ts"],
    ),
  ).toEqual([{ path: "ci/check.test.ts", line: 42, column: 7 }]);
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

// Prevent relocating the owner integration from silently removing required consumer coverage.
test("Hermes source integration stays admitted to the required package QA lane", async () => {
  const scope = await readQualityScope(resolve(import.meta.dir, "../.."), undefined, []);
  const path = "packages/native-hermes-adapter/src/source-provider/harness-quality.test.ts";
  expect(scope.testFiles).toContain(path);
  expect(scope.exclusions.stagedTests.some((row) => row.path === path)).toBe(false);
});

test("location omission and input refusal remain explicit incomplete diagnostics", () => {
  const input = Array.from({ length: 30 }, (_, i) => `at /source/ci/check.test.ts:${i + 1}:1`).join(
    "\n",
  );
  expect(sourceTestFailureDiagnostics(input, "/source", ["ci/check.test.ts"])).toMatchObject({
    total: 30,
    omitted: 14,
    diagnosticComplete: false,
  });
  expect(
    sourceTestFailureDiagnostics("x".repeat(1_048_577), "/source", ["ci/check.test.ts"]),
  ).toEqual({ locations: [], total: null, omitted: null, diagnosticComplete: false });
});
test("actual source child exports finite allowlisted failure evidence before throwing", async () => {
  const rows: Record<string, unknown>[] = [];
  let thrown = false;
  try {
    await runSourceCommand(
      "/tmp",
      [
        process.execPath,
        "-e",
        `import {writeSync} from 'node:fs';writeSync(2,'PRIVATE-SOURCE-SENTINEL\\nat /tmp/ci/check.test.ts:42:7\\n');process.exit(17);`,
      ],
      {
        knownTestFiles: ["ci/check.test.ts"],
        observe: async (row) => {
          expect(thrown).toBe(false);
          rows.push(row);
        },
      },
    );
  } catch {
    thrown = true;
  }
  expect(thrown).toBe(true);
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({
    status: "fail",
    failureKind: "nonzero",
    exitCode: 17,
    timedOut: false,
    failureLocations: [{ path: "ci/check.test.ts", line: 42, column: 7 }],
  });
  expect(JSON.stringify(rows)).not.toContain("PRIVATE-SOURCE-SENTINEL");
  expect(Buffer.byteLength(JSON.stringify(rows[0]) + "\n")).toBeLessThanOrEqual(2048);
});
test("actual source child preserves cap/spawn/timeout failures and bounded positive evidence", async () => {
  const rows: Record<string, unknown>[] = [];
  const observe = async (row: Record<string, unknown>) => {
    rows.push(row);
  };
  await runSourceCommand("/tmp", [process.execPath, "-e", "process.stdout.write('ok')"], {
    maxBytes: 2,
    observe,
  });
  expect(rows.at(-1)).toMatchObject({
    status: "pass",
    exitCode: 0,
    transport: { stdoutBytes: 2, captureComplete: true },
  });
  await expect(
    runSourceCommand(
      "/tmp",
      [process.execPath, "-e", "process.stdout.write('PRIVATE-SOURCE-SENTINEL')"],
      { maxBytes: 2, observe },
    ),
  ).rejects.toBeInstanceOf(ProcessOutputLimitError);
  expect(rows.at(-1)).toMatchObject({
    status: "fail",
    failureKind: "output-limit",
    diagnosticComplete: false,
    transport: { retainedBytes: 2, captureComplete: false, retryAllowed: false },
  });
  await expect(
    runSourceCommand("/tmp", ["/missing-source-fixture"], { observe }),
  ).rejects.toThrow();
  expect(rows.at(-1)).toMatchObject({
    status: "fail",
    failureKind: "spawn-or-transport",
    diagnosticComplete: false,
  });
  await expect(
    runSourceCommand("/tmp", [process.execPath, "-e", "setTimeout(()=>{},5000)"], {
      timeoutMs: 50,
      observe,
    }),
  ).rejects.toThrow("SOURCE_COMMAND_TIMEOUT");
  expect(rows.at(-1)).toMatchObject({ status: "fail", failureKind: "timeout", timedOut: true });
  expect(JSON.stringify(rows)).not.toContain("PRIVATE-SOURCE-SENTINEL");
});

test("source failure diagnostics survive later parallel success metadata without becoming completion", async () => {
  const rows: Record<string, unknown>[] = [];
  await expect(
    runSourceCommand("/tmp", [process.execPath, "-e", "process.exit(17)"], {
      knownTestFiles: ["ci/check.test.ts"],
      observe: async (row) => {
        rows.push(row);
      },
    }),
  ).rejects.toThrow("SOURCE_COMMAND_NONZERO");
  const failure = rows[0]!;
  const stdout = Buffer.from(
    JSON.stringify({ recordClass: "source-only-prior-metadata" }) +
      "\n" +
      JSON.stringify(failure) +
      "\n" +
      JSON.stringify({ ...failure, status: "pass", failureKind: null, exitCode: 0 }) +
      "\n",
  );
  expect(
    parseDriverDiagnostic(stdout, "candidate", command("source"), ["ci/check.test.ts"]),
  ).toEqual(failure);
  expect(
    parseDriverDiagnostic(
      Buffer.from(JSON.stringify({ ...failure, status: "pass" }) + "\n"),
      "candidate",
      command("source"),
      [],
    ),
  ).toBeNull();
  expect(
    parseDriverDiagnostic(
      Buffer.from(
        JSON.stringify({ ...failure, transport: { private: "PRIVATE-SOURCE-SENTINEL" } }) + "\n",
      ),
      "candidate",
      command("source"),
      [],
    ),
  ).toBeNull();
  await expect(
    runSourceCommand("/tmp", [process.execPath, "-e", "process.exit(17)"], {
      observe: async () => {
        throw Error("fixture journal unavailable");
      },
    }),
  ).rejects.toThrow("SOURCE_COMMAND_NONZERO");
});
