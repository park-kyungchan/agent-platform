import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import ts from "typescript";
import { permittedSecretSource } from "./secret-check";

export interface QualityScope {
  schema: "agent-platform.quality-scope.v1";
  root: string;
  canonicalTypescript: string[];
  auxiliaryTypescript: string[];
  testFiles: string[];
  testSupportFiles: string[];
  testTypescript: string[];
  lintFiles: string[];
  formatFiles: string[];
  secretFiles: string[];
  workflows: string[];
  actions: string[];
  exclusions: {
    stagedSources: string[];
    generatedSources: string[];
    stagedTests: { path: string; dependency: string; reason: string }[];
  };
}

interface Registry {
  schema: "agent-platform.capability-source-owners.v1";
  capabilities: {
    packagePath: string;
    sourcePins: { relativePath: string; sha256: string }[];
  }[];
}
interface Policy {
  schema: "agent-platform.architecture-quality-policy.v1";
  stagedSources: { path: string; sha256: string }[];
  ownedDefinitionInputs: { path: string; sha256: string }[];
}

const unix = (path: string): string => path.split(sep).join("/");
const sorted = (paths: Iterable<string>): string[] => [...new Set(paths)].sort();
const generated = (path: string): boolean =>
  /(?:^|\/)(?:generated|dist|node_modules)\//.test(path) || path.endsWith(".d.ts");
const sourceFile = (path: string): boolean => /\.[cm]?tsx?$/.test(path);
export const testFile = (path: string): boolean => /\.(?:test|spec)\.[cm]?tsx?$/.test(path);
const testSupport = (path: string): boolean =>
  sourceFile(path) && /(?:^|\/)(?:tests?|__tests__|fixtures)\//.test(path);

export function safeSourcePath(path: string): boolean {
  return (
    path.length > 0 &&
    !isAbsolute(path) &&
    !path.includes("\\") &&
    !path.split("/").some((part) => ["", ".", ".."].includes(part))
  );
}

export async function exactSourceFile(root: string, path: string): Promise<string> {
  if (!safeSourcePath(path)) throw Error("Unsafe quality source path: " + path);
  const absolute = resolve(root, path);
  const info = await lstat(absolute);
  if (!info.isFile() || info.isSymbolicLink() || (await realpath(absolute)) !== absolute)
    throw Error("Quality source must be an exact regular file: " + path);
  if (info.size > 2_000_000) throw Error("Quality source exceeds the bounded file budget: " + path);
  return readFile(absolute, "utf8");
}

async function walk(root: string, directory: string): Promise<string[]> {
  const output: string[] = [];
  async function visit(path: string): Promise<void> {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      if (["node_modules", ".git", "dist", ".private", "generated"].includes(entry.name)) continue;
      const absolute = join(path, entry.name),
        name = unix(relative(root, absolute));
      if (entry.isSymbolicLink()) throw Error("Unregistered quality source symlink: " + name);
      if (entry.isDirectory()) await visit(absolute);
      else if (entry.isFile()) output.push(name);
      if (output.length > 4096)
        throw Error("Quality source denominator exceeds the bounded file budget.");
    }
  }
  try {
    await visit(resolve(root, directory));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  return output.sort();
}

/** Parse import expressions instead of matching text inside comments or fixture strings. */
export function importSpecifiers(path: string, text: string): string[] {
  const file = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  const imports: string[] = [];
  const visit = (node: ts.Node): void => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    )
      imports.push(node.moduleSpecifier.text);
    if (
      ts.isImportTypeNode(node) &&
      ts.isLiteralTypeNode(node.argument) &&
      ts.isStringLiteralLike(node.argument.literal)
    )
      imports.push(node.argument.literal.text);
    if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === "require"))
    ) {
      const argument = node.arguments[0];
      if (argument && ts.isStringLiteralLike(argument)) imports.push(argument.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return sorted(imports);
}

function relativeCandidates(from: string, specifier: string): string[] {
  const path = unix(relative("/", resolve("/", dirname(from), specifier)));
  return [
    path,
    path + ".ts",
    path + ".tsx",
    path + ".json",
    path + "/index.ts",
    ...(path.endsWith(".js") ? [path.slice(0, -3) + ".ts"] : []),
  ];
}

/** A test importing staged implementation is staged transitively, even if its own name looks canonical. */
export function classifyTests(
  tests: string[],
  admitted: Set<string>,
  staged: Set<string>,
  imports: ReadonlyMap<string, readonly string[]>,
): {
  admitted: string[];
  support: string[];
  staged: { path: string; dependency: string; reason: string }[];
} {
  const known = new Set([...admitted, ...staged, ...imports.keys()]);
  const stagedOnlyRoots = sorted([...staged].map(dirname)).filter(
    (root) => ![...admitted].some((path) => sourceFile(path) && path.startsWith(root + "/")),
  );
  const blocker = (
    path: string,
    seen: Set<string>,
  ): { dependency: string; reason: string } | undefined => {
    if (staged.has(path))
      return {
        dependency: path,
        reason: "Imports staged implementation directly or through a maintained test helper.",
      };
    const stagedRoot = stagedOnlyRoots.find((root) => path.startsWith(root + "/"));
    if (stagedRoot)
      return {
        dependency: [...staged].find((source) => source.startsWith(stagedRoot + "/"))!,
        reason:
          "Test or test dependency belongs to a source subtree with staged implementation and no admitted canonical source.",
      };
    if (seen.has(path)) return undefined;
    seen.add(path);
    for (const specifier of imports.get(path) ?? []) {
      if (!specifier.startsWith(".")) continue;
      const target = relativeCandidates(path, specifier).find((candidate) => known.has(candidate));
      if (!target) continue;
      if (
        sourceFile(target) &&
        !admitted.has(target) &&
        !staged.has(target) &&
        !testFile(target) &&
        !testSupport(target) &&
        !generated(target)
      )
        return {
          dependency: target,
          reason:
            "Imports an unadmitted TypeScript implementation outside the test-support namespace.",
        };
      const excluded = blocker(target, seen);
      if (excluded) return excluded;
    }
    return undefined;
  };
  const accepted: string[] = [],
    support = new Set<string>(),
    excluded: { path: string; dependency: string; reason: string }[] = [];
  for (const path of sorted(tests)) {
    const visited = new Set<string>(),
      dependency = blocker(path, visited);
    if (dependency) excluded.push({ path, ...dependency });
    else {
      accepted.push(path);
      for (const target of visited)
        if (testSupport(target) && !testFile(target) && !generated(target)) support.add(target);
    }
  }
  return { admitted: accepted, support: sorted(support), staged: excluded };
}

export async function readQualityScope(
  rootPath: string,
  policyPath = "architecture/quality-policy.json",
): Promise<QualityScope> {
  const root = resolve(rootPath);
  if ((await realpath(root)) !== root)
    throw Error("Quality root must have an exact canonical path.");
  const registry = JSON.parse(
    await exactSourceFile(root, "architecture/capability-source-owners.json"),
  ) as Registry;
  const policy = JSON.parse(await exactSourceFile(root, policyPath)) as Policy;
  if (
    registry.schema !== "agent-platform.capability-source-owners.v1" ||
    !Array.isArray(registry.capabilities) ||
    policy.schema !== "agent-platform.architecture-quality-policy.v1" ||
    !Array.isArray(policy.stagedSources)
  )
    throw Error("Unknown quality ownership contracts.");
  const admitted = new Set<string>(),
    staged = new Set<string>();
  for (const capability of registry.capabilities) {
    if (!safeSourcePath(capability.packagePath) || !Array.isArray(capability.sourcePins))
      throw Error("Invalid canonical capability scope.");
    for (const pin of capability.sourcePins) {
      if (
        !safeSourcePath(pin.relativePath) ||
        !pin.relativePath.startsWith(capability.packagePath + "/") ||
        !/^[a-f0-9]{64}$/.test(pin.sha256)
      )
        throw Error("Invalid canonical source pin.");
      admitted.add(pin.relativePath);
    }
  }
  for (const pin of policy.stagedSources) {
    if (!safeSourcePath(pin.path) || !/^[a-f0-9]{64}$/.test(pin.sha256) || admitted.has(pin.path))
      throw Error("Invalid or overlapping staged source classification.");
    staged.add(pin.path);
  }
  const packageFiles = await walk(root, "packages");
  const imports = new Map<string, string[]>();
  for (const path of packageFiles.filter((path) => sourceFile(path) && !generated(path)))
    imports.set(path, importSpecifiers(path, await exactSourceFile(root, path)));
  const canonicalTypescript = sorted(
    [...admitted].filter((path) => sourceFile(path) && !generated(path)),
  );
  if (!canonicalTypescript.length) throw Error("Canonical TypeScript quality scope is empty.");
  const classified = classifyTests(packageFiles.filter(testFile), admitted, staged, imports);
  const auxiliaryTypescript = sorted(
    (await Promise.all(["ci", "verification", "scripts"].map((path) => walk(root, path))))
      .flat()
      .filter((path) => sourceFile(path) && !generated(path)),
  );
  const compatibilityTests = (
    await walk(root, "compatibility/codex-harness-engineering/tests")
  ).filter(testFile);
  const testFiles = sorted([...classified.admitted, ...compatibilityTests]),
    testSupportFiles = classified.support;
  const testTypescript = sorted([...testFiles, ...testSupportFiles]);
  const lintFiles = sorted([...canonicalTypescript, ...auxiliaryTypescript, ...testTypescript]);
  const githubFiles = await walk(root, ".github");
  const workflows = githubFiles.filter((path) => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(path));
  const actions = githubFiles.filter((path) => /(?:^|\/)action\.ya?ml$/.test(path));
  if (!workflows.length) throw Error("Workflow quality scope is empty.");
  const configuration = [
    ".prettierrc.json",
    "package.json",
    "architecture/quality-policy.json",
    ...packageFiles.filter((path) =>
      /^packages\/[^/]+\/(?:package|tsconfig(?:\.[a-z-]+)?)\.json$/.test(path),
    ),
    ...(await walk(root, "ci/quality")).filter(
      (path) => path.endsWith(".json") && !path.endsWith("any-baseline.json"),
    ),
  ];
  for (const path of [...lintFiles, ...workflows, ...actions, ...configuration])
    await exactSourceFile(root, path);
  const formatFiles = sorted([
    ...lintFiles,
    ...configuration,
    ...githubFiles.filter((path) => /\.ya?ml$/.test(path)),
  ]);
  const secretFiles = sorted([
    ...formatFiles,
    ...[...admitted].filter((path) => !generated(path)),
    ...policy.ownedDefinitionInputs.map((pin) => pin.path),
    "architecture/capability-source-owners.json",
    "ci/quality/any-baseline.json",
  ]);
  if (secretFiles.some((path) => !permittedSecretSource(path)))
    throw Error("A protected or unsupported source entered the secret-scan denominator.");
  for (const path of secretFiles) await exactSourceFile(root, path);
  if (
    formatFiles.some(
      (path) => staged.has(path) || generated(path) || path.startsWith("owner-fixture/"),
    )
  )
    throw Error("A protected file entered the formatting scope.");
  return {
    schema: "agent-platform.quality-scope.v1",
    root,
    canonicalTypescript,
    auxiliaryTypescript,
    testFiles,
    testSupportFiles,
    testTypescript,
    lintFiles,
    formatFiles,
    secretFiles,
    workflows,
    actions,
    exclusions: {
      stagedSources: sorted(staged),
      generatedSources: sorted([...admitted].filter(generated)),
      stagedTests: classified.staged,
    },
  };
}
