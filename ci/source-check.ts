import { runNativeCommand, sourceTestFailureDiagnostics } from "./quality/native-tools";
import { processFailureMetadata } from "./quality/trusted/process";
import { operationalTimeouts } from "./quality/operational-timeouts";
import {
  validateReceiptWithEvidence,
  validatePlatformReviewWithEvidence,
} from "./quality/receipt-validation-gate";
import { runIndependentPackageTests } from "./package-tests";
import {
  contextProjectionPaths,
  workMethodProjectionPath,
  verifyLiveSourceProjectionAgreement,
  verifyProjectedSourceQa,
} from "./source-projection";
import { validateSourceProviderProjections } from "./verify-runtime-layout";
export { validateIndependentReviewReceipt } from "./quality/receipt-validator";
import { captureInstalledRuntimeClosure } from "../packages/native-hermes-adapter/src/source-provider/dependency-closure.ts";
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

let sourceRoot = resolve(import.meta.dir, "..");
let trustedControllerRoot: string | null = null;
let trustedSourceRunning = false;
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const json = async (path: string) => JSON.parse(await readFile(path, "utf8"));
const writeJson = async (path: string, value: unknown) => {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(value, null, 2) + "\n");
};
const commandEvidence: Record<string, unknown>[] = [];
let commandSequence = 0;
let diagnosticRoot: string | null = null;
let diagnosticPaths: string[] = [];

/** The real source child boundary: bounded metadata is emitted before its failure escapes. */
export async function runSourceCommand(
  cwd: string,
  argv: string[],
  options: {
    maxBytes?: number;
    timeoutMs?: number;
    knownTestFiles?: readonly string[];
    observe?: (diagnostic: Record<string, unknown>) => Promise<void>;
  } = {},
): Promise<void> {
  const ordinal = ++commandSequence;
  const startedAt = new Date().toISOString();
  const started = performance.now();
  let result: Awaited<ReturnType<typeof runNativeCommand>> | undefined;
  let failure: unknown;
  let failed = false;
  try {
    result = await runNativeCommand(
      "source-command", argv, cwd,
      options.timeoutMs ?? operationalTimeouts.workerMs,
      {}, { maxBytes: options.maxBytes },
    );
    if (result.exitCode !== 0 || result.timedOut) {
      failed = true;
      failure = Error(result.timedOut ? "SOURCE_COMMAND_TIMEOUT" : "SOURCE_COMMAND_NONZERO");
    }
  } catch (error) {
    failed = true;
    failure = error;
  }
  const transport = result?.rawOutput ?? processFailureMetadata(failure);
  const locations = sourceTestFailureDiagnostics(
    result?.stderr ?? "",
    options.knownTestFiles ? cwd : diagnosticRoot ?? cwd,
    options.knownTestFiles ?? diagnosticPaths,
  );
  if (!result || result.timedOut || (failed && !(options.knownTestFiles ?? diagnosticPaths).length)) {
    locations.total = null;
    locations.omitted = null;
    locations.diagnosticComplete = false;
  }
  const diagnostic = {
    schema: "agent-platform.source-command-diagnostic.v1",
    commandOrdinal: ordinal,
    label: "source-command",
    status: failed ? "fail" : "pass",
    failureKind: result ? result.timedOut ? "timeout" : result.exitCode !== 0 ? "nonzero" : null
      : processFailureMetadata(failure).failureKind,
    exitCode: result?.exitCode ?? ("exitCode" in transport ? transport.exitCode : null),
    timedOut: result?.timedOut ?? ("timedOut" in transport && transport.timedOut === true),
    transport,
    failureLocations: locations.locations,
    locationTotal: locations.total,
    locationOmitted: locations.omitted,
    diagnosticComplete: locations.diagnosticComplete,
  };
  while (Buffer.byteLength(JSON.stringify(diagnostic) + "\n") > 2048 && diagnostic.failureLocations.length) {
    diagnostic.failureLocations.pop();
    diagnostic.diagnosticComplete = false;
    if (diagnostic.locationTotal !== null)
      diagnostic.locationOmitted = diagnostic.locationTotal - diagnostic.failureLocations.length;
  }
  commandEvidence.push({
    ...diagnostic,
    commandSha256: sha(JSON.stringify(argv)),
    startedAt,
    finishedAt: new Date().toISOString(),
    durationMs: Math.round(performance.now() - started),
  });
  try {
    if (Buffer.byteLength(JSON.stringify(diagnostic) + "\n") > 2048)
      throw Error("SOURCE_COMMAND_DIAGNOSTIC_BUDGET");
    if (options.observe) await options.observe(diagnostic);
    else await new Promise<void>((done, reject) => {
      process.stdout.write(JSON.stringify(diagnostic) + "\n", (error) => error ? reject(error) : done());
    });
  } catch (error) {
    if (!failed) throw error;
    // A diagnostic write failure does not replace the original command failure or permit replay.
  }
  if (failed) throw failure;
}
const run = runSourceCommand;
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
const contextPaths = contextProjectionPaths("v6");
const nativeCodexPaths = [
  "package.json",
  "bun.lock",
  "packages/native-codex-adapter/package.json",
  ...nativeCodexModules.map((name) => `packages/native-codex-adapter/src/native/${name}.ts`),
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
    "src/native/context-owner.ts",
    "src/native/context-provider.ts",
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
  await verifyLiveSourceProjectionAgreement(
    sourceRoot,
    resolve(sourceRoot, "..", "codex-harness-engineering"),
  );
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
  const common = join(qaBase, "projects/agent-platform");
  // Fresh token-free metadata retains the actual committed source and tracked paths.
  // Copy current source data below; never copy native Git configuration or hooks.
  await mkdir(dirname(common), { recursive: true });
  await run(qaBase, [
    "git",
    "-c",
    "core.hooksPath=/dev/null",
    "clone",
    "--no-hardlinks",
    "--no-checkout",
    "--",
    sourceRoot,
    common,
  ]);
  await run(common, ["git", "-c", "core.hooksPath=/dev/null", "read-tree", "HEAD"]);
  await cp(sourceRoot, common, {
    recursive: true,
    dereference: false,
    filter: (path) =>
      !relative(sourceRoot, path)
        .split(sep)
        .some((part) => ["node_modules", ".git", ".private"].includes(part)),
  });
  if (trustedControllerRoot) {
    const { pinQaController } = await import("./quality/trusted/qa-inputs");
    await pinQaController(common, trustedControllerRoot);
  }
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
      "src/native/context-owner.ts",
      "src/native/context-provider.ts",
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
    const destination = join(qaBase, "projects", entry.owner, entry.path);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, bytes);
  }
  if (expected.size) throw Error("Finite owner guard fixture incomplete");
  // QA-only identity overlay. No production guard, registration or project bytes are changed.
  const project = await json(join(common, "project.json"));
  project.payload.details.sourceOwnerRoot = common;
  await writeJson(join(common, "project.json"), project);
  await run(common, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
  // Use the same canonical installed dependency observer as the source provider.
  // The synthetic owner must keep the already frozen runtime, including transitive bindings.
  const qaRuntime = await captureInstalledRuntimeClosure(
    common,
    async (path, maxBytes) => {
      const bytes = await readFile(path);
      if (bytes.length > maxBytes) throw Error("QA dependency source exceeds its byte bound");
      return bytes;
    },
    sha,
    false,
    [join(common, "packages/work-continuity/src/context/index.ts")],
  );
  for (const owner of ["codex-harness-engineering"]) {
    const root = join(qaBase, "projects", owner);
    await mkdir(join(root, "releases"), { recursive: true });
    await cp(
      join(common, "releases/palantirkc-ontology-0.2.0.tgz"),
      join(root, "releases/palantirkc-ontology-0.2.0.tgz"),
    );
    const dependencies = {
      "@palantirkc/ontology": "file:releases/palantirkc-ontology-0.2.0.tgz",
      "@palantirkc/work-continuity": "file:../agent-platform/packages/work-continuity",
      "@palantirkc/native-codex-adapter": "file:../agent-platform/packages/native-codex-adapter",
    };
    await writeJson(join(root, "package.json"), {
      name: owner + "-source-ci-fixture",
      private: true,
      type: "module",
      dependencies,
      overrides: Object.fromEntries(
        qaRuntime.dependencies
          .filter(({ name }) => name !== "@palantirkc/ontology")
          .map(({ name, root: dependencyRoot }) => [
            name,
            "file:" + relative(root, join(common, dependencyRoot)),
          ]),
      ),
    });
    // Synthetic owner lock is generated only from finite local package bindings, then frozen.
    await run(root, [process.execPath, "install", "--ignore-scripts"]);
    await run(root, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
  }
  const codex = join(qaBase, "projects/codex-harness-engineering");
  const policy = await json(join(common, "architecture/runtime-filesystem-contract.json"));
  validateSourceProviderProjections(policy.sourceProviderProjections);
  const sourceProjections = [
    {
      ...policy.sourceProviderProjections[0],
      canonicalConsumerRoot: codex,
      canonicalProviderRoot: common,
      consumerRoot: join(qaBase, "code/shared/codex-harness-engineering"),
      providerRoot: join(qaBase, "code/shared/agent-platform"),
    },
  ];
  const context = {
    schema: "codex.context-provider-source-closure.v6",
    consumerRoot: codex,
    providerRoot: common,
    providerProjectId: "agent-platform",
    packageName: "@palantirkc/work-continuity",
    modulePath: "packages/work-continuity/src/context/index.ts",
    gitWorkModulePath: "packages/work-continuity/src/git-work/index.ts",
    sourcePins: await sourcePins(common, contextPaths),
    sourceProjections,
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
    schema: "codex.native-adapter-source-closure.v2",
    consumerRoot: codex,
    providerRoot: common,
    packageName: "@palantirkc/native-codex-adapter",
    sourcePins: await sourcePins(common, nativeCodexPaths),
    sourceProjections,
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
  // The admitted Hermes image is the canonical generated guard. Rebuild only
  // fictional QA identities/lineage through its existing composer, never by
  // editing that guard or relaxing its facade/dependency/first-import checks.
  const { generateHermesSourceFixture, verifySourceFixture } = (await import(
    join(common, "packages/native-hermes-adapter/src/source-provider/generate.ts")
  )) as typeof import("../packages/native-hermes-adapter/src/source-provider/generate.ts");
  const hermesFixture = await generateHermesSourceFixture();
  await verifySourceFixture(hermesFixture);
  const guardPath = "src/ontology-driven/provider-source.ts";
  const guardBody = (bytes: string) => {
    for (const name of ["expectedRegistrationSha256", "expectedSuccessorSha256"]) {
      const marker = new RegExp(`const ${name} = "[a-f0-9]{64}";`, "gu");
      if ([...bytes.matchAll(marker)].length !== 1)
        throw Error("EXACT_GENERATED_QA_GUARD_MARKERS_REQUIRED");
      bytes = bytes.replace(marker, `const ${name} = "${"0".repeat(64)}";`);
    }
    return bytes;
  };
  if (
    guardBody(await readFile(join(hermesFixture.codeOwnerRoot, guardPath), "utf8")) !==
    guardBody(
      await readFile(join(common, "owner-fixture/hermes-harness-engineering", guardPath), "utf8"),
    )
  )
    throw Error("ADMITTED_HERMES_GUARD_DIFFERS_FROM_CANONICAL_GENERATOR");
  return {
    qaBase,
    common,
    codex,
    hermes: hermesFixture.codeOwnerRoot,
    hermesProviderRoot: hermesFixture.providerRoot,
    hermesQaLineage: {
      fixtureRoot: hermesFixture.fixtureRoot,
      manifestSha256: hermesFixture.manifestSha256,
      verificationClass: "canonical-generated-source-qa-only",
      admittedGuardBodyPreserved: true,
      nativePermissionGranted: false,
    },
  };
}
async function sourceChecks() {
  await liveOwnerFixtureCheck();
  await nativePluginSourceCheck(sourceRoot);
  const { qaBase, common, codex, hermes, hermesProviderRoot, hermesQaLineage } =
    await prepareQaTree();
  console.log(
    JSON.stringify(
      await verifyProjectedSourceQa({
        common,
        ownerFixture: join(common, "owner-fixture/codex-harness-engineering"),
        contextPaths,
        nativePaths: nativeCodexPaths,
        contextModules,
        contextVersion: "v6",
      }),
    ),
  );
  // Resolve the scope from the installed QA tree; a clean checkout needs no root dependencies.
  const { readQualityScope, readTrackedMarkdown, testFile } = (await import(
    join(trustedControllerRoot ?? common, "ci/quality/scope.ts")
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
  diagnosticRoot = common;
  diagnosticPaths = [...scope.testFiles];
  if (trustedControllerRoot) {
    const required = await readQualityScope(trustedControllerRoot);
    scope.testFiles = [...new Set([...required.testFiles, ...scope.testFiles])].sort();
    scope.lintFiles = [...new Set([...required.lintFiles, ...scope.lintFiles])].sort();
  }
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
  const packageChecks: (() => Promise<void>)[] = [];
  const packageDirectories = await readdir(join(common, "packages"), { withFileTypes: true });
  for (const entry of packageDirectories.sort((left, right) =>
    left.name.localeCompare(right.name),
  )) {
    if (!entry.isDirectory()) continue;
    const packageRoot = join(common, "packages", entry.name);
    const tests = scope.testFiles
      .filter((path) => path.startsWith(`packages/${entry.name}/`))
      .map((path) => join(common, path));
    if (tests.length)
      packageChecks.push(() =>
        run(packageRoot, [
          process.execPath,
          "test",
          "--timeout",
          String(operationalTimeouts.sourceTestMs),
          ...tests,
        ]),
      );
  }
  const packageStarted = performance.now();
  const packageStartedAt = new Date().toISOString();
  await runIndependentPackageTests(packageChecks);
  const packageTestExecution = {
    concurrency: 2,
    packages: packageChecks.length,
    startedAt: packageStartedAt,
    finishedAt: new Date().toISOString(),
    durationMs: Math.round(performance.now() - packageStarted),
    durationMeaning:
      "Elapsed package phase wall time; overlapping command durations are not summed.",
  };
  const qualityTests = scope.lintFiles.filter((path) => path.startsWith("ci/") && testFile(path));
  diagnosticPaths = [...new Set([...scope.testFiles, ...qualityTests])];
  if (qualityTests.length)
    await run(common, [
      process.execPath,
      "test",
      "--timeout",
      String(operationalTimeouts.sourceTestMs),
      ...qualityTests.map((path) => join(common, path)),
    ]);
  // Each guard runs in a fresh process, retaining native first-import/cache checks.
  const closureProbe = join(qaBase, "source-closure-probe.ts");
  await writeFile(
    closureProbe,
    `import {loadCodexOwnerContext} from ${JSON.stringify(join(codex, "src/native/context-owner.ts"))};\nimport {readNativeAdapterSourceClosure} from ${JSON.stringify(join(codex, "src/native/provider-closure.ts"))};\nimport {observeHermesContextProvider} from ${JSON.stringify(join(hermes, "src/ontology-driven/provider-source.ts"))};\nconst context=await loadCodexOwnerContext();await context.verifyCurrentSourceClosure();\nconst native=await readNativeAdapterSourceClosure();const hermes=await observeHermesContextProvider(${JSON.stringify(hermes)});await hermes.verify();\nif(native.providerRoot!==${JSON.stringify(common)}||!context.sourcePins.length||!context.gitWork||['resolveGitWorkSelection','openGitWork','readGitWorkContext','compactGitWorkContext','decodeCompactGitWorkContext'].some(name=>typeof context.gitWork[name]!=='function'))throw Error('Source owner closure or fixed Git Work API absent');\nconst home=await import(${JSON.stringify(join(common, "packages/native-codex-adapter/src/native/home-context.ts"))});\nif(['readHomeContextContractValue','gitWorkDeliveryPolicy'].some(name=>typeof home[name]!=='function'))throw Error('Source owner Home or delivery-policy API absent');\nif(!context.workMethod||['validateWorkMethodStanding','validateWorkMethodPacket','validateWorkMethodReceipt','workMethodPacketSha256','workMethodReceiptSha256','workMethodRequiredSources','decideWorkMethodAdmission','evaluateWorkMethodCoverage'].some(name=>typeof context.workMethod[name]!=='function'))throw Error('Source owner v6 Work method API absent');\nconsole.log(JSON.stringify({recordClass:'source-only-finite-owner-closure',contextPins:context.sourcePins.length,nativePins:native.pins.length,hermesPins:hermes.pins.length,gitWorkApiSourceProven:true,homeValueApiSourceProven:true,sourceDeliveryPolicyApiSourceProven:true,workMethodApiSourceProven:true,nativePermissionGranted:false}));\n`,
  );
  await run(common, [process.execPath, closureProbe]);
  // Source-only adversarial proof uses QA copies and in-memory registration overlays.
  // The historical graph is data for the pure reader only. Current QA code is
  // never rewritten/imported as a legacy implementation or declared native use.
  const currentContextIndex = await readFile(
    join(common, "packages/work-continuity/src/context/index.ts"),
    "utf8",
  );
  const workMethodExports = [
    ...currentContextIndex.matchAll(/^export \* from ["']\.\.\/work-method(?:\.ts)?["'];\r?\n/gm),
  ];
  if (workMethodExports.length !== 1)
    throw Error("Exact additive work-method context index export required");
  const legacyContextIndex = currentContextIndex.replace(workMethodExports[0]![0], "");
  const providerProbe = join(qaBase, "context-provider-proof-probe.ts");
  await writeFile(
    providerProbe,
    String.raw`import {strict as assert} from 'node:assert';
import {readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {readRegisteredContextProvider} from ${JSON.stringify(join(codex, "src/native/context-provider.ts"))};
const root=${JSON.stringify(codex)},provider=${JSON.stringify(common)};
const registrationPath=join(root,'architecture/context-provider.json'),module=join(root,'src/continuity/index.ts');
const original=JSON.parse(await readFile(registrationPath,'utf8'));
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const gitPaths=${JSON.stringify(gitWorkPaths)};
const memoryPaths=${JSON.stringify(memoryPaths)};
const workMethodPath=${JSON.stringify(workMethodProjectionPath)};
const legacyIndex=Buffer.from(${JSON.stringify(legacyContextIndex)});
function legacyGraph(value,overlays,version){
 value.schema='codex.context-provider-source-closure.'+version;
 value.sourcePins=value.sourcePins.filter(pin=>pin.path!==workMethodPath);
 if(version!=='v5')delete value.sourceProjections;
 overlays.set(join(provider,'packages/work-continuity/src/context/index.ts'),legacyIndex);
}
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
assert.equal(current.pins.length,27);assert.equal(current.workMethodAvailable,true);
for(const version of ['v4','v5']){const legacy=await proof((value,overlays)=>legacyGraph(value,overlays,version));assert.equal(legacy.pins.length,26);assert.equal(legacy.workMethodAvailable,undefined);}
const v3=await proof((value,overlays)=>{legacyGraph(value,overlays,'v3');value.sourcePins=value.sourcePins.filter(pin=>!memoryPaths.includes(pin.path));});
assert.equal(v3.pins.length,23);
const v2=await proof((value,overlays)=>{legacyGraph(value,overlays,'v2');delete value.gitWorkModulePath;value.sourcePins=value.sourcePins.filter(pin=>!gitPaths.includes(pin.path)&&!memoryPaths.includes(pin.path));});
assert.equal(v2.gitWorkModulePath,undefined);assert.equal(v2.pins.length,16);
const v1=await proof((value,overlays)=>{
 legacyGraph(value,overlays,'v1');delete value.gitWorkModulePath;
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
await assert.rejects(()=>proof((value,overlays)=>overlays.set(join(provider,'packages/work-continuity/src/context/index.ts'),legacyIndex)),/requires its existing context index export/);
await assert.rejects(()=>proof(value=>{value.schema='codex.context-provider-source-closure.v5';value.sourcePins=value.sourcePins.filter(pin=>pin.path!==workMethodPath);}),/requires the v6 source closure/);
console.log(JSON.stringify({recordClass:'source-only-context-provider-v6-adversarial-proof',legacySchemasPreserved:5,legacyGraphsArePureReaderOverlays:true,rejectedCases:10,nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, providerProbe]);
  const firstImportProbe = join(qaBase, "git-work-first-import-probe.ts");
  await writeFile(
    firstImportProbe,
    `import {strict as assert} from 'node:assert';
await import(${JSON.stringify(join(common, "packages/work-continuity/src/git-work/index.ts"))});
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(codex, "src/native/context-owner.ts"))});
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
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(codex, "src/native/context-owner.ts"))});
await assert.rejects(()=>loadCodexOwnerContext(),/imported before its observed source closure/);
console.log(JSON.stringify({recordClass:'source-only-memory-first-import-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, memoryFirstImportProbe]);
  const workMethodFirstImportProbe = join(qaBase, "work-method-first-import-probe.ts");
  await writeFile(
    workMethodFirstImportProbe,
    String.raw`import {strict as assert} from 'node:assert';
await import(${JSON.stringify(join(common, workMethodProjectionPath))});
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(codex, "src/native/context-owner.ts"))});
await assert.rejects(()=>loadCodexOwnerContext(),/imported before its observed source closure/);
console.log(JSON.stringify({recordClass:'source-only-work-method-first-import-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, workMethodFirstImportProbe]);
  const gitDriftProbe = join(qaBase, "git-work-source-drift-probe.ts");
  await writeFile(
    gitDriftProbe,
    String.raw`import {strict as assert} from 'node:assert';
import {readFile,writeFile} from 'node:fs/promises';
import {loadCodexOwnerContext} from ${JSON.stringify(join(codex, "src/native/context-owner.ts"))};
const path=${JSON.stringify(join(common, "packages/work-continuity/src/git-work/source.ts"))},bytes=await readFile(path),context=await loadCodexOwnerContext();
try{await writeFile(path,Buffer.concat([bytes,Buffer.from('\n')]));await assert.rejects(()=>context.verifyCurrentSourceClosure(),/Context provider source changed/);}finally{await writeFile(path,bytes);}
await context.verifyCurrentSourceClosure();
console.log(JSON.stringify({recordClass:'source-only-git-work-source-drift-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, gitDriftProbe]);
  const workMethodDriftProbe = join(qaBase, "work-method-source-drift-probe.ts");
  await writeFile(
    workMethodDriftProbe,
    String.raw`import {strict as assert} from 'node:assert';
import {readFile,writeFile} from 'node:fs/promises';
import {loadCodexOwnerContext} from ${JSON.stringify(join(codex, "src/native/context-owner.ts"))};
const path=${JSON.stringify(join(common, workMethodProjectionPath))},bytes=await readFile(path),context=await loadCodexOwnerContext();
try{await writeFile(path,Buffer.concat([bytes,Buffer.from('\n')]));await assert.rejects(()=>context.verifyCurrentSourceClosure(),/Context provider source changed/);}finally{await writeFile(path,bytes);}
await context.verifyCurrentSourceClosure();
console.log(JSON.stringify({recordClass:'source-only-work-method-source-drift-refusal',nativePermissionGranted:false}));
`,
  );
  await run(common, [process.execPath, workMethodDriftProbe]);
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
        join(hermesProviderRoot, `packages/native-hermes-adapter/src/ontology-driven/${name}.ts`),
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
      packageTestExecution,
      hermesQaLineage,
      commands: commandEvidence,
    }),
  );
}

/** P-loaded finite controller; subject source is an explicit parameter, never a module import. */
export async function runTrustedSourceChecks(
  targetRoot: string,
  policyRoot: string,
): Promise<void> {
  if (
    trustedSourceRunning ||
    resolve(policyRoot) !== resolve(import.meta.dir, "..") ||
    resolve(targetRoot) === resolve(policyRoot) ||
    (await realpath(targetRoot)) !== targetRoot
  )
    throw Error("Exact distinct trusted controller and source roots required");
  trustedSourceRunning = true;
  const prior = sourceRoot;
  sourceRoot = targetRoot;
  trustedControllerRoot = policyRoot;
  try {
    await sourceChecks();
  } finally {
    diagnosticRoot = null;
    diagnosticPaths = [];
    sourceRoot = prior;
    trustedControllerRoot = null;
    trustedSourceRunning = false;
  }
}

export interface ReviewGateOptions {
  env?: NodeJS.ProcessEnv;
  readApi?: (path: string) => Promise<unknown>;
  now?: () => number;
  delay?: (milliseconds: number) => Promise<void>;
}

export async function reviewGate(options: ReviewGateOptions = {}) {
  const env = options.env ?? process.env;
  const repository = env.GITHUB_REPOSITORY,
    head = env.REVIEW_HEAD_SHA,
    token = env.GITHUB_TOKEN;
  const expectedCreator = env.NATIVE_REVIEW_STATUS_CREATOR_ID,
    expectedLogin = env.NATIVE_REVIEW_STATUS_CREATOR_LOGIN;
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
  const now = options.now ?? (() => performance.now());
  const deadline = now() + operationalTimeouts.reviewPublicationMs;
  const trustedMainPush =
    env.GITHUB_ACTIONS === "true" &&
    env.GITHUB_EVENT_NAME === "push" &&
    env.GITHUB_REF === "refs/heads/main" &&
    env.GITHUB_REF_PROTECTED === "true" &&
    env.GITHUB_SHA === head;
  let readingStatuses = true;
  async function api(path: string) {
    const response = await fetch("https://api.github.com/repos/" + repository + "/" + path, {
      headers: {
        Authorization: "Bearer " + token,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
      redirect: "error",
      signal: AbortSignal.timeout(
        readingStatuses && trustedMainPush
          ? Math.max(1, Math.floor(Math.min(operationalTimeouts.networkMs, deadline - now())))
          : operationalTimeouts.networkMs,
      ),
    });
    if (!response.ok) throw Error("Native review API read failed: " + response.status);
    return (await response.json()) as any;
  }
  const readApi = options.readApi ?? api;
  const delay =
    options.delay ??
    ((milliseconds) => new Promise<void>((done) => setTimeout(done, milliseconds)));
  let statuses = await readApi(`commits/${head}/statuses?per_page=100`);
  let waitingAnnounced = false;
  for (;;) {
    if (trustedMainPush && now() >= deadline)
      throw Error("Native independent review status missing within publication wait");
    if (
      !Array.isArray(statuses) ||
      statuses.length > 100 ||
      statuses.some((value: unknown) => {
        if (!value || typeof value !== "object" || Array.isArray(value)) return true;
        const entry = value as Record<string, unknown>;
        return typeof entry.context !== "string" || !entry.context;
      })
    )
      throw Error("Exact-head native review statuses unavailable or malformed");
    // A present status always proceeds to the unchanged validation below, including holds.
    if (
      statuses.some(
        (value: { context: string }) => value.context === "native-independent-review",
      ) ||
      !trustedMainPush
    )
      break;
    if (statuses.length === 100)
      throw Error("Exact-head review absence cannot be established from a full status page");
    if (
      statuses.some(
        (entry: Record<string, unknown>) =>
          !Number.isSafeInteger(entry.id) ||
          Number(entry.id) <= 0 ||
          !["success", "pending", "failure", "error"].includes(String(entry.state)),
      )
    )
      throw Error("Exact-head native review absence response is malformed");
    const remaining = deadline - now();
    if (remaining <= 0) break;
    if (!waitingAnnounced) {
      console.log(
        "Exact-head review status absent on protected main push; waiting up to 300 seconds for publication",
      );
      waitingAnnounced = true;
    }
    await delay(Math.min(5000, remaining));
    if (now() >= deadline) break;
    statuses = await readApi(`commits/${head}/statuses?per_page=100`);
  }
  readingStatuses = false;
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
  const content = await readApi(`contents/native-reviews/${head}.json?ref=${receiptCommit}`);
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
  const commit = await readApi("git/commits/" + head);
  if (commit.sha !== head) throw Error("GitHub API returned another commit");
  await validatePlatformReviewWithEvidence(
    receipt,
    head,
    commit.tree?.sha,
    repository,
    {
      readTree: async (treeSha) => readApi(`git/trees/${treeSha}?recursive=1`),
      readFile: async (path, ref) => readApi(`contents/${path}?ref=${ref}`),
      readCommit: async (sha) => readApi(`git/commits/${sha}`),
      readStatuses: async (sha) => readApi(`commits/${sha}/statuses?per_page=100`),
    },
    { id: expectedCreator, login: expectedLogin },
  );
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
