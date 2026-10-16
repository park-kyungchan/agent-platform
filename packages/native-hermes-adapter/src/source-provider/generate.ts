/** NEW common-maintained source successor. Only exclusive, fictional temporary fixtures can be emitted. */
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { cp, mkdir, mkdtemp, open, readdir, realpath, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { captureInstalledRuntimeClosure } from "./dependency-closure.ts";

const canonicalRoot = resolve(import.meta.dir, "../../../..");
const packagePath = "packages/native-hermes-adapter";
export const modules = [
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
export const sourcePaths = [
  "package.json",
  "bun.lock",
  "releases/palantirkc-ontology-0.2.0.tgz",
  packagePath + "/package.json",
  packagePath + "/releases/palantirkc-ontology-0.2.0.tgz",
  ...modules.map((name) => packagePath + "/src/" + name + ".ts"),
] as const;
const generationPaths = [
  packagePath + "/src/source-provider/generate.ts",
  packagePath + "/src/source-provider/guard.template.ts",
  "architecture/hermes-new-source-successor.json",
  packagePath + "/src/source-provider/dependency-closure.ts",
];
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const json = (value: unknown) => JSON.stringify(value, null, 2) + "\n";
async function exact(path: string, max = 2 * 1024 * 1024) {
  if (resolve(path) !== path || (await realpath(path)) !== path)
    throw Error("EXACT_BOUNDED_GENERATION_INPUT_REQUIRED");
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat();
    // Frozen local Bun dependencies may share cache hardlinks. All bytes are snapshotted and rechecked;
    // newly generated projection files are exclusive copies, never writes through those links.
    if (!before.isFile() || before.size > max || before.nlink < 1 || before.nlink > 64)
      throw Error("EXACT_BOUNDED_GENERATION_INPUT_REQUIRED");
    const bytes = Buffer.alloc(before.size + 1);
    let count = 0;
    while (count < bytes.length) {
      const part = await file.read(bytes, count, bytes.length - count, null);
      if (!part.bytesRead) break;
      count += part.bytesRead;
    }
    const after = await file.stat();
    if (
      count !== before.size ||
      before.ino !== after.ino ||
      before.dev !== after.dev ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs
    )
      throw Error("GENERATION_INPUT_CHANGED");
    return bytes.subarray(0, count);
  } finally {
    await file.close();
  }
}
const initialGenerator = sha(await exact(import.meta.path));
/** A finite static TypeScript graph; source pinning alone must not silently admit a new executable edge. */
export function validateV1SourceGraph(sources: ReadonlyMap<string, Uint8Array>) {
  const admitted = new Set(modules.map((name) => packagePath + "/src/" + name + ".ts"));
  if (sources.size !== admitted.size || [...admitted].some((path) => !sources.has(path)))
    throw Error("INCOMPLETE_V1_SOURCE_GRAPH");
  for (const [path, bytes] of sources) {
    if (!admitted.has(path)) throw Error("UNKNOWN_V1_SOURCE_FILE");
    const source = Buffer.from(bytes).toString();
    const parser = new Bun.Transpiler({ loader: "ts", target: "bun" });
    let imports: ReturnType<Bun.Transpiler["scanImports"]>;
    let normalized: string;
    try {
      imports = parser.scanImports(source);
      // Parsing/transformation never evaluates the source or resolves/imports any module.
      normalized = parser.transformSync(source);
    } catch {
      throw Error("MALFORMED_V1_SOURCE");
    }
    const check = (specifier: string) => {
      if (
        [
          "node:path",
          "node:util",
          "node:url",
          "node:fs",
          "node:fs/promises",
          "@palantirkc/ontology",
        ].includes(specifier)
      )
        return;
      const target = relative(canonicalRoot, resolve(canonicalRoot, dirname(path), specifier));
      if (
        !specifier.startsWith(".") ||
        !admitted.has(target.endsWith(".ts") ? target : target + ".ts")
      )
        throw Error("FOREIGN_V1_SOURCE_IMPORT");
    };
    for (const edge of imports) {
      if (edge.kind !== "import-statement") throw Error("UNKNOWN_V1_EXECUTABLE_LOADER");
      check(edge.path);
    }
    // Keep the one admitted delayed immutable-pack import separate from the provider graph.
    // Conservative refusal also covers computed imports that scanImports intentionally omits.
    const packImport = 'import(pathToFileURL(join(root, "tooling/validate.ts")).href)';
    if (path.endsWith("/ontology-driven/work-context.ts")) {
      if (normalized.split(packImport).length !== 2)
        throw Error("IMMUTABLE_PACK_IMPORT_CONTRACT_CHANGED");
      normalized = normalized.replace(packImport, "undefined");
    }
    const gap = String.raw`(?:\s|/\*[\s\S]*?\*/|//[^\n]*(?:\n|$))*`;
    if (new RegExp(String.raw`\bimport${gap}\(`, "u").test(normalized))
      throw Error("UNKNOWN_V1_DYNAMIC_IMPORT");
    if (new RegExp(String.raw`\b(?:require|eval|Function)${gap}\(`, "u").test(normalized))
      throw Error("UNKNOWN_V1_EXECUTABLE_LOADER");
  }
}
type FilePin = { path: string; sha256: string; bytes: number };
type LinkPin = { path: string; target: string };
const finiteOwnerPaths = [
  "codex-harness-engineering/src/legacy/harness/owner-context-loader.ts",
  "codex-harness-engineering/src/legacy/harness/provider-closure.ts",
  "codex-harness-engineering/src/native/provider-closure.ts",
  "codex-harness-engineering/src/native/hook-cli.ts",
  "codex-harness-engineering/src/native/hooks.ts",
  "codex-harness-engineering/src/native/home-context.ts",
  "codex-harness-engineering/src/native/evaluation-ledger.ts",
  "hermes-harness-engineering/src/ontology-driven/provider-source.ts",
] as const;
/** Fictional source QA completeness, not a claim that eight live native owner sources were recovered. */
export function assertFiniteSourceCompleteness(pins: readonly { path: string; sha256: string }[]) {
  if (
    pins.length !== 8 ||
    new Set(pins.map((pin) => pin.path)).size !== 8 ||
    pins.some((pin) => !/^[a-f0-9]{64}$/u.test(pin.sha256)) ||
    finiteOwnerPaths.some((path) => !pins.some((pin) => pin.path === path))
  )
    throw Error("EXACT_EIGHT_FICTIONAL_SOURCE_PINS_REQUIRED");
}
export interface SourceFixture {
  fixtureRoot: string;
  codeOwnerRoot: string;
  providerRoot: string;
  manifestPath: string;
  manifestSha256: string;
}
async function tree(root: string) {
  const files: FilePin[] = [];
  const links: LinkPin[] = [];
  async function visit(directory: string) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name),
        name = relative(root, path);
      if (name === "fixture-manifest.json") continue;
      if (entry.isSymbolicLink()) {
        const target = await realpath(path);
        if (
          name !== "hermes-harness-engineering/node_modules/@palantirkc/native-hermes-adapter" ||
          target !== join(root, "agent-platform/packages/native-hermes-adapter")
        )
          throw Error("UNKNOWN_FIXTURE_SYMLINK");
        links.push({ path: name, target: relative(root, target) });
      } else if (entry.isDirectory()) await visit(path);
      else if (entry.isFile()) {
        const bytes = await exact(path);
        files.push({ path: name, bytes: bytes.length, sha256: sha(bytes) });
      } else throw Error("UNKNOWN_FIXTURE_FILE");
      if (files.length > 2048) throw Error("FIXTURE_FILE_LIMIT");
    }
  }
  await visit(root);
  return { files: files.sort((a, b) => a.path.localeCompare(b.path)), links };
}
/** The caller pins the manifest digest out of band. Unknown files, changed bytes and foreign roots refuse. */
export async function verifySourceFixture(fixture: SourceFixture) {
  if (
    resolve(fixture.fixtureRoot) !== fixture.fixtureRoot ||
    (await realpath(fixture.fixtureRoot)) !== fixture.fixtureRoot ||
    fixture.codeOwnerRoot !== join(fixture.fixtureRoot, "hermes-harness-engineering") ||
    fixture.providerRoot !== join(fixture.fixtureRoot, "agent-platform") ||
    fixture.manifestPath !== join(fixture.fixtureRoot, "fixture-manifest.json")
  )
    throw Error("FOREIGN_SOURCE_FIXTURE_ROOT");
  const bytes = await exact(fixture.manifestPath);
  if (sha(bytes) !== fixture.manifestSha256) throw Error("FIXTURE_MANIFEST_CHANGED");
  const manifest = JSON.parse(bytes.toString());
  if (
    !manifest ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    Object.keys(manifest).sort().join("\0") !==
      [
        "schema",
        "originalSourceRestored",
        "nativePermissionGranted",
        "authority",
        "canonicalRoot",
        "inputs",
        "finiteOwnerSourceQa",
        "files",
        "links",
      ]
        .sort()
        .join("\0")
  )
    throw Error("UNKNOWN_OR_MISSING_FIXTURE_FIELD");
  if (
    manifest.schema !== "hermes.new-source-fictional-fixture.v1" ||
    manifest.originalSourceRestored !== false ||
    manifest.nativePermissionGranted !== false ||
    manifest.authority !== "none" ||
    manifest.canonicalRoot !== canonicalRoot ||
    !Array.isArray(manifest.inputs) ||
    manifest.inputs.length !== sourcePaths.length + generationPaths.length
  )
    throw Error("INVALID_SOURCE_FIXTURE_MANIFEST");
  const expected = [...sourcePaths, ...generationPaths];
  if (
    new Set(manifest.inputs.map((pin: FilePin) => pin.path)).size !== expected.length ||
    expected.some((path) => !manifest.inputs.some((pin: FilePin) => pin.path === path))
  )
    throw Error("INCOMPLETE_FIXTURE_SOURCE_INPUTS");
  for (const pin of manifest.inputs as FilePin[])
    if (sha(await exact(join(canonicalRoot, pin.path))) !== pin.sha256)
      throw Error("CURRENT_CANONICAL_SOURCE_CHANGED");
  assertFiniteSourceCompleteness(manifest.finiteOwnerSourceQa);
  for (const pin of manifest.finiteOwnerSourceQa as { path: string; sha256: string }[]) {
    const actual = pin.path.startsWith("codex-")
      ? join(canonicalRoot, "owner-fixture", pin.path)
      : join(fixture.fixtureRoot, pin.path);
    if (sha(await exact(actual)) !== pin.sha256) throw Error("CURRENT_FINITE_QA_SOURCE_CHANGED");
  }
  const current = await tree(fixture.fixtureRoot);
  if (JSON.stringify(current) !== JSON.stringify({ files: manifest.files, links: manifest.links }))
    throw Error("SOURCE_FIXTURE_CLOSURE_CHANGED");
  return manifest;
}
/** No output path parameter: this entrypoint cannot publish to an actual native owner location. */
export async function generateHermesSourceFixture(): Promise<SourceFixture> {
  if ((await realpath(canonicalRoot)) !== canonicalRoot || Bun.version !== "1.4.3")
    throw Error("EXACT_COMMON_SOURCE_AND_BUN143_REQUIRED");
  const inputs = new Map<string, Buffer>();
  for (const path of [...sourcePaths, ...generationPaths])
    inputs.set(path, await exact(join(canonicalRoot, path)));
  if (sha(inputs.get(generationPaths[0])!) !== initialGenerator)
    throw Error("GENERATOR_CHANGED_AFTER_IMPORT_USE_FRESH_PROCESS");
  validateV1SourceGraph(
    new Map(
      [...inputs].filter(([path]) =>
        modules.some((name) => path === packagePath + "/src/" + name + ".ts"),
      ),
    ),
  );
  const fixtureRoot = await mkdtemp(join(await realpath(tmpdir()), "hermes-new-source-fictional-"));
  const codeOwnerRoot = join(fixtureRoot, "hermes-harness-engineering"),
    providerRoot = join(fixtureRoot, "agent-platform");
  const put = async (path: string, bytes: Uint8Array | string) => {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, bytes, { flag: "wx" });
  };
  await put(
    join(fixtureRoot, "fixture-owner.json"),
    json({
      schema: "exclusive-source-fixture-owner.v1",
      generatedOnly: true,
      originalSourceRestored: false,
      nativePermissionGranted: false,
      authority: "none",
    }),
  );
  for (const path of [...sourcePaths, ...generationPaths])
    await put(join(providerRoot, path), inputs.get(path)!);
  // This manifest/release is needed only to resolve the local lock's already admitted transitive binding.
  for (const path of [
    "packages/work-continuity/package.json",
    "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
  ])
    await put(join(providerRoot, path), await exact(join(canonicalRoot, path)));
  const seen = new Set<string>();
  async function stageDependency(name: string, from: string) {
    if (seen.has(name)) return;
    seen.add(name);
    if (seen.size > 32 || !/^(@[a-z0-9-]+\/)?[a-z0-9-]+$/u.test(name))
      throw Error("DEPENDENCY_SCOPE_LIMIT");
    const manifestPath = Bun.resolveSync(name + "/package.json", from);
    const manifest = JSON.parse((await exact(manifestPath)).toString());
    const target = join(providerRoot, "node_modules", name);
    await mkdir(dirname(target), { recursive: true });
    await cp(dirname(manifestPath), target, {
      recursive: true,
      dereference: false,
      filter: (path) => !["node_modules", "project.json"].includes(path.split("/").at(-1)!),
    });
    for (const child of Object.keys(manifest.dependencies ?? {}).sort())
      await stageDependency(child, dirname(manifestPath));
  }
  await stageDependency("@palantirkc/ontology", canonicalRoot);
  const ownerPackage = {
    name: "@palantirkc/hermes-new-source-fictional-facade",
    private: true,
    type: "module",
    sourceOnly: true,
    originalSourceRestored: false,
    dependencies: {
      "@palantirkc/native-hermes-adapter": "file:../agent-platform/packages/native-hermes-adapter",
    },
  };
  await put(join(codeOwnerRoot, "package.json"), json(ownerPackage));
  const install = Bun.spawn(
    [process.execPath, "install", "--offline", "--ignore-scripts", "--lockfile-only"],
    {
      cwd: codeOwnerRoot,
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, BUN_CONFIG_NO_CLEAR_TERMINAL: "1" },
    },
  );
  const [status, stdout, stderr] = await Promise.all([
    install.exited,
    new Response(install.stdout).text(),
    new Response(install.stderr).text(),
  ]);
  if (status !== 0 || stdout.length + stderr.length > 65536)
    throw Error("OFFLINE_FICTIONAL_LOCK_GENERATION_FAILED");
  await exact(join(codeOwnerRoot, "bun.lock"));
  await mkdir(join(codeOwnerRoot, "node_modules/@palantirkc"), { recursive: true });
  await symlink(
    join(providerRoot, packagePath),
    join(codeOwnerRoot, "node_modules/@palantirkc/native-hermes-adapter"),
  );
  for (const name of modules)
    await put(
      join(codeOwnerRoot, "src/" + name + ".ts"),
      "export * from '@palantirkc/native-hermes-adapter/" + name + "';\n",
    );
  const registrationBytes = json({
    schema: "hermes.context-source-provider.v1",
    consumerRoot: codeOwnerRoot,
    providerRoot,
    packageName: "@palantirkc/native-hermes-adapter",
    sourcePins: sourcePaths.map((path) => ({ path, sha256: sha(inputs.get(path)!) })),
    authority: "none",
    nativePermissionGranted: false,
  });
  await put(join(codeOwnerRoot, "architecture/hermes-context-provider.json"), registrationBytes);
  const successorBytes = json({
    schema: "hermes.new-source-generated-projection.v1",
    originalSourceRestored: false,
    codeOwner: "agent-platform",
    semanticOwners: ["ontology-platform", "hermes-harness-engineering"],
    nativeAndDataOwnersRetained: true,
    nativePermissionGranted: false,
    authority: "none",
    lineage: "architecture/hermes-new-source-successor.json",
    inputs: [...inputs].map(([path, bytes]) => ({ path, sha256: sha(bytes) })),
    runtimeDependencies: await captureInstalledRuntimeClosure(
      providerRoot,
      exact,
      sha,
      false,
      modules.map((name) => join(providerRoot, packagePath, "src", name + ".ts")),
    ),
  });
  await put(join(codeOwnerRoot, "source-successor.json"), successorBytes);

  const template = inputs.get(generationPaths[1])!.toString();
  const inlineMarker = "// __INLINE_CANONICAL_DEPENDENCY_CLOSURE__";
  if (template.split(inlineMarker).length !== 2)
    throw Error("EXACT_DEPENDENCY_PROJECTION_REQUIRED");
  const composed = template.replace(
    inlineMarker,
    inputs.get(generationPaths[3])!.toString() +
      "\ncaptureDependencies = captureInstalledRuntimeClosure;\n",
  );
  const marker = 'const expectedRegistrationSha256 = "' + "0".repeat(64) + '";';
  const lineageMarker = 'const expectedSuccessorSha256 = "' + "0".repeat(64) + '";';
  if (composed.split(marker).length !== 2 || composed.split(lineageMarker).length !== 2)
    throw Error("EXACT_TEMPLATE_SUBSTITUTION_REQUIRED");
  await put(
    join(codeOwnerRoot, "src/ontology-driven/provider-source.ts"),
    composed
      .replace(marker, 'const expectedRegistrationSha256 = "' + sha(registrationBytes) + '";')
      .replace(lineageMarker, 'const expectedSuccessorSha256 = "' + sha(successorBytes) + '";'),
  );

  const historicalQa = JSON.parse(
    (await exact(join(canonicalRoot, "owner-fixture/manifest.json"))).toString(),
  );
  const finiteOwnerSourceQa: { path: string; sha256: string }[] = [];
  for (const path of finiteOwnerPaths) {
    if (path.startsWith("hermes-")) {
      finiteOwnerSourceQa.push({ path, sha256: sha(await exact(join(fixtureRoot, path))) });
    } else {
      const row = historicalQa.sources.find(
        (pin: { owner: string; path: string }) => pin.owner + "/" + pin.path === path,
      );
      const bytes = await exact(join(canonicalRoot, "owner-fixture", path));
      if (!row || row.sha256 !== sha(bytes)) throw Error("HISTORICAL_CODEX_QA_SOURCE_CHANGED");
      // Existing seven sanitized Codex fixtures remain QA inputs, never source recovery outputs.
      finiteOwnerSourceQa.push({ path, sha256: sha(bytes) });
    }
  }
  assertFiniteSourceCompleteness(finiteOwnerSourceQa);
  // Recheck every authored source after lock/dependency staging. No changed input can return a qualified fixture.
  for (const [path, bytes] of inputs)
    if (sha(await exact(join(canonicalRoot, path))) !== sha(bytes))
      throw Error("SOURCE_CHANGED_DURING_GENERATION");
  const manifestPath = join(fixtureRoot, "fixture-manifest.json");
  const manifestBytes = json({
    schema: "hermes.new-source-fictional-fixture.v1",
    originalSourceRestored: false,
    nativePermissionGranted: false,
    authority: "none",
    canonicalRoot,
    inputs: [...inputs].map(([path, bytes]) => ({ path, bytes: bytes.length, sha256: sha(bytes) })),
    finiteOwnerSourceQa,
    ...(await tree(fixtureRoot)),
  });
  await put(manifestPath, manifestBytes);
  const result = {
    fixtureRoot,
    codeOwnerRoot,
    providerRoot,
    manifestPath,
    manifestSha256: sha(manifestBytes),
  };
  await verifySourceFixture(result);
  return result;
}
if (import.meta.main) {
  if (process.argv.length !== 2)
    throw Error("No native output path or publication argument is supported");
  console.log(json(await generateHermesSourceFixture()));
}
