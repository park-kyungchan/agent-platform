import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import {
  loadImportScanner,
  readArchitectureSnapshot,
  validateArchitecture,
  type ArchitecturePolicy,
  type ArchitectureSnapshot,
  type SourceFile,
} from "./architecture-quality";

function fixture(): { snapshot: ArchitectureSnapshot; policy: ArchitecturePolicy } {
  const root = "/fixture/agent-platform",
    digest = "a".repeat(64);
  const nodes = [
    {
      capabilityId: "common",
      path: "packages/common",
      name: "@fixture/common",
      layer: 1,
      allowedWorkspaceDependencies: [],
    },
    {
      capabilityId: "adapter",
      path: "packages/adapter",
      name: "@fixture/adapter",
      layer: 2,
      allowedWorkspaceDependencies: ["@fixture/common"],
    },
  ];
  const sourceProviders = [
    {
      id: "native-context",
      registration: "../native/architecture/provider.json",
      ownerGuard: "../native/src/guard.ts",
    },
  ];
  return {
    policy: {
      schema: "agent-platform.architecture-quality-policy.v1",
      packages: nodes,
      sourceProviders,
      ownedDefinitionInputs: [],
      stagedSources: [],
    },
    snapshot: {
      sourceRoot: root,
      exportFiles: [],
      transientPaths: [],
      registry: {
        schema: "agent-platform.capability-source-owners.v1",
        codeOwner: "agent-platform",
        sourceProviders: structuredClone(sourceProviders),
        capabilities: nodes.map((node) => ({
          id: node.capabilityId,
          packagePath: node.path,
          packageName: node.name,
          semanticOwner: node.capabilityId + "-semantic-owner",
          nativePermissionGranted: false,
          packageManifest: { path: root + "/" + node.path + "/package.json", sha256: digest },
          sourcePins: [
            {
              path: root + "/" + node.path + "/src/index.ts",
              relativePath: node.path + "/src/index.ts",
              sha256: digest,
            },
          ],
        })),
      },
      packages: nodes.map((node) => ({
        path: node.path,
        manifestSha256: digest,
        manifest: {
          name: node.name,
          exports: { ".": "./src/index.ts" },
          dependencies:
            node.capabilityId === "adapter" ? { "@fixture/common": "workspace:*" } : undefined,
        },
      })),
      files: nodes.map((node) => ({
        path: node.path + "/src/index.ts",
        sha256: digest,
        imports:
          node.capabilityId === "adapter" ? [{ specifier: "@fixture/common", kind: "static" }] : [],
      })),
    },
  };
}
const codes = (value: ReturnType<typeof fixture>): string[] =>
  validateArchitecture(value.snapshot, value.policy).errors.map((error) => error.code);
function stage(value: ReturnType<typeof fixture>, file: SourceFile): void {
  value.snapshot.files.push(file);
  value.policy.stagedSources.push({
    path: file.path,
    sha256: file.sha256,
    ownerRef: "native-owner/current-work",
    classification: "staged-source-with-unreconciled-admission",
    reason: "Existing source slice has not been reconciled with canonical source ownership.",
  });
}

test("valid hierarchical package graph proves only canonical source architecture", () => {
  const value = fixture(),
    report = validateArchitecture(value.snapshot, value.policy);
  expect(report.status).toBe("pass");
  expect(report.counts.dependencyEdges).toBe(1);
  expect(report.environmentAdoptionEstablished).toBe(false);
  expect(report.acceptanceDecision).toBeNull();
});
test("duplicate canonical owner and mismatched actual package identity are rejected", () => {
  const value = fixture();
  value.snapshot.registry.capabilities.push(
    structuredClone(value.snapshot.registry.capabilities[0]!),
  );
  value.snapshot.packages[0]!.manifest.name = "@fixture/counterfeit";
  expect(codes(value)).toContain("duplicate-owner");
  expect(codes(value)).toContain("manifest-identity");
});
test("stale source pins and an additional unclassified source fail independently", () => {
  const value = fixture();
  value.snapshot.files[0]!.sha256 = "b".repeat(64);
  value.snapshot.files.push({
    path: "packages/common/src/surprise.ts",
    sha256: "a".repeat(64),
    imports: [],
  });
  expect(codes(value)).toContain("source-pin-drift");
  expect(codes(value)).toContain("unclassified-source");
});
test("known staged bytes remain visible as currentness debt without granting admission", () => {
  const value = fixture();
  stage(value, {
    path: "packages/adapter/src/work-specific.ts",
    sha256: "b".repeat(64),
    imports: [{ kind: "static", specifier: "/native-owner/work/suite-only-fixture" }],
  });
  const report = validateArchitecture(value.snapshot, value.policy);
  expect(report.status).toBe("pass");
  expect(report.stagedDebt).toHaveLength(1);
  expect(report.stagedDiagnostics.map((issue) => issue.code)).toContain("absolute-source-import");
  expect(report.counts.admittedSourceFiles).toBe(2);
  value.snapshot.files.at(-1)!.sha256 = "c".repeat(64);
  expect(codes(value)).toContain("staged-drift");
});
test("workspace-only staged absence is portable while present drift and new source remain blocked", () => {
  const value = fixture(),
    path = "packages/adapter/src/workspace-only.ts";
  stage(value, {
    path,
    sha256: "b".repeat(64),
    imports: [{ kind: "static", specifier: "/native-owner/current-work.ts" }],
  });
  value.policy.stagedSources[0]!.presence = "workspace-only-if-present";
  value.snapshot.files = value.snapshot.files.filter((file) => file.path !== path);
  const clean = validateArchitecture(value.snapshot, value.policy);
  expect(clean.status).toBe("pass");
  expect(clean.stagedPresence).toEqual([
    { path, present: false, presence: "workspace-only-if-present" },
  ]);
  expect(clean.stagedDebt).toHaveLength(1);
  expect(clean.counts.knownStagedSourceFiles).toBe(1);
  expect(clean.counts.stagedSourceFiles).toBe(0);
  expect(clean.stagedDiagnostics).toHaveLength(0);
  value.snapshot.files.push({
    path,
    sha256: "b".repeat(64),
    imports: [{ kind: "static", specifier: "/native-owner/current-work.ts" }],
  });
  const present = validateArchitecture(value.snapshot, value.policy);
  expect(present.status).toBe("pass");
  expect(present.stagedPresence[0]!.present).toBe(true);
  expect(present.counts.stagedSourceFiles).toBe(1);
  expect(present.stagedDiagnostics[0]!.code).toBe("absolute-source-import");
  value.snapshot.files.at(-1)!.sha256 = "c".repeat(64);
  value.snapshot.files.push({
    path: "packages/adapter/src/unexplained.ts",
    sha256: "d".repeat(64),
    imports: [],
  });
  expect(codes(value)).toContain("staged-drift");
  expect(codes(value)).toContain("unclassified-source");
});
test("staged presence defaults to required and missing staged dependencies cannot be admitted", () => {
  const value = fixture(),
    path = "packages/adapter/src/work-specific.ts";
  stage(value, { path, sha256: "b".repeat(64), imports: [] });
  value.snapshot.files = value.snapshot.files.filter((file) => file.path !== path);
  expect(codes(value)).toContain("staged-drift");
  value.policy.stagedSources[0]!.presence = "workspace-only-if-present";
  value.snapshot.files[1]!.imports.push({ kind: "type", specifier: "./work-specific" });
  expect(codes(value)).toContain("canonical-to-staged");
});
test("canonical imports and exports cannot admit a staged implementation implicitly", () => {
  const value = fixture();
  stage(value, {
    path: "packages/adapter/src/work-specific.ts",
    sha256: "b".repeat(64),
    imports: [],
  });
  value.snapshot.files[1]!.imports.push({ specifier: "./work-specific", kind: "type" });
  value.snapshot.packages[1]!.manifest.exports = { ".": "./src/work-specific.ts" };
  expect(codes(value)).toContain("canonical-to-staged");
  expect(codes(value)).toContain("export-staged-source");
});
test("manifest and export file pins cannot make unadmitted public source canonical", () => {
  const value = fixture(),
    path = "packages/common/unregistered.ts";
  value.snapshot.packages[0]!.manifest.exports = {
    ".": "./src/index.ts",
    "./unregistered": "./unregistered.ts",
  };
  // The manifest still matches its registration, and the public file exists with an exact hash.
  value.snapshot.exportFiles.push({ path, sha256: "b".repeat(64) });
  expect(codes(value)).toContain("export-unadmitted-source");
  // Observing its implementation does not grant source admission either.
  value.snapshot.files.push({ path, sha256: "b".repeat(64), imports: [] });
  expect(codes(value)).toContain("export-unadmitted-source");
  expect(codes(value)).toContain("unclassified-source");
});
test("generated exports require the existing Ontology identity and an admitted source counterpart", () => {
  const value = fixture(),
    rule = value.policy.packages[0]!,
    capability = value.snapshot.registry.capabilities[0]!,
    pkg = value.snapshot.packages[0]!,
    file = value.snapshot.files[0]!;
  value.policy.packages = [rule];
  value.snapshot.registry.capabilities = [capability];
  value.snapshot.packages = [pkg];
  value.snapshot.files = [file];
  rule.capabilityId = capability.id = "ontology-foundation";
  rule.path = capability.packagePath = pkg.path = "packages/ontology";
  rule.name = capability.packageName = pkg.manifest.name = "@palantirkc/ontology-source";
  file.path = capability.sourcePins[0]!.relativePath = "packages/ontology/src/index.ts";
  capability.sourcePins[0]!.path = value.snapshot.sourceRoot + "/" + file.path;
  capability.packageManifest.path = value.snapshot.sourceRoot + "/packages/ontology/package.json";
  pkg.manifest.exports = { ".": { types: "./dist/index.d.ts", import: "./dist/index.js" } };
  value.snapshot.exportFiles = ["index.js", "index.d.ts"].map((name) => ({
    path: "packages/ontology/dist/" + name,
    sha256: "b".repeat(64),
  }));
  expect(codes(value)).toEqual([]);
  pkg.manifest.exports = { ".": "./dist/unregistered.js" };
  value.snapshot.exportFiles.push({
    path: "packages/ontology/dist/unregistered.js",
    sha256: "b".repeat(64),
  });
  expect(codes(value)).toContain("export-unadmitted-source");
  pkg.manifest.exports = { ".": "./dist/index.js" };
  rule.capabilityId = capability.id = "counterfeit-compiler";
  expect(codes(value)).toContain("export-unadmitted-source");
});
test("filesystem discovery classifies package-root and arbitrary-directory executable source", async () => {
  const sourceRoot = process.env.ARCHITECTURE_SOURCE_ROOT ?? resolve(import.meta.dir, "../.."),
    root = await mkdtemp(join(sourceRoot, ".architecture-quality-test-")),
    value = fixture(),
    digest = (bytes: string): string => createHash("sha256").update(bytes).digest("hex");
  try {
    value.snapshot.sourceRoot = root;
    for (const pkg of value.snapshot.packages) {
      const capability = value.snapshot.registry.capabilities.find(
          (item) => item.packagePath === pkg.path,
        )!,
        source = pkg.path.endsWith("adapter") ? 'import "@fixture/common";\n' : "export {};\n",
        manifest = JSON.stringify(pkg.manifest);
      await mkdir(join(root, pkg.path, "src"), { recursive: true });
      await writeFile(join(root, pkg.path, "src/index.ts"), source);
      await writeFile(join(root, pkg.path, "package.json"), manifest);
      capability.packageManifest = {
        path: join(root, pkg.path, "package.json"),
        sha256: digest(manifest),
      };
      capability.sourcePins[0]!.path = join(root, pkg.path, "src/index.ts");
      capability.sourcePins[0]!.sha256 = digest(source);
    }
    await mkdir(join(root, "architecture"));
    await writeFile(
      join(root, "architecture/capability-source-owners.json"),
      JSON.stringify(value.snapshot.registry),
    );
    expect(validateArchitecture(await readArchitectureSnapshot(root), value.policy).status).toBe(
      "pass",
    );
    const unknown = [
        "packages/common/unregistered.ts",
        "packages/common/bootstrap.py",
        "packages/common/custom-runtime/worker.js",
      ],
      excluded = [
        "packages/common/releases/immutable.py",
        "packages/common/dist/generated.js",
        "packages/common/tests/helper.ts",
        "packages/common/work-specific.test.ts",
        "packages/common/tsconfig.json",
      ];
    for (const path of [...unknown, ...excluded]) {
      await mkdir(dirname(join(root, path)), { recursive: true });
      await writeFile(join(root, path), "// MUST_NOT_EXECUTE\n");
    }
    const snapshot = await readArchitectureSnapshot(root),
      report = validateArchitecture(snapshot, value.policy);
    expect(snapshot.files.map((item) => item.path)).toEqual(expect.arrayContaining(unknown));
    for (const path of excluded)
      expect(snapshot.files.some((item) => item.path === path)).toBe(false);
    expect(
      report.errors
        .filter((error) => error.code === "unclassified-source")
        .map((error) => error.path),
    ).toEqual([...unknown].sort());
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
test("lower layers cannot depend on native adapters and cyclic graphs fail", () => {
  const value = fixture();
  value.snapshot.packages[0]!.manifest.dependencies = { "@fixture/adapter": "workspace:*" };
  value.snapshot.files[0]!.imports.push({ kind: "static", specifier: "@fixture/adapter" });
  expect(codes(value)).toContain("dependency-direction");
  expect(codes(value)).toContain("dependency-cycle");
});
test("cross-package relative imports and private subpaths bypassing exports fail", () => {
  const value = fixture();
  value.snapshot.files[1]!.imports = [
    { kind: "type", specifier: "../../common/src/index" },
    { kind: "static", specifier: "@fixture/common/private" },
  ];
  expect(codes(value)).toContain("cross-package-source-import");
  expect(codes(value)).toContain("private-package-import");
});
test("undeclared dependencies and absolute executable imports fail", () => {
  const value = fixture();
  value.snapshot.files[0]!.imports = [
    { kind: "dynamic", specifier: "/personal/native/fixture.ts" },
    { kind: "require", specifier: "undeclared-package" },
  ];
  expect(codes(value)).toContain("absolute-source-import");
  expect(codes(value)).toContain("undeclared-dependency");
});
test("unknown package, missing public target and provider guard substitution fail", () => {
  const value = fixture();
  value.snapshot.packages.push({
    path: "packages/unowned",
    manifestSha256: "a".repeat(64),
    manifest: { name: "@fixture/unowned" },
  });
  value.snapshot.packages[0]!.manifest.exports = { ".": "./src/dead.ts" };
  value.snapshot.registry.sourceProviders[0]!.ownerGuard = "../native/src/no-guard.ts";
  expect(codes(value)).toContain("unregistered-package");
  expect(codes(value)).toContain("export-target");
  expect(codes(value)).toContain("provider-denominator");
});
test("schema/vocabulary input changes require their owning definition successor", () => {
  const value = fixture(),
    path = "packages/common/ontology/vocabulary.json";
  value.snapshot.files.push({ path, sha256: "c".repeat(64), imports: [] });
  value.policy.ownedDefinitionInputs.push({
    path,
    capabilityId: "common",
    sha256: "b".repeat(64),
    reason: "Canonical implementation consumes its own vocabulary definition.",
  });
  expect(codes(value)).toContain("definition-drift");
});
test("parser finds type-only, export, import-type, literal dynamic and require dependencies without executing source", async () => {
  const sourceRoot = process.env.ARCHITECTURE_SOURCE_ROOT ?? resolve(import.meta.dir, "../..");
  const scanner = await loadImportScanner(sourceRoot);
  const source = `import type { T } from 'typed-only'; export * from 'reexport'; type X = import('import-type').X; const x = import('dynamic'); const y = require('required'); throw Error('MUST_NOT_EXECUTE'); // import 'comment-only'`;
  const imports = scanner("fixture.ts", source);
  expect(imports.map((item) => item.specifier).sort()).toEqual([
    "dynamic",
    "import-type",
    "reexport",
    "required",
    "typed-only",
  ]);
});
