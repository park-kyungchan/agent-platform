import { resolve, dirname, basename } from "node:path";
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
  canonicalProviderRoot?: string;
  sourceLocationId?: string;
  /** Present only for the exact fixed Git Work export proven by a v3 registration. */
  gitWorkModulePath?: string;
  /** Explicit v6 admission only; exposed through the existing context index. */
  workMethodAvailable?: true;
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
export const memoryProviderRelativeSources = [
  "packages/work-continuity/src/workspace/recall-request.ts",
  "packages/work-continuity/src/workspace/lexical-recall.ts",
  "packages/work-continuity/src/workspace/qualification.ts",
] as const;
export const workMethodProviderRelativeSources = [
  "packages/work-continuity/src/work-method.ts",
] as const;
const memoryModuleRelativePath = memoryProviderRelativeSources[0];
const legacyContextProviderRelativeSources = contextProviderRelativeSources.filter(
  (path) => path !== "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
);
const packageName = "@palantirkc/work-continuity";
const gitWorkModuleRelativePath = "packages/work-continuity/src/git-work/index.ts";
/** Closed source-only declaration; selection never reads cwd, HOME, Work or native identity. */
export function selectRegisteredProviderLocation(root: string, value: {
  consumerRoot: unknown; providerRoot: unknown; sourceProjections: unknown;
}): { providerRoot: string; canonicalProviderRoot: string; sourceLocationId: string } {
  const exactRoot = (path: unknown): path is string => typeof path === "string" &&
    path.startsWith("/") && resolve(path) === path && !path.includes("\\") && !path.includes("\0");
  const fail = () => { throw Error("Unregistered source provider location"); };
  if (!exactRoot(value.consumerRoot) || !exactRoot(value.providerRoot) ||
      basename(value.consumerRoot) !== "codex-harness-engineering" ||
      basename(dirname(value.consumerRoot)) !== "projects" ||
      value.providerRoot !== resolve(value.consumerRoot, "../agent-platform") ||
      !Array.isArray(value.sourceProjections) || value.sourceProjections.length !== 1) return fail();
  const projection = value.sourceProjections[0];
  if (!projection || typeof projection !== "object" || Array.isArray(projection) ||
      Object.keys(projection).sort().join("|") !==
        "authority|canonicalConsumerRoot|canonicalProviderRoot|classification|consumerRoot|id|nativePermissionGranted|packages|providerProjectId|providerRoot" ||
      projection.id !== "herdr-runtime-codex-source" ||
      projection.classification !== "explicit-runtime-source-checkout-only" ||
      projection.canonicalConsumerRoot !== value.consumerRoot ||
      projection.canonicalProviderRoot !== value.providerRoot ||
      projection.consumerRoot !== resolve(value.consumerRoot, "../../code/shared/codex-harness-engineering") ||
      projection.providerRoot !== resolve(value.consumerRoot, "../../code/shared/agent-platform") ||
      projection.consumerRoot === value.consumerRoot ||
      projection.providerProjectId !== "agent-platform" ||
      JSON.stringify(projection.packages) !== JSON.stringify(["@palantirkc/work-continuity", "@palantirkc/native-codex-adapter"]) ||
      projection.authority !== "none" || projection.nativePermissionGranted !== false) return fail();
  const canonical = root === value.consumerRoot;
  if (!canonical && root !== projection.consumerRoot) return fail();
  const providerRoot = canonical ? value.providerRoot : projection.providerRoot;
  if (providerRoot !== resolve(root, "../agent-platform")) return fail();
  return { providerRoot, canonicalProviderRoot: value.providerRoot,
    sourceLocationId: canonical ? "canonical-host-source" : projection.id };
}

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
  const version6 = value?.schema === "codex.context-provider-source-closure.v6";
  const version5 = version6 || value?.schema === "codex.context-provider-source-closure.v5";
  const version4 = version5 || value?.schema === "codex.context-provider-source-closure.v4";
  const location = version5 ? selectRegisteredProviderLocation(root, value) : undefined;
  const providerRoot = location?.providerRoot ?? value?.providerRoot;
  const version3 = version4 || value?.schema === "codex.context-provider-source-closure.v3";
  const version2 = version3 || value?.schema === "codex.context-provider-source-closure.v2";
  const requiredSources = version6
    ? [...contextProviderRelativeSources, ...gitWorkProviderRelativeSources, ...memoryProviderRelativeSources, ...workMethodProviderRelativeSources]
    : version4
    ? [...contextProviderRelativeSources, ...gitWorkProviderRelativeSources, ...memoryProviderRelativeSources]
    : version3
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
    ...(version5 ? ["sourceProjections"] : []),
  ];
  if (
    !value ||
    Object.keys(value).sort().join("|") !== keys.sort().join("|") ||
    ![
      "codex.context-provider-source-closure.v1",
      "codex.context-provider-source-closure.v2",
      "codex.context-provider-source-closure.v3",
      "codex.context-provider-source-closure.v4",
      "codex.context-provider-source-closure.v5",
      "codex.context-provider-source-closure.v6",
    ].includes(value.schema) ||
    (!version5 && value.consumerRoot !== root) ||
    providerRoot !== resolve(root, "../agent-platform") ||
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
    const path = resolve(providerRoot, pin.path),
      source = await readSource(path);
    if (source.bytes.byteLength > 2097152 || source.pin.sha256 !== pin.sha256)
      throw Error("Context provider source changed");
    pins.push(source.pin);
    sources.set(pin.path, source);
  }
  validateContextProviderMetadata(sources, value.providerRoot, version6 ? 6 : version4 ? 4 : version3 ? 3 : version2 ? 2 : 1);
  const expected = resolve(providerRoot, value.modulePath),
    actual = Bun.resolveSync(packageName + "/context", dirname(module));
  // readSource performs exact canonical containment/type/2MiB checks. Resolution imports no code.
  if (actual !== expected)
    throw Error("Context provider package resolves outside registered canonical module");
  const gitWorkModulePath = version3
    ? resolve(providerRoot, gitWorkModuleRelativePath)
    : undefined;
  if (
    gitWorkModulePath &&
    Bun.resolveSync(packageName + "/git-work", dirname(module)) !== gitWorkModulePath
  )
    throw Error("Git Work provider package resolves outside registered canonical module");
  if (version4 && Bun.resolveSync(packageName + "/memory/recall-request", dirname(module)) !==
      resolve(providerRoot, memoryModuleRelativePath))
    throw Error("Memory provider package resolves outside registered canonical module");
  await recheckContextProvider({ pins }, readSource);
  return {
    pins,
    providerRoot,
    ...(location ? { canonicalProviderRoot: location.canonicalProviderRoot, sourceLocationId: location.sourceLocationId } : {}),
    ...(gitWorkModulePath ? { gitWorkModulePath } : {}),
    ...(version6 ? { workMethodAvailable: true as const } : {}),
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

/** Pure semantic validation, also used by dry source composition. It observes no target runtime. */
export function validateContextProviderMetadata(
  sources: ReadonlyMap<string, ProviderRead>, canonicalProviderRoot: string, version: 1 | 2 | 3 | 4 | 6,
): void {
  const version2 = version >= 2, version3 = version >= 3, version4 = version >= 4;
  const json = (path: string) =>
    JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(sources.get(path)!.bytes));
  const project = json("project.json"),
    upper = json("ontology-pin.json"),
    pkg = json("packages/work-continuity/package.json"),
    rootPackage = json("package.json");
  const indexSource = sources.get("packages/work-continuity/src/context/index.ts");
  if (!indexSource) throw Error("Context provider index source required");
  const imports = new Bun.Transpiler({ loader: "ts" }).scanImports(
    new TextDecoder("utf-8", { fatal: true }).decode(indexSource.bytes),
  );
  const importsWorkMethod = imports.some(({ path }) => path === "../work-method" || path === "../work-method.ts");
  if (version === 6 ? !importsWorkMethod : importsWorkMethod)
    throw Error(version === 6 ? "Work method provider requires its existing context index export" : "Work method executable requires the v6 source closure");
  if (
    project.kind !== "project" ||
    project.projectId !== "agent-platform" ||
    project.payload?.details?.sourceOwnerRoot !== canonicalProviderRoot ||
    project.payload?.ontologyPackage?.name !== "@palantirkc/ontology" ||
    project.payload?.ontologyPackage?.version !== "0.2.0" ||
    upper.name !== "@palantirkc/ontology" ||
    upper.version !== "0.2.0" ||
    upper.sourceOwner !== "ontology-foundation" ||
    upper.releasePin?.sha256 !== sources.get("releases/palantirkc-ontology-0.2.0.tgz")!.pin.sha256 ||
    pkg.name !== packageName ||
    pkg.exports?.["./context"] !== "./src/context/index.ts" ||
    (version === 6 && !sources.has(workMethodProviderRelativeSources[0])) ||
    (version3 && pkg.exports?.["./git-work"] !== "./src/git-work/index.ts") ||
    (version4 && pkg.exports?.["./memory/recall-request"] !== "./src/workspace/recall-request.ts") ||
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
}
