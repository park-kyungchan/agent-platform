import { operationalTimeouts } from "./operational-timeouts";
import { createHash } from "node:crypto";
import {
  appendFile,
  chmod,
  lstat,
  mkdtemp,
  readFile,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, join, resolve, sep } from "node:path";

const release = {
  version: "1.4.3",
  url: "https://github.com/oven-sh/bun/releases/download/bun-v1.4.3/bun-linux-x64.zip",
  archiveBytes: 42015139,
  archiveSha256: "3e730528d89775f03f87f05c06787c4df4099d0a37f3f252e7b829cb23816e18",
  binaryBytes: 88979072,
  binarySha256: "7ce7d6b654eddeec20afddf2396317fba6dc19ab52788a27e894e1806bf8f24b",
} as const;
const qualityModes = ["all", "format", "lint", "workflows", "architecture"] as const;
export type SourceCiRole = "source-check" | (typeof qualityModes)[number];
const root = resolve(import.meta.dir, "../..");
const digest = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");

/** This finite source role is unrelated to native reviewer authentication or activation. */
export function needsSourceRuntime(
  role: SourceCiRole,
  version: string,
  env: NodeJS.ProcessEnv,
  platform = process.platform,
  arch = process.arch,
): boolean {
  if (role !== "source-check" && !qualityModes.includes(role))
    throw Error("Unknown source CI runtime role");
  if (version === release.version) return false;
  const jobs =
    role === "source-check" ? ["source", "merge-integration"] : ["quality", "merge-integration"];
  if (
    version !== "1.3.14" ||
    env.GITHUB_ACTIONS !== "true" ||
    !jobs.includes(env.GITHUB_JOB ?? "") ||
    env.RUNNER_OS !== "Linux" ||
    env.RUNNER_ARCH !== "X64" ||
    platform !== "linux" ||
    arch !== "x64"
  )
    throw Error(
      "Source checks require Bun 1.4.3; only explicit GitHub Actions Linux X64 source roles may prepare it from 1.3.14",
    );
  if (env.NODE_OPTIONS || env.BUN_OPTIONS)
    throw Error("Source runtime preparation refuses preload/injector options");
  return true;
}

export function verifySourceRuntimeArtifact(kind: "archive" | "binary", bytes: Uint8Array): void {
  if (kind !== "archive" && kind !== "binary") throw Error("Unknown source runtime artifact");
  if (
    bytes.byteLength !== release[kind === "archive" ? "archiveBytes" : "binaryBytes"] ||
    digest(bytes) !== release[kind === "archive" ? "archiveSha256" : "binarySha256"]
  )
    throw Error("Pinned source Bun " + kind + " size or SHA256 mismatch");
}

/** Never accept a symlink, shared writable directory, or another user's command file. */
export async function sourceRuntimePaths(
  env: NodeJS.ProcessEnv,
): Promise<{ temporary: string; commandFile: string }> {
  const temporary = env.RUNNER_TEMP,
    commandFile = env.GITHUB_PATH;
  if (
    !temporary ||
    !commandFile ||
    !isAbsolute(temporary) ||
    !isAbsolute(commandFile) ||
    resolve(temporary) !== temporary ||
    resolve(commandFile) !== commandFile ||
    /[\r\n\0]/.test(temporary + commandFile) ||
    temporary === "/" ||
    !commandFile.startsWith(temporary + sep)
  )
    throw Error("Source runtime requires an owned RUNNER_TEMP and contained GITHUB_PATH");
  const uid = process.getuid?.();
  for (const [path, directory] of [
    [temporary, true],
    [commandFile, false],
  ] as const) {
    const info = await lstat(path);
    if (
      (await realpath(path)) !== path ||
      info.uid !== uid ||
      info.mode & 0o022 ||
      (directory ? !info.isDirectory() : !info.isFile() || info.nlink !== 1 || info.size > 65536)
    )
      throw Error("Source runtime path must be exact, owned, and bounded");
  }
  return { temporary, commandFile };
}

async function boundedBytes(
  stream: ReadableStream<Uint8Array> | null,
  maximum: number,
): Promise<Uint8Array> {
  if (!stream) throw Error("Source runtime output body missing");
  const reader = stream.getReader(),
    chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.byteLength;
      if (size > maximum) throw Error("Source runtime output exceeds its byte limit");
      chunks.push(next.value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}

async function download(signal: AbortSignal): Promise<Uint8Array> {
  let url: string = release.url;
  for (let redirects = 0; redirects < 4; redirects++) {
    const target = new URL(url);
    if (
      target.protocol !== "https:" ||
      target.username ||
      target.password ||
      target.port ||
      ![
        "github.com",
        "release-assets.githubusercontent.com",
        "objects.githubusercontent.com",
      ].includes(target.hostname)
    )
      throw Error("Unexpected public Bun release redirect");
    const response = await fetch(url, { redirect: "manual", credentials: "omit", signal });
    if ([301, 302, 303, 307, 308].includes(response.status)) {
      const location = response.headers.get("location");
      await response.body?.cancel();
      if (!location) throw Error("Bun release redirect missing location");
      url = new URL(location, url).href;
      continue;
    }
    if (!response.ok) {
      await response.body?.cancel();
      throw Error("Public Bun release download failed: " + response.status);
    }
    const length = response.headers.get("content-length");
    if (length && Number(length) !== release.archiveBytes) {
      await response.body?.cancel();
      throw Error("Pinned Bun release download size mismatch");
    }
    return boundedBytes(response.body, release.archiveBytes);
  }
  throw Error("Bun release redirect limit exceeded");
}

async function fixedCommand(
  argv: string[],
  maximum: number,
  cwd: string,
  signal: AbortSignal,
): Promise<Uint8Array> {
  const child = Bun.spawn(argv, {
    cwd,
    env: { PATH: "/usr/bin:/bin", LANG: "C" },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const abort = () => {
    child.kill();
  };
  signal.addEventListener("abort", abort, { once: true });
  if (signal.aborted) abort();
  try {
    const [stdout] = await Promise.all([
      boundedBytes(child.stdout, maximum),
      boundedBytes(child.stderr, 4096),
    ]);
    if ((await child.exited) !== 0 || signal.aborted)
      throw Error("Source runtime fixed command failed or timed out");
    return stdout;
  } finally {
    signal.removeEventListener("abort", abort);
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}

/** No command, download, or output-path injection API: only these source CLI targets can continue. */
export async function continueSourceCi(role: SourceCiRole): Promise<boolean> {
  if (!needsSourceRuntime(role, Bun.version, process.env)) return false;
  const { temporary, commandFile } = await sourceRuntimePaths(process.env);
  const scratch = await mkdtemp(join(temporary, "agent-platform-source-bun143-"));
  await chmod(scratch, 0o700);
  const controller = new AbortController(),
    timer = setTimeout(() => controller.abort(), operationalTimeouts.sourceRuntimeMs);
  let published = false;
  try {
    const archiveBytes = await download(controller.signal);
    verifySourceRuntimeArtifact("archive", archiveBytes);
    const archive = join(scratch, "bun.zip"),
      binary = join(scratch, "bun");
    await writeFile(archive, archiveBytes, { flag: "wx", mode: 0o600 });
    const entries = new TextDecoder()
      .decode(
        await fixedCommand(["/usr/bin/unzip", "-Z1", archive], 4096, scratch, controller.signal),
      )
      .trim()
      .split("\n");
    if (entries.filter((entry) => entry !== "bun-linux-x64/").join("\n") !== "bun-linux-x64/bun")
      throw Error("Unexpected Bun zip entries");
    // Stream the single fixed member; archive paths never become filesystem destinations.
    const binaryBytes = await fixedCommand(
      ["/usr/bin/unzip", "-p", archive, "bun-linux-x64/bun"],
      release.binaryBytes,
      scratch,
      controller.signal,
    );
    verifySourceRuntimeArtifact("binary", binaryBytes);
    await writeFile(binary, binaryBytes, { flag: "wx", mode: 0o600 });
    await chmod(binary, 0o700);
    const version = new TextDecoder()
      .decode(await fixedCommand([binary, "--version"], 128, scratch, controller.signal))
      .trim();
    if (version !== release.version) throw Error("Pinned source Bun version mismatch");
    verifySourceRuntimeArtifact("binary", await readFile(binary));
    await rm(archive);
    await sourceRuntimePaths(process.env);
    await appendFile(commandFile, scratch + "\n");
    published = true;
    console.log(
      JSON.stringify({
        recordClass: "verified-source-ci-runtime",
        version,
        archiveSha256: release.archiveSha256,
        binarySha256: release.binarySha256,
        nativePermissionGranted: false,
      }),
    );
    clearTimeout(timer);
    const env: NodeJS.ProcessEnv = {
      ...process.env,
      PATH: scratch + ":" + (process.env.PATH ?? "/usr/bin:/bin"),
    };
    delete env.NODE_OPTIONS;
    delete env.BUN_OPTIONS;
    const child = Bun.spawn(
      [
        binary,
        role === "source-check"
          ? join(root, "ci/source-check.ts")
          : join(root, "ci/quality/run.ts"),
        ...(role === "source-check" ? [] : [role]),
      ],
      { cwd: root, env, stdin: "ignore", stdout: "inherit", stderr: "inherit" },
    );
    process.exitCode = await child.exited;
    return true;
  } finally {
    clearTimeout(timer);
    if (!published) await rm(scratch, { recursive: true, force: true });
  }
}
