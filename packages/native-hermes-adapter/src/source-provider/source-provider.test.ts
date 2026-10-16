import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { lstat, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  assertFiniteSourceCompleteness,
  generateHermesSourceFixture,
  modules,
  sourcePaths,
  validateV1SourceGraph,
  verifySourceFixture,
  type SourceFixture,
} from "./generate.ts";

const root = resolve(import.meta.dir, "../../../..");
const fixtures: SourceFixture[] = [];
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
afterEach(async () => {
  for (const fixture of fixtures.splice(0))
    await rm(fixture.fixtureRoot, { recursive: true, force: true });
});
async function fixture() {
  const value = await generateHermesSourceFixture();
  fixtures.push(value);
  return value;
}
async function child(f: SourceFixture, body: string) {
  const source = `const f=${JSON.stringify(f)};const api=await import(f.codeOwnerRoot+'/src/ontology-driven/provider-source.ts');${body}`;
  const process = Bun.spawn([Bun.which("bun")!, "-e", source], { stdout: "pipe", stderr: "pipe" });
  const [status, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  return { status, stdout, stderr };
}
async function repin(f: SourceFixture, change: (registration: Record<string, unknown>) => void) {
  const path = join(f.codeOwnerRoot, "architecture/hermes-context-provider.json");
  const registration = JSON.parse(await readFile(path, "utf8"));
  change(registration);
  const bytes = JSON.stringify(registration, null, 2) + "\n";
  await writeFile(path, bytes);
  const guard = join(f.codeOwnerRoot, "src/ontology-driven/provider-source.ts");
  await writeFile(
    guard,
    (await readFile(guard, "utf8")).replace(
      /const expectedRegistrationSha256 = "[a-f0-9]{64}";/u,
      `const expectedRegistrationSha256 = "${sha(bytes)}";`,
    ),
  );
}

test("new fictional facade has nine byte-exact canonical forwarders,14/19/20 proof and no owner data", async () => {
  const f = await fixture(),
    manifest = await verifySourceFixture(f);
  expect(manifest.originalSourceRestored).toBe(false);
  expect(manifest.nativePermissionGranted).toBe(false);
  expect(manifest.finiteOwnerSourceQa).toHaveLength(8);
  for (const name of modules)
    expect(await readFile(join(f.codeOwnerRoot, "src/" + name + ".ts"), "utf8")).toBe(
      `export * from '@palantirkc/native-hermes-adapter/${name}';\n`,
    );
  expect(sourcePaths).toHaveLength(14);
  expect(
    manifest.files.some((pin: { path: string }) =>
      /(?:^|\/)(?:project\.json|state|work|originals)(?:\/|$)/u.test(pin.path),
    ),
  ).toBe(false);
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);await proof.verify();
    const work=await import(f.codeOwnerRoot+'/src/ontology-driven/work-context.ts');
    if(typeof work.readWorkContext!=='function')throw Error('actual reader missing');
    let refused=false;try{await work.readWorkContext({},()=>{throw Error('NO_DATA_READ_ALLOWED');});}catch{refused=true;}
    if(!refused)throw Error('missing actual input accepted');
    await proof.verify();console.log(JSON.stringify({observed:proof.pins.length,consumer:proof.pins.length+1,nativePermissionGranted:false}));`,
  );
  expect(result.status).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({
    observed: 19,
    consumer: 20,
    nativePermissionGranted: false,
  });
});

test("generator has no arbitrary output target and refuses actual owner publication arguments", async () => {
  const process = Bun.spawn(
    [
      Bun.which("bun")!,
      join(import.meta.dir, "generate.ts"),
      "/home/palantirkc/code/shared/hermes-harness-engineering",
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [status, stderr] = await Promise.all([process.exited, new Response(process.stderr).text()]);
  expect(status).not.toBe(0);
  expect(stderr).toContain("No native output path");
});

test("generation source graph rejects missing sources, foreign static edges and unknown dynamic imports", async () => {
  const sources = new Map(
    await Promise.all(
      modules.map(async (name) => {
        const path = "packages/native-hermes-adapter/src/" + name + ".ts";
        return [path, await readFile(join(root, path))] as const;
      }),
    ),
  );
  expect(() => validateV1SourceGraph(sources)).not.toThrow();
  const first = [...sources.keys()][0];
  const other = [...sources.keys()][1];
  const missing = new Map(sources);
  missing.delete(first);
  expect(() => validateV1SourceGraph(missing)).toThrow("INCOMPLETE_V1_SOURCE_GRAPH");
  const foreign = new Map(sources);
  foreign.set(first, Buffer.from("import '../foreign.ts';"));
  expect(() => validateV1SourceGraph(foreign)).toThrow("FOREIGN_V1_SOURCE_IMPORT");
  const dynamic = new Map(sources);
  dynamic.set(other, Buffer.from("await import(process.env.UNTRUSTED!);"));
  expect(() => validateV1SourceGraph(dynamic)).toThrow("UNKNOWN_V1_DYNAMIC_IMPORT");
  const malformed = new Map(sources);
  malformed.set(first, Buffer.from("export {"));
  expect(() => validateV1SourceGraph(malformed)).toThrow("MALFORMED_V1_SOURCE");
  const loader = new Map(sources);
  loader.set(other, Buffer.from("require(process.env.UNTRUSTED);"));
  expect(() => validateV1SourceGraph(loader)).toThrow("UNKNOWN_V1_EXECUTABLE_LOADER");
});

test("finite fictional eight-source completeness never accepts7,duplicates,foreign rows or malformed hashes", async () => {
  const manifest = await verifySourceFixture(await fixture());
  const pins = manifest.finiteOwnerSourceQa;
  expect(() => assertFiniteSourceCompleteness(pins)).not.toThrow();
  expect(() => assertFiniteSourceCompleteness(pins.slice(0, 7))).toThrow("EXACT_EIGHT");
  expect(() => assertFiniteSourceCompleteness([...pins.slice(0, 7), pins[0]])).toThrow(
    "EXACT_EIGHT",
  );
  expect(() =>
    assertFiniteSourceCompleteness(
      pins.map((pin: { path: string; sha256: string }, i: number) =>
        i ? pin : { ...pin, path: "foreign/source.ts" },
      ),
    ),
  ).toThrow("EXACT_EIGHT");
  expect(() =>
    assertFiniteSourceCompleteness(
      pins.map((pin: { path: string; sha256: string }, i: number) =>
        i ? pin : { ...pin, sha256: "bad" },
      ),
    ),
  ).toThrow("EXACT_EIGHT");
});

test("fixture manifest currentness refuses unknown outputs,foreign roots and independently repinned malformed metadata", async () => {
  const f = await fixture();
  await expect(verifySourceFixture({ ...f, codeOwnerRoot: f.providerRoot })).rejects.toThrow(
    "FOREIGN_SOURCE_FIXTURE_ROOT",
  );
  await writeFile(join(f.codeOwnerRoot, "unknown.ts"), "throw Error('MUST_NOT_IMPORT');\n");
  await expect(verifySourceFixture(f)).rejects.toThrow("SOURCE_FIXTURE_CLOSURE_CHANGED");
  await rm(join(f.codeOwnerRoot, "unknown.ts"));
  const manifest = JSON.parse(await readFile(f.manifestPath, "utf8"));
  manifest.unreviewed = true;
  const bytes = JSON.stringify(manifest);
  await writeFile(f.manifestPath, bytes);
  await expect(verifySourceFixture(f)).rejects.toThrow("FIXTURE_MANIFEST_CHANGED");
  await expect(verifySourceFixture({ ...f, manifestSha256: sha(bytes) })).rejects.toThrow(
    "UNKNOWN_OR_MISSING_FIXTURE_FIELD",
  );
});

test("first import refuses changed canonical canary before executing it", async () => {
  const f = await fixture();
  await writeFile(
    join(f.providerRoot, "packages/native-hermes-adapter/src/ontology-driven/work-context.ts"),
    "throw Error('CANARY_EXECUTED');\n",
  );
  const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("Hermes source provider changed:");
  expect(result.stderr).not.toContain("CANARY_EXECUTED");
});

test("first import refuses preimported actual Work code without clearing native caches", async () => {
  const f = await fixture();
  const result = await child(
    f,
    "await import(f.codeOwnerRoot+'/src/ontology-driven/work-context.ts');await api.observeHermesContextProvider(f.codeOwnerRoot);",
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("loaded before source observation");
});

test("post-read check refuses canonical or owner-source mutation", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);
    await Bun.write(f.providerRoot+'/packages/native-hermes-adapter/src/ontology-driven/contracts.ts','// mutation');await proof.verify();`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("Hermes code source changed during packaging");
  const g = await fixture();
  const owner = await child(
    g,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);
    await Bun.write(f.codeOwnerRoot+'/src/ontology-driven/contracts.ts','// mutation');await proof.verify();`,
  );
  expect(owner.status).not.toBe(0);
  expect(owner.stderr).toContain("Hermes code source changed during packaging");
});

test("guard rejects unknown facade files and fixed-root substitution", async () => {
  const f = await fixture();
  const root = await child(f, "await api.observeHermesContextProvider(f.providerRoot);");
  expect(root.status).not.toBe(0);
  expect(root.stderr).toContain("fixed code owner");
  await writeFile(join(f.codeOwnerRoot, "unknown.ts"), "throw Error('CANARY_EXECUTED');");
  const extra = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(extra.status).not.toBe(0);
  expect(extra.stderr).toContain("UNKNOWN_FACADE_FILE");
});

test("repinned incomplete registration,unknown fields and foreign provider root remain refused", async () => {
  for (const [mutation, message] of [
    [
      (value: Record<string, unknown>) => {
        (value.sourcePins as unknown[]).pop();
      },
      "Incomplete Hermes source provider denominator",
    ],
    [
      (value: Record<string, unknown>) => {
        value.unknown = true;
      },
      "UNKNOWN_OR_MISSING_SOURCE_FIELD",
    ],
    [
      (value: Record<string, unknown>) => {
        value.providerRoot = "/tmp/foreign";
      },
      "Invalid fixed Hermes source provider registration",
    ],
  ] as const) {
    const f = await fixture();
    await repin(f, mutation);
    const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain(message);
  }
});

test("removing registration cannot qualify canonical forwarders or fall back to standalone proof", async () => {
  const f = await fixture();
  await rm(join(f.codeOwnerRoot, "architecture/hermes-context-provider.json"));
  const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("INCOMPLETE_OR_UNKNOWN_FACADE_FILE");
});

test("cached guard successor refuses changed disk image even when the new registration marker is valid", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `await api.observeHermesContextProvider(f.codeOwnerRoot);
    const path=f.codeOwnerRoot+'/src/ontology-driven/provider-source.ts';await Bun.write(path,(await Bun.file(path).text())+${JSON.stringify("\n// successor\n")});
    await api.observeHermesContextProvider(f.codeOwnerRoot);`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("guard changed after import; use a fresh process");
});

test("generated facade is deterministic modulo exclusive fixture roots", async () => {
  const first = await fixture(),
    second = await fixture();
  const normalize = (value: string, f: SourceFixture) =>
    value.replaceAll(f.fixtureRoot, "$FIXTURE");
  for (const path of [
    "package.json",
    "bun.lock",
    "src/ontology-driven/work-context.ts",
    "architecture/hermes-context-provider.json",
    "source-successor.json",
  ]) {
    const a = normalize(await readFile(join(first.codeOwnerRoot, path), "utf8"), first);
    const b = normalize(await readFile(join(second.codeOwnerRoot, path), "utf8"), second);
    expect(a).toBe(b);
  }
  expect(await lstat(first.fixtureRoot)).not.toEqual(await lstat(second.fixtureRoot));
  expect((await readdir(first.codeOwnerRoot)).sort()).toEqual([
    "architecture",
    "bun.lock",
    "node_modules",
    "package.json",
    "source-successor.json",
    "src",
  ]);
});
