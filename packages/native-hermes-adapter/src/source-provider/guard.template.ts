/** NEW source-only compatibility guard. The generator substitutes only the registration digest.
 * This source is not recovered historical owner code and confers no native permission. */
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, openSync, readSync } from "node:fs";
import { open, readdir, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

// Keep the existing v1 consumer's exact single-line marker grammar.
// prettier-ignore
const expectedRegistrationSha256 = "0000000000000000000000000000000000000000000000000000000000000000";
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
  const lineage = keys(
    JSON.parse((await exact(join(codeOwnerRoot, "source-successor.json"))).toString()),
    [
      "schema",
      "originalSourceRestored",
      "codeOwner",
      "semanticOwners",
      "nativeAndDataOwnersRetained",
      "nativePermissionGranted",
      "authority",
      "lineage",
      "inputs",
    ],
  );
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
  const marker = 'const expectedRegistrationSha256 = "' + "0".repeat(64) + '";';
  if (
    template.split(marker).length !== 2 ||
    sha(
      template.replace(
        marker,
        'const expectedRegistrationSha256 = "' + expectedRegistrationSha256 + '";',
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
  const module = join(
    providerRoot,
    "packages/native-hermes-adapter/src/ontology-driven/work-context.ts",
  );
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
  // Every generated source file participates in cache/readback currentness, beyond the compatible19 proof.
  const facadePins = await Promise.all(
    facadePaths.map(async (name) => ({
      path: join(codeOwnerRoot, name),
      sha256: sha(await exact(join(codeOwnerRoot, name))),
    })),
  );
  const closure = sha(JSON.stringify([...pins, ...facadePins, ...provenancePins]));
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
      moduleNames.some((name) => Object.hasOwn(cache, join(codeOwnerRoot, "src/" + name + ".ts")))
    )
      throw Error("Hermes Work code was loaded before source observation; use a fresh process");
  }
  const verify = async () => {
    await facadeFiles();
    for (const pin of [...pins, ...facadePins, ...provenancePins])
      if (sha(await exact(pin.path)) !== pin.sha256)
        throw Error("Hermes code source changed during packaging: " + pin.path);
  };
  await verify();
  imported.set(module, closure);
  return {
    schema: "hermes.observed-context-source-provider.v1" as const,
    codeOwnerRoot,
    providerRoot,
    packageName: registration.packageName,
    pins,
    verify,
  };
}
