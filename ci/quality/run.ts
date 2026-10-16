import { createHash } from "node:crypto";
import { verifyOntologyBuild } from "./ontology-build";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { ESLint } from "eslint";
import * as prettier from "prettier";
import ts from "typescript";
import tseslint from "typescript-eslint";
import {
  readArchitectureSnapshot,
  validateArchitecture,
  type ArchitecturePolicy,
} from "./architecture-quality";
import {
  installPinnedTool,
  readToolPins,
  runNativeCommand,
  runWorkflowTools,
} from "./native-tools";
import { runSecretCheck, verifySecretCanary, type VerifiedSecretTool } from "./secret-check";
import { exactSourceFile, readQualityScope, type QualityScope } from "./scope";
import {
  captureQualitySources,
  compareQualitySources,
  type QualitySourceSnapshot,
} from "./source-snapshot";

export interface AnyDebt {
  path: string;
  contextSha256: string;
  count: number;
}
export interface AnyBaseline {
  schema: "agent-platform.explicit-any-baseline.v1";
  owner: "agent-platform";
  reason: string;
  entries: AnyDebt[];
}
export interface CheckResult {
  id: string;
  status: "pass" | "fail" | "skipped";
  durationMs: number;
  details: unknown;
}
export interface QualityReport {
  schema: "agent-platform.quality-report.v1";
  status: "pass" | "fail";
  verificationClass: "canonical-source-quality-only";
  sourceRoot: string;
  mode: string;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  scope?: QualityScope;
  checks: CheckResult[];
  sourceIdentity?: {
    actualCheckoutSha: string | null;
    githubEventSha: string | null;
    sourceRegistrySha256: string;
    lockSha256: string;
    workingTreeHasChanges: boolean | null;
  };
  sourceSnapshot?: QualitySourceSnapshot;
  environmentAdoptionEstablished: false;
  nativePermissionGranted: false;
  acceptanceDecision: null;
}

const hash = (text: string): string => createHash("sha256").update(text).digest("hex");
const anyKey = (entry: AnyDebt): string => entry.path + ":" + entry.contextSha256;

/** Native diagnostics can contain source text; retain command identity without uploading raw output. */
export function metadataOnlyCommandOutput(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(metadataOnlyCommandOutput);
  if (!value || typeof value !== "object") return value;
  const result: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    if (["stdout", "stderr", "example"].includes(key) && typeof item === "string") {
      result[key + "Sha256"] = hash(item);
      result[key + "Bytes"] = Buffer.byteLength(item);
    } else result[key] = metadataOnlyCommandOutput(item);
  }
  return result;
}

/** Anchor legacy any occurrences to their AST context; new contexts and extra copies fail. */
export function explicitAnyDebt(path: string, source: string): AnyDebt[] {
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  const debts = new Map<string, AnyDebt>();
  const visit = (node: ts.Node): void => {
    if (node.kind === ts.SyntaxKind.AnyKeyword) {
      const context = node.parent.getText(file).replace(/\s+/g, " ").trim();
      const contextSha256 = hash(context),
        key = path + ":" + contextSha256;
      const current = debts.get(key);
      if (current) current.count++;
      else debts.set(key, { path, contextSha256, count: 1 });
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return [...debts.values()].sort((left, right) => anyKey(left).localeCompare(anyKey(right)));
}

export function compareAnyBaseline(current: AnyDebt[], baseline: AnyBaseline): AnyDebt[] {
  if (
    baseline.schema !== "agent-platform.explicit-any-baseline.v1" ||
    baseline.owner !== "agent-platform" ||
    !Array.isArray(baseline.entries)
  )
    throw Error("Unknown explicit-any audit baseline.");
  const allowed = new Map<string, number>();
  for (const entry of baseline.entries) {
    if (
      !Number.isInteger(entry.count) ||
      entry.count < 1 ||
      !/^[a-f0-9]{64}$/.test(entry.contextSha256) ||
      allowed.has(anyKey(entry))
    )
      throw Error("Invalid or duplicate explicit-any audit allowance.");
    allowed.set(anyKey(entry), entry.count);
  }
  return current.filter((entry) => entry.count > (allowed.get(anyKey(entry)) ?? 0));
}

async function collectAnyDebt(scope: QualityScope): Promise<AnyDebt[]> {
  const result: AnyDebt[] = [];
  for (const path of scope.lintFiles)
    result.push(...explicitAnyDebt(path, await exactSourceFile(scope.root, path)));
  return result;
}

export async function checkFormatting(
  scope: QualityScope,
  write = false,
): Promise<{ files: number; changed: string[]; status: "pass" | "fail" }> {
  const config = JSON.parse(
    await exactSourceFile(scope.root, ".prettierrc.json"),
  ) as prettier.Options;
  const changed: string[] = [];
  for (const path of scope.formatFiles) {
    const source = await exactSourceFile(scope.root, path),
      options = { ...config, filepath: join(scope.root, path) };
    if (await prettier.check(source, options)) continue;
    changed.push(path);
    if (write) await writeFile(join(scope.root, path), await prettier.format(source, options));
  }
  return {
    files: scope.formatFiles.length,
    changed,
    status: changed.length && !write ? "fail" : "pass",
  };
}

/** Programmatic flat configuration keeps the maintained configuration in TypeScript without a jiti loader. */
export function createTypedLinter(root: string, projectPath: string): ESLint {
  return new ESLint({
    cwd: root,
    overrideConfigFile: true,
    ignore: false,
    overrideConfig: [
      {
        files: ["**/*.{ts,tsx,mts,cts}"],
        languageOptions: {
          parser: tseslint.parser,
          parserOptions: { project: [projectPath], tsconfigRootDir: root, sourceType: "module" },
        },
        plugins: { "@typescript-eslint": tseslint.plugin },
        rules: {
          "@typescript-eslint/no-floating-promises": "error",
          "@typescript-eslint/no-misused-promises": "error",
          "@typescript-eslint/await-thenable": "error",
          "@typescript-eslint/switch-exhaustiveness-check": "error",
          "no-unsafe-finally": "error",
          "no-unreachable": "error",
        },
      },
    ],
  });
}

export async function checkTypedLint(scope: QualityScope): Promise<{
  files: number;
  findings: {
    path: string;
    line: number;
    column: number;
    ruleId: string | null;
    severity: number;
    message: string;
  }[];
  anyAudit: { occurrences: number; newDebt: AnyDebt[] };
  status: "pass" | "fail";
}> {
  const temporary = await mkdtemp(
    join(process.env.RUNNER_TEMP ?? tmpdir(), "agent-platform-quality-types-"),
  );
  try {
    const projectPath = join(temporary, "tsconfig.json");
    await writeFile(
      projectPath,
      JSON.stringify(
        {
          extends: join(scope.root, "ci/quality/tsconfig.json"),
          files: scope.lintFiles.map((path) => join(scope.root, path)),
          include: [],
        },
        null,
        2,
      ) + "\n",
    );
    const results = await createTypedLinter(scope.root, projectPath).lintFiles(scope.lintFiles);
    const findings = results.flatMap((result) =>
      result.messages.map((message) => ({
        path: result.filePath.slice(scope.root.length + 1),
        line: message.line,
        column: message.column,
        ruleId: message.ruleId,
        severity: message.severity,
        message: "Typed lint finding; inspect the recorded source location.",
      })),
    );
    const current = await collectAnyDebt(scope);
    const baseline = JSON.parse(
      await exactSourceFile(scope.root, "ci/quality/any-baseline.json"),
    ) as AnyBaseline;
    const newDebt = compareAnyBaseline(current, baseline);
    return {
      files: scope.lintFiles.length,
      findings,
      anyAudit: { occurrences: current.reduce((sum, entry) => sum + entry.count, 0), newDebt },
      status: findings.length || newDebt.length ? "fail" : "pass",
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export type QualityMode =
  "all" | "format" | "format:write" | "lint" | "workflows" | "architecture" | "baseline:any";

export async function runQuality(rootPath: string, mode: QualityMode): Promise<QualityReport> {
  const started = performance.now(),
    startedAt = new Date().toISOString(),
    root = resolve(rootPath);
  const checks: CheckResult[] = [];
  let scope: QualityScope | undefined;
  let sourceIdentity: QualityReport["sourceIdentity"];
  let sourceSnapshot: QualitySourceSnapshot | undefined;
  const perform = async (
    id: string,
    callback: () => Promise<unknown>,
    successful: (result: unknown) => boolean = () => true,
  ): Promise<boolean> => {
    const phase = performance.now();
    try {
      const details = await callback(),
        pass = successful(details);
      checks.push({
        id,
        status: pass ? "pass" : "fail",
        durationMs: Math.round(performance.now() - phase),
        details: metadataOnlyCommandOutput(details),
      });
      return pass;
    } catch (error) {
      checks.push({
        id,
        status: "fail",
        durationMs: Math.round(performance.now() - phase),
        details: { error: error instanceof Error ? error.message : String(error) },
      });
      return false;
    }
  };
  const allowed = await perform("mutation-policy", async () => {
    if (process.env.GITHUB_ACTIONS === "true" && ["format:write", "baseline:any"].includes(mode))
      throw Error("Hosted CI cannot rewrite formatting or its own legacy type-debt allowance");
    return { hostedMutationAllowed: false };
  });
  if (allowed)
    await perform("scope", async () => {
      scope = await readQualityScope(root);
      return {
        canonicalTypescript: scope.canonicalTypescript.length,
        auxiliaryTypescript: scope.auxiliaryTypescript.length,
        tests: scope.testTypescript.length,
        formatter: scope.formatFiles.length,
        workflows: scope.workflows.length,
        stagedSourcesExcluded: scope.exclusions.stagedSources.length,
        stagedTestsExcluded: scope.exclusions.stagedTests.length,
        generatedExcluded: scope.exclusions.generatedSources.length,
      };
    });
  if (scope) {
    const selected = scope;
    if (!["format:write", "baseline:any"].includes(mode))
      await perform("source-snapshot", async () => {
        sourceSnapshot = await captureQualitySources(selected);
        return sourceSnapshot;
      });
    await perform("source-identity", async () => {
      const checkout = await runNativeCommand("checkout-sha", ["git", "rev-parse", "HEAD"], root);
      const dirty = await runNativeCommand(
        "working-tree-observation",
        ["git", "status", "--porcelain", "--untracked-files=normal"],
        root,
      );
      sourceIdentity = {
        actualCheckoutSha:
          checkout.exitCode === 0 && /^[a-f0-9]{40}$/.test(checkout.stdout.trim())
            ? checkout.stdout.trim()
            : null,
        githubEventSha: /^[a-f0-9]{40}$/.test(process.env.GITHUB_SHA ?? "")
          ? process.env.GITHUB_SHA!
          : null,
        sourceRegistrySha256: hash(
          await exactSourceFile(root, "architecture/capability-source-owners.json"),
        ),
        lockSha256: hash(await exactSourceFile(root, "bun.lock")),
        workingTreeHasChanges: dirty.exitCode === 0 ? Boolean(dirty.stdout.trim()) : null,
      };
      if (process.env.GITHUB_ACTIONS === "true" && !sourceIdentity.actualCheckoutSha)
        throw Error("Hosted verification must identify the actual checked-out commit");
      return sourceIdentity;
    });
    if (mode === "baseline:any")
      await perform("explicit-any-baseline-write", async () => {
        const entries = await collectAnyDebt(selected);
        const baseline: AnyBaseline = {
          schema: "agent-platform.explicit-any-baseline.v1",
          owner: "agent-platform",
          reason:
            "Reviewed legacy explicit-any AST contexts. Future contexts or extra occurrences fail the quality gate; this is an audit allowance rather than blanket lint suppression.",
          entries,
        };
        await writeFile(
          join(root, "ci/quality/any-baseline.json"),
          JSON.stringify(baseline, null, 2) + "\n",
        );
        return {
          contexts: entries.length,
          occurrences: entries.reduce((sum, entry) => sum + entry.count, 0),
          explicitBaselineMutation: true,
        };
      });
    if (mode === "all" || mode === "architecture") {
      const built = await perform(
        "build-public-ontology-package",
        () => verifyOntologyBuild(root),
        (result) => (result as { status: string }).status === "pass",
      );
      if (built)
        await perform(
          "architecture",
          async () => {
            const policy = JSON.parse(
              await exactSourceFile(root, "architecture/quality-policy.json"),
            ) as ArchitecturePolicy;
            return validateArchitecture(await readArchitectureSnapshot(root), policy);
          },
          (result) => (result as { status: string }).status === "pass",
        );
      else
        checks.push({
          id: "architecture",
          status: "skipped",
          durationMs: 0,
          details: {
            reason:
              "The public package build failed, so a complete export denominator is unavailable.",
          },
        });
    }
    if (mode === "all" || mode === "format" || mode === "format:write")
      await perform(
        "format",
        () => checkFormatting(selected, mode === "format:write"),
        (result) => (result as { status: string }).status === "pass",
      );
    if (mode === "all" || mode === "lint")
      await perform(
        "typed-lint-and-explicit-any-audit",
        () => checkTypedLint(selected),
        (result) => (result as { status: string }).status === "pass",
      );
    if (mode === "all" || mode === "workflows")
      await perform(
        "workflow-correctness-and-security",
        () => runWorkflowTools(root, selected.workflows, selected.actions),
        (result) =>
          (result as { commands: { exitCode: number; timedOut: boolean }[] }).commands.every(
            (command) => command.exitCode === 0 && !command.timedOut,
          ),
      );
    if (mode === "all")
      await perform(
        "dependency-vulnerabilities",
        () =>
          runNativeCommand(
            "bun-audit",
            [process.execPath, "audit", "--json", "--audit-level=high"],
            root,
          ),
        (result) =>
          (result as { exitCode: number; timedOut: boolean }).exitCode === 0 &&
          !(result as { timedOut: boolean }).timedOut,
      );
    if (mode === "all") {
      let installedSecretTool: VerifiedSecretTool | undefined;
      await perform(
        "maintained-source-secrets",
        async () => {
          const pin = (await readToolPins(root)).find((pin) => pin.name === "gitleaks");
          if (!pin) throw Error("The exact secret-scanner pin is missing");
          const tool = await installPinnedTool(pin);
          installedSecretTool = {
            path: tool.path,
            binarySha256: tool.binarySha256,
            pin: { name: "gitleaks", version: pin.version, sha256: pin.sha256 },
          };
          const canary = await verifySecretCanary(installedSecretTool);
          const scan = await runSecretCheck(root, selected.secretFiles, installedSecretTool);
          return { canary, scan };
        },
        (result) => (result as { scan: { status: string } }).scan.status === "pass",
      );
      if (installedSecretTool)
        await perform(
          "quality-negative-fixtures",
          () =>
            runNativeCommand(
              "quality-negative-fixtures",
              [process.execPath, "test", "ci/"],
              root,
              90_000,
              {
                QUALITY_NATIVE_TESTS: "1",
                GITLEAKS_VERIFY_BINARY: installedSecretTool!.path,
                GITLEAKS_VERIFY_BINARY_SHA256: installedSecretTool!.binarySha256,
              },
            ),
          (result) =>
            (result as { exitCode: number; timedOut: boolean }).exitCode === 0 &&
            !(result as { timedOut: boolean }).timedOut,
        );
    }
  }
  if (sourceSnapshot) {
    const initialSnapshot = sourceSnapshot;
    await perform(
      "source-currentness",
      async () =>
        compareQualitySources(
          initialSnapshot,
          await captureQualitySources(await readQualityScope(root)),
        ),
      (result) => (result as { status: string }).status === "pass",
    );
  }
  if (scope && (mode === "all" || mode === "architecture")) {
    await perform(
      "architecture-currentness",
      async () => {
        const policy = JSON.parse(
          await exactSourceFile(root, "architecture/quality-policy.json"),
        ) as ArchitecturePolicy;
        return validateArchitecture(await readArchitectureSnapshot(root), policy);
      },
      (result) => (result as { status: string }).status === "pass",
    );
  }
  return {
    schema: "agent-platform.quality-report.v1",
    status: checks.some((check) => check.status !== "pass") ? "fail" : "pass",
    verificationClass: "canonical-source-quality-only",
    sourceRoot: root,
    mode,
    startedAt,
    finishedAt: new Date().toISOString(),
    durationMs: Math.round(performance.now() - started),
    ...(scope ? { scope } : {}),
    checks,
    ...(sourceIdentity ? { sourceIdentity } : {}),
    ...(sourceSnapshot ? { sourceSnapshot } : {}),
    environmentAdoptionEstablished: false,
    nativePermissionGranted: false,
    acceptanceDecision: null,
  };
}

if (import.meta.main) {
  const allowed = new Set<QualityMode>([
    "all",
    "format",
    "format:write",
    "lint",
    "workflows",
    "architecture",
    "baseline:any",
  ]);
  const raw = process.argv.slice(2),
    mode = (raw[0] ?? "all") as QualityMode;
  const root =
    raw.length === 3 && raw[1] === "--root" ? resolve(raw[2]!) : resolve(import.meta.dir, "../..");
  let report: QualityReport;
  if (!allowed.has(mode) || (raw.length > 1 && !(raw.length === 3 && raw[1] === "--root"))) {
    const now = new Date().toISOString();
    report = {
      schema: "agent-platform.quality-report.v1",
      status: "fail",
      verificationClass: "canonical-source-quality-only",
      sourceRoot: root,
      mode,
      startedAt: now,
      finishedAt: now,
      durationMs: 0,
      checks: [
        {
          id: "arguments",
          status: "fail",
          durationMs: 0,
          details: {
            error:
              "Usage: bun ci/quality/run.ts [all|format|format:write|lint|workflows|architecture|baseline:any] [--root <path>]",
          },
        },
      ],
      environmentAdoptionEstablished: false,
      nativePermissionGranted: false,
      acceptanceDecision: null,
    };
  } else {
    // Only the canonical read-only source CLI roles prepare their runtime. Custom roots
    // and local mutation modes never acquire installer authority.
    if (
      raw.length <= 1 &&
      (mode === "all" ||
        mode === "format" ||
        mode === "lint" ||
        mode === "workflows" ||
        mode === "architecture")
    ) {
      const { continueSourceCi } = await import("./source-runtime");
      if (await continueSourceCi(mode)) process.exit(process.exitCode ?? 0);
    } else if (Bun.version !== "1.4.3") {
      throw Error(
        "Custom-root or mutating source quality requires Bun 1.4.3; automatic preparation is unavailable",
      );
    }
    report = await runQuality(root, mode);
  }
  const reportPath = process.env.QUALITY_REPORT_PATH;
  if (reportPath) {
    try {
      await mkdir(dirname(resolve(reportPath)), { recursive: true });
      await writeFile(resolve(reportPath), JSON.stringify(report, null, 2) + "\n");
    } catch (error) {
      report.status = "fail";
      report.checks.push({
        id: "report-write",
        status: "fail",
        durationMs: 0,
        details: { error: error instanceof Error ? error.message : String(error) },
      });
    }
  }
  console.log(JSON.stringify(report, null, 2));
  if (report.status !== "pass") process.exitCode = 1;
}
