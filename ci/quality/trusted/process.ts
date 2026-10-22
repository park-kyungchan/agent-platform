import { operationalTimeouts } from "../operational-timeouts";
import { spawn } from "node:child_process";
import { insist, sha256 } from "./policy";

export interface ProcessResult {
  code: number;
  timedOut: boolean;
  stdout: Buffer;
  stderr: Buffer;
}
/** Bounded, non-shell, credential-free child execution. Never inherit the controller environment. */
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
  insist(argv.length > 0 && argv.every((a) => !a.includes("\0")), "FIXED_ARGV_REQUIRED");
  const child = spawn(argv[0]!, argv.slice(1), {
    cwd: options.cwd,
    env: { PATH: "/usr/local/bin:/usr/bin:/bin", LANG: "C.UTF-8", ...options.env },
    stdio: ["pipe", "pipe", "pipe"],
    detached: true,
  });
  const chunks = { stdout: [] as Buffer[], stderr: [] as Buffer[] };
  let size = 0,
    timedOut = false,
    exceeded = false;
  const kill = () => {
    try {
      process.kill(-child.pid!, "SIGKILL");
    } catch {
      child.kill("SIGKILL");
    }
  };
  for (const key of ["stdout", "stderr"] as const)
    child[key].on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size > (options.maxBytes ?? 4_000_000)) {
        exceeded = true;
        kill();
      } else chunks[key].push(Buffer.from(chunk));
    });
  child.stdin.on("error", () => {});
  child.stdin.end(options.stdin);
  const timer = setTimeout(() => {
    timedOut = true;
    kill();
  }, options.timeoutMs ?? operationalTimeouts.processMs);
  try {
    const code = await new Promise<number>((done, reject) => {
      child.once("error", reject);
      child.once("close", (code) => done(code ?? 255));
    });
    insist(!exceeded, "PROCESS_OUTPUT_LIMIT");
    return {
      code,
      timedOut,
      stdout: Buffer.concat(chunks.stdout),
      stderr: Buffer.concat(chunks.stderr),
    };
  } finally {
    clearTimeout(timer);
  }
}
export const outputMetadata = (r: ProcessResult) => ({
  code: r.code,
  timedOut: r.timedOut,
  stdoutBytes: r.stdout.length,
  stdoutSha256: sha256(r.stdout),
  stderrBytes: r.stderr.length,
  stderrSha256: sha256(r.stderr),
});
