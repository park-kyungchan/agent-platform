import { createHash } from "node:crypto";
import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { builtinModules } from "node:module";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

export interface Pin {
  path: string;
  sha256: string;
}
export interface Capability {
  id: string;
  packagePath: string;
  packageName: string;
  semanticOwner: string;
  nativePermissionGranted: false;
  packageManifest: Pin;
  sourcePins: { path: string; relativePath: string; sha256: string }[];
}
export interface SourceProvider {
  id: string;
  registration: string;
  ownerGuard: string;
}
export interface OwnerRegistry {
  schema: "agent-platform.capability-source-owners.v1";
  codeOwner: "agent-platform";
  capabilities: Capability[];
  sourceProviders: SourceProvider[];
}
export interface PackageRule {
  capabilityId: string;
  path: string;
  name: string;
  layer: number;
  allowedWorkspaceDependencies: string[];
}
export interface StagedSource extends Pin {
  ownerRef: string;
  classification: "staged-source-with-unreconciled-admission";
  presence?: "required" | "workspace-only-if-present";
  reason: string;
}
export interface ArchitecturePolicy {
  schema: "agent-platform.architecture-quality-policy.v1";
  packages: PackageRule[];
  sourceProviders: SourceProvider[];
  ownedDefinitionInputs: (Pin & { capabilityId: string; reason: string })[];
  stagedSources: StagedSource[];
}
export interface PackageManifest {
  name: string;
  exports?: unknown;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}
export interface ModuleImport {
  specifier: string;
  kind: "static" | "type" | "dynamic" | "require";
}
export interface SourceFile extends Pin {
  imports: ModuleImport[];
}
export interface PackageSnapshot {
  path: string;
  manifest: PackageManifest;
  manifestSha256: string;
}
export interface ArchitectureSnapshot {
  sourceRoot: string;
  registry: OwnerRegistry;
  packages: PackageSnapshot[];
  files: SourceFile[];
  exportFiles: Pin[];
  transientPaths: string[];
}
export interface ArchitectureIssue {
  code: string;
  path: string;
  detail: string;
}
export interface ArchitectureReport {
  schema: "agent-platform.architecture-quality-report.v1";
  status: "pass" | "fail";
  verificationClass: "canonical-source-architecture-only";
  canonicalSourceScopeClean: boolean;
  errors: ArchitectureIssue[];
  stagedDebt: StagedSource[];
  stagedPresence: {
    path: string;
    present: boolean;
    presence: "required" | "workspace-only-if-present";
  }[];
  stagedDiagnostics: ArchitectureIssue[];
  transientPaths: string[];
  counts: {
    packages: number;
    admittedSourceFiles: number;
    stagedSourceFiles: number;
    knownStagedSourceFiles: number;
    dependencyEdges: number;
  };
  environmentAdoptionEstablished: false;
  nativePermissionGranted: false;
  acceptanceDecision: null;
}

const hash = (bytes: Uint8Array | string): string =>
  createHash("sha256").update(bytes).digest("hex");
const safeRelative = (path: string): boolean =>
  path.length > 0 &&
  !isAbsolute(path) &&
  !path.includes("\\") &&
  !path.split("/").some((part) => part === ".." || part === "." || part === "");
const inside = (base: string, path: string): boolean =>
  path === base || path.startsWith(base + "/");
const packageName = (specifier: string): string =>
  specifier.startsWith("@") ? specifier.split("/").slice(0, 2).join("/") : specifier.split("/")[0]!;
const builtins = new Set(builtinModules.flatMap((name) => [name, "node:" + name]));
const governedFile = (path: string): boolean =>
  /\.(?:[cm]?[jt]sx?|json|py|sh|bash|ya?ml)$/.test(path) &&
  !/(?:^|\/)(?:tests?|__tests__)\//.test(path) &&
  !/\.(?:test|spec)\.[cm]?[jt]sx?$/.test(path);
const packageMetadata = new Set(["package.json", "tsconfig.json", "tsconfig.build.json"]);

/** Only the existing Ontology compiler projection may export generated executable files. */
function generatedExportSource(rule: PackageRule, target: string): string | undefined {
  if (
    rule.capabilityId !== "ontology-foundation" ||
    rule.path !== "packages/ontology" ||
    rule.name !== "@palantirkc/ontology-source"
  )
    return undefined;
  const match = /^\.\/dist\/(.+)\.(?:d\.ts|js)$/.exec(target);
  return match && safeRelative(match[1]!) ? rule.path + "/src/" + match[1] + ".ts" : undefined;
}

function exportTargets(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(exportTargets);
  if (value !== null && typeof value === "object")
    return Object.values(value).flatMap(exportTargets);
  return [];
}

/** Pure validation is shared by the filesystem CLI and adversarial in-memory fixtures. */
export function validateArchitecture(
  snapshot: ArchitectureSnapshot,
  policy: ArchitecturePolicy,
): ArchitectureReport {
  const errors: ArchitectureIssue[] = [],
    stagedDiagnostics: ArchitectureIssue[] = [];
  const fail = (code: string, path: string, detail: string): void => {
    errors.push({ code, path, detail });
  };
  const files = new Map(snapshot.files.map((file) => [file.path, file]));
  if (files.size !== snapshot.files.length)
    fail("duplicate-file", "packages", "One source identity must have one observed file record.");
  const rules = new Map(policy.packages.map((rule) => [rule.path, rule]));
  const names = new Map(policy.packages.map((rule) => [rule.name, rule]));
  if (
    policy.schema !== "agent-platform.architecture-quality-policy.v1" ||
    rules.size !== policy.packages.length ||
    names.size !== policy.packages.length
  )
    fail(
      "invalid-policy",
      "architecture",
      "Package paths and names must each be unique in the typed policy.",
    );
  if (
    snapshot.registry.schema !== "agent-platform.capability-source-owners.v1" ||
    snapshot.registry.codeOwner !== "agent-platform"
  )
    fail(
      "invalid-registry",
      "architecture/capability-source-owners.json",
      "The canonical code owner or registry schema differs.",
    );
  const capabilities = new Map(
    snapshot.registry.capabilities.map((capability) => [capability.packagePath, capability]),
  );
  const ids = new Set(snapshot.registry.capabilities.map((capability) => capability.id));
  if (
    capabilities.size !== snapshot.registry.capabilities.length ||
    ids.size !== snapshot.registry.capabilities.length
  )
    fail(
      "duplicate-owner",
      "architecture/capability-source-owners.json",
      "Capability identity and package ownership must each be unique.",
    );
  if (
    snapshot.registry.sourceProviders.length !== policy.sourceProviders.length ||
    policy.sourceProviders.some(
      (expected) =>
        !snapshot.registry.sourceProviders.some(
          (actual) =>
            actual.id === expected.id &&
            actual.registration === expected.registration &&
            actual.ownerGuard === expected.ownerGuard,
        ),
    )
  ) {
    fail(
      "provider-denominator",
      "architecture/capability-source-owners.json",
      "Exactly the three current native source providers and owner guard paths must remain registered.",
    );
  }
  const admitted = new Map<string, string>(),
    staged = new Map<string, StagedSource>();
  const graph = new Map(policy.packages.map((rule) => [rule.name, new Set<string>()]));
  const packageByPath = new Map(snapshot.packages.map((pkg) => [pkg.path, pkg]));
  if (packageByPath.size !== snapshot.packages.length)
    fail("duplicate-package", "packages", "One package directory must have one manifest.");
  const addEdge = (from: PackageRule, to: PackageRule, path: string): void => {
    graph.get(from.name)!.add(to.name);
    if (!from.allowedWorkspaceDependencies.includes(to.name) || to.layer >= from.layer)
      fail(
        "dependency-direction",
        path,
        `${from.name} -> ${to.name} is outside the accepted dependency direction.`,
      );
  };
  for (const rule of policy.packages) {
    const capability = capabilities.get(rule.path),
      pkg = packageByPath.get(rule.path);
    if (
      !pkg ||
      !capability ||
      capability.id !== rule.capabilityId ||
      capability.packageName !== rule.name ||
      !capability.semanticOwner ||
      capability.nativePermissionGranted !== false
    ) {
      fail(
        "package-owner",
        rule.path,
        "The package, semantic owner and canonical capability must have one matching identity.",
      );
      continue;
    }
    if (
      pkg.manifest.name !== rule.name ||
      pkg.manifestSha256 !== capability.packageManifest.sha256 ||
      !capability.packageManifest.path.endsWith("/" + rule.path + "/package.json")
    )
      fail(
        "manifest-identity",
        rule.path + "/package.json",
        "Actual manifest name/path/hash must match the canonical source owner registration.",
      );
    for (const pin of capability.sourcePins) {
      if (
        !safeRelative(pin.relativePath) ||
        !inside(rule.path, pin.relativePath) ||
        !pin.path.endsWith("/" + pin.relativePath)
      ) {
        fail(
          "source-path",
          pin.relativePath,
          "An owned source pin must stay inside its exact package.",
        );
        continue;
      }
      if (admitted.has(pin.relativePath))
        fail(
          "duplicate-source-owner",
          pin.relativePath,
          "An admitted source must have exactly one canonical owner.",
        );
      admitted.set(pin.relativePath, rule.capabilityId);
      if (files.get(pin.relativePath)?.sha256 !== pin.sha256)
        fail("source-pin-drift", pin.relativePath, "The admitted source pin is missing or stale.");
    }
    const targets = exportTargets(pkg.manifest.exports);
    if (!targets.length)
      fail(
        "missing-exports",
        rule.path + "/package.json",
        "An admitted implementation package must declare its public interface.",
      );
    for (const target of targets) {
      const path = rule.path + "/" + target.slice(2);
      if (
        !target.startsWith("./") ||
        !safeRelative(target.slice(2)) ||
        target.includes("*") ||
        (!files.has(path) &&
          !snapshot.exportFiles.some((file) => file.path === path) &&
          target !== "./package.json")
      )
        fail(
          "export-target",
          rule.path + "/package.json",
          "Public exports must resolve to existing explicit package-owned files: " + target,
        );
    }
    for (const dependency of Object.keys(pkg.manifest.dependencies ?? {})) {
      const target = names.get(dependency);
      if (target) addEdge(rule, target, rule.path + "/package.json");
    }
  }
  for (const path of packageByPath.keys())
    if (!rules.has(path))
      fail(
        "unregistered-package",
        path,
        "A new package requires an explicit capability owner and dependency rule.",
      );
  for (const path of capabilities.keys())
    if (!rules.has(path))
      fail(
        "unclassified-capability",
        path,
        "A new capability requires a dependency policy successor.",
      );
  for (const input of policy.ownedDefinitionInputs) {
    if (
      admitted.has(input.path) ||
      !safeRelative(input.path) ||
      !policy.packages.some(
        (rule) => rule.capabilityId === input.capabilityId && inside(rule.path, input.path),
      )
    )
      fail(
        "definition-owner",
        input.path,
        "An additional definition input must have one existing package owner.",
      );
    admitted.set(input.path, input.capabilityId);
    if (files.get(input.path)?.sha256 !== input.sha256)
      fail(
        "definition-drift",
        input.path,
        "An exact owned definition input is missing or changed.",
      );
  }
  for (const debt of policy.stagedSources) {
    if (
      !safeRelative(debt.path) ||
      staged.has(debt.path) ||
      admitted.has(debt.path) ||
      !debt.ownerRef ||
      !debt.reason ||
      debt.classification !== "staged-source-with-unreconciled-admission" ||
      (debt.presence !== undefined &&
        !["required", "workspace-only-if-present"].includes(debt.presence))
    )
      fail(
        "staged-classification",
        debt.path,
        "Staged debt must be exact, owned, explained and distinct from canonical admission.",
      );
    staged.set(debt.path, debt);
    const observed = files.get(debt.path);
    if (
      (!observed && debt.presence !== "workspace-only-if-present") ||
      (observed && observed.sha256 !== debt.sha256)
    )
      fail(
        "staged-drift",
        debt.path,
        "Present staged bytes must retain their exact hash; required staged sources must remain present. Reconcile a successor with its owner.",
      );
  }
  for (const pkg of snapshot.packages)
    for (const target of exportTargets(pkg.manifest.exports)) {
      const path = pkg.path + "/" + target.slice(2);
      if (staged.has(path))
        fail(
          "export-staged-source",
          path,
          "A staged source cannot become public before its canonical owner admits it.",
        );
      else if (target !== "./package.json" && !admitted.has(path)) {
        const rule = rules.get(pkg.path),
          generatedSource = rule && generatedExportSource(rule, target);
        if (!generatedSource || !admitted.has(generatedSource))
          fail(
            "export-unadmitted-source",
            path,
            "Public files must be admitted source, or the existing Ontology dist projection of an admitted TypeScript source.",
          );
      }
    }
  for (const file of snapshot.files) {
    if (!admitted.has(file.path) && !staged.has(file.path))
      fail(
        "unclassified-source",
        file.path,
        "New source must be admitted by its canonical owner or explicitly classified as staged debt.",
      );
    const from = policy.packages.find((rule) => inside(rule.path, file.path));
    if (!from) continue;
    const pkg = packageByPath.get(from.path)!;
    const issue = (code: string, detail: string): void => {
      const row = { code, path: file.path, detail };
      if (staged.has(file.path)) stagedDiagnostics.push(row);
      else errors.push(row);
    };
    for (const imported of file.imports) {
      const specifier = imported.specifier;
      if (builtins.has(specifier) || specifier === "bun" || specifier.startsWith("bun:")) continue;
      if (isAbsolute(specifier) || /^(?:file|https?):/.test(specifier)) {
        issue(
          "absolute-source-import",
          "Executable imports must use portable package or package-local paths: " + specifier,
        );
        continue;
      }
      if (specifier.startsWith(".")) {
        const path = relative(
          snapshot.sourceRoot,
          resolve(snapshot.sourceRoot, dirname(file.path), specifier),
        )
          .split(sep)
          .join("/");
        const candidates = [
          path,
          path + ".ts",
          path + ".tsx",
          path + ".json",
          path + "/index.ts",
          ...(path.endsWith(".js") ? [path.slice(0, -3) + ".ts"] : []),
        ];
        const target = candidates.find((candidate) => files.has(candidate));
        const targetPackage = policy.packages.find((rule) => inside(rule.path, path));
        if (!targetPackage || targetPackage.path !== from.path) {
          issue(
            "cross-package-source-import",
            "Cross-package relative source imports bypass the declared public interface: " +
              specifier,
          );
          if (targetPackage && !staged.has(file.path)) addEdge(from, targetPackage, file.path);
        }
        if (!target && path !== from.path + "/package.json")
          issue(
            "unresolved-local-import",
            "Package-local source import is outside the observed denominator: " + specifier,
          );
        const stagedTarget = candidates.find((candidate) => staged.has(candidate));
        if (stagedTarget && admitted.has(file.path))
          issue(
            "canonical-to-staged",
            "Admitted source cannot depend on staged unregistered source: " + stagedTarget,
          );
        continue;
      }
      const name = packageName(specifier),
        targetRule = names.get(name);
      const dependencies = {
        ...pkg.manifest.dependencies,
        ...(file.path.includes("/scripts/") ? pkg.manifest.devDependencies : {}),
      };
      if (name !== from.name && !Object.hasOwn(dependencies, name))
        issue(
          "undeclared-dependency",
          "Imported dependency is absent from the actual package manifest: " + name,
        );
      if (targetRule) {
        if (!staged.has(file.path) && name !== from.name) addEdge(from, targetRule, file.path);
        const targetPackage = packageByPath.get(targetRule.path)!;
        const subpath = specifier === name ? "." : "." + specifier.slice(name.length);
        const exports = targetPackage.manifest.exports;
        const declared =
          exports && typeof exports === "object" && !Array.isArray(exports)
            ? (exports as Record<string, unknown>)[subpath]
            : subpath === "."
              ? exports
              : undefined;
        if (declared === undefined)
          issue(
            "private-package-import",
            "The imported package subpath is not publicly exported: " + specifier,
          );
        if (
          admitted.has(file.path) &&
          exportTargets(declared).some((target) =>
            staged.has(targetRule.path + "/" + target.slice(2)),
          )
        )
          issue("canonical-to-staged", "A declared export resolves to staged source: " + specifier);
      }
    }
  }
  const state = new Map<string, number>();
  const visit = (name: string, stack: string[]): void => {
    if (state.get(name) === 1) {
      fail("dependency-cycle", "packages", [...stack, name].join(" -> "));
      return;
    }
    if (state.get(name) === 2) return;
    state.set(name, 1);
    for (const next of graph.get(name) ?? []) visit(next, [...stack, name]);
    state.set(name, 2);
  };
  for (const name of graph.keys()) visit(name, []);
  const stagedPresence = [...staged.values()].map((debt) => ({
    path: debt.path,
    present: files.has(debt.path),
    presence: debt.presence ?? ("required" as const),
  }));
  return {
    schema: "agent-platform.architecture-quality-report.v1",
    status: errors.length ? "fail" : "pass",
    verificationClass: "canonical-source-architecture-only",
    canonicalSourceScopeClean: errors.length === 0,
    errors,
    stagedDebt: [...staged.values()],
    stagedPresence,
    stagedDiagnostics,
    transientPaths: snapshot.transientPaths,
    counts: {
      packages: snapshot.packages.length,
      admittedSourceFiles: admitted.size,
      stagedSourceFiles: stagedPresence.filter((source) => source.present).length,
      knownStagedSourceFiles: staged.size,
      dependencyEdges: [...graph.values()].reduce((total, edges) => total + edges.size, 0),
    },
    environmentAdoptionEstablished: false,
    nativePermissionGranted: false,
    acceptanceDecision: null,
  };
}

// The canonical package already pins TypeScript. Loading the parser does not import implementation sources.
interface SyntaxNode {
  kind: number;
  text?: string;
  moduleSpecifier?: SyntaxNode;
  argument?: SyntaxNode;
  literal?: SyntaxNode;
  expression?: SyntaxNode;
  arguments?: readonly SyntaxNode[];
}
interface TypeScriptParser {
  ScriptTarget: { Latest: number };
  SyntaxKind: { ImportKeyword: number };
  createSourceFile(path: string, source: string, target: number, setParents: boolean): SyntaxNode;
  forEachChild(node: SyntaxNode, callback: (child: SyntaxNode) => void): void;
  isImportDeclaration(node: SyntaxNode): boolean;
  isExportDeclaration(node: SyntaxNode): boolean;
  isImportTypeNode(node: SyntaxNode): boolean;
  isCallExpression(node: SyntaxNode): boolean;
  isStringLiteralLike(node: SyntaxNode): boolean;
  isIdentifier(node: SyntaxNode): boolean;
}
export async function loadImportScanner(
  sourceRoot: string,
): Promise<(path: string, source: string) => ModuleImport[]> {
  const compilerPath = Bun.resolveSync("typescript", join(sourceRoot, "packages/ontology"));
  const ts = (await import(compilerPath)) as unknown as TypeScriptParser;
  return (path, source) => {
    const result: ModuleImport[] = [];
    const collect = (node: SyntaxNode | undefined, kind: ModuleImport["kind"]): void => {
      if (node && ts.isStringLiteralLike(node) && typeof node.text === "string")
        result.push({ specifier: node.text, kind });
    };
    const visit = (node: SyntaxNode): void => {
      if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node))
        collect(node.moduleSpecifier, "static");
      if (ts.isImportTypeNode(node)) collect(node.argument?.literal, "type");
      if (ts.isCallExpression(node) && node.expression?.kind === ts.SyntaxKind.ImportKeyword)
        collect(node.arguments?.[0], "dynamic");
      if (
        ts.isCallExpression(node) &&
        node.expression &&
        ts.isIdentifier(node.expression) &&
        node.expression.text === "require"
      )
        collect(node.arguments?.[0], "require");
      ts.forEachChild(node, visit);
    };
    visit(ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true));
    return result;
  };
}
export async function readArchitectureSnapshot(sourceRoot: string): Promise<ArchitectureSnapshot> {
  const root = resolve(sourceRoot),
    scanImports = await loadImportScanner(root);
  if ((await realpath(root)) !== root)
    throw Error("Architecture source root must be its exact canonical path.");
  const exactFile = async (path: string): Promise<Buffer> => {
    const stat = await lstat(path);
    if (!stat.isFile() || stat.size > 2_097_152 || (await realpath(path)) !== path)
      throw Error("Architecture source must be an exact bounded regular file: " + path);
    return readFile(path);
  };
  const files: SourceFile[] = [],
    exportFiles: Pin[] = [],
    transientPaths: string[] = [],
    packages: PackageSnapshot[] = [];
  const walk = async (directory: string, packagePath: string): Promise<void> => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name),
        ownedPath = relative(root, path).split(sep).join("/");
      if (
        ["node_modules", ".git", "dist", "releases", "tests", "test", "__tests__"].includes(
          entry.name,
        )
      )
        continue;
      if (entry.isSymbolicLink()) throw Error("Unregistered source symlink: " + ownedPath);
      if (entry.isDirectory()) await walk(path, packagePath);
      else if (
        entry.isFile() &&
        governedFile(ownedPath) &&
        !(dirname(ownedPath) === packagePath && packageMetadata.has(entry.name))
      ) {
        const bytes = await exactFile(path);
        files.push({
          path: ownedPath,
          sha256: hash(bytes),
          imports: /\.[cm]?[jt]sx?$/.test(ownedPath)
            ? scanImports(ownedPath, bytes.toString())
            : [],
        });
      } else if (entry.isFile() && entry.name.endsWith(".tmp")) transientPaths.push(ownedPath);
      if (files.length > 4096)
        throw Error("Architecture source denominator exceeds its bounded package budget.");
    }
  };
  for (const entry of await readdir(join(root, "packages"), { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.isSymbolicLink())
      throw Error("A workspace package must be an exact regular directory.");
    const path = "packages/" + entry.name,
      bytes = await exactFile(join(root, path, "package.json"));
    const manifest = JSON.parse(bytes.toString()) as PackageManifest;
    packages.push({ path, manifest, manifestSha256: hash(bytes) });
    for (const target of new Set(exportTargets(manifest.exports))) {
      if (!target.startsWith("./") || !safeRelative(target.slice(2)) || target.includes("*"))
        continue;
      const file = path + "/" + target.slice(2);
      try {
        exportFiles.push({ path: file, sha256: hash(await exactFile(join(root, file))) });
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
    }
    await walk(join(root, path), path);
  }
  return {
    sourceRoot: root,
    registry: JSON.parse(
      (await exactFile(join(root, "architecture/capability-source-owners.json"))).toString(),
    ) as OwnerRegistry,
    packages: packages.sort((a, b) => a.path.localeCompare(b.path)),
    files: files.sort((a, b) => a.path.localeCompare(b.path)),
    exportFiles,
    transientPaths: transientPaths.sort(),
  };
}
if (import.meta.main) {
  const root = resolve(process.argv[2] ?? join(import.meta.dir, "../..")),
    policyPath = resolve(process.argv[3] ?? join(root, "architecture/quality-policy.json"));
  const policy = JSON.parse(await readFile(policyPath, "utf8")) as ArchitecturePolicy;
  const report = validateArchitecture(await readArchitectureSnapshot(root), policy);
  console.log(JSON.stringify(report, null, 2));
  if (report.status !== "pass") process.exitCode = 1;
}
