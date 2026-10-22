import { operationalTimeouts } from "../operational-timeouts";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";

export interface ProcessResult {
  code: number;
  timedOut: boolean;
  stdout: Buffer;
  stderr: Buffer;
}
export interface OutputLimitMetadata {
  maxBytes: number;
  retainedBytes: number;
  observedStdoutBytes: number;
  observedStderrBytes: number;
  observedCombinedBytes: number;
  observedStdoutSha256: string;
  observedStderrSha256: string;
  hashScope: "observed-raw-bytes";
  captureComplete: false;
  limitExceeded: true;
  timedOut: boolean;
  exitCode: number | null;
  signal: string | null;
  retryAllowed: false;
}
export class ProcessOutputLimitError extends Error {
  constructor(readonly metadata: OutputLimitMetadata) {
    super("TRUSTED_CI_PROCESS_OUTPUT_LIMIT");
  }
}

/** Shared byte accounting only: no policy imports, environment or process ownership. */
export class ProcessOutputCapture {
  readonly chunks = { stdout: [] as Buffer[], stderr: [] as Buffer[] };
  private readonly hashes = { stdout: createHash("sha256"), stderr: createHash("sha256") };
  private readonly counts = { stdout: 0, stderr: 0 };
  retainedBytes = 0;
  exceeded = false;
  constructor(readonly maxBytes = 4_000_000) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0)
      throw Error("TRUSTED_CI_PROCESS_OUTPUT_BUDGET_INVALID");
  }
  observe(key: "stdout" | "stderr", chunk: Uint8Array): void {
    this.counts[key] += chunk.byteLength;
    this.hashes[key].update(chunk);
    const remaining = this.maxBytes - this.retainedBytes;
    if (remaining > 0) {
      const kept = Buffer.from(chunk.subarray(0, remaining));
      this.chunks[key].push(kept);
      this.retainedBytes += kept.length;
    }
    this.exceeded = this.counts.stdout + this.counts.stderr > this.maxBytes;
  }
  bytes(key: "stdout" | "stderr"): Buffer {
    return Buffer.concat(this.chunks[key]);
  }
  failure(timedOut: boolean, exitCode: number | null, signal: string | null) {
    return new ProcessOutputLimitError({
      maxBytes: this.maxBytes,
      retainedBytes: this.retainedBytes,
      observedStdoutBytes: this.counts.stdout,
      observedStderrBytes: this.counts.stderr,
      observedCombinedBytes: this.counts.stdout + this.counts.stderr,
      observedStdoutSha256: this.hashes.stdout.copy().digest("hex"),
      observedStderrSha256: this.hashes.stderr.copy().digest("hex"),
      hashScope: "observed-raw-bytes",
      captureComplete: false,
      limitExceeded: true,
      timedOut,
      exitCode,
      signal,
      retryAllowed: false,
    });
  }
}

/** Finite failure metadata only. An unknown transport outcome never permits replay. */
export function processFailureMetadata(error: unknown) {
  if (error instanceof ProcessOutputLimitError)
    return { failureKind: "output-limit", ...error.metadata };
  let message: unknown;
  try {
    const descriptor = error && Object.getOwnPropertyDescriptor(error, "message");
    message = descriptor && "value" in descriptor ? descriptor.value : undefined;
  } catch {
    // No accessor or arbitrary exception conversion is allowed on the diagnostic path.
  }
  return {
    failureKind: "spawn-or-transport",
    errorSha256: typeof message === "string" ? createHash("sha256").update(message).digest("hex") : null,
    captureComplete: false,
    retryAllowed: false,
  };
}

/** Bounded, non-shell child execution. Never inherit the controller environment. */
export async function fixedProcess(
  argv: readonly string[],
  options: {
    cwd: string;
    env?: Record<string, string>;
    stdin?: Uint8Array;
    timeoutMs?: number;
    maxBytes?: number;
  },
): Promise<ProcessResult> {
  if (!argv.length || !argv.every((a) => !a.includes("\0")))
    throw Error("TRUSTED_CI_FIXED_ARGV_REQUIRED");
  const capture = new ProcessOutputCapture(options.maxBytes);
  const child = spawn(argv[0]!, argv.slice(1), {
    cwd: options.cwd,
    env: { PATH: "/usr/local/bin:/usr/bin:/bin", LANG: "C.UTF-8", ...options.env },
    stdio: ["pipe", "pipe", "pipe"],
    detached: true,
  });
  let timedOut = false,
    killed = false;
  const kill = () => {
    if (killed) return;
    killed = true;
    try {
      if (child.pid) process.kill(-child.pid, "SIGKILL");
      else child.kill("SIGKILL");
    } catch {
      child.kill("SIGKILL");
    }
  };
  const stdout = (chunk: Buffer) => {
    capture.observe("stdout", chunk);
    if (capture.exceeded) kill();
  };
  const stderr = (chunk: Buffer) => {
    capture.observe("stderr", chunk);
    if (capture.exceeded) kill();
  };
  let streamError: Error | undefined;
  const readError = (error: Error) => {
    streamError ??= error;
    kill();
  };
  child.stdout.on("error", readError);
  child.stderr.on("error", readError);
  child.stdout.on("data", stdout);
  child.stderr.on("data", stderr);
  child.stdin.on("error", () => {});
  child.stdin.end(options.stdin);
  const timer = setTimeout(() => {
    timedOut = true;
    kill();
  }, options.timeoutMs ?? operationalTimeouts.processMs);
  try {
    // Even spawn errors settle at close; output and the exact child are drained first.
    const result = await new Promise<{ code: number | null; signal: string | null; error?: Error }>((done) => {
      let spawnError: Error | undefined;
      child.once("error", (error) => { spawnError = error; });
      child.once("close", (code, signal) => done({ code, signal, error: spawnError ?? streamError }));
    });
    if (capture.exceeded) throw capture.failure(timedOut, result.code, result.signal);
    if (result.error) throw result.error;
    return {
      code: result.code ?? 255,
      timedOut,
      stdout: capture.bytes("stdout"),
      stderr: capture.bytes("stderr"),
    };
  } finally {
    clearTimeout(timer);
    child.stdout.off("error", readError);
    child.stderr.off("error", readError);
    child.stdout.off("data", stdout);
    child.stderr.off("data", stderr);
  }
}
export const outputMetadata = (r: ProcessResult) => ({
  code: r.code,
  timedOut: r.timedOut,
  stdoutBytes: r.stdout.length,
  stdoutSha256: createHash("sha256").update(r.stdout).digest("hex"),
  stderrBytes: r.stderr.length,
  stderrSha256: createHash("sha256").update(r.stderr).digest("hex"),
});
