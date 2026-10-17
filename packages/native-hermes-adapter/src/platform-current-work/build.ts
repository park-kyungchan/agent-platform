import { constants } from "node:fs";
import { open, realpath, mkdtemp, readFile, rm, lstat, type FileHandle } from "node:fs/promises";
import { isAbsolute, join, resolve, relative } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { currentWorkOutputPolicy } from "./output-policy.ts";
import {
  observeHermesPlatformSource,
  platformEntryPoints,
  sourceDigest,
  type PlatformSourceRegistration,
} from "./provider.ts";

export interface PlatformBuildRequest {
  registrationPath: string;
  registrationSha256: string;
  providerRoot: string;
  compiler: { path: string; sha256: string };
  legacyManifestSha256: string;
  runtimeProviderRoot: string;
  runtimeRegistrationPath: string;
  /** Exact owner-selected native bindings; absence builds a held current hook, never a legacy fallback. */
  currentWorkBindings?: { runtimePath: string; sha256: string };
  /** Explicit version selection; omission preserves the existing v1 wire consumer. */
  currentWorkOutputVersion?: "v1" | "v2";
  currentWorkMaxChars?: number;
  currentWorkMetadataEncoding?: "metadata.v1";
}
async function compilerPin(path: string) {
  if (resolve(path) !== path || (await realpath(path)) !== path)
    throw Error("EXACT_INSTALLED_COMPILER_REQUIRED");
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat();
    if (!before.isFile() || before.size > 256 * 1024 * 1024)
      throw Error("BOUNDED_INSTALLED_COMPILER_REQUIRED");
    const digest = createHash("sha256"),
      buffer = Buffer.alloc(65536);
    let bytes = 0;
    while (true) {
      const part = await file.read(buffer, 0, buffer.length, null);
      if (!part.bytesRead) break;
      bytes += part.bytesRead;
      if (bytes > 256 * 1024 * 1024) throw Error("COMPILER_BYTE_LIMIT");
      digest.update(buffer.subarray(0, part.bytesRead));
    }
    const after = await file.stat();
    if (
      before.dev !== after.dev ||
      before.ino !== after.ino ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs ||
      bytes !== before.size
    )
      throw Error("INSTALLED_COMPILER_CHANGED");
    return {
      path,
      sha256: digest.digest("hex"),
      bytes,
      dev: before.dev,
      ino: before.ino,
      mtimeMs: before.mtimeMs,
      ctimeMs: before.ctimeMs,
    };
  } finally {
    await file.close();
  }
}
async function boundedOutput(stream: ReadableStream<Uint8Array>) {
  const reader = stream.getReader(),
    hash = createHash("sha256");
  let bytes = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.length;
      if (bytes > 65536) throw Error("COMPILER_DIAGNOSTIC_LIMIT");
      hash.update(next.value);
    }
    return { sha256: hash.digest("hex"), bytes };
  } finally {
    reader.releaseLock();
  }
}
function runtimePath(path: string) {
  if (
    !isAbsolute(path) ||
    resolve(path) !== path ||
    path.startsWith("/home/") ||
    path.startsWith("/mnt/") ||
    /[\u0000-\u001f]/u.test(path)
  )
    throw Error("EXPLICIT_PC_INDEPENDENT_RUNTIME_PATH_REQUIRED");
}
/** Finite installed compiler with complete pre/post source, dependency, generator and compiler pins. */
export async function buildHermesPlatformSource(request: PlatformBuildRequest) {
  request = structuredClone(request);
  if (
    request.currentWorkOutputVersion !== undefined &&
    !["v1", "v2"].includes(request.currentWorkOutputVersion)
  )
    throw Error("EXPLICIT_CURRENT_WORK_OUTPUT_VERSION_REQUIRED");
  const outputPolicy = currentWorkOutputPolicy(
    request.currentWorkOutputVersion ?? "v1",
    request.currentWorkMaxChars,
  );
  if (!outputPolicy.valid) throw Error("INVALID_CURRENT_WORK_OUTPUT_BUDGET");
  if (
    request.currentWorkMetadataEncoding !== undefined &&
    (request.currentWorkMetadataEncoding !== "metadata.v1" ||
      request.currentWorkOutputVersion !== "v2")
  )
    throw Error("EXPLICIT_CURRENT_WORK_METADATA_ENCODING_REQUIRED");
  if (
    !/^[a-f0-9]{64}$/u.test(request.compiler.sha256) ||
    !/^[a-f0-9]{64}$/u.test(request.legacyManifestSha256)
  )
    throw Error("EXACT_COMPILER_AND_LEGACY_MANIFEST_PINS_REQUIRED");
  runtimePath(request.runtimeProviderRoot);
  runtimePath(request.runtimeRegistrationPath);
  if (request.currentWorkBindings) {
    runtimePath(request.currentWorkBindings.runtimePath);
    if (!/^[a-f0-9]{64}$/u.test(request.currentWorkBindings.sha256))
      throw Error("EXACT_CURRENT_WORK_BINDINGS_PIN_REQUIRED");
  }
  const source = await observeHermesPlatformSource(request.registrationPath, request),
    before = await compilerPin(request.compiler.path);
  if (before.sha256 !== request.compiler.sha256) throw Error("INSTALLED_COMPILER_PIN_DRIFT");
  const nativeRegistration: PlatformSourceRegistration = {
    ...structuredClone(source.registration),
    providerRoot: request.runtimeProviderRoot,
  };
  const nativeRegistrationBytes = JSON.stringify(nativeRegistration, null, 2) + "\n",
    nativeRegistrationSha256 = sourceDigest(nativeRegistrationBytes);
  const directory = await mkdtemp(join(tmpdir(), "hermes-platform-finite-build-"));
  let compilerDescriptor: FileHandle | undefined;
  try {
    compilerDescriptor = await open(
      request.compiler.path,
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    const stat = await compilerDescriptor.stat();
    if (
      !stat.isFile() ||
      stat.dev !== before.dev ||
      stat.ino !== before.ino ||
      stat.size !== before.bytes ||
      stat.mtimeMs !== before.mtimeMs ||
      stat.ctimeMs !== before.ctimeMs
    )
      throw Error("INSTALLED_COMPILER_CHANGED");
    const compilerExecutable = "/proc/" + process.pid + "/fd/" + compilerDescriptor.fd;
    const bundles = [];
    for (const [name, entry] of Object.entries(platformEntryPoints)) {
      await source.verify();
      if (JSON.stringify(await compilerPin(request.compiler.path)) !== JSON.stringify(before))
        throw Error("INSTALLED_COMPILER_CHANGED");
      const output = join(directory, name),
        metadata = join(directory, name + ".meta.json");
      const argv = [
        compilerExecutable,
        "--no-env-file",
        "build",
        join(request.providerRoot, entry),
        "--target=bun",
        "--outfile",
        output,
        "--metafile=" + metadata,
        "--define",
        "PLATFORM_NATIVE_MANIFEST_SHA256=" + JSON.stringify(request.legacyManifestSha256),
        "--define",
        "HERMES_PLATFORM_REGISTRATION_SHA256=" + JSON.stringify(nativeRegistrationSha256),
        "--define",
        "HERMES_PLATFORM_PROVIDER_ROOT=" + JSON.stringify(request.runtimeProviderRoot),
        "--define",
        "HERMES_PLATFORM_REGISTRATION_PATH=" + JSON.stringify(request.runtimeRegistrationPath),
        "--define",
        "HERMES_CURRENT_WORK_BINDINGS_PATH=" +
          (request.currentWorkBindings
            ? JSON.stringify(request.currentWorkBindings.runtimePath)
            : "undefined"),
        "--define",
        "HERMES_CURRENT_WORK_BINDINGS_SHA256=" +
          (request.currentWorkBindings
            ? JSON.stringify(request.currentWorkBindings.sha256)
            : "undefined"),
        "--define",
        "HERMES_CURRENT_WORK_OUTPUT_VERSION=" +
          JSON.stringify(request.currentWorkOutputVersion ?? "v1"),
        "--define",
        "HERMES_CURRENT_WORK_MAX_CHARS=" + JSON.stringify(outputPolicy.policy.maxChars),
      ];
      if (request.currentWorkMetadataEncoding)
        argv.push(
          "--define",
          "HERMES_CURRENT_WORK_METADATA_ENCODING=" +
            JSON.stringify(request.currentWorkMetadataEncoding),
        );
      const child = Bun.spawn(argv, {
        cwd: request.providerRoot,
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      });
      const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
      let completed: {
        exitCode: number;
        stdout: { sha256: string; bytes: number };
        stderr: { sha256: string; bytes: number };
      };
      try {
        const [exitCode, stdout, stderr] = await Promise.all([
          child.exited,
          boundedOutput(child.stdout),
          boundedOutput(child.stderr),
        ]);
        completed = { exitCode, stdout, stderr };
      } catch {
        child.kill("SIGKILL");
        await child.exited;
        throw Error("FINITE_PLATFORM_COMPILER_FAILED");
      } finally {
        clearTimeout(timer);
      }
      if (completed.exitCode !== 0) throw Error("FINITE_PLATFORM_COMPILER_FAILED");
      if ((await lstat(metadata)).size > 1048576 || (await lstat(output)).size > 8 * 1024 * 1024)
        throw Error("PLATFORM_BUILD_OUTPUT_LIMIT");
      const rawMetadata: unknown = JSON.parse(await readFile(metadata, "utf8"));
      if (
        !rawMetadata ||
        typeof rawMetadata !== "object" ||
        Array.isArray(rawMetadata) ||
        !("inputs" in rawMetadata) ||
        !rawMetadata.inputs ||
        typeof rawMetadata.inputs !== "object" ||
        Array.isArray(rawMetadata.inputs)
      )
        throw Error("COMPILER_INPUT_MANIFEST_REQUIRED");
      const inputPaths = (
        await Promise.all(
          Object.keys(rawMetadata.inputs).map(async (path) =>
            relative(request.providerRoot, await realpath(resolve(request.providerRoot, path))),
          ),
        )
      ).sort();
      const pins = new Map(
        [...source.registration.sourcePins, ...source.registration.dependencyPins].map((pin) => [
          pin.path,
          pin,
        ]),
      );
      if (inputPaths.some((path) => !pins.has(path)))
        throw Error("UNPINNED_TRANSITIVE_COMPILER_INPUT");
      for (const input of inputPaths) {
        const dependency = source.registration.dependencies.find(
          (item) =>
            item.name === "@palantirkc/work-continuity" && input.startsWith(item.root + "/"),
        );
        if (dependency) {
          const canonical = "packages/work-continuity/" + input.slice(dependency.root.length + 1);
          if (!pins.has(canonical) || pins.get(canonical)!.sha256 !== pins.get(input)!.sha256)
            throw Error("INSTALLED_WORKSPACE_PROJECTION_DRIFT");
        }
      }
      const bytes = new Uint8Array(await readFile(output));
      if (Buffer.from(bytes).includes("/home/") || Buffer.from(bytes).includes("/mnt/"))
        throw Error("BUILD_CONTAINS_PC_SPECIFIC_PATH");
      await source.verify();
      if (JSON.stringify(await compilerPin(request.compiler.path)) !== JSON.stringify(before))
        throw Error("INSTALLED_COMPILER_CHANGED");
      bundles.push({
        name,
        bytes,
        sha256: sourceDigest(bytes),
        inputs: inputPaths,
        compiler: completed,
      });
    }
    return {
      schema: "hermes.platform-source-build.v2" as const,
      bundles,
      nativeRegistration: {
        bytes: new TextEncoder().encode(nativeRegistrationBytes),
        sha256: nativeRegistrationSha256,
        path: request.runtimeRegistrationPath,
      },
      sourceRegistration: source.registration,
      compiler: before,
      compilerExecution: "held-parent-file-descriptor" as const,
      sourceClosureVerifiedBeforeAndAfter: true,
      transitiveCompilerInputsVerified: true,
      temporaryDirectoryRemovedBeforeReturn: true,
      nativeActivation: false as const,
      currentWorkOutputVersion: request.currentWorkOutputVersion ?? "v1",
      currentWorkOutputPolicy: outputPolicy.policy,
      ...(request.currentWorkMetadataEncoding
        ? { currentWorkMetadataEncoding: request.currentWorkMetadataEncoding }
        : {}),
      authority: "none" as const,
      nativePermissionGranted: false as const,
    };
  } finally {
    await compilerDescriptor?.close();
    await rm(directory, { recursive: true, force: true });
  }
}
