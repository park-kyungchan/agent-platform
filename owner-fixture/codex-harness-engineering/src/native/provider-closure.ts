import { lstat, open, realpath } from "node:fs/promises";
import { constants } from "node:fs";
import { resolve, dirname } from "node:path";
import { createHash } from "node:crypto";

// Builtins only: the native entrypoint imports this guard before any provider implementation.
const root = resolve(import.meta.dir, "../.."),
  common = resolve(root, "../agent-platform"),
  name = "@palantirkc/native-codex-adapter";
const modules = ["hooks", "home-context", "evaluation-ledger"] as const;
const required = [
  "package.json", "bun.lock", "packages/native-codex-adapter/package.json",
  ...modules.map((module) => "packages/native-codex-adapter/src/native/" + module + ".ts"),
];
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
interface SourcePin { path: string; sha256: string }
async function source(path: string) {
  if (resolve(path) !== path || (await realpath(path)) !== path) throw Error("EXACT_NATIVE_SOURCE_REQUIRED");
  let directory = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    const parts = path.split("/").slice(1);
    for (const part of parts.slice(0, -1)) {
      const next = await open("/proc/self/fd/" + directory.fd + "/" + part,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
      await directory.close(); directory = next;
    }
    const file = await open("/proc/self/fd/" + directory.fd + "/" + parts.at(-1),
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    try {
      const before = await file.stat();
      if (!before.isFile() || before.nlink < 1 || before.nlink > 8 || before.size > 2097152)
        throw Error("BOUNDED_REGULAR_NATIVE_SOURCE_REQUIRED");
      const buffer = Buffer.alloc(before.size + 1); let count = 0;
      while (count < buffer.length) {
        const result = await file.read(buffer, count, buffer.length - count, null);
        if (!result.bytesRead) break; count += result.bytesRead;
      }
      const bytes = buffer.subarray(0, count), after = await file.stat();
      if (bytes.length !== before.size || before.dev !== after.dev || before.ino !== after.ino ||
          before.nlink !== after.nlink || before.size !== after.size || before.mtimeMs !== after.mtimeMs ||
          before.ctimeMs !== after.ctimeMs || (await realpath(path)) !== path)
        throw Error("NATIVE_SOURCE_CHANGED_DURING_READ");
      return { bytes, pin: { path, sha256: hash(bytes) } };
    } finally { await file.close(); }
  } finally { await directory.close(); }
}
function row(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw Error("NATIVE_SOURCE_RECORD_REQUIRED");
  return value as Record<string, unknown>;
}
/** Fixed SOURCE verification only. Native /hooks retains exact definition trust and permission. */
export async function readNativeAdapterSourceClosure() {
  const owner = await Promise.all(modules.map((module) => source(resolve(root, "src/native/" + module + ".ts"))));
  const marker = /^\/\/ codex-native-provider-registration-sha256: ([a-f0-9]{64})\n/.exec(owner[0]!.bytes.toString());
  if (!marker) {
    let registered = false;
    try { await lstat(resolve(root, "architecture/native-codex-provider.json")); registered = true; }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; }
    if (registered || owner.some((item) => item.bytes.toString().includes(name)))
      throw Error("REGISTERED_NATIVE_SOURCE_MARKER_REQUIRED");
    // Preserve the pre-registration owner-only reader. Registered providers can never downgrade.
    const legacyPins = Object.freeze(owner.map((item) => Object.freeze({ ...item.pin })));
    const verify = async () => {
      for (const pin of legacyPins)
        if ((await source(pin.path)).pin.sha256 !== pin.sha256) throw Error("NATIVE_SOURCE_CLOSURE_CHANGED");
      try { await lstat(resolve(root, "architecture/native-codex-provider.json")); }
      catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return; throw error; }
      throw Error("LEGACY_NATIVE_SOURCE_REGISTRATION_CHANGED");
    };
    await verify();
    return Object.freeze({ pins: legacyPins, providerRoot: null, verify });
  }
  const registration = await source(resolve(root, "architecture/native-codex-provider.json"));
  if (registration.pin.sha256 !== marker[1]) throw Error("NATIVE_SOURCE_REGISTRATION_CHANGED");
  const value = row(JSON.parse(registration.bytes.toString()));
  if (value.schema !== "codex.native-adapter-source-closure.v1" || value.consumerRoot !== root ||
      value.providerRoot !== common || value.packageName !== name || value.authority !== "none" ||
      value.nativePermissionGranted !== false || Object.keys(value).sort().join("|") !==
      "authority|consumerRoot|nativePermissionGranted|packageName|providerRoot|schema|sourcePins")
    throw Error("NATIVE_SOURCE_SCOPE_CHANGED");
  if (!Array.isArray(value.sourcePins) || value.sourcePins.length !== required.length)
    throw Error("EXACT_NATIVE_PROVIDER_SOURCE_SET_REQUIRED");
  const declared = value.sourcePins.map((input: unknown): SourcePin => {
    const item = row(input);
    if (Object.keys(item).sort().join("|") !== "path|sha256" || typeof item.path !== "string" ||
        !required.includes(item.path) || typeof item.sha256 !== "string" || !/^[a-f0-9]{64}$/.test(item.sha256))
      throw Error("EXACT_NATIVE_PROVIDER_SOURCE_SET_REQUIRED");
    return { path: item.path, sha256: item.sha256 };
  });
  if (new Set(declared.map((item) => item.path)).size !== required.length)
    throw Error("EXACT_NATIVE_PROVIDER_SOURCE_SET_REQUIRED");
  const pins = [...owner.map((item) => item.pin), registration.pin], provider = new Map<string, Buffer>();
  for (const declaredPin of declared) {
    const observed = await source(resolve(common, declaredPin.path));
    if (observed.pin.sha256 !== declaredPin.sha256) throw Error("NATIVE_PROVIDER_SOURCE_CHANGED");
    pins.push(observed.pin); provider.set(declaredPin.path, observed.bytes);
  }
  const pkg = row(JSON.parse(provider.get("packages/native-codex-adapter/package.json")!.toString())),
    consumer = await source(resolve(root, "package.json")), lock = await source(resolve(root, "bun.lock"));
  pins.push(consumer.pin, lock.pin);
  if (pkg.name !== name || row(pkg.dependencies)["@palantirkc/work-continuity"] !== "file:../work-continuity" ||
      row(row(JSON.parse(consumer.bytes.toString())).dependencies)[name] !== "file:../agent-platform/packages/native-codex-adapter")
    throw Error("NATIVE_PACKAGE_BINDING_CHANGED");
  for (const module of modules) {
    const expected = resolve(common, "packages/native-codex-adapter/src/native/" + module + ".ts");
    if (row(pkg.exports)["./" + module] !== "./src/native/" + module + ".ts" ||
        Bun.resolveSync(name + "/" + module, dirname(resolve(root, "src/native/" + module + ".ts"))) !== expected)
      throw Error("NATIVE_PACKAGE_RESOLUTION_CHANGED");
  }
  for (const path of ["src/legacy/harness/owner-context-loader.ts", "src/legacy/harness/provider-closure.ts",
    "src/native/provider-closure.ts", "src/native/hook-cli.ts"])
    pins.push((await source(resolve(root, path))).pin);
  const frozen = Object.freeze(pins.map((pin) => Object.freeze({ ...pin })));
  const verify = async () => {
    for (const pin of frozen)
      if ((await source(pin.path)).pin.sha256 !== pin.sha256) throw Error("NATIVE_SOURCE_CLOSURE_CHANGED");
    for (const module of modules)
      if (Bun.resolveSync(name + "/" + module, resolve(root, "src/native")) !==
          resolve(common, "packages/native-codex-adapter/src/native/" + module + ".ts"))
        throw Error("NATIVE_PACKAGE_RESOLUTION_CHANGED");
  };
  await verify();
  return Object.freeze({ pins: frozen, providerRoot: common, verify });
}
