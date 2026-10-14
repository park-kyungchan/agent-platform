import { resolve, dirname } from "node:path";
export interface ProviderPin {
  path: string;
  sha256: string;
}
export interface ProviderRead {
  bytes: Uint8Array;
  pin: ProviderPin;
}
export interface ContextProviderProof {
  pins: ProviderPin[];
  providerRoot?: string;
  /** Present only for the exact fixed Git Work export proven by a v3 registration. */
  gitWorkModulePath?: string;
}
export const contextProviderRelativeSources = [
  "project.json",
  "ontology-pin.json",
  "package.json",
  "bun.lock",
  "packages/work-continuity/package.json",
  "releases/palantirkc-ontology-0.2.0.tgz",
  ...["index", "types", "validation", "source", "store", "context", "boundary", "sections"].map(
    (name) => "packages/work-continuity/src/context/" + name + ".ts",
  ),
  "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
] as const;
export const gitWorkProviderRelativeSources = [
  ...["index", "source", "records", "context", "types"].map(
    (name) => "packages/work-continuity/src/git-work/" + name + ".ts",
  ),
  "packages/work-continuity/src/logical-identity.ts",
  "packages/work-continuity/src/operational-state.ts",
] as const;
const legacyContextProviderRelativeSources = contextProviderRelativeSources.filter(
  (path) => path !== "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
);
const packageName = "@palantirkc/work-continuity";
const gitWorkModuleRelativePath = "packages/work-continuity/src/git-work/index.ts";
/** Exact source-owner bridge only. No arbitrary package/module/Home routing. */
export async function readRegisteredContextProvider(
  root: string,
  module: string,
  indexBytes: Uint8Array,
  readSource: (path: string) => Promise<ProviderRead>,
): Promise<ContextProviderProof> {
  const text = new TextDecoder("utf-8", { fatal: true }).decode(indexBytes);
  if (!text.includes(packageName + "/context")) return { pins: [] };
  const match =
    /^\/\/ codex-context-provider-registration-sha256: ([a-f0-9]{64})\nexport \* from '@palantirkc\/work-continuity\/context';\n$/.exec(
      text,
    );
  if (!match) throw Error("Unregistered context provider forwarder");
  const registration = await readSource(resolve(root, "architecture/context-provider.json"));
  if (registration.bytes.byteLength > 2097152 || registration.pin.sha256 !== match[1])
    throw Error("Pinned context provider registration changed");
  const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(registration.bytes));
  const version3 = value?.schema === "codex.context-provider-source-closure.v3";
  const version2 = version3 || value?.schema === "codex.context-provider-source-closure.v2";
  const requiredSources = version3
    ? [...contextProviderRelativeSources, ...gitWorkProviderRelativeSources]
    : version2
      ? contextProviderRelativeSources
      : legacyContextProviderRelativeSources;
  const keys = [
    "schema",
    "consumerRoot",
    "providerRoot",
    "providerProjectId",
    "packageName",
    "modulePath",
    "sourcePins",
    "authority",
    "nativePermissionGranted",
    ...(version3 ? ["gitWorkModulePath"] : []),
  ];
  if (
    !value ||
    Object.keys(value).sort().join("|") !== keys.sort().join("|") ||
    ![
      "codex.context-provider-source-closure.v1",
      "codex.context-provider-source-closure.v2",
      "codex.context-provider-source-closure.v3",
    ].includes(value.schema) ||
    value.consumerRoot !== root ||
    value.providerRoot !== resolve(root, "../agent-platform") ||
    value.providerProjectId !== "agent-platform" ||
    value.packageName !== packageName ||
    value.modulePath !== "packages/work-continuity/src/context/index.ts" ||
    (version3 && value.gitWorkModulePath !== gitWorkModuleRelativePath) ||
    value.authority !== "none" ||
    value.nativePermissionGranted !== false
  )
    throw Error("Unregistered context provider owner/scope");
  if (
    !Array.isArray(value.sourcePins) ||
    value.sourcePins.length !== requiredSources.length ||
    new Set(value.sourcePins.map((x: ProviderPin) => x.path)).size !== requiredSources.length ||
    requiredSources.some(
      (path) => !value.sourcePins.some((x: ProviderPin) => x.path === path),
    ) ||
    value.sourcePins.some(
      (x: ProviderPin) =>
        !x ||
        Object.keys(x).sort().join("|") !== "path|sha256" ||
        !/^[a-f0-9]{64}$/.test(x.sha256),
    )
  )
    throw Error("Exact immutable context provider source closure required");
  const pins: ProviderPin[] = [registration.pin];
  const sources = new Map<string, ProviderRead>();
  for (const pin of value.sourcePins) {
    const path = resolve(value.providerRoot, pin.path),
      source = await readSource(path);
    if (source.bytes.byteLength > 2097152 || source.pin.sha256 !== pin.sha256)
      throw Error("Context provider source changed");
    pins.push(source.pin);
    sources.set(pin.path, source);
  }
  const json = (path: string) =>
    JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(sources.get(path)!.bytes));
  const project = json("project.json"),
    upper = json("ontology-pin.json"),
    pkg = json("packages/work-continuity/package.json"),
    rootPackage = json("package.json");
  if (
    project.kind !== "project" ||
    project.projectId !== "agent-platform" ||
    project.payload?.details?.sourceOwnerRoot !== value.providerRoot ||
    project.payload?.ontologyPackage?.name !== "@palantirkc/ontology" ||
    project.payload?.ontologyPackage?.version !== "0.2.0" ||
    upper.name !== "@palantirkc/ontology" ||
    upper.version !== "0.2.0" ||
    upper.sourceOwner !== "ontology-foundation" ||
    upper.releasePin?.sha256 !== sources.get("releases/palantirkc-ontology-0.2.0.tgz")!.pin.sha256 ||
    pkg.name !== packageName ||
    pkg.exports?.["./context"] !== "./src/context/index.ts" ||
    (version3 && pkg.exports?.["./git-work"] !== "./src/git-work/index.ts") ||
    rootPackage.dependencies?.["@palantirkc/ontology"] !==
      "file:releases/palantirkc-ontology-0.2.0.tgz" ||
    (version2
      ? pkg.dependencies?.["@palantirkc/ontology"] !==
          "file:releases/palantirkc-ontology-0.2.0.tgz" ||
        pkg.peerDependencies?.["@palantirkc/ontology"] !== undefined ||
        rootPackage.overrides?.["@palantirkc/ontology"] !== undefined ||
        sources.get("packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz")!.pin
          .sha256 !== sources.get("releases/palantirkc-ontology-0.2.0.tgz")!.pin.sha256
      : pkg.peerDependencies?.["@palantirkc/ontology"] !== "0.2.0" ||
        rootPackage.overrides?.["@palantirkc/ontology"] !==
          "file:releases/palantirkc-ontology-0.2.0.tgz")
  )
    throw Error("Context provider immutable owner/package/Ontology binding differs");
  const expected = resolve(value.providerRoot, value.modulePath),
    actual = Bun.resolveSync(packageName + "/context", dirname(module));
  // readSource performs exact canonical containment/type/2MiB checks. Resolution imports no code.
  if (actual !== expected)
    throw Error("Context provider package resolves outside registered canonical module");
  const gitWorkModulePath = version3
    ? resolve(value.providerRoot, gitWorkModuleRelativePath)
    : undefined;
  if (
    gitWorkModulePath &&
    Bun.resolveSync(packageName + "/git-work", dirname(module)) !== gitWorkModulePath
  )
    throw Error("Git Work provider package resolves outside registered canonical module");
  await recheckContextProvider({ pins }, readSource);
  return {
    pins,
    providerRoot: value.providerRoot,
    ...(gitWorkModulePath ? { gitWorkModulePath } : {}),
  };
}
export async function recheckContextProvider(
  proof: ContextProviderProof,
  readSource: (path: string) => Promise<ProviderRead>,
): Promise<void> {
  for (const pin of proof.pins) {
    const source = await readSource(pin.path);
    if (source.bytes.byteLength > 2097152 || source.pin.sha256 !== pin.sha256)
      throw Error("Context provider changed during read");
  }
}
