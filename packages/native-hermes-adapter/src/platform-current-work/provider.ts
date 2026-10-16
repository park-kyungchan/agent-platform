import { createHash } from "node:crypto";
import { open, realpath } from "node:fs/promises";
import { constants } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";

import { captureInstalledRuntimeClosure } from "../source-provider/dependency-closure.ts";

/** Bootstrap guard depends only on builtins, before any current/legacy implementation import. */
export async function readExactFile(path: string, maxBytes: number, expectedSha256?: string) {
  if (!isAbsolute(path) || resolve(path) !== path)
    throw Error("EXACT_PLATFORM_SOURCE_PATH_REQUIRED");
  const parts = path.split("/").slice(1);
  let directory = await open(
    "/",
    constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
  );
  try {
    for (const part of parts.slice(0, -1)) {
      const next = await open(
        "/proc/self/fd/" + directory.fd + "/" + part,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await directory.close();
      directory = next;
    }
    const file = await open(
      "/proc/self/fd/" + directory.fd + "/" + parts.at(-1),
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    try {
      const before = await file.stat();
      if (!before.isFile() || before.nlink < 1 || before.nlink > 8 || before.size > maxBytes)
        throw Error("BOUNDED_PLATFORM_SOURCE_REQUIRED");
      const buffer = Buffer.alloc(before.size + 1);
      let count = 0;
      while (count < buffer.length) {
        const next = await file.read(buffer, count, buffer.length - count, null);
        if (!next.bytesRead) break;
        count += next.bytesRead;
      }
      const raw = buffer.subarray(0, count);
      const after = await file.stat();
      if (
        raw.length !== before.size ||
        before.dev !== after.dev ||
        before.ino !== after.ino ||
        before.nlink !== after.nlink ||
        before.size !== after.size ||
        before.mtimeMs !== after.mtimeMs ||
        before.ctimeMs !== after.ctimeMs ||
        (expectedSha256 && sourceDigest(raw) !== expectedSha256)
      )
        throw Error("PLATFORM_SOURCE_CHANGED");
      return raw;
    } finally {
      await file.close();
    }
  } finally {
    await directory.close();
  }
}

export const platformEntryPoints = {
  "read-context.js": "packages/native-hermes-adapter/src/platform-rearchitecture/read-context.ts",
  "native-hook.js": "packages/native-hermes-adapter/src/platform-rearchitecture/native-hook.ts",
  "read-current-work.js":
    "packages/native-hermes-adapter/src/platform-current-work/read-current-work.ts",
  "current-work-native-hook.js":
    "packages/native-hermes-adapter/src/platform-current-work/native-hook.ts",
} as const;
export const platformSourcePaths = [
  "package.json",
  "bun.lock",
  "releases/palantirkc-ontology-0.2.0.tgz",
  "packages/native-hermes-adapter/package.json",
  "packages/native-hermes-adapter/releases/palantirkc-ontology-0.2.0.tgz",
  "packages/work-continuity/package.json",
  "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
  ...[
    "read-context",
    "native-hook",
    "input",
    "work-reader",
    "review-reader",
    "work-continuity",
    "work-identity",
    "work-identity-contracts",
    "work-records",
    "owner-readers",
  ].map((name) => "packages/native-hermes-adapter/src/platform-rearchitecture/" + name + ".ts"),
  ...["context", "read-current-work", "provider", "build", "hook-context", "native-hook"].map(
    (name) => "packages/native-hermes-adapter/src/platform-current-work/" + name + ".ts",
  ),
  ...["boundary-custody", "contracts", "bot-correlation"].map(
    (name) => "packages/native-hermes-adapter/src/platform-governance/" + name + ".ts",
  ),
  "packages/work-continuity/src/handoff.ts",
  "packages/work-continuity/src/runtime-index.ts",
  ...["index", "source", "records", "context", "types"].map(
    (name) => "packages/work-continuity/src/git-work/" + name + ".ts",
  ),
  "packages/work-continuity/src/logical-identity.ts",
  "packages/work-continuity/src/operational-state.ts",
  "packages/native-hermes-adapter/src/source-provider/dependency-closure.ts",
].sort();
export interface PlatformPin {
  path: string;
  sha256: string;
  bytes: number;
}
export interface PlatformDependency {
  name: string;
  version: string;
  root: string;
}
export interface PlatformSourceRegistration {
  schema: "hermes.platform-source-provider.v2";
  providerRoot: string;
  packageName: "@palantirkc/native-hermes-adapter";
  entrypoints: typeof platformEntryPoints;
  sourcePins: PlatformPin[];
  dependencyPins: PlatformPin[];
  dependencies: PlatformDependency[];
  authority: "none";
  nativePermissionGranted: false;
}
export const sourceDigest = (bytes: Uint8Array | string) =>
  createHash("sha256").update(bytes).digest("hex");
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object") {
    for (const item of Object.values(value)) freeze(item);
    Object.freeze(value);
  }
  return value;
}
function safePath(path: unknown): path is string {
  return (
    typeof path === "string" &&
    path.length < 2048 &&
    !isAbsolute(path) &&
    !path.includes("\\") &&
    !path.split("/").some((part) => !part || part === "." || part === "..")
  );
}
async function pin(root: string, path: string): Promise<PlatformPin> {
  if (!safePath(path)) throw Error("INVALID_PLATFORM_SOURCE_PATH");
  const bytes = await readExactFile(join(root, path), 1048576);
  return { path, sha256: sourceDigest(bytes), bytes: bytes.length };
}
export async function captureHermesPlatformSource(
  providerRoot: string,
): Promise<PlatformSourceRegistration> {
  if (resolve(providerRoot) !== providerRoot || (await realpath(providerRoot)) !== providerRoot)
    throw Error("EXACT_PLATFORM_SOURCE_OWNER_REQUIRED");
  const sourcePins = await Promise.all(platformSourcePaths.map((path) => pin(providerRoot, path)));
  return {
    schema: "hermes.platform-source-provider.v2",
    providerRoot,
    packageName: "@palantirkc/native-hermes-adapter",
    entrypoints: { ...platformEntryPoints },
    sourcePins,
    ...(await captureInstalledRuntimeClosure(providerRoot, readExactFile, sourceDigest)),
    authority: "none",
    nativePermissionGranted: false,
  };
}
/** Caller owns the exact approved registration digest. This is first-import source proof, never native authority. */
export async function observeHermesPlatformSource(
  registrationPath: string,
  expected: { providerRoot: string; registrationSha256: string },
) {
  const providerRoot = expected.providerRoot,
    registrationSha256 = expected.registrationSha256;
  if (
    !/^[a-f0-9]{64}$/u.test(registrationSha256) ||
    resolve(registrationPath) !== registrationPath ||
    (await realpath(registrationPath)) !== registrationPath
  )
    throw Error("EXACT_PLATFORM_REGISTRATION_REQUIRED");
  const bytes = await readExactFile(registrationPath, 1048576, registrationSha256);
  const value: unknown = JSON.parse(bytes.toString());
  const actual = await captureHermesPlatformSource(providerRoot);
  if (sourceDigest(JSON.stringify(value)) !== sourceDigest(JSON.stringify(actual)))
    throw Error("PLATFORM_SOURCE_CLOSURE_DRIFT");
  const snapshot = freeze(structuredClone(actual));
  const verify = async () => {
    await readExactFile(registrationPath, 1048576, registrationSha256);
    const current = await captureHermesPlatformSource(providerRoot);
    if (JSON.stringify(current) !== JSON.stringify(snapshot))
      throw Error("PLATFORM_SOURCE_CLOSURE_CHANGED");
  };
  return {
    registration: snapshot,
    registrationPath,
    registrationSha256,
    verify,
    authority: "none" as const,
    nativePermissionGranted: false as const,
  };
}
