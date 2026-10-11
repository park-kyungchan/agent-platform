import { constants } from "node:fs";
import { access, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { dirname, join, resolve, sep } from "node:path";

export const EXPECTED_ORIGIN = "https://github.com/D4Vinci/Scrapling.git";

export interface RuntimeValidation {
  cliPassed: boolean;
  dynamicPassed: boolean;
  staticDidNotContainToken: boolean;
  dynamicContainedToken: boolean;
}

export interface ActiveRuntime {
  schemaVersion: 1;
  upstreamSha: string;
  candidateId: string;
  runtimeDirectory: string;
  pythonExecutable: string;
  cliExecutable: string;
  browserExecutable: string;
  promotedAt: string;
  validation: RuntimeValidation;
  dependencyFreezeSha256: string;
}

export interface CommandAttempt {
  step: string;
  argv: string[];
  startedAt: string;
  durationMs: number;
  exitCode: number;
  timedOut: boolean;
  stdout: string;
  stderr: string;
}

export interface UpstreamUpdateReceipt {
  schemaVersion: 1;
  attemptId: string;
  startedAt: string;
  finishedAt: string;
  status: "promoted" | "unchanged" | "failed";
  checkout: string;
  beforeSha: string | null;
  afterSha: string | null;
  previousActiveSha: string | null;
  activeSha: string | null;
  activeRuntimePreserved: boolean;
  commands: CommandAttempt[];
  failures: { step: string; message: string }[];
  candidate: ActiveRuntime | null;
}

export interface UpstreamUpdateOptions {
  projectRoot: string;
  checkout?: string;
  runtimeRoot?: string;
  expectedOrigin?: string;
  expectedBranch?: string;
  pythonExecutable?: string;
  browserExecutable?: string;
  timeoutMs?: number;
  commandTimeoutMs?: number;
  extras?: "fetchers" | "rag";
}

export interface SuccessfulUpdateCheck {
  schemaVersion: 1;
  attemptId: string;
  checkedAt: string;
  upstreamSha: string;
  status: "promoted" | "unchanged";
  activeSha: string;
}

export interface UpstreamStatus {
  activeRuntime: ActiveRuntime | null;
  lastSuccessfulCheck: SuccessfulUpdateCheck | null;
  latestAttemptSummary: {
    attemptId: string;
    finishedAt: string;
    status: "promoted" | "unchanged" | "failed";
    activeSha: string | null;
    failures: { step: string; message: string }[];
  } | null;
}

export interface RuntimeBuildContext {
  sha: string;
  candidateId: string;
  checkout: string;
  runtimeRoot: string;
  pythonExecutable: string;
  browserExecutable: string;
  extras: "fetchers" | "rag";
  run: (step: string, argv: string[], cwd?: string) => Promise<string>;
}

/** Injection is for isolated safety tests. The production CLI uses the real builder. */
export interface UpstreamDependencies {
  prepareRuntime?: (context: RuntimeBuildContext) => Promise<ActiveRuntime>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalizeOrigin(value: string): string {
  return value.trim().replace(/\/$/, "").replace(/\.git$/, "");
}

function assertValidation(value: unknown): value is RuntimeValidation {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return ["cliPassed", "dynamicPassed", "staticDidNotContainToken", "dynamicContainedToken"]
    .every((key) => record[key] === true);
}

export async function resolveActiveRuntime(runtimeRoot: string): Promise<ActiveRuntime | null> {
  let input: unknown;
  try {
    input = JSON.parse(await readFile(join(runtimeRoot, "active.json"), "utf8"));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw new Error(`Invalid active runtime pointer: ${errorMessage(error)}`);
  }
  if (!input || typeof input !== "object") throw new Error("Invalid active runtime pointer object");
  const value = input as Record<string, unknown>;
  const required = ["upstreamSha", "candidateId", "runtimeDirectory", "pythonExecutable", "cliExecutable",
    "browserExecutable", "promotedAt", "dependencyFreezeSha256"];
  if (value.schemaVersion !== 1 || required.some((key) => typeof value[key] !== "string") ||
    !/^[a-f0-9]{40}$/.test(String(value.upstreamSha)) ||
    !/^[a-f0-9]{64}$/.test(String(value.dependencyFreezeSha256)) ||
    !assertValidation(value.validation)) throw new Error("Active runtime pointer failed its contract");
  const runtime = value as unknown as ActiveRuntime;
  const candidateRoot = resolve(runtimeRoot, "candidates") + sep;
  if (!resolve(runtime.runtimeDirectory).startsWith(candidateRoot) ||
    runtime.cliExecutable !== join(runtime.runtimeDirectory, "venv/bin/scrapling") ||
    runtime.pythonExecutable !== join(runtime.runtimeDirectory, "venv/bin/python")) {
    throw new Error("Active runtime paths are outside the candidate layout");
  }
  await Promise.all([runtime.cliExecutable, runtime.pythonExecutable, runtime.browserExecutable]
    .map((path) => access(path, constants.X_OK)));
  return runtime;
}

async function readOptionalRecord(path: string): Promise<Record<string, unknown> | null> {
  let value: unknown;
  try {
    value = JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw new Error(`Invalid state at ${path}: ${errorMessage(error)}`);
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`Invalid state object at ${path}`);
  return value as Record<string, unknown>;
}

export async function readUpstreamStatus(runtimeRoot: string): Promise<UpstreamStatus> {
  const [activeRuntime, check, attempt] = await Promise.all([
    resolveActiveRuntime(runtimeRoot), readOptionalRecord(join(runtimeRoot, "last-successful-check.json")),
    readOptionalRecord(join(runtimeRoot, "latest-attempt.json")),
  ]);
  let lastSuccessfulCheck: SuccessfulUpdateCheck | null = null;
  if (check) {
    if (check.schemaVersion !== 1 || typeof check.attemptId !== "string" || typeof check.checkedAt !== "string" ||
      typeof check.upstreamSha !== "string" || typeof check.activeSha !== "string" ||
      (check.status !== "promoted" && check.status !== "unchanged")) throw new Error("Successful update check failed its contract");
    lastSuccessfulCheck = { schemaVersion: 1, attemptId: check.attemptId, checkedAt: check.checkedAt,
      upstreamSha: check.upstreamSha, activeSha: check.activeSha, status: check.status };
  }
  let latestAttemptSummary: UpstreamStatus["latestAttemptSummary"] = null;
  if (attempt) {
    if (attempt.schemaVersion !== 1 || typeof attempt.attemptId !== "string" || typeof attempt.finishedAt !== "string" ||
      (attempt.status !== "promoted" && attempt.status !== "unchanged" && attempt.status !== "failed") ||
      (attempt.activeSha !== null && typeof attempt.activeSha !== "string") || !Array.isArray(attempt.failures)) {
      throw new Error("Latest update attempt failed its contract");
    }
    const failures = attempt.failures.map((failure: unknown) => {
      if (!failure || typeof failure !== "object" || !("step" in failure) || !("message" in failure) ||
        typeof failure.step !== "string" || typeof failure.message !== "string") throw new Error("Invalid attempt failure record");
      return { step: failure.step, message: failure.message };
    });
    latestAttemptSummary = { attemptId: attempt.attemptId, finishedAt: attempt.finishedAt,
      status: attempt.status, activeSha: attempt.activeSha, failures };
  }
  return { activeRuntime, lastSuccessfulCheck, latestAttemptSummary };
}

export async function acquireUpdateLock(runtimeRoot: string): Promise<() => Promise<void>> {
  await mkdir(runtimeRoot, { recursive: true });
  const lock = join(runtimeRoot, "update.lock");
  try {
    await mkdir(lock);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") {
      throw new Error(`Update lock already exists at ${lock}; inspect its owner before manual recovery`);
    }
    throw error;
  }
  try {
    await writeFile(join(lock, "owner.json"), JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() }) + "\n");
  } catch (error) {
    await rm(lock, { recursive: true });
    throw error;
  }
  return () => rm(lock, { recursive: true });
}

async function atomicJson(path: string, value: unknown): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const temporary = `${path}.${crypto.randomUUID()}.tmp`;
  await writeFile(temporary, JSON.stringify(value, null, 2) + "\n", { flag: "wx" });
  await rename(temporary, path);
}

export async function prepareRuntime(context: RuntimeBuildContext): Promise<ActiveRuntime> {
  const runtimeDirectory = join(context.runtimeRoot, "candidates", context.candidateId);
  const source = join(runtimeDirectory, "source");
  const archive = join(runtimeDirectory, "source.tar");
  const venv = join(runtimeDirectory, "venv");
  await mkdir(source, { recursive: true });
  await context.run("archive-source", ["git", "-C", context.checkout, "archive", "--format=tar", `--output=${archive}`, context.sha]);
  await context.run("extract-source", ["tar", "-xf", archive, "-C", source]);
  await context.run("create-venv", [context.pythonExecutable, "-m", "venv", venv]);
  const pythonExecutable = join(venv, "bin/python");
  const cliExecutable = join(venv, "bin/scrapling");
  await context.run("install-dependencies", [pythonExecutable, "-m", "pip", "install", "--disable-pip-version-check",
    "--no-input", `${source}[${context.extras}]`]);
  await context.run("dependency-consistency", [pythonExecutable, "-m", "pip", "check"]);
  const freeze = await context.run("dependency-freeze", [pythonExecutable, "-m", "pip", "freeze", "--all"]);
  await writeFile(join(runtimeDirectory, "dependencies.freeze.txt"), freeze);
  await context.run("cli-help", [cliExecutable, "--help"]);
  await access(context.browserExecutable, constants.X_OK);
  const token = `DYNAMIC_${crypto.randomUUID().replaceAll("-", "")}`;
  // CLI promotion remains an independent static/dynamic check; capture uses the API serialization bridge.
  const fixture = `<!doctype html><html><body><h1>Static fixture</h1><script>setTimeout(()=>{const e=document.createElement('p');e.id='ready';e.textContent=['DYNAMIC','${token.slice(8)}'].join('_');document.body.append(e)},120)</script></body></html>`;
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(fixture, { headers: { "Content-Type": "text/html" } }) });
  const staticFile = join(runtimeDirectory, "validation-static.txt");
  const dynamicFile = join(runtimeDirectory, "validation-dynamic.txt");
  try {
    const url = `http://127.0.0.1:${server.port}/fixture`;
    await context.run("static-fixture", [cliExecutable, "extract", "get", url, staticFile]);
    await context.run("dynamic-fixture", [cliExecutable, "extract", "fetch", url, dynamicFile, "--headless",
      "--executable-path", context.browserExecutable, "--wait-selector", "#ready", "--css-selector", "#ready", "--timeout", "20000"]);
    const staticText = await readFile(staticFile, "utf8");
    const dynamicText = await readFile(dynamicFile, "utf8");
    const validation: RuntimeValidation = { cliPassed: true, dynamicPassed: dynamicText.includes(token),
      staticDidNotContainToken: !staticText.includes(token), dynamicContainedToken: dynamicText.includes(token) };
    if (!assertValidation(validation)) throw new Error("Dynamic fixture did not prove JavaScript rendering against its static baseline");
    const runtime: ActiveRuntime = { schemaVersion: 1, upstreamSha: context.sha, candidateId: context.candidateId,
      runtimeDirectory, pythonExecutable, cliExecutable, browserExecutable: context.browserExecutable,
      promotedAt: new Date().toISOString(), validation, dependencyFreezeSha256: createHash("sha256").update(freeze).digest("hex") };
    await atomicJson(join(runtimeDirectory, "validation.json"), runtime);
    return runtime;
  } finally {
    await server.stop(true);
  }
}

export async function updateUpstream(options: UpstreamUpdateOptions, dependencies: UpstreamDependencies = {}): Promise<UpstreamUpdateReceipt> {
  const projectRoot = resolve(options.projectRoot);
  const checkout = resolve(options.checkout ?? join(projectRoot, "upstream"));
  const runtimeRoot = resolve(options.runtimeRoot ?? join(projectRoot, ".runtime"));
  const attemptId = `${new Date().toISOString().replaceAll(/[:.]/g, "-")}-${crypto.randomUUID().slice(0, 8)}`;
  const receipt: UpstreamUpdateReceipt = { schemaVersion: 1, attemptId, startedAt: new Date().toISOString(),
    finishedAt: "", status: "failed", checkout, beforeSha: null, afterSha: null, previousActiveSha: null,
    activeSha: null, activeRuntimePreserved: true, commands: [], failures: [], candidate: null };
  const timeoutMs = options.timeoutMs ?? 900_000;
  const commandTimeoutMs = options.commandTimeoutMs ?? 600_000;
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || !Number.isFinite(commandTimeoutMs) || commandTimeoutMs <= 0) {
    throw new Error("Timeouts must be positive finite milliseconds");
  }
  const deadline = Date.now() + timeoutMs;
  let step = "acquire-lock";
  let release: (() => Promise<void>) | undefined;
  const run = async (label: string, argv: string[], cwd = projectRoot): Promise<string> => {
    step = label;
    const remaining = Math.min(commandTimeoutMs, deadline - Date.now());
    if (remaining <= 0) throw new Error(`Update exceeded ${timeoutMs} ms total timeout`);
    const started = Date.now();
    const process = Bun.spawn(["/usr/bin/timeout", "--signal=TERM", "--kill-after=5s", `${remaining / 1000}s`, ...argv], {
      cwd, stdout: "pipe", stderr: "pipe", env: { ...Bun.env, GIT_TERMINAL_PROMPT: "0", PIP_NO_INPUT: "1", PYTHONDONTWRITEBYTECODE: "1" },
    });
    const [stdout, stderr, exitCode] = await Promise.all([new Response(process.stdout).text(), new Response(process.stderr).text(), process.exited]);
    receipt.commands.push({ step: label, argv, startedAt: new Date(started).toISOString(), durationMs: Date.now() - started,
      exitCode, timedOut: exitCode === 124 || exitCode === 137, stdout: stdout.slice(-16_000), stderr: stderr.slice(-16_000) });
    if (exitCode !== 0) throw new Error(`${label} exited ${exitCode}: ${stderr.slice(-2000)}`);
    return stdout;
  };
  try {
    release = await acquireUpdateLock(runtimeRoot);
    step = "read-active-runtime";
    const previous = await resolveActiveRuntime(runtimeRoot);
    receipt.previousActiveSha = previous?.upstreamSha ?? null;
    receipt.activeSha = receipt.previousActiveSha;
    const origin = (await run("check-origin", ["git", "-C", checkout, "remote", "get-url", "origin"])).trim();
    if (normalizeOrigin(origin) !== normalizeOrigin(options.expectedOrigin ?? EXPECTED_ORIGIN)) throw new Error(`Unexpected origin: ${origin}`);
    const branch = (await run("check-branch", ["git", "-C", checkout, "branch", "--show-current"])).trim();
    if (branch !== (options.expectedBranch ?? "main")) throw new Error(`Unexpected branch: ${branch || "detached HEAD"}`);
    const dirty = (await run("check-clean-checkout", ["git", "-C", checkout, "status", "--porcelain=v1", "--untracked-files=all"])).trim();
    if (dirty) throw new Error(`Dirty upstream checkout refused: ${dirty}`);
    receipt.beforeSha = (await run("before-sha", ["git", "-C", checkout, "rev-parse", "HEAD"])).trim();
    receipt.afterSha = receipt.beforeSha;
    await run("pull-main", ["git", "-C", checkout, "pull", "--ff-only", "origin", options.expectedBranch ?? "main"]);
    receipt.afterSha = (await run("after-sha", ["git", "-C", checkout, "rev-parse", "HEAD"])).trim();
    const remoteSha = (await run("remote-sha", ["git", "-C", checkout, "rev-parse", `origin/${options.expectedBranch ?? "main"}`])).trim();
    if (remoteSha !== receipt.afterSha) throw new Error("Local HEAD differs from origin main after fast-forward pull");
    if (previous?.upstreamSha === receipt.afterSha) {
      receipt.status = "unchanged";
    } else {
      step = "prepare-runtime";
      const runtime = await (dependencies.prepareRuntime ?? prepareRuntime)({ sha: receipt.afterSha,
        candidateId: `${receipt.afterSha}-${attemptId}`, checkout, runtimeRoot,
        pythonExecutable: options.pythonExecutable ?? "/usr/bin/python3",
        browserExecutable: options.browserExecutable ?? "/usr/bin/google-chrome", extras: options.extras ?? "fetchers", run });
      receipt.candidate = runtime;
      if (runtime.upstreamSha !== receipt.afterSha || !assertValidation(runtime.validation)) throw new Error("Candidate validation or upstream SHA mismatch");
      step = "verify-checkout-remained-clean";
      const postDirty = (await run(step, ["git", "-C", checkout, "status", "--porcelain=v1", "--untracked-files=all"])).trim();
      if (postDirty) throw new Error(`Runtime setup modified upstream checkout: ${postDirty}`);
      step = "promote-runtime";
      await atomicJson(join(runtimeRoot, "active.json"), runtime);
      receipt.activeSha = runtime.upstreamSha;
      receipt.activeRuntimePreserved = false;
      receipt.status = "promoted";
    }
  } catch (error) {
    receipt.failures.push({ step, message: errorMessage(error) });
  } finally {
    receipt.finishedAt = new Date().toISOString();
    try {
      await atomicJson(join(runtimeRoot, "attempts", `${attemptId}.json`), receipt);
      await atomicJson(join(runtimeRoot, "latest-attempt.json"), receipt);
      if (receipt.status !== "failed") {
        await atomicJson(join(runtimeRoot, "last-successful-check.json"), {
          schemaVersion: 1, attemptId, checkedAt: receipt.finishedAt, upstreamSha: receipt.afterSha,
          status: receipt.status, activeSha: receipt.activeSha,
        });
      }
    } finally {
      if (release) await release();
    }
  }
  return receipt;
}
