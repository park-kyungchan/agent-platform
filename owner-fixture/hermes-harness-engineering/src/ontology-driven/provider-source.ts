/** NEW source-only compatibility guard. The generator embeds the canonical dependency observer
 * and substitutes only the registration and successor-manifest digests.
 * This source is not recovered historical owner code and confers no native permission. */
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, openSync, readSync } from "node:fs";
import { lstat, open, readdir, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

let captureDependencies!: (
  root: string,
  read: (path: string, maxBytes: number) => Promise<Uint8Array>,
  digest: (bytes: Uint8Array | string) => string,
  includeWorkContinuity: boolean,
  sourceImporters: readonly string[],
) => Promise<{
  dependencies: { name: string; version: string; root: string }[];
  dependencyPins: { path: string; sha256: string; bytes: number }[];
}>;
/** Single canonical finite dependency observer shared by v2 and the generated v1 guard.
 * The generator embeds this byte-exact builtin-only module as a projection, never a second implementation. */
import {
  lstat as dependencyLstat,
  readdir as dependencyReaddir,
  realpath as dependencyRealpath,
} from "node:fs/promises";
import {
  dirname as dependencyDirname,
  isAbsolute as dependencyIsAbsolute,
  join as dependencyJoin,
  relative as dependencyRelative,
} from "node:path";

export interface RuntimeDependencyPin {
  path: string;
  sha256: string;
  bytes: number;
}
export interface RuntimeDependency {
  name: string;
  version: string;
  root: string;
}
export async function captureInstalledRuntimeClosure(
  root: string,
  read: (path: string, maxBytes: number) => Promise<Uint8Array>,
  digest: (bytes: Uint8Array | string) => string,
  includeWorkContinuity = true,
  sourceImporters: readonly string[] = [],
): Promise<{ dependencies: RuntimeDependency[]; dependencyPins: RuntimeDependencyPin[] }> {
  const dependencies: RuntimeDependency[] = [],
    dependencyPins: RuntimeDependencyPin[] = [];
  const seen = new Set<string>();
  let totalBytes = 0;
  function within(base: string, path: string) {
    const name = dependencyRelative(base, path);
    return name !== "" && !name.startsWith("..") && !dependencyIsAbsolute(name);
  }
  async function correspondence(packageRoot: string, installed: RuntimeDependencyPin[]) {
    const releasePath = dependencyJoin(root, "releases/palantirkc-ontology-0.2.0.tgz");
    const release = await read(releasePath, 1048576);
    // This immutable release pin is existing Upper Ontology0.2.0, never a newly authored release.
    if (digest(release) !== "3f9995d0b91d2ad39dfc7cedbb6e35b1cb9d29c2e57223ca0c076d2030319406")
      throw Error("IMMUTABLE_ONTOLOGY_RELEASE_CLOSURE_DRIFT");
    const archive = await new Bun.Archive(release).files();
    if (!archive.size || archive.size > 2048) throw Error("IMMUTABLE_RELEASE_FILE_LIMIT");
    const expected = new Map<string, string>();
    let bytes = 0;
    for (const [path, file] of archive) {
      if (
        !path.startsWith("package/") ||
        path.includes("\\") ||
        path.split("/").some((part) => !part || part === "." || part === "..") ||
        file.size > 1048576
      )
        throw Error("INVALID_IMMUTABLE_RELEASE_MEMBER");
      bytes += file.size;
      if (bytes > 32 * 1024 * 1024) throw Error("IMMUTABLE_RELEASE_BYTE_LIMIT");
      const name = path.slice("package/".length);
      if (expected.has(name)) throw Error("DUPLICATE_IMMUTABLE_RELEASE_MEMBER");
      expected.set(name, digest(new Uint8Array(await file.arrayBuffer())));
    }
    const observed = new Map(
      installed.map((pin) => [
        dependencyRelative(packageRoot, dependencyJoin(root, pin.path)),
        pin.sha256,
      ]),
    );
    // The fixture generator never creates an owner project record. An immutable package's non-executable
    // project metadata may consequently be absent, but if installed it must match the same release.
    for (const [path, hash] of expected)
      if (!(path === "project.json" && !observed.has(path)) && observed.get(path) !== hash)
        throw Error("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
    for (const [path, hash] of observed)
      if (expected.get(path) !== hash) throw Error("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
  }
  async function visit(name: string, from: string) {
    if (!/^(@[a-z0-9-]+\/)?[a-z0-9-]+$/u.test(name))
      throw Error("INVALID_PLATFORM_DEPENDENCY_NAME");
    const packagePath = await dependencyRealpath(Bun.resolveSync(name + "/package.json", from));
    const packageRoot = dependencyDirname(packagePath);
    if (
      name === "@palantirkc/work-continuity" &&
      packageRoot === dependencyJoin(root, "packages/work-continuity")
    )
      return;
    if (!within(dependencyJoin(root, "node_modules"), packagePath))
      throw Error("PLATFORM_DEPENDENCY_ESCAPED_OWNER");
    if (seen.has(packageRoot)) return;
    seen.add(packageRoot);
    if (seen.size > 32) throw Error("PLATFORM_DEPENDENCY_COUNT_LIMIT");
    const manifest: unknown = JSON.parse(Buffer.from(await read(packagePath, 65536)).toString());
    if (!manifest || typeof manifest !== "object" || Array.isArray(manifest))
      throw Error("INVALID_PLATFORM_DEPENDENCY");
    const data = manifest as Record<string, unknown>;
    if (
      data.name !== name ||
      typeof data.version !== "string" ||
      !data.version ||
      (name === "@palantirkc/ontology" && data.version !== "0.2.0")
    )
      throw Error("PLATFORM_DEPENDENCY_IDENTITY_DRIFT");
    dependencies.push({ name, version: data.version, root: dependencyRelative(root, packageRoot) });
    const installed: RuntimeDependencyPin[] = [];
    async function walk(directory: string) {
      for (const entry of await dependencyReaddir(directory, { withFileTypes: true })) {
        // A resolver may select a package beside any importing file, not only the
        // declared package root. Never omit such an executable lookup boundary.
        if (entry.name === "node_modules") throw Error("UNADMITTED_NESTED_DEPENDENCY_ROOT");
        const path = dependencyJoin(directory, entry.name);
        if (entry.isSymbolicLink()) throw Error("UNADMITTED_PLATFORM_DEPENDENCY_SYMLINK");
        if (entry.isDirectory()) await walk(path);
        else if (entry.isFile()) {
          if (dependencyPins.length >= 2048) throw Error("PLATFORM_DEPENDENCY_FILE_LIMIT");
          const bytes = await read(path, 1048576);
          totalBytes += bytes.length;
          if (totalBytes > 32 * 1024 * 1024) throw Error("PLATFORM_DEPENDENCY_BYTE_LIMIT");
          const pin = {
            path: dependencyRelative(root, path),
            sha256: digest(bytes),
            bytes: bytes.length,
          };
          dependencyPins.push(pin);
          installed.push(pin);
        } else throw Error("UNADMITTED_PLATFORM_DEPENDENCY_FILE");
      }
    }
    await walk(packageRoot);
    if (name === "@palantirkc/ontology") await correspondence(packageRoot, installed);
    if (data.dependencies !== undefined) {
      if (
        !data.dependencies ||
        typeof data.dependencies !== "object" ||
        Array.isArray(data.dependencies)
      )
        throw Error("INVALID_PLATFORM_DEPENDENCIES");
      const children = Object.keys(data.dependencies);
      if (children.length > 32) throw Error("PLATFORM_DEPENDENCY_COUNT_LIMIT");
      for (const child of children.sort()) await visit(child, packageRoot);
    }
  }
  await visit("@palantirkc/ontology", root);
  if (includeWorkContinuity)
    await visit(
      "@palantirkc/work-continuity",
      dependencyJoin(root, "packages/native-hermes-adapter"),
    );
  const ontology = dependencies.find((entry) => entry.name === "@palantirkc/ontology");
  if (
    !ontology ||
    !sourceImporters.length ||
    sourceImporters.length > 128 ||
    new Set(sourceImporters).size !== sourceImporters.length
  )
    throw Error("FINITE_CANONICAL_IMPORTERS_REQUIRED");
  const ontologyRoot = dependencyJoin(root, ontology.root);
  const ontologyManifest = dependencyJoin(ontologyRoot, "package.json");
  const ontologyEntry = await dependencyRealpath(Bun.resolveSync("@palantirkc/ontology", root));
  if (
    !within(ontologyRoot, ontologyEntry) ||
    !dependencyPins.some((entry) => dependencyJoin(root, entry.path) === ontologyEntry)
  )
    throw Error("UNADMITTED_ONTOLOGY_ENTRYPOINT");
  for (const importer of sourceImporters) {
    if (!within(root, importer) || (await dependencyRealpath(importer)) !== importer)
      throw Error("FOREIGN_CANONICAL_IMPORTER");
    // Inspect physical lookup candidates too: a cached resolver answer must not
    // conceal a newly introduced package-local override during final readback.
    let directory = dependencyDirname(importer);
    while (true) {
      const candidate = dependencyJoin(directory, "node_modules/@palantirkc/ontology");
      let present = false;
      try {
        await dependencyLstat(candidate);
        present = true;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
      if (
        present &&
        (await dependencyRealpath(dependencyJoin(candidate, "package.json"))) !== ontologyManifest
      )
        throw Error("UNADMITTED_ONTOLOGY_LOOKUP_ROOT");
      if (directory === root) break;
      directory = dependencyDirname(directory);
    }
    if (
      (await dependencyRealpath(
        Bun.resolveSync("@palantirkc/ontology/package.json", dependencyDirname(importer)),
      )) !== ontologyManifest ||
      (await dependencyRealpath(
        Bun.resolveSync("@palantirkc/ontology", dependencyDirname(importer)),
      )) !== ontologyEntry
    )
      throw Error("ONTOLOGY_IMPORTER_RESOLUTION_DRIFT");
  }
  return {
    dependencies: dependencies.sort((a, b) => a.name.localeCompare(b.name)),
    dependencyPins: dependencyPins.sort((a, b) => a.path.localeCompare(b.path)),
  };
}

captureDependencies = captureInstalledRuntimeClosure;


// Keep the existing v1 consumer's exact single-line marker grammar.
// prettier-ignore
const expectedRegistrationSha256 = "f0c1c39aeec6c75dfd119db9802dbf3d788711abfe1f946e9ac3041c37a85227";
// prettier-ignore
const expectedSuccessorSha256 = "5aa4f30c406a3f72ab6570e032b0d34f0ce612ff8819e7ede5ce4f6232462b53";
const codeOwnerRoot = resolve(import.meta.dir, "../..");
const providerRoot = resolve(codeOwnerRoot, "../agent-platform");
const selfPath = join(codeOwnerRoot, "src/ontology-driven/provider-source.ts");
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
// Bind the loaded guard image too: repinning changed disk bytes cannot qualify cached code.
const initialSelf = (() => {
  if (import.meta.path !== selfPath) return null;
  const file = openSync(selfPath, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = fstatSync(file);
    if (!before.isFile() || before.size > 100_000 || before.nlink !== 1)
      throw Error("EXACT_GENERATED_GUARD_REQUIRED");
    const bytes = Buffer.alloc(before.size + 1);
    let count = 0;
    while (count < bytes.length) {
      const size = readSync(file, bytes, count, bytes.length - count, null);
      if (!size) break;
      count += size;
    }
    const after = fstatSync(file);
    if (
      count !== before.size ||
      before.ino !== after.ino ||
      before.dev !== after.dev ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs
    )
      throw Error("GENERATED_GUARD_CHANGED_DURING_LOAD");
    return sha(bytes.subarray(0, count));
  } finally {
    closeSync(file);
  }
})();
const moduleNames = [
  "ontology-driven/work-context",
  "ontology-driven/contracts",
  "ontology-driven/current-intent",
  "ontology-driven/domain-context",
  "ontology-driven/role-registry",
  "ontology-driven/audit-adapter",
  "ontology-driven/pinned-reader",
  "adoption/semantic-context",
  "adoption/evaluation-budget",
] as const;
const sourcePaths = [
  "package.json",
  "bun.lock",
  "releases/palantirkc-ontology-0.2.0.tgz",
  "packages/native-hermes-adapter/package.json",
  "packages/native-hermes-adapter/releases/palantirkc-ontology-0.2.0.tgz",
  ...moduleNames.map((name) => "packages/native-hermes-adapter/src/" + name + ".ts"),
];
const facadePaths = [
  "package.json",
  "bun.lock",
  "architecture/hermes-context-provider.json",
  "source-successor.json",
  "src/ontology-driven/provider-source.ts",
  ...moduleNames.map((name) => "src/" + name + ".ts"),
].sort();
// Only these existing legacy consumer data entrypoints are outside generated source ownership.
// We inspect file metadata, never read/hash their content or include it in source proof/cache identity.
const ownerDataPaths = new Set(["project.json", "state/next-session.json"]);
type Pin = { path: string; sha256: string };
type Registration = {
  schema: string;
  consumerRoot: string;
  providerRoot: string;
  packageName: string;
  sourcePins: Pin[];
  authority: string;
  nativePermissionGranted: boolean;
};
function object(input: unknown): Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input))
    throw Error("INVALID_SOURCE_OBJECT");
  return input as Record<string, unknown>;
}
function keys(input: unknown, expected: string[]) {
  const value = object(input);
  if (Object.keys(value).sort().join("\0") !== [...expected].sort().join("\0"))
    throw Error("UNKNOWN_OR_MISSING_SOURCE_FIELD");
  return value;
}
async function exact(path: string, max = 2 * 1024 * 1024) {
  if (resolve(path) !== path || (await realpath(path)) !== path)
    throw Error("EXACT_SOURCE_PATH_REQUIRED");
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat();
    if (!before.isFile() || before.size > max || before.nlink !== 1)
      throw Error("BOUNDED_REGULAR_SOURCE_REQUIRED");
    const buffer = Buffer.alloc(before.size + 1);
    let count = 0;
    while (count < buffer.length) {
      const part = await file.read(buffer, count, buffer.length - count, null);
      if (!part.bytesRead) break;
      count += part.bytesRead;
    }
    const after = await file.stat();
    if (
      count !== before.size ||
      before.dev !== after.dev ||
      before.ino !== after.ino ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs
    )
      throw Error("SOURCE_CHANGED_DURING_READ");
    return buffer.subarray(0, count);
  } finally {
    await file.close();
  }
}
async function facadeFiles() {
  const files: string[] = [];
  async function visit(directory: string, prefix: string) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = prefix + entry.name;
      if (!prefix && entry.name === "node_modules") {
        if (!entry.isDirectory() || entry.isSymbolicLink())
          throw Error("INVALID_FACADE_DEPENDENCY_ROOT");
        continue; // actual package resolution is separately fixed and pinned below
      }
      if (entry.isSymbolicLink()) throw Error("UNADMITTED_FACADE_SYMLINK");
      if (ownerDataPaths.has(path)) {
        const absolute = join(directory, entry.name),
          info = await lstat(absolute);
        if (
          !entry.isFile() ||
          !info.isFile() ||
          info.isSymbolicLink() ||
          (await realpath(absolute)) !== absolute
        )
          throw Error("INVALID_EXISTING_OWNER_DATA_ENTRYPOINT");
        continue;
      }
      if (entry.isDirectory()) await visit(join(directory, entry.name), path + "/");
      else if (entry.isFile()) files.push(path);
      else throw Error("UNADMITTED_FACADE_FILE");
      if (files.length > facadePaths.length) throw Error("UNKNOWN_FACADE_FILE");
    }
  }
  await visit(codeOwnerRoot, "");
  if (files.sort().join("\0") !== facadePaths.join("\0"))
    throw Error("INCOMPLETE_OR_UNKNOWN_FACADE_FILE");
}
const imported = new Map<string, string>();
const importedFacades = new Map<string, readonly Pin[]>();
/** v1-compatible exact14 source rows, observed19 pins; caller includes this guard as pin20. */
export async function observeHermesContextProvider(sourceRoot: string) {
  if (
    sourceRoot !== codeOwnerRoot ||
    (await realpath(codeOwnerRoot)) !== codeOwnerRoot ||
    (await realpath(providerRoot)) !== providerRoot ||
    initialSelf === null
  )
    throw Error("Hermes source provider requires the fixed code owner, not a data or native root");
  if (sha(await exact(selfPath, 100_000)) !== initialSelf)
    throw Error("Hermes guard changed after import; use a fresh process");
  const module = join(
    providerRoot,
    "packages/native-hermes-adapter/src/ontology-driven/work-context.ts",
  );
  // Classify a changed previously observed source facade before initial malformed-wrapper validation.
  // This never permits a malformed first import and never clears a native module cache.
  for (const pin of importedFacades.get(module) ?? [])
    if (sha(await exact(pin.path)) !== pin.sha256)
      throw Error("Hermes Work provider changed after import; use a fresh process");
  await facadeFiles();
  const registrationPath = join(codeOwnerRoot, "architecture/hermes-context-provider.json");
  const bytes = await exact(registrationPath);
  if (sha(bytes) !== expectedRegistrationSha256)
    throw Error(
      "Hermes source provider registration changed; use reviewed current source and a fresh packaging process",
    );
  const value = keys(JSON.parse(bytes.toString()), [
    "schema",
    "consumerRoot",
    "providerRoot",
    "packageName",
    "sourcePins",
    "authority",
    "nativePermissionGranted",
  ]);
  if (
    value.schema !== "hermes.context-source-provider.v1" ||
    value.consumerRoot !== codeOwnerRoot ||
    value.providerRoot !== providerRoot ||
    value.packageName !== "@palantirkc/native-hermes-adapter" ||
    value.authority !== "none" ||
    value.nativePermissionGranted !== false ||
    !Array.isArray(value.sourcePins)
  )
    throw Error("Invalid fixed Hermes source provider registration");
  const registration = value as unknown as Registration;
  if (
    registration.sourcePins.length !== sourcePaths.length ||
    new Set(registration.sourcePins.map((pin) => pin.path)).size !== sourcePaths.length ||
    sourcePaths.some((path) => !registration.sourcePins.some((pin) => pin.path === path))
  )
    throw Error("Incomplete Hermes source provider denominator");
  const lineageBytes = await exact(join(codeOwnerRoot, "source-successor.json"));
  if (sha(lineageBytes) !== expectedSuccessorSha256)
    throw Error("NEW_SOURCE_SUCCESSOR_MANIFEST_CHANGED");
  const lineage = keys(JSON.parse(lineageBytes.toString()), [
    "schema",
    "originalSourceRestored",
    "codeOwner",
    "semanticOwners",
    "nativeAndDataOwnersRetained",
    "nativePermissionGranted",
    "authority",
    "lineage",
    "inputs",
    "runtimeDependencies",
  ]);
  if (
    lineage.schema !== "hermes.new-source-generated-projection.v1" ||
    lineage.originalSourceRestored !== false ||
    lineage.codeOwner !== "agent-platform" ||
    lineage.nativeAndDataOwnersRetained !== true ||
    lineage.nativePermissionGranted !== false ||
    lineage.authority !== "none" ||
    JSON.stringify(lineage.semanticOwners) !==
      '["ontology-platform","hermes-harness-engineering"]' ||
    lineage.lineage !== "architecture/hermes-new-source-successor.json"
  )
    throw Error("INVALID_NEW_SOURCE_LINEAGE");
  const provenancePaths = [
    "packages/native-hermes-adapter/src/source-provider/generate.ts",
    "packages/native-hermes-adapter/src/source-provider/guard.template.ts",
    "architecture/hermes-new-source-successor.json",
    "packages/native-hermes-adapter/src/source-provider/dependency-closure.ts",
  ];
  if (
    !Array.isArray(lineage.inputs) ||
    lineage.inputs.length !== sourcePaths.length + provenancePaths.length ||
    new Set(lineage.inputs.map((pin: Pin) => pin.path)).size !== lineage.inputs.length ||
    [...sourcePaths, ...provenancePaths].some(
      (path) => !(lineage.inputs as Pin[]).some((pin) => pin.path === path),
    )
  )
    throw Error("INCOMPLETE_NEW_SOURCE_LINEAGE");
  const provenancePins: Pin[] = [];
  for (const pin of lineage.inputs as Pin[]) {
    keys(pin, ["path", "sha256"]);
    if (sourcePaths.includes(pin.path)) {
      if (registration.sourcePins.find((row) => row.path === pin.path)?.sha256 !== pin.sha256)
        throw Error("NEW_SOURCE_LINEAGE_DRIFT");
      continue; // the compatible v1 observation below verifies these exact14 current source bytes
    }
    const path = join(providerRoot, pin.path);
    if (
      typeof pin.sha256 !== "string" ||
      !/^[a-f0-9]{64}$/u.test(pin.sha256) ||
      sha(await exact(path)) !== pin.sha256
    )
      throw Error("NEW_SOURCE_LINEAGE_DRIFT");
    provenancePins.push({ path, sha256: pin.sha256 });
  }
  const template = (await exact(join(providerRoot, provenancePaths[1]))).toString();
  const dependencySource = (await exact(join(providerRoot, provenancePaths[3]))).toString();
  const inlineMarker = ["//", "__INLINE_CANONICAL_DEPENDENCY_CLOSURE__"].join(" ");
  if (template.split(inlineMarker).length !== 2)
    throw Error("EXACT_DEPENDENCY_PROJECTION_REQUIRED");
  const composed = template.replace(
    inlineMarker,
    dependencySource + "\ncaptureDependencies = captureInstalledRuntimeClosure;\n",
  );
  const marker = 'const expectedRegistrationSha256 = "' + "0".repeat(64) + '";';
  const lineageMarker = 'const expectedSuccessorSha256 = "' + "0".repeat(64) + '";';
  if (
    composed.split(marker).length !== 2 ||
    composed.split(lineageMarker).length !== 2 ||
    sha(
      composed
        .replace(marker, 'const expectedRegistrationSha256 = "' + expectedRegistrationSha256 + '";')
        .replace(
          lineageMarker,
          'const expectedSuccessorSha256 = "' + expectedSuccessorSha256 + '";',
        ),
    ) !== initialSelf
  )
    throw Error("GENERATED_GUARD_TEMPLATE_BINDING_CHANGED");
  const pins: Pin[] = [{ path: registrationPath, sha256: sha(bytes) }];
  for (const row of registration.sourcePins) {
    keys(row, ["path", "sha256"]);
    if (typeof row.sha256 !== "string" || !/^[a-f0-9]{64}$/u.test(row.sha256))
      throw Error("INVALID_HERMES_SOURCE_DIGEST");
    const path = join(providerRoot, row.path);
    if (sha(await exact(path)) !== row.sha256)
      throw Error("Hermes source provider changed: " + path);
    pins.push({ path, sha256: row.sha256 });
  }
  const manifest = object(
    JSON.parse(
      (await exact(join(providerRoot, "packages/native-hermes-adapter/package.json"))).toString(),
    ),
  );
  const owner = object(JSON.parse((await exact(join(codeOwnerRoot, "package.json"))).toString()));
  if (
    object(owner.dependencies)[registration.packageName] !==
      "file:../agent-platform/packages/native-hermes-adapter" ||
    manifest.name !== registration.packageName ||
    object(manifest.dependencies)["@palantirkc/ontology"] !==
      "file:releases/palantirkc-ontology-0.2.0.tgz"
  )
    throw Error("Hermes source provider native dependency binding changed");
  for (const name of moduleNames) {
    if (
      (await exact(join(codeOwnerRoot, "src/" + name + ".ts"))).toString() !==
      "export * from '@palantirkc/native-hermes-adapter/" + name + "';\n"
    )
      throw Error("NONCANONICAL_GENERATED_FORWARDER");
    if (
      Bun.resolveSync(registration.packageName + "/" + name, codeOwnerRoot) !==
      join(providerRoot, "packages/native-hermes-adapter/src/" + name + ".ts")
    )
      throw Error("Hermes actual implementation resolution escaped canonical owner");
  }
  const installedPath = Bun.resolveSync("@palantirkc/ontology/package.json", dirname(module));
  if (!installedPath.startsWith(providerRoot + "/"))
    throw Error("FOREIGN_IMMUTABLE_DEPENDENCY_ROOT");
  const installed = object(JSON.parse((await exact(installedPath)).toString()));
  if (installed.name !== "@palantirkc/ontology" || installed.version !== "0.2.0")
    throw Error("Hermes actual immutable Ontology version changed");
  if (
    registration.sourcePins.find((pin) => pin.path === sourcePaths[2])!.sha256 !==
    registration.sourcePins.find((pin) => pin.path === sourcePaths[4])!.sha256
  )
    throw Error("Hermes immutable release copy differs");
  for (const path of [
    installedPath,
    Bun.resolveSync("@palantirkc/ontology", dirname(module)),
    join(codeOwnerRoot, "package.json"),
    join(codeOwnerRoot, "bun.lock"),
  ])
    pins.push({ path, sha256: sha(await exact(path)) });
  const importers = moduleNames.map((name) =>
    join(providerRoot, "packages/native-hermes-adapter/src", name + ".ts"),
  );
  const runtimeDependencies = await captureDependencies(providerRoot, exact, sha, false, importers);
  if (JSON.stringify(runtimeDependencies) !== JSON.stringify(lineage.runtimeDependencies))
    throw Error("EXECUTABLE_DEPENDENCY_CLOSURE_DRIFT");
  const runtimePins = runtimeDependencies.dependencyPins.map((pin) => ({
    path: join(providerRoot, pin.path),
    sha256: pin.sha256,
  }));
  // Every generated source file participates in cache/readback currentness, beyond the compatible19 proof.
  const facadePins = await Promise.all(
    facadePaths.map(async (name) => ({
      path: join(codeOwnerRoot, name),
      sha256: sha(await exact(join(codeOwnerRoot, name))),
    })),
  );
  const closure = sha(JSON.stringify([...pins, ...facadePins, ...provenancePins, ...runtimePins]));
  const prior = imported.get(module);
  if (prior !== undefined && prior !== closure)
    throw Error("Hermes Work provider changed after import; use a fresh process");
  if (prior === undefined) {
    const cache = require.cache;
    if (
      !cache ||
      moduleNames.some((name) =>
        Object.hasOwn(
          cache,
          join(providerRoot, "packages/native-hermes-adapter/src/" + name + ".ts"),
        ),
      ) ||
      moduleNames.some((name) =>
        Object.hasOwn(cache, join(codeOwnerRoot, "src/" + name + ".ts")),
      ) ||
      runtimePins.some((pin) => /\.[cm]?[jt]s$/u.test(pin.path) && Object.hasOwn(cache, pin.path))
    )
      throw Error("Hermes Work code was loaded before source observation; use a fresh process");
  }
  // Consumers may annotate/reorder their public proof. Its rows must never alias verification state.
  const verificationPins = Object.freeze(
    [...pins, ...facadePins, ...provenancePins, ...runtimePins].map((pin) =>
      Object.freeze({ ...pin }),
    ),
  );
  const verify = async () => {
    await facadeFiles();
    for (const pin of verificationPins)
      if (sha(await exact(pin.path)) !== pin.sha256)
        throw Error("Hermes code source changed during packaging: " + pin.path);
    const current = await captureDependencies(providerRoot, exact, sha, false, importers);
    if (JSON.stringify(current) !== JSON.stringify(runtimeDependencies))
      throw Error("Hermes executable dependency closure changed during read");
  };
  await verify();
  imported.set(module, closure);
  importedFacades.set(module, facadePins);
  return {
    schema: "hermes.observed-context-source-provider.v1" as const,
    codeOwnerRoot,
    providerRoot,
    packageName: registration.packageName,
    pins: pins.map((pin) => ({ ...pin })),
    verify,
  };
}
