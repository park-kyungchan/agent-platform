import { ProcessOutputLimitError } from "./trusted/process";
import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, symlink, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import * as prettier from "prettier";
import {
  runNativeCommand,
  installPinnedTool,
  runWorkflowTools,
  type ToolPin,
} from "./native-tools";
import {
  runQuality,
  writeQualityReportSummary,
  qualityReportLimit,
  qualitySummaryLimit,
  type QualityReport,
  compareAnyBaseline,
  createTypedLinter,
  explicitAnyDebt,
  metadataOnlyCommandOutput,
  type AnyBaseline,
} from "./run";
import { classifyTests, importSpecifiers, safeSourcePath } from "./scope";
import prettierOptions from "../../.prettierrc.json";
import { compareQualitySources, type QualitySourceSnapshot } from "./source-snapshot";

const temporary: string[] = [];
async function scratch(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "agent-platform-quality-negative-"));
  temporary.push(path);
  return path;
}
afterEach(async () => {
  for (const path of temporary.splice(0)) await rm(path, { recursive: true, force: true });
});

describe("quality source denominator", () => {
  test("rejects changed bytes, added check inputs and a changed denominator during verification", () => {
    const before: QualitySourceSnapshot = {
      schema: "agent-platform.quality-source-snapshot.v1",
      files: [{ path: "ci/check.ts", sha256: "a".repeat(64) }],
      contentSha256: "a".repeat(64),
      scopeSha256: "a".repeat(64),
    };
    expect(compareQualitySources(before, structuredClone(before)).status).toBe("pass");
    expect(
      compareQualitySources(before, {
        ...before,
        files: [{ path: "ci/check.ts", sha256: "b".repeat(64) }],
      }),
    ).toMatchObject({ status: "fail", changedPaths: ["ci/check.ts"] });
    expect(
      compareQualitySources(before, {
        ...before,
        files: [...before.files, { path: "ci/new.ts", sha256: "c".repeat(64) }],
      }),
    ).toMatchObject({ status: "fail", changedPaths: ["ci/new.ts"] });
    expect(compareQualitySources(before, { ...before, scopeSha256: "b".repeat(64) })).toMatchObject(
      { status: "fail", scopeChanged: true },
    );
  });
  test("rejects path traversal and alternate separators", () => {
    for (const path of [
      "../source.ts",
      "/tmp/source.ts",
      "packages/../source.ts",
      "packages\\source.ts",
      "packages//source.ts",
    ])
      expect(safeSourcePath(path)).toBe(false);
    expect(safeSourcePath("packages/common/src/index.ts")).toBe(true);
  });

  test("parses static, dynamic and import-type edges without reading fixture strings as imports", () => {
    const source =
      "import type { A } from './canonical'; type B = typeof import('./staged'); const text = \"import('./fixture')\"; await import('./dynamic');";
    expect(importSpecifiers("source.ts", source)).toEqual(["./canonical", "./dynamic", "./staged"]);
  });

  test("keeps canonical tests and their maintained helpers while rejecting staged implementation transitively", () => {
    const canonical = "packages/common/src/index.ts",
      staged = "packages/common/src/staged.ts";
    const good = "packages/common/tests/good.test.ts",
      bad = "packages/common/tests/bad.test.ts";
    const helper = "packages/common/tests/helper.ts",
      stagedHelper = "packages/common/tests/staged-helper.ts";
    const imports = new Map([
      [good, ["./helper"]],
      [helper, ["../src/index"]],
      [canonical, []],
      [bad, ["./staged-helper"]],
      [stagedHelper, ["../src/staged"]],
      [staged, []],
    ]);
    const result = classifyTests([good, bad], new Set([canonical]), new Set([staged]), imports);
    expect(result.admitted).toEqual([good]);
    expect(result.support).toEqual([helper]);
    expect(result.staged).toMatchObject([{ path: bad, dependency: staged }]);
    expect(result.staged[0]?.reason).toContain("staged");
  });

  test("does not promote source-inspection tests belonging to a staged-only Work directory", () => {
    const canonical = "packages/common/src/canonical/index.ts",
      staged = "packages/common/src/work-policy/policy.ts";
    const inspector = "packages/common/src/work-policy/inspect.test.ts";
    const result = classifyTests(
      [inspector],
      new Set([canonical]),
      new Set([staged]),
      new Map([[inspector, ["node:fs/promises"]]]),
    );
    expect(result.admitted).toEqual([]);
    expect(result.staged).toMatchObject([{ path: inspector, dependency: staged }]);
    expect(result.staged[0]?.reason).toContain("subtree");
  });
});

describe("formatting and typed correctness", () => {
  test("quality findings and nested command evidence omit raw source and output", () => {
    const fixture = "synthetic-sensitive-fixture-never-upload";
    expect(
      JSON.stringify(explicitAnyDebt("fixture.ts", `const value: any = '${fixture}';`)),
    ).not.toContain(fixture);
    const evidence = metadataOnlyCommandOutput({
      commands: [{ stdout: fixture, stderr: fixture }],
    });
    expect(JSON.stringify(evidence)).not.toContain(fixture);
    expect(evidence).toMatchObject({
      commands: [{ stdoutBytes: fixture.length, stderrBytes: fixture.length }],
    });
  });
  test("rejects an unformatted TypeScript fixture and accepts its canonical formatting", async () => {
    const source = "const answer=  42";
    const options = { ...prettierOptions, filepath: "fixture.ts" } as prettier.Options;
    expect(await prettier.check(source, options)).toBe(false);
    expect(await prettier.check(await prettier.format(source, options), options)).toBe(true);
  });

  test("typed lint rejects an unhandled Promise and accepts an awaited one", async () => {
    const root = await scratch(),
      file = join(root, "promise.ts"),
      acceptedFile = join(root, "awaited.ts"),
      project = join(root, "tsconfig.json");
    await writeFile(
      project,
      JSON.stringify({
        compilerOptions: {
          target: "ESNext",
          module: "ESNext",
          moduleResolution: "Bundler",
          strict: true,
          noEmit: true,
        },
        files: [file, acceptedFile],
      }),
    );
    await writeFile(
      file,
      "export async function operation(): Promise<void> { return; }\noperation();\n",
    );
    await writeFile(
      acceptedFile,
      "export async function operation(): Promise<void> { return; }\nawait operation();\n",
    );
    const linter = createTypedLinter(root, project);
    const rejected = await linter.lintFiles([file]);
    expect(
      rejected
        .flatMap((result) => result.messages)
        .some((message) => message.ruleId === "@typescript-eslint/no-floating-promises"),
    ).toBe(true);
    const accepted = await createTypedLinter(root, project).lintFiles([acceptedFile]);
    expect(accepted.flatMap((result) => result.messages)).toEqual([]);
  });

  test("legacy any allowance rejects new contexts and duplicate occurrences", () => {
    const existing = explicitAnyDebt("source.ts", "const value: any = input;");
    const baseline: AnyBaseline = {
      schema: "agent-platform.explicit-any-baseline.v1",
      owner: "agent-platform",
      reason: "Fixture allowance",
      entries: existing,
    };
    expect(
      compareAnyBaseline(explicitAnyDebt("source.ts", "const value: any = input;"), baseline),
    ).toEqual([]);
    expect(
      compareAnyBaseline(explicitAnyDebt("source.ts", "const another: any = input;"), baseline),
    ).toHaveLength(1);
    expect(
      compareAnyBaseline(
        explicitAnyDebt("source.ts", "const value: any = input; const value: any = input;"),
        baseline,
      ),
    ).toHaveLength(1);
    expect(compareAnyBaseline([], baseline)).toEqual([]);
  });
});

test("a modified cached native binary fails hash verification without replacement", async () => {
  const cache = await scratch();
  const pin: ToolPin = {
    name: "actionlint",
    version: "1.7.12",
    url: "https://github.com/rhysd/actionlint/releases/download/v1.7.12/fixture.tar.gz",
    sha256: "a".repeat(64),
    binarySha256: createHash("sha256").update("original binary").digest("hex"),
    executable: "actionlint",
    archivePath: "actionlint",
  };
  const directory = join(cache, pin.name, pin.version, "linux-x64", pin.sha256);
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, pin.executable), "changed binary");
  const originalHash = createHash("sha256").update("original binary").digest("hex");
  await writeFile(
    join(directory, "receipt.json"),
    JSON.stringify({
      schema: "agent-platform.verified-ci-tool-cache.v1",
      name: pin.name,
      version: pin.version,
      platform: "linux-x64",
      archiveSha256: pin.sha256,
      binarySha256: originalHash,
      executable: pin.executable,
    }),
  );
  await expect(installPinnedTool(pin, cache)).rejects.toThrow("hash or receipt mismatch");
  const receipt = await Bun.file(join(directory, "receipt.json")).json();
  receipt.binarySha256 = createHash("sha256").update("changed binary").digest("hex");
  await writeFile(join(directory, "receipt.json"), JSON.stringify(receipt));
  await expect(installPinnedTool(pin, cache)).rejects.toThrow("hash or receipt mismatch");
});

/** Opt in after the pinned binaries have been independently installed/read back. No network in the default test lane. */
test.skipIf(process.env.QUALITY_NATIVE_TESTS !== "1")(
  "workflow tools reject unsafe interpolation and an unknown needs edge",
  async () => {
    const root = await scratch(),
      path = ".github/workflows/unsafe.yml";
    await mkdir(join(root, ".github/workflows"), { recursive: true });
    await mkdir(join(root, "ci/quality"), { recursive: true });
    const sourceRoot = resolve(import.meta.dir, "../..");
    await writeFile(
      join(root, "ci/quality/tool-pins.json"),
      await Bun.file(join(sourceRoot, "ci/quality/tool-pins.json")).text(),
    );
    await writeFile(
      join(root, path),
      'name: unsafe\non: pull_request_target\npermissions: write-all\njobs:\n  unsafe:\n    needs: missing\n    runs-on: ubuntu-24.04\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo "${{ github.event.pull_request.title }}"\n',
    );
    const result = await runWorkflowTools(root, [path]);
    expect(result.commands.find((command) => command.name === "actionlint")?.exitCode).not.toBe(0);
    const security = result.commands.find((command) => command.name === "zizmor");
    expect(security?.exitCode).not.toBe(0);
    expect((security?.stdout ?? "") + (security?.stderr ?? "")).toContain("template-injection");
  },
);

test("actual native command enforces combined raw byte limits before text decoding", async () => {
  for (const size of [0, 63, 64, 65]) {
    for (const stream of ["stdout", "stderr", "both"] as const) {
      const first = stream === "stderr" ? 0 : stream === "both" ? Math.floor(size / 2) : size;
      const argv = [
        process.execPath,
        "-e",
        `import {writeSync} from 'node:fs';writeSync(1,Buffer.alloc(${first},97));writeSync(2,Buffer.alloc(${size - first},98));`,
      ];
      const pending = runNativeCommand("boundary", argv, "/tmp", 5000, {}, { maxBytes: 64 });
      if (size <= 64) {
        const result = await pending;
        expect(result.exitCode).toBe(0);
        expect(result.timedOut).toBe(false);
        expect(result.rawOutput!.stdoutBytes + result.rawOutput!.stderrBytes).toBe(size);
        expect(result.rawOutput!.captureComplete).toBe(true);
      } else {
        let failure: unknown;
        try {
          await pending;
        } catch (error) {
          failure = error;
        }
        expect(failure).toBeInstanceOf(ProcessOutputLimitError);
        expect((failure as ProcessOutputLimitError).metadata).toMatchObject({
          maxBytes: 64,
          retainedBytes: 64,
          captureComplete: false,
          retryAllowed: false,
        });
      }
    }
  }
}, 30_000);

test("native byte hashes preserve invalid/split UTF8 and explicit transport outcomes", async () => {
  const argv = [
    process.execPath,
    "-e",
    "import {writeSync} from 'node:fs';writeSync(1,Buffer.from([240]));writeSync(1,Buffer.from([159,152,128]));writeSync(2,Buffer.from([255]));",
  ];
  const result = await runNativeCommand("utf8", argv, "/tmp", 5000, {}, { maxBytes: 5 });
  expect(result.stdout).toBe("😀");
  expect(result.rawOutput).toMatchObject({
    stdoutBytes: 4,
    stderrBytes: 1,
    stdoutSha256: createHash("sha256")
      .update(Buffer.from([240, 159, 152, 128]))
      .digest("hex"),
    stderrSha256: createHash("sha256")
      .update(Buffer.from([255]))
      .digest("hex"),
    hashScope: "observed-raw-bytes",
  });
  await expect(
    runNativeCommand("utf8", argv, "/tmp", 5000, {}, { maxBytes: 4 }),
  ).rejects.toBeInstanceOf(ProcessOutputLimitError);
  for (const maxBytes of [0, -1, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1])
    await expect(
      runNativeCommand("invalid", ["/missing-fixture"], "/tmp", 5000, {}, { maxBytes }),
    ).rejects.toThrow("OUTPUT_BUDGET_INVALID");
  expect(
    (await runNativeCommand("nonzero", [process.execPath, "-e", "process.exit(17)"], "/tmp"))
      .exitCode,
  ).toBe(17);
  await expect(runNativeCommand("missing", ["/missing-fixture"], "/tmp")).rejects.toThrow();
  expect(
    (
      await runNativeCommand(
        "timeout",
        [process.execPath, "-e", "setTimeout(()=>{},5000)"],
        "/tmp",
        50,
      )
    ).timedOut,
  ).toBe(true);
});
function summaryReport(): QualityReport {
  return {
    schema: "agent-platform.quality-report.v1",
    status: "fail",
    verificationClass: "canonical-source-quality-only",
    sourceRoot: "/source",
    mode: "format",
    startedAt: "2026-10-22T00:00:00.000Z",
    finishedAt: "2026-10-22T00:00:01.000Z",
    durationMs: 1000,
    sourceIdentity: {
      actualCheckoutSha: "a".repeat(40),
      githubEventSha: null,
      sourceRegistrySha256: "b".repeat(64),
      lockSha256: "c".repeat(64),
      workingTreeHasChanges: false,
    },
    checks: [
      {
        id: "format",
        status: "fail",
        durationMs: 1,
        details: { arbitrary: "PRIVATE-ARTIFACT-SENTINEL", error: "PRIVATE-ARTIFACT-SENTINEL" },
      },
    ],
    environmentAdoptionEstablished: false,
    nativePermissionGranted: false,
    acceptanceDecision: null,
  };
}
test("summary writes and verifies the actual safe bounded report before exposing its reference", async () => {
  const root = await scratch(),
    path = join(root, "agent-platform-quality.json"),
    report = summaryReport();
  Object.assign(report, { unknownField: "PRIVATE-ARTIFACT-SENTINEL" });
  const summary = await writeQualityReportSummary(report, path, root);
  const bytes = await readFile(path);
  expect(summary).toMatchObject({
    status: "fail",
    retained: true,
    reference: "agent-platform-quality.json",
    reportBytes: bytes.length,
    reportSha256: createHash("sha256").update(bytes).digest("hex"),
    retentionDays: 14,
    diagnosticComplete: false,
  });
  expect(Buffer.byteLength(JSON.stringify(summary) + "\n")).toBeLessThanOrEqual(
    qualitySummaryLimit,
  );
  expect(bytes.length).toBeLessThanOrEqual(qualityReportLimit);
  expect(bytes.toString()).not.toContain("PRIVATE-ARTIFACT-SENTINEL");
  const observed = JSON.parse(bytes.toString());
  expect(observed.checks).toHaveLength(report.checks.length);
  expect(observed.sourceIdentity).toEqual(report.sourceIdentity);
  expect(observed.schema).toBe(report.schema);
  await expect(writeQualityReportSummary(report, path, root)).rejects.toThrow();
});

test("report retention refuses escape, symlink, stale, oversized and mismatched schema/source", async () => {
  const root = await scratch();
  const target = join(root, "agent-platform-quality.json");
  const other = await scratch();
  await symlink(join(other, "never-written.json"), target);
  await expect(writeQualityReportSummary(summaryReport(), target, root)).rejects.toThrow();
  await expect(
    writeQualityReportSummary(summaryReport(), join(other, "agent-platform-quality.json"), root),
  ).rejects.toThrow("DESTINATION_REQUIRED");
  await expect(
    writeQualityReportSummary(summaryReport(), join(root, "arbitrary.json"), root),
  ).rejects.toThrow("DESTINATION_REQUIRED");
  await symlink(other, join(root, "linked"));
  await expect(
    writeQualityReportSummary(
      summaryReport(),
      join(root, "linked/agent-platform-quality.json"),
      join(root, "linked"),
    ),
  ).rejects.toThrow("DESTINATION_REQUIRED");
  for (const report of [
    { ...summaryReport(), schema: "unknown" } as unknown as QualityReport,
    { ...summaryReport(), status: "pass" } as QualityReport,
    {
      ...summaryReport(),
      sourceIdentity: { ...summaryReport().sourceIdentity!, lockSha256: "not-a-hash" },
    },
    {
      ...summaryReport(),
      checks: [
        {
          id: "format",
          status: "fail" as const,
          durationMs: 1,
          details: { value: "x".repeat(qualityReportLimit) },
        },
      ],
    },
  ])
    await expect(
      writeQualityReportSummary(report, join(other, "agent-platform-quality.json"), other),
    ).rejects.toThrow();
  expect(await Bun.file(join(other, "agent-platform-quality.json")).exists()).toBe(false);
});

test("actual quality catch retains a finite label/hash without exporting the failing path", async () => {
  const root = await scratch();
  const report = await runQuality(join(root, "PRIVATE-CATCH-SENTINEL"), "format");
  expect(report.status).toBe("fail");
  const scope = report.checks.find((c) => c.id === "scope")!;
  expect(scope.details).toMatchObject({
    failureKind: "quality-check-exception",
    captureComplete: false,
    retryAllowed: false,
  });
  expect(JSON.stringify(scope.details)).not.toContain("PRIVATE-CATCH-SENTINEL");
  expect(JSON.stringify(scope.details)).not.toContain('"error":');
});

test("actual quality CLI summary preflights before checks and preserves legacy default JSON", async () => {
  const root = resolve(import.meta.dir, "../.."),
    temporaryRoot = await scratch();
  const preflight = await runNativeCommand(
    "summary-preflight",
    [process.execPath, "--no-env-file", "--no-install", join(root, "ci/quality/run.ts"), "format"],
    root,
    10_000,
    { QUALITY_REPORT_OUTPUT: "summary", QUALITY_REPORT_PATH: "", RUNNER_TEMP: temporaryRoot },
  );
  expect(preflight.exitCode).toBe(1);
  expect(preflight.timedOut).toBe(false);
  expect(JSON.parse(preflight.stdout)).toMatchObject({
    schema: "agent-platform.quality-report-summary.v1",
    status: "fail",
    retained: false,
    reference: null,
    diagnosticComplete: false,
  });
  expect(preflight.stderr).toBe("");
  const legacy = await runNativeCommand(
    "legacy-quality-cli",
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      "-e",
      `delete process.env.QUALITY_REPORT_OUTPUT; const child=Bun.spawn([process.execPath,'--no-env-file','--no-install',${JSON.stringify(join(root, "ci/quality/run.ts"))},'format','--root',${JSON.stringify(temporaryRoot)}],{stdout:'inherit',stderr:'inherit',env:process.env});process.exitCode=await child.exited;`,
    ],
    root,
    10_000,
    { QUALITY_REPORT_PATH: "" },
  );
  expect(JSON.parse(legacy.stdout)).toMatchObject({
    schema: "agent-platform.quality-report.v1",
    status: "fail",
  });
  expect(legacy.exitCode).toBe(1);
}, 30_000);
