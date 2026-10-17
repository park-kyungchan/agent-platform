import { validateReceiptWithEvidence } from "./quality/receipt-validation-gate";
export { validateIndependentReviewReceipt } from "./quality/receipt-validator";
import { createHash } from "node:crypto";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";

const sourceRoot = resolve(import.meta.dir, "..");
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const json = async (path: string) => JSON.parse(await readFile(path, "utf8"));
const writeJson = async (path: string, value: unknown) => {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(value, null, 2) + "\n");
};
const commandEvidence: { command: string[]; durationMs: number; exitCode: number }[] = [];
async function run(cwd: string, argv: string[]): Promise<void> {
  const started = performance.now();
  const child = Bun.spawn(argv, {
    cwd,
    stdout: "inherit",
    stderr: "inherit",
    env: { ...process.env, CI: "true" },
  });
  const exitCode = await child.exited;
  commandEvidence.push({
    command: argv.slice(1),
    durationMs: Math.round(performance.now() - started),
    exitCode,
  });
  if (exitCode !== 0) throw Error("Source check failed: " + argv.join(" ").slice(0, 300));
}
async function files(root: string): Promise<string[]> {
  const result: string[] = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    if (["node_modules", ".git", ".private"].includes(entry.name)) continue;
    const path = join(root, entry.name);
    if (entry.isSymbolicLink())
      throw Error("Source-only CI refuses unregistered source symlink: " + relative(root, path));
    if (entry.isDirectory()) result.push(...(await files(path)));
    else if (entry.isFile()) result.push(path);
  }
  return result.sort();
}
async function sourcePins(root: string, paths: string[]) {
  return Promise.all(
    paths.map(async (path) => ({ path, sha256: sha(await readFile(join(root, path))) })),
  );
}
const contextModules = [
  "index",
  "types",
  "validation",
  "source",
  "store",
  "context",
  "boundary",
  "sections",
];
const nativeCodexModules = ["hooks", "home-context", "evaluation-ledger"];
const hermesModules = [
  "work-context",
  "contracts",
  "current-intent",
  "domain-context",
  "role-registry",
  "audit-adapter",
  "pinned-reader",
];
const gitWorkPaths = [
  ...["index", "source", "records", "context", "types"].map(
    (name) => `packages/work-continuity/src/git-work/${name}.ts`,
  ),
  "packages/work-continuity/src/logical-identity.ts",
  "packages/work-continuity/src/operational-state.ts",
];
const memoryPaths = ["recall-request", "lexical-recall", "qualification"].map(
  (name) => `packages/work-continuity/src/workspace/${name}.ts`,
);
const contextPaths = [
  "project.json",
  "ontology-pin.json",
  "package.json",
  "bun.lock",
  "packages/work-continuity/package.json",
  "releases/palantirkc-ontology-0.2.0.tgz",
  ...contextModules.map((name) => `packages/work-continuity/src/context/${name}.ts`),
  "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
  ...gitWorkPaths,
  ...memoryPaths,
];
const nativeCodexPaths = [
  "package.json",
  "bun.lock",
  "packages/native-codex-adapter/package.json",
  ...nativeCodexModules.map((name) => `packages/native-codex-adapter/src/native/${name}.ts`),
];
const hermesPaths = [
  "package.json",
  "bun.lock",
  "releases/palantirkc-ontology-0.2.0.tgz",
  "packages/native-hermes-adapter/package.json",
  "packages/native-hermes-adapter/releases/palantirkc-ontology-0.2.0.tgz",
  ...hermesModules.map((name) => `packages/native-hermes-adapter/src/ontology-driven/${name}.ts`),
  "packages/native-hermes-adapter/src/adoption/semantic-context.ts",
  "packages/native-hermes-adapter/src/adoption/evaluation-budget.ts",
];

const nativePluginNames = ["source-review-readonly", "ocp-readonly-review", "native-task-controls"];
const nativePluginPaths = nativePluginNames
  .flatMap((name) =>
    ["__init__.py", "plugin.yaml"].map(
      (file) => "packages/native-hermes-adapter/native/plugins/" + name + "/" + file,
    ),
  )
  .sort();
/** Source-only native interface closure; never imports a Hermes plugin or invokes its runtime. */
async function nativePluginSourceCheck(root: string) {
  const registry = await json(join(root, "architecture/capability-source-owners.json"));
  const capability = registry.capabilities.find(
    (entry: any) => entry.id === "native-hermes-context",
  );
  if (
    !capability ||
    capability.packagePath !== "packages/native-hermes-adapter" ||
    JSON.stringify(capability.nativePluginSources?.relativePaths) !==
      JSON.stringify(nativePluginPaths) ||
    capability.nativePluginSources?.nativePermissionGranted !== false
  )
    throw Error("Canonical native plugin source denominator changed");
  for (const path of nativePluginPaths) {
    const pins = capability.sourcePins.filter((pin: any) => pin.relativePath === path);
    const file = join(root, path),
      info = await lstat(file);
    if (
      pins.length !== 1 ||
      (await realpath(file)) !== file ||
      !info.isFile() ||
      info.size > 100_000 ||
      sha(await readFile(file)) !== pins[0].sha256
    )
      throw Error("Canonical native plugin source pin drift: " + path);
    if (path.endsWith("/plugin.yaml")) {
      const manifest = Bun.YAML.parse(await readFile(file, "utf8")) as any;
      if (
        manifest?.name !== path.split("/").at(-2) ||
        typeof manifest.version !== "string" ||
        !manifest.settings ||
        typeof manifest.settings !== "object" ||
        Array.isArray(manifest.settings)
      )
        throw Error("Canonical native plugin manifest identity changed");
    }
  }
  await run(root, [
    "python3",
    "-c",
    "import ast,pathlib,sys; [ast.parse(pathlib.Path(p).read_bytes(), filename=p) for p in sys.argv[1:]]",
    ...nativePluginPaths.filter((path) => path.endsWith(".py")).map((path) => join(root, path)),
  ]);
  console.log(
    JSON.stringify({
      recordClass: "source-only-native-plugin-source-closure",
      sourcePins: 4,
      pythonSyntaxOnly: true,
      nativePluginImports: 0,
      nativeModelCalls: 0,
      nativePermissionGranted: false,
    }),
  );
}

const finiteOwnerSources = [
  ...[
    "src/legacy/harness/owner-context-loader.ts",
    "src/legacy/harness/provider-closure.ts",
    "src/native/provider-closure.ts",
    "src/native/hook-cli.ts",
    ...nativeCodexModules.map((name) => `src/native/${name}.ts`),
  ].map((path) => ({ owner: "codex-harness-engineering", path })),
  { owner: "hermes-harness-engineering", path: "src/ontology-driven/provider-source.ts" },
];
async function liveOwnerFixtureCheck(refresh = false, requireOwners = false) {
  if (refresh && process.env.GITHUB_ACTIONS === "true")
    throw Error("Hosted CI cannot refresh its own owner fixtures");
  const fixtureRoot = join(sourceRoot, "owner-fixture"),
    manifest = await json(join(fixtureRoot, "manifest.json"));
  let available = 0;
  for (const entry of finiteOwnerSources) {
    try {
      await lstat(resolve(sourceRoot, "..", entry.owner, entry.path));
      available++;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
  }
  if (!available && !requireOwners && !refresh) return;
  if (available !== finiteOwnerSources.length)
    throw Error("Exact finite live owner fixture source set unavailable");
  const rows = [];
  for (const entry of finiteOwnerSources) {
    const path = resolve(sourceRoot, "..", entry.owner, entry.path),
      stat = await lstat(path);
    if ((await realpath(path)) !== path || !stat.isFile() || stat.size > 100_000)
      throw Error("Owner source fixture must be exact bounded regular source");
    const bytes = await readFile(path);
    if (bytes.includes("/home/palantirkc"))
      throw Error(
        "Selected owner fixture source contains a personal root; sanitize by a separately reviewed source change",
      );
    const current = manifest.sources.find(
      (candidate: any) => candidate.owner === entry.owner && candidate.path === entry.path,
    );
    if (!refresh && (!current || current.sha256 !== sha(bytes)))
      throw Error(
        "Live owner guard hash drift: " +
          entry.owner +
          "/" +
          entry.path +
          ". Review exact owner changes, then run bun ci/source-check.ts --refresh-owner-fixtures and review fixture/manifest diff before commit.",
      );
    rows.push({
      ...entry,
      bytes,
      sha256: sha(bytes),
      classification: "source-owned-code-only-no-runtime-or-user-data",
    });
  }
  if (refresh) {
    for (const row of rows) await writeFile(join(fixtureRoot, row.owner, row.path), row.bytes);
    await writeJson(join(fixtureRoot, "manifest.json"), {
      ...manifest,
      sources: rows.map(({ bytes, ...entry }) => entry),
    });
  }
  console.log(
    JSON.stringify({
      recordClass: refresh
        ? "refreshed-finite-owner-code-fixture"
        : "live-finite-owner-code-fixture-readback",
      sourceCount: rows.length,
      nativePermissionGranted: false,
      requiresIndependentReview: refresh,
    }),
  );
}
async function prepareQaTree() {
  const qaBase = await mkdtemp(
    join(resolve(process.env.RUNNER_TEMP ?? tmpdir()), "agent-platform-source-"),
  );
  const common = join(qaBase, "agent-platform");
  await cp(sourceRoot, common, {
    recursive: true,
    dereference: false,
    filter: (path) =>
      !relative(sourceRoot, path)
        .split(sep)
        .some((part) => ["node_modules", ".git", ".private"].includes(part)),
  });
  const fixture = await json(join(common, "owner-fixture/manifest.json"));
  if (
    fixture.schema !== "agent-platform.finite-native-owner-code-fixture.v1" ||
    fixture.nativeModelCalls !== 0 ||
    fixture.remoteEffects !== false ||
    !Array.isArray(fixture.sources)
  )
    throw Error("Invalid finite source-only fixture");
  const expected = new Set([
    ...[
      "src/legacy/harness/owner-context-loader.ts",
      "src/legacy/harness/provider-closure.ts",
      "src/native/provider-closure.ts",
      "src/native/hook-cli.ts",
      ...nativeCodexModules.map((name) => `src/native/${name}.ts`),
    ].map((path) => "codex-harness-engineering/" + path),
    "hermes-harness-engineering/src/ontology-driven/provider-source.ts",
  ]);
  for (const entry of fixture.sources) {
    const key = entry.owner + "/" + entry.path;
    if (
      !expected.delete(key) ||
      entry.classification !== "source-owned-code-only-no-runtime-or-user-data"
    )
      throw Error("Unknown/duplicate finite fixture path");
    const path = join(common, "owner-fixture", entry.owner, entry.path);
    if ((await realpath(path)) !== path || !(await lstat(path)).isFile())
      throw Error("Fixture escaped exact source ownership");
    const bytes = await readFile(path);
    if (bytes.length > 100_000 || sha(bytes) !== entry.sha256 || bytes.includes("/home/palantirkc"))
      throw Error("Finite native owner source fixture drifted");
    const destination = join(qaBase, entry.owner, entry.path);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, bytes);
  }
  if (expected.size) throw Error("Finite owner guard fixture incomplete");
  // QA-only identity overlay. No production guard, registration or project bytes are changed.
  const project = await json(join(common, "project.json"));
  project.payload.details.sourceOwnerRoot = common;
  await writeJson(join(common, "project.json"), project);
  await run(common, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
  for (const owner of ["codex-harness-engineering", "hermes-harness-engineering"]) {
    const root = join(qaBase, owner);
    await mkdir(join(root, "releases"), { recursive: true });
    await cp(
      join(common, "releases/palantirkc-ontology-0.2.0.tgz"),
      join(root, "releases/palantirkc-ontology-0.2.0.tgz"),
    );
    const dependencies = {
      "@palantirkc/ontology": "file:releases/palantirkc-ontology-0.2.0.tgz",
      ...(owner.startsWith("codex")
        ? {
            "@palantirkc/work-continuity": "file:../agent-platform/packages/work-continuity",
            "@palantirkc/native-codex-adapter":
              "file:../agent-platform/packages/native-codex-adapter",
          }
        : {
            "@palantirkc/native-hermes-adapter":
              "file:../agent-platform/packages/native-hermes-adapter",
          }),
    };
    await writeJson(join(root, "package.json"), {
      name: owner + "-source-ci-fixture",
      private: true,
      type: "module",
      dependencies,
    });
    // Synthetic owner lock is generated only from finite local package bindings, then frozen.
    await run(root, [process.execPath, "install", "--ignore-scripts"]);
    await run(root, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
  }
  const codex = join(qaBase, "codex-harness-engineering"),
    hermes = join(qaBase, "hermes-harness-engineering");
  const context = {
    schema: "codex.context-provider-source-closure.v4",
    consumerRoot: codex,
    providerRoot: common,
    providerProjectId: "agent-platform",
    packageName: "@palantirkc/work-continuity",
    modulePath: "packages/work-continuity/src/context/index.ts",
    gitWorkModulePath: "packages/work-continuity/src/git-work/index.ts",
    sourcePins: await sourcePins(common, contextPaths),
    authority: "none",
    nativePermissionGranted: false,
  };
  await writeJson(join(codex, "architecture/context-provider.json"), context);
  for (const name of contextModules) {
    const path = join(codex, `src/continuity/${name}.ts`);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(
      path,
      name === "index"
        ? `// codex-context-provider-registration-sha256: ${sha(await readFile(join(codex, "architecture/context-provider.json")))}\nexport * from '@palantirkc/work-continuity/context';\n`
        : `export * from '@palantirkc/work-continuity/context/${name}';\n`,
    );
  }
  const native = {
    schema: "codex.native-adapter-source-closure.v1",
    consumerRoot: codex,
    providerRoot: common,
    packageName: "@palantirkc/native-codex-adapter",
    sourcePins: await sourcePins(common, nativeCodexPaths),
    authority: "none",
    nativePermissionGranted: false,
  };
  await writeJson(join(codex, "architecture/native-codex-provider.json"), native);
  const hooksPath = join(codex, "src/native/hooks.ts");
  await writeFile(
    hooksPath,
    (await readFile(hooksPath, "utf8")).replace(
      /codex-native-provider-registration-sha256: [a-f0-9]{64}/,
      "codex-native-provider-registration-sha256: " +
        sha(await readFile(join(codex, "architecture/native-codex-provider.json"))),
    ),
  );
  const hermesRegistration = {
    schema: "hermes.context-source-provider.v1",
    consumerRoot: hermes,
    providerRoot: common,
    packageName: "@palantirkc/native-hermes-adapter",
    sourcePins: await sourcePins(common, hermesPaths),
    authority: "none",
    nativePermissionGranted: false,
  };
  await writeJson(join(hermes, "architecture/hermes-context-provider.json"), hermesRegistration);
  const hermesGuard = join(hermes, "src/ontology-driven/provider-source.ts");
  await writeFile(
    hermesGuard,
    (await readFile(hermesGuard, "utf8")).replace(
      /const expectedRegistrationSha256 = "[a-f0-9]{64}";/,
      `const expectedRegistrationSha256 = "${sha(await readFile(join(hermes, "architecture/hermes-context-provider.json")))}";`,
    ),
  );
  for (const name of hermesModules)
    await writeFile(
      join(hermes, `src/ontology-driven/${name}.ts`),
      `export * from '@palantirkc/native-hermes-adapter/ontology-driven/${name}';\n`,
    );
  for (const name of ["semantic-context", "evaluation-budget"]) {
    const path = join(hermes, `src/adoption/${name}.ts`);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, `export * from '@palantirkc/native-hermes-adapter/adoption/${name}';\n`);
  }
  return { qaBase, common, codex, hermes };
}
async function sourceChecks() {
  await liveOwnerFixtureCheck();
  await nativePluginSourceCheck(sourceRoot);
  const { qaBase, common, codex, hermes } = await prepareQaTree();
  // Resolve the scope from the installed QA tree; a clean checkout needs no root dependencies.
  const { readQualityScope, readTrackedMarkdown, testFile } = (await import(
    join(common, "ci/quality/scope.ts")
  )) as {
    readTrackedMarkdown(root: string): Promise<string[]>;
    testFile(path: string): boolean;
    readQualityScope(
      root: string,
      policyPath?: string,
      trackedMarkdown?: readonly string[],
    ): Promise<{
      lintFiles: string[];
      testTypescript: string[];
      testFiles: string[];
      exclusions: { stagedSources: string[]; stagedTests: { path: string; dependency: string }[] };
    }>;
  };
  const scope = await readQualityScope(common, undefined, await readTrackedMarkdown(sourceRoot));
  const immutable = {
    "0.1.0": "650385dfebbeb0c78711ac5be0f4762ffd8a19aefc79e06be505c841d4db3add",
    "0.2.0": "3f9995d0b91d2ad39dfc7cedbb6e35b1cb9d29c2e57223ca0c076d2030319406",
  };
  for (const path of await files(common)) {
    const match = /palantirkc-ontology-(0\.[12]\.0)\.tgz$/.exec(path);
    if (match && sha(await readFile(path)) !== immutable[match[1] as keyof typeof immutable])
      throw Error("Immutable Ontology archive changed: " + path);
  }
  for (const name of ["ontology", "evidence-foundry"])
    await run(join(common, "packages", name), [process.execPath, "scripts/generate.ts", "--check"]);
  await run(common, [process.execPath, "test", "./owner-fixture/review-gate.test.ts"]);
  await run(common, [process.execPath, "test", "./owner-fixture/native-hook-bootstrap.test.ts"]);
  const projection = await json(join(common, "architecture/hermes-semantic-projection.json"));
  if (
    sha(await readFile(join(common, projection.canonicalSource.path))) !==
    projection.canonicalSource.sha256
  )
    throw Error("Hermes semantic projection source pin drift");
  await run(common, [process.execPath, "run", "check:handoff"]);
  await run(common, [process.execPath, "run", "check:imports"]);
  const tsFiles = scope.lintFiles.map((path) => join(common, path));
  await run(join(common, "packages/ontology"), [
    process.execPath,
    join(
      dirname(Bun.resolveSync("typescript/package.json", join(common, "packages/ontology"))),
      "bin/tsc",
    ),
    "--strict",
    "--noEmit",
    "--target",
    "esnext",
    "--module",
    "esnext",
    "--moduleResolution",
    "bundler",
    "--allowImportingTsExtensions",
    "--resolveJsonModule",
    "--esModuleInterop",
    "--skipLibCheck",
    "--types",
    "bun-types",
    ...tsFiles,
  ]);
  for (const entry of await readdir(join(common, "packages"), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const packageRoot = join(common, "packages", entry.name);
    const tests = scope.testFiles
      .filter((path) => path.startsWith(`packages/${entry.name}/`))
      .map((path) => join(common, path));
    if (tests.length) await run(packageRoot, [process.execPath, "test", ...tests]);
  }
  const qualityTests = scope.lintFiles.filter((path) => path.startsWith("ci/") && testFile(path));
  if (qualityTests.length)
    await run(common, [
      process.execPath,
      "test",
      ...qualityTests.map((path) => join(common, path)),
    ]);
  // Each guard runs in a fresh process, retaining native first-import/cache checks.
  const closureProbe = join(qaBase, "source-closure-probe.ts");
  await writeFile(
    closureProbe,
    `import {loadCodexOwnerContext} from ${JSON.stringify(join(codex, "src/legacy/harness/owner-context-loader.ts"))};\nimport {readNativeAdapterSourceClosure} from ${JSON.stringify(join(codex, "src/native/provider-closure.ts"))};\nimport {observeHermesContextProvider} from ${JSON.stringify(join(hermes, "src/ontology-driven/provider-source.ts"))};\nconst context=await loadCodexOwnerContext();await context.verifyCurrentSourceClosure();\nconst native=await readNativeAdapterSourceClosure();const hermes=await observeHermesContextProvider(${JSON.stringify(hermes)});await hermes.verify();\nif(native.providerRoot!==${JSON.stringify(common)}||!context.sourcePins.length||!context.gitWork||['resolveGitWorkSelection','openGitWork','readGitWorkContext','compactGitWorkContext','decodeCompactGitWorkContext'].some(name=>typeof context.gitWork[name]!=='function'))throw Error('Source owner closure or fixed Git Work API absent');\nconst home=await import(${JSON.stringify(join(common, "packages/native-codex-adapter/src/native/home-context.ts"))});\nif(['readHomeContextContractValue','gitWorkDeliveryPolicy'].some(name=>typeof home[name]!=='function'))throw Error('Source owner Home or delivery-policy API absent');\nconsole.log(JSON.stringify({recordClass:'source-only-finite-owner-closure',contextPins:context.sourcePins.length,nativePins:native.pins.length,hermesPins:hermes.pins.length,gitWorkApiSourceProven:true,homeValueApiSourceProven:true,sourceDeliveryPolicyApiSourceProven:true,nativePermissionGranted:false}));\n`,
  );
  await run(common, [process.execPath, closureProbe]);
  // Source-only adversarial proof uses QA copies and in-memory registration overlays.
  const providerProbe = join(qaBase, "context-provider-proof-probe.ts");
  await writeFile(
    providerProbe,
    String.raw`import {strict as assert} from 'node:assert';
import {readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {readRegisteredContextProvider} from ${JSON.stringify(join(codex, "src/legacy/harness/provider-closure.ts"))};
const root=${JSON.stringify(codex)},provider=${JSON.stringify(common)};
const registrationPath=join(root,'architecture/context-provider.json'),module=join(root,'src/continuity/index.ts');
const original=JSON.parse(await readFile(registrationPath,'utf8'));
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const gitPaths=${JSON.stringify(gitWorkPaths)};
const memoryPaths=${JSON.stringify(memoryPaths)};
async function proof(change,overlays=new Map(),driftPath){
 const value=structuredClone(original);change(value,overlays);
 for(const pin of value.sourcePins)if(overlays.has(join(provider,pin.path)))pin.sha256=hash(overlays.get(join(provider,pin.path)));
 const bytes=Buffer.from(JSON.stringify(value)+'\n');overlays.set(registrationPath,bytes);
 const counts=new Map();
 const read=async path=>{let data=overlays.get(path)??await readFile(path);const count=(counts.get(path)??0)+1;counts.set(path,count);if(path===driftPath&&count>1)data=Buffer.concat([data,Buffer.from('changed')]);return{bytes:data,pin:{path,sha256:hash(data)}};};
 const index=Buffer.from('// codex-context-provider-registration-sha256: '+hash(bytes)+"\nexport * from '@palantirkc/work-continuity/context';\n");
 return readRegisteredContextProvider(root,module,index,read);
}
const current=await proof(()=>{});
assert.equal(current.gitWorkModulePath,join(provider,'packages/work-continuity/src/git-work/index.ts'));
assert.equal(current.pins.length,26);
const v3=await proof(value=>{value.schema='codex.context-provider-source-closure.v3';value.sourcePins=value.sourcePins.filter(pin=>!memoryPaths.includes(pin.path));});
assert.equal(v3.pins.length,23);
const v2=await proof(value=>{value.schema='codex.context-provider-source-closure.v2';delete value.gitWorkModulePath;value.sourcePins=value.sourcePins.filter(pin=>!gitPaths.includes(pin.path)&&!memoryPaths.includes(pin.path));});
assert.equal(v2.gitWorkModulePath,undefined);assert.equal(v2.pins.length,16);
const v1=await proof((value,overlays)=>{
 value.schema='codex.context-provider-source-closure.v1';delete value.gitWorkModulePath;
 value.sourcePins=value.sourcePins.filter(pin=>!gitPaths.includes(pin.path)&&!memoryPaths.includes(pin.path)&&pin.path!=='packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz');
 const pkgPath=join(provider,'packages/work-continuity/package.json'),rootPath=join(provider,'package.json');
 const pkg=JSON.parse(${JSON.stringify(await readFile(join(common, "packages/work-continuity/package.json"), "utf8"))}),rootPkg=JSON.parse(${JSON.stringify(await readFile(join(common, "package.json"), "utf8"))});
 delete pkg.dependencies['@palantirkc/ontology'];pkg.peerDependencies={'@palantirkc/ontology':'0.2.0'};
 rootPkg.overrides={'@palantirkc/ontology':'file:releases/palantirkc-ontology-0.2.0.tgz'};
 overlays.set(pkgPath,Buffer.from(JSON.stringify(pkg)));overlays.set(rootPath,Buffer.from(JSON.stringify(rootPkg)));
});
assert.equal(v1.gitWorkModulePath,undefined);assert.equal(v1.pins.length,15);
for(const change of [
 value=>{delete value.gitWorkModulePath;},
 value=>{value.gitWorkModulePath='packages/work-continuity/src/context/index.ts';},
 value=>{value.extra=true;},
 value=>{value.sourcePins.pop();},
 value=>{value.sourcePins[value.sourcePins.length-1]=structuredClone(value.sourcePins[0]);},
 value=>{value.sourcePins[value.sourcePins.length-1].sha256='0'.repeat(64);},
])await assert.rejects(()=>proof(change));
await assert.rejects(()=>proof((value,overlays)=>{
 const path=join(provider,'packages/work-continuity/package.json');
 const pkg=JSON.parse(${JSON.stringify(await readFile(join(common, "packages/work-continuity/package.json"), "utf8"))});
 pkg.exports['./git-work']='./src/context/index.ts';overlays.set(path,Buffer.from(JSON.stringify(pkg)));
}),/immutable owner\/package\/Ontology/);
await assert.rejects(()=>proof(()=>{},new Map(),join(provider,'packages/work-continuity/src/git-work/source.ts')),/changed during read/);
console.log(JSON.stringify({recordClass:'source-only-context-provider-v4-adversarial-proof',legacySchemasPreserved:3,rejectedCases:8,nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, providerProbe]);
  const firstImportProbe = join(qaBase, "git-work-first-import-probe.ts");
  await writeFile(
    firstImportProbe,
    `import {strict as assert} from 'node:assert';
await import(${JSON.stringify(join(common, "packages/work-continuity/src/git-work/index.ts"))});
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(codex, "src/legacy/harness/owner-context-loader.ts"))});
await assert.rejects(()=>loadCodexOwnerContext(),/imported before its observed source closure/);
console.log(JSON.stringify({recordClass:'source-only-git-work-first-import-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, firstImportProbe]);
  const memoryFirstImportProbe = join(qaBase, "memory-first-import-probe.ts");
  await writeFile(
    memoryFirstImportProbe,
    String.raw`import {strict as assert} from 'node:assert';
await import(${JSON.stringify(join(common, "packages/work-continuity/src/workspace/recall-request.ts"))});
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(codex, "src/legacy/harness/owner-context-loader.ts"))});
await assert.rejects(()=>loadCodexOwnerContext(),/imported before its observed source closure/);
console.log(JSON.stringify({recordClass:'source-only-memory-first-import-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, memoryFirstImportProbe]);
  const gitDriftProbe = join(qaBase, "git-work-source-drift-probe.ts");
  await writeFile(
    gitDriftProbe,
    String.raw`import {strict as assert} from 'node:assert';
import {readFile,writeFile} from 'node:fs/promises';
import {loadCodexOwnerContext} from ${JSON.stringify(join(codex, "src/legacy/harness/owner-context-loader.ts"))};
const path=${JSON.stringify(join(common, "packages/work-continuity/src/git-work/source.ts"))},bytes=await readFile(path),context=await loadCodexOwnerContext();
try{await writeFile(path,Buffer.concat([bytes,Buffer.from('\n')]));await assert.rejects(()=>context.verifyCurrentSourceClosure(),/Context provider source changed/);}finally{await writeFile(path,bytes);}
await context.verifyCurrentSourceClosure();
console.log(JSON.stringify({recordClass:'source-only-git-work-source-drift-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, gitDriftProbe]);
  const identityProbe = join(qaBase, "source-identity-probe.ts");
  await writeFile(
    identityProbe,
    `import {realpath} from 'node:fs/promises';\nconst cases=${JSON.stringify([
      [
        join(codex, "src/continuity/index.ts"),
        join(common, "packages/work-continuity/src/context/index.ts"),
      ],
      ...nativeCodexModules
        .filter((name) => name !== "hooks")
        .map((name) => [
          join(codex, `src/native/${name}.ts`),
          join(common, `packages/native-codex-adapter/src/native/${name}.ts`),
        ]),
      ...hermesModules.map((name) => [
        join(hermes, `src/ontology-driven/${name}.ts`),
        join(common, `packages/native-hermes-adapter/src/ontology-driven/${name}.ts`),
      ]),
    ])};\nfor(const [oldPath,newPath]of cases){const old=await import(oldPath),current=await import(newPath);if(await realpath(newPath)!==newPath||Object.keys(old).sort().join('|')!==Object.keys(current).sort().join('|')||Object.keys(current).some(key=>old[key]!==current[key]))throw Error('Canonical module identity differs: '+newPath);}\nconsole.log(JSON.stringify({recordClass:'source-only-same-module-identity',modulePairs:cases.length}));\n`,
  );
  await run(common, [process.execPath, identityProbe]);
  await run(common, [process.execPath, join(common, "scripts/verify-common-work.ts"), sourceRoot]);
  console.log(
    JSON.stringify({
      schema: "agent-platform.source-ci.v1",
      status: "pass",
      qaBase,
      sourceOnly: true,
      nativeModelCalls: 0,
      nativePermissionGranted: false,
      sourceRegistrySha256: sha(
        await readFile(join(sourceRoot, "architecture/capability-source-owners.json")),
      ),
      canonicalTestFiles: scope.testFiles.length + 1,
      qualityTestFiles: qualityTests.length,
      stagedSourceFilesExcluded: scope.exclusions.stagedSources.length,
      stagedTestsExcluded: scope.exclusions.stagedTests,
      commands: commandEvidence,
    }),
  );
}

export async function reviewGate() {
  const repository = process.env.GITHUB_REPOSITORY,
    head = process.env.REVIEW_HEAD_SHA,
    token = process.env.GITHUB_TOKEN;
  const expectedCreator = process.env.NATIVE_REVIEW_STATUS_CREATOR_ID,
    expectedLogin = process.env.NATIVE_REVIEW_STATUS_CREATOR_LOGIN;
  if (
    !repository ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository) ||
    !head ||
    !/^[a-f0-9]{40}$/.test(head) ||
    !token ||
    !expectedCreator ||
    !/^[1-9][0-9]*$/.test(expectedCreator)
  )
    throw Error(
      "Exact native review head, repository, authenticated read and expected status creator are required",
    );
  async function api(path: string) {
    const response = await fetch("https://api.github.com/repos/" + repository + "/" + path, {
      headers: {
        Authorization: "Bearer " + token,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
      redirect: "error",
    });
    if (!response.ok) throw Error("Native review API read failed: " + response.status);
    return (await response.json()) as any;
  }
  const statuses = await api(`commits/${head}/statuses?per_page=100`);
  if (!Array.isArray(statuses)) throw Error("Exact-head native review statuses unavailable");
  const status = statuses.find((value: any) => value.context === "native-independent-review");
  if (
    !status ||
    status.state !== "success" ||
    String(status.creator?.id) !== expectedCreator ||
    (expectedLogin && status.creator?.login !== expectedLogin)
  )
    throw Error(
      "Native independent review status missing, unsuccessful or from unexpected creator",
    );
  const target = new URL(status.target_url);
  const prefix = `/${repository}/blob/`,
    suffix = `/native-reviews/${head}.json`;
  if (
    target.protocol !== "https:" ||
    target.hostname !== "github.com" ||
    target.port ||
    target.username ||
    target.password ||
    target.search ||
    target.hash ||
    !target.pathname.startsWith(prefix) ||
    !target.pathname.endsWith(suffix)
  )
    throw Error("Native review URL must be exact immutable same-repository receipt");
  const receiptCommit = target.pathname.slice(prefix.length, -suffix.length);
  if (!/^[a-f0-9]{40}$/.test(receiptCommit) || receiptCommit === head)
    throw Error("Native review receipt requires immutable full commit SHA");
  const content = await api(`contents/native-reviews/${head}.json?ref=${receiptCommit}`);
  if (
    content.type !== "file" ||
    content.encoding !== "base64" ||
    typeof content.content !== "string" ||
    !Number.isSafeInteger(content.size) ||
    content.size <= 0 ||
    content.size > 32768
  )
    throw Error("Native review receipt is not a bounded repository file");
  const receipt = JSON.parse(Buffer.from(content.content, "base64").toString("utf8"));
  const commit = await api("git/commits/" + head);
  if (commit.sha !== head) throw Error("GitHub API returned another commit");
  await validateReceiptWithEvidence(receipt, head, commit.tree?.sha, repository, {
    readTree: async (treeSha) => api(`git/trees/${treeSha}?recursive=1`),
    readFile: async (path, ref) => api(`contents/${path}?ref=${ref}`),
  });
  console.log(
    JSON.stringify({
      schema: "agent-platform.native-review-ci.v1",
      headSha: head,
      receiptCommit,
      receiptSha256: sha(Buffer.from(content.content, "base64")),
      reviewedTreeSha: receipt.reviewedTreeSha,
      status: "pass",
    }),
  );
}
if (import.meta.main) {
  if (process.argv.length === 2) {
    const { continueSourceCi } = await import("./quality/source-runtime");
    if (!(await continueSourceCi("source-check"))) await sourceChecks();
  } else if (process.argv.length === 3 && process.argv[2] === "--review") await reviewGate();
  else if (process.argv.length === 3 && process.argv[2] === "--verify-owner-fixtures")
    await liveOwnerFixtureCheck(false, true);
  else if (process.argv.length === 3 && process.argv[2] === "--refresh-owner-fixtures")
    await liveOwnerFixtureCheck(true, true);
  else
    throw Error(
      "Usage: bun ci/source-check.ts [--review|--verify-owner-fixtures|--refresh-owner-fixtures]",
    );
}
