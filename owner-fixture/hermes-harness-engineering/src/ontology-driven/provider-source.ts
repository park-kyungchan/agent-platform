import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

const codeOwnerRoot = resolve(import.meta.dir, "../..");
const providerRoot = resolve(codeOwnerRoot, "../agent-platform");
const expectedRegistrationSha256 = "c75726b1726583183a5dabd43a4f66ef1fb73e23d2060f6b80a3c3e8af7ec8a4";
const sha = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
export async function observeHermesContextProvider(sourceRoot: string) {
  if (sourceRoot !== codeOwnerRoot || await realpath(codeOwnerRoot) !== codeOwnerRoot || await realpath(providerRoot) !== providerRoot) throw Error("Hermes source provider requires the fixed code owner, not a data or native root");
  const registrationPath = join(codeOwnerRoot, "architecture/hermes-context-provider.json"), bytes = await readFile(registrationPath);
  if (sha(bytes) !== expectedRegistrationSha256) throw Error("Hermes source provider registration changed; use reviewed current source and a fresh packaging process");
  const registration = JSON.parse(bytes.toString());
  if (registration.schema !== "hermes.context-source-provider.v1" || registration.consumerRoot !== codeOwnerRoot || registration.providerRoot !== providerRoot || registration.packageName !== "@palantirkc/native-hermes-adapter" || registration.authority !== "none" || registration.nativePermissionGranted !== false) throw Error("Invalid fixed Hermes source provider registration");
  const relativePaths = ["package.json", "bun.lock", "releases/palantirkc-ontology-0.2.0.tgz", "packages/native-hermes-adapter/package.json", "packages/native-hermes-adapter/releases/palantirkc-ontology-0.2.0.tgz", ...["work-context","contracts","current-intent","domain-context","role-registry","audit-adapter","pinned-reader"].map(name => "packages/native-hermes-adapter/src/ontology-driven/" + name + ".ts"), "packages/native-hermes-adapter/src/adoption/semantic-context.ts", "packages/native-hermes-adapter/src/adoption/evaluation-budget.ts"];
  if (registration.sourcePins.length !== relativePaths.length || new Set(registration.sourcePins.map((pin: any) => pin.path)).size !== relativePaths.length || relativePaths.some(path => !registration.sourcePins.some((pin: any) => pin.path === path))) throw Error("Incomplete Hermes source provider denominator");
  const pins = [{ path: registrationPath, sha256: sha(bytes) }];
  for (const source of registration.sourcePins) {
    const path = join(providerRoot, source.path);
    if (resolve(path) !== path || await realpath(path) !== path || !(await lstat(path)).isFile() || sha(await readFile(path)) !== source.sha256) throw Error("Hermes source provider changed: " + path);
    pins.push({ path, sha256: source.sha256 });
  }
  const manifest = JSON.parse(await readFile(join(providerRoot, "packages/native-hermes-adapter/package.json"), "utf8")), owner = JSON.parse(await readFile(join(codeOwnerRoot, "package.json"), "utf8"));
  if (owner.dependencies?.[registration.packageName] !== "file:../agent-platform/packages/native-hermes-adapter" || manifest.name !== registration.packageName || manifest.dependencies?.["@palantirkc/ontology"] !== "file:releases/palantirkc-ontology-0.2.0.tgz") throw Error("Hermes source provider native dependency binding changed");
  const module = Bun.resolveSync(registration.packageName + "/ontology-driven/work-context", codeOwnerRoot);
  if (module !== join(providerRoot, "packages/native-hermes-adapter/src/ontology-driven/work-context.ts")) throw Error("Hermes actual implementation resolution escaped canonical owner");
  const installedPath = Bun.resolveSync("@palantirkc/ontology/package.json", dirname(module)), installed = JSON.parse(await readFile(installedPath, "utf8"));
  if (installed.name !== "@palantirkc/ontology" || installed.version !== "0.2.0") throw Error("Hermes actual immutable Ontology version changed");
  if (registration.sourcePins.find((pin: any) => pin.path === "releases/palantirkc-ontology-0.2.0.tgz").sha256 !== registration.sourcePins.find((pin: any) => pin.path === "packages/native-hermes-adapter/releases/palantirkc-ontology-0.2.0.tgz").sha256) throw Error("Hermes immutable release copy differs");
  for (const path of [installedPath, Bun.resolveSync("@palantirkc/ontology", dirname(module)), join(codeOwnerRoot, "package.json"), join(codeOwnerRoot, "bun.lock")]) pins.push({ path, sha256: sha(await readFile(path)) });
  return { schema: "hermes.observed-context-source-provider.v1" as const, codeOwnerRoot, providerRoot, packageName: registration.packageName, pins, verify: async () => { for (const source of pins) if (await realpath(source.path) !== source.path || !(await lstat(source.path)).isFile() || sha(await readFile(source.path)) !== source.sha256) throw Error("Hermes code source changed during packaging: " + source.path); } };
}
