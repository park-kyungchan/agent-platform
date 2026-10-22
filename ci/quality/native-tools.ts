import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { exactSourceFile, safeSourcePath } from "./scope";

export interface ToolPin {
  name: "actionlint" | "shellcheck" | "zizmor" | "gitleaks";
  version: string;
  url: string;
  sha256: string;
  binarySha256: string;
  executable: string;
  archivePath: string;
}
interface ToolPinFile {
  schema: "agent-platform.ci-tool-pins.v1";
  platforms: string[];
  tools: ToolPin[];
}
interface BinaryReceipt {
  schema: "agent-platform.verified-ci-tool-cache.v1";
  name: ToolPin["name"];
  version: string;
  platform: "linux-x64";
  archiveSha256: string;
  binarySha256: string;
  executable: string;
}
export interface NativeCommandResult {
  name: string;
  exitCode: number;
  durationMs: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}
export interface InstalledTool {
  pin: ToolPin;
  path: string;
  binarySha256: string;
  cacheHit: boolean;
}

const hash = (bytes: Uint8Array | string): string =>
  createHash("sha256").update(bytes).digest("hex");

export async function runNativeCommand(
  name: string,
  argv: string[],
  cwd: string,
  timeoutMs = 90_000,
  environment: Record<string, string> = {},
): Promise<NativeCommandResult> {
  const started = performance.now();
  const child = Bun.spawn(argv, {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, ...environment, CI: "true" },
  });
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    child.kill();
  }, timeoutMs);
  try {
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    return {
      name,
      exitCode,
      durationMs: Math.round(performance.now() - started),
      stdout,
      stderr,
      timedOut,
    };
  } finally {
    clearTimeout(timer);
  }
}

export async function readToolPins(root: string): Promise<ToolPin[]> {
  const record = JSON.parse(
    await exactSourceFile(root, "ci/quality/tool-pins.json"),
  ) as ToolPinFile;
  if (
    record.schema !== "agent-platform.ci-tool-pins.v1" ||
    !Array.isArray(record.tools) ||
    !Array.isArray(record.platforms) ||
    !record.platforms.includes("linux-x64")
  )
    throw Error("Unknown native quality tool pin contract.");
  const expected = new Set(["actionlint", "shellcheck", "zizmor", "gitleaks"]);
  for (const pin of record.tools) {
    if (
      !expected.delete(pin.name) ||
      !/^\d+\.\d+\.\d+$/.test(pin.version) ||
      !/^[a-f0-9]{64}$/.test(pin.sha256) ||
      !/^[a-f0-9]{64}$/.test(pin.binarySha256) ||
      !/^https:\/\/github\.com\/[^/]+\/[^/]+\/releases\/download\//.test(pin.url) ||
      pin.executable !== pin.name ||
      !safeSourcePath(pin.archivePath)
    )
      throw Error("Invalid or duplicate native quality tool pin.");
  }
  if (expected.size) throw Error("The native quality tool denominator is incomplete.");
  return record.tools;
}

async function readCached(pin: ToolPin, directory: string): Promise<InstalledTool> {
  const receiptPath = join(directory, "receipt.json"),
    binaryPath = join(directory, pin.executable);
  for (const path of [directory, receiptPath, binaryPath])
    if ((await realpath(path)) !== path)
      throw Error("Native tool cache contains a symlink: " + pin.name);
  const receiptInfo = await lstat(receiptPath),
    binaryInfo = await lstat(binaryPath);
  if (!receiptInfo.isFile() || !binaryInfo.isFile() || binaryInfo.size > 100_000_000)
    throw Error("Native tool cache is not an exact regular binary/receipt pair: " + pin.name);
  const receipt = JSON.parse(await readFile(receiptPath, "utf8")) as BinaryReceipt;
  const binarySha256 = hash(await readFile(binaryPath));
  if (
    receipt.schema !== "agent-platform.verified-ci-tool-cache.v1" ||
    receipt.name !== pin.name ||
    receipt.version !== pin.version ||
    receipt.platform !== "linux-x64" ||
    receipt.archiveSha256 !== pin.sha256 ||
    receipt.executable !== pin.executable ||
    receipt.binarySha256 !== binarySha256 ||
    binarySha256 !== pin.binarySha256
  )
    throw Error("Native tool cached binary hash or receipt mismatch: " + pin.name);
  return { pin, path: binaryPath, binarySha256, cacheHit: true };
}

/** Download only immutable pinned archives; install exactly one regular member into the user's cache. */
export async function installPinnedTool(pin: ToolPin, cacheRoot?: string): Promise<InstalledTool> {
  if (!/^[a-f0-9]{64}$/.test(pin.sha256) || !/^[a-f0-9]{64}$/.test(pin.binarySha256))
    throw Error("Native tools require source-owned archive and binary hashes");
  if (process.platform !== "linux" || process.arch !== "x64")
    throw Error("Native quality tools are currently pinned only for linux-x64.");
  const base = resolve(
    cacheRoot ??
      join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "agent-platform", "ci-tools"),
  );
  const parent = join(base, pin.name, pin.version, "linux-x64"),
    destination = join(parent, pin.sha256);
  let present = false;
  try {
    await lstat(destination);
    present = true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  if (present) return readCached(pin, destination);
  await mkdir(parent, { recursive: true });
  if ((await realpath(parent)) !== parent)
    throw Error("Native tool cache parent must have an exact canonical path.");
  const temporary = await mkdtemp(join(parent, ".verified-install-"));
  try {
    const response = await fetch(pin.url, { signal: AbortSignal.timeout(60_000) });
    if (!response.ok)
      throw Error("Native tool download failed: " + pin.name + " HTTP " + response.status);
    const declaredLength = Number(response.headers.get("content-length") ?? "0");
    if (declaredLength > 100_000_000)
      throw Error("Native tool archive exceeds its download budget: " + pin.name);
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.byteLength > 100_000_000 || hash(bytes) !== pin.sha256)
      throw Error("Native tool archive hash mismatch: " + pin.name);
    const archive = join(temporary, "archive.tar.gz");
    await writeFile(archive, bytes);
    const listing = await runNativeCommand(
      "tar-member-list",
      ["tar", "-tzf", archive, "--no-wildcards", "--", pin.archivePath],
      temporary,
    );
    if (listing.exitCode !== 0 || listing.timedOut || listing.stdout.trim() !== pin.archivePath)
      throw Error(
        "Pinned native tool archive member is absent or ambiguous: " +
          pin.name +
          "/" +
          pin.archivePath +
          " " +
          listing.stderr.slice(0, 500),
      );
    const extraction = await runNativeCommand(
      "tar-member-extract",
      [
        "tar",
        "-xzf",
        archive,
        "--no-same-owner",
        "--no-same-permissions",
        "--no-wildcards",
        "-C",
        temporary,
        "--",
        pin.archivePath,
      ],
      temporary,
    );
    if (extraction.exitCode !== 0 || extraction.timedOut)
      throw Error(
        "Pinned native tool member extraction failed: " +
          pin.name +
          " " +
          extraction.stderr.slice(0, 500),
      );
    const member = join(temporary, pin.archivePath),
      info = await lstat(member);
    if (
      !info.isFile() ||
      info.isSymbolicLink() ||
      info.size > 100_000_000 ||
      (await realpath(member)) !== member
    )
      throw Error("Pinned archive member is not an exact regular binary: " + pin.name);
    const prepared = join(temporary, "prepared");
    await mkdir(prepared);
    const binaryPath = join(prepared, pin.executable);
    await copyFile(member, binaryPath);
    await chmod(binaryPath, 0o755);
    const binarySha256 = hash(await readFile(binaryPath));
    if (binarySha256 !== pin.binarySha256)
      throw Error("Pinned archive binary hash mismatch: " + pin.name);
    const receipt: BinaryReceipt = {
      schema: "agent-platform.verified-ci-tool-cache.v1",
      name: pin.name,
      version: pin.version,
      platform: "linux-x64",
      archiveSha256: pin.sha256,
      binarySha256,
      executable: pin.executable,
    };
    await writeFile(join(prepared, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n");
    try {
      await rename(prepared, destination);
    } catch (error) {
      if (!["EEXIST", "ENOTEMPTY"].includes((error as NodeJS.ErrnoException).code ?? ""))
        throw error;
      return await readCached(pin, destination);
    }
    return { ...(await readCached(pin, destination)), cacheHit: false };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export async function runWorkflowTools(
  root: string,
  workflows: string[],
  actions: string[] = [],
  cacheRoot?: string,
  pinRoot = root,
): Promise<{ tools: InstalledTool[]; commands: NativeCommandResult[] }> {
  if (!workflows.length) throw Error("Workflow audit refuses an empty denominator.");
  for (const file of [...workflows, ...actions]) await exactSourceFile(root, file);
  const pins = await readToolPins(pinRoot),
    tools: InstalledTool[] = [];
  for (const pin of pins.filter((pin) => pin.name !== "gitleaks"))
    tools.push(await installPinnedTool(pin, cacheRoot));
  const tool = (name: ToolPin["name"]): InstalledTool => {
    const found = tools.find((entry) => entry.pin.name === name);
    if (!found) throw Error("Missing pinned native tool: " + name);
    return found;
  };
  const commands = [
    await runNativeCommand(
      "actionlint",
      [tool("actionlint").path, "-shellcheck", tool("shellcheck").path, ...workflows],
      root,
    ),
    await runNativeCommand(
      "zizmor",
      [
        tool("zizmor").path,
        "--offline",
        "--strict-collection",
        "--persona=regular",
        ...workflows,
        ...actions,
      ],
      root,
    ),
  ];
  return { tools, commands };
}
