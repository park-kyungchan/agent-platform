import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { cp, lstat, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
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
}, 25000);

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
}, 25000);

test("public proof row and array mutations cannot change private verification or another proof", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `const fs=await import('node:fs/promises');const {createHash}=await import('node:crypto');
    const first=await api.observeHermesContextProvider(f.codeOwnerRoot);
    const second=await api.observeHermesContextProvider(f.codeOwnerRoot);
    if(first.pins.length!==19||second.pins.length!==19)throw Error('PUBLIC_DENOMINATOR_CHANGED');
    const path=f.providerRoot+'/packages/native-hermes-adapter/src/ontology-driven/contracts.ts';
    const bytes=(await fs.readFile(path,'utf8'))+'\\n// inert alias regression\\n';
    await fs.writeFile(path,bytes);
    first.pins.find(p=>p.path===path).sha256=createHash('sha256').update(bytes).digest('hex');
    const refusal=async fn=>{try{await fn();return false;}catch(error){return error.message.startsWith('Hermes code source changed during packaging:');}};
    const rowRefused=await refusal(first.verify);first.pins.splice(0,first.pins.length);
    console.log(JSON.stringify({rowRefused,arrayRefused:await refusal(first.verify),secondRefused:await refusal(second.verify),secondPins:second.pins.length}));`,
  );
  expect(result.status).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({
    rowRefused: true,
    arrayRefused: true,
    secondRefused: true,
    secondPins: 19,
  });
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
}, 25000);

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

test("existing exact project/state entrypoints remain outside source proof and source cache identity", async () => {
  const f = await fixture();
  // A test-only overlay stands for existing owner data; the generator produced none of these records.
  await writeFile(
    join(f.codeOwnerRoot, "project.json"),
    '{"classification":"synthetic-existing-owner-data"}',
  );
  await mkdir(join(f.codeOwnerRoot, "state"));
  await writeFile(
    join(f.codeOwnerRoot, "state/next-session.json"),
    '{"classification":"synthetic-existing-owner-data"}',
  );
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);
    await Bun.write(f.codeOwnerRoot+'/project.json','changed owner data, intentionally not parsed by source guard');
    await Bun.write(f.codeOwnerRoot+'/state/next-session.json','changed owner data');
    await proof.verify();const again=await api.observeHermesContextProvider(f.codeOwnerRoot);
    console.log(JSON.stringify({observed:again.pins.length,consumer:again.pins.length+1,dataAttested:again.pins.some(p=>p.path.endsWith('/project.json')||p.path.endsWith('/next-session.json'))}));`,
  );
  expect(result.status).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({ observed: 19, consumer: 20, dataAttested: false });
  // The stricter generator-output manifest does not silently adopt the subsequent owner-data overlay.
  await expect(verifySourceFixture(f)).rejects.toThrow("SOURCE_FIXTURE_CLOSURE_CHANGED");
});

test("owner data boundary is exact and cannot hide executable, extra or symlink paths", async () => {
  for (const path of ["state/hidden.ts", "state/unregistered.json"]) {
    const f = await fixture();
    await mkdir(join(f.codeOwnerRoot, "state"));
    await writeFile(join(f.codeOwnerRoot, path), "throw Error('MUST_NOT_IMPORT');");
    const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("UNKNOWN_FACADE_FILE");
  }
  const f = await fixture();
  await symlink(join(f.codeOwnerRoot, "package.json"), join(f.codeOwnerRoot, "project.json"));
  const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("UNADMITTED_FACADE_SYMLINK");
});

test("observed wrapper successor keeps the original literal cache refusal before malformed-wrapper classification", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);await proof.verify();
    await import(f.codeOwnerRoot+'/src/ontology-driven/work-context.ts');
    const path=f.codeOwnerRoot+'/src/ontology-driven/work-context.ts';
    await Bun.write(path,(await Bun.file(path).text())+${JSON.stringify("\nthrow Error('MUST_NOT_IMPORT_OWNER_SUCCESSOR');\n")});
    try{await api.observeHermesContextProvider(f.codeOwnerRoot);throw Error('UNSAFE_ACCEPT');}
    catch(error){console.log(JSON.stringify({message:error.message,observed:proof.pins.length,consumer:proof.pins.length+1}));}`,
  );
  expect(result.status).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({
    message: "Hermes Work provider changed after import; use a fresh process",
    observed: 19,
    consumer: 20,
  });
  const g = await fixture();
  await writeFile(
    join(g.codeOwnerRoot, "src/ontology-driven/work-context.ts"),
    "throw Error('MUST_NOT_IMPORT');",
  );
  const initial = await child(g, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(initial.status).not.toBe(0);
  expect(initial.stderr).toContain("NONCANONICAL_GENERATED_FORWARDER");
});

test("finite8 verifier rejects a repinned false generated Hermes eighth digest", async () => {
  const f = await fixture();
  const manifest = JSON.parse(await readFile(f.manifestPath, "utf8"));
  manifest.finiteOwnerSourceQa.find((pin: { path: string }) =>
    pin.path.startsWith("hermes-"),
  ).sha256 = "0".repeat(64);
  const bytes = JSON.stringify(manifest, null, 2) + "\n";
  await writeFile(f.manifestPath, bytes);
  await expect(verifySourceFixture({ ...f, manifestSha256: sha(bytes) })).rejects.toThrow(
    "CURRENT_FINITE_QA_SOURCE_CHANGED",
  );
});

test("immutable Ontology transitive canary is refused before first Work import", async () => {
  const f = await fixture(),
    canary = join(f.fixtureRoot, "TRANSITIVE_CANARY_EXECUTED");
  await writeFile(
    join(f.providerRoot, "node_modules/@palantirkc/ontology/dist/validation.js"),
    `await Bun.write(${JSON.stringify(canary)},'unsafe');throw Error('TRANSITIVE_CANARY_EXECUTED');`,
  );
  const result = await child(
    f,
    `await api.observeHermesContextProvider(f.codeOwnerRoot);
    await import(f.codeOwnerRoot+'/src/ontology-driven/work-context.ts');`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
  expect(await Bun.file(canary).exists()).toBe(false);
});

test("transitive canary mutation after observation is withheld by final source readback", async () => {
  const f = await fixture(),
    canary = join(f.fixtureRoot, "TRANSITIVE_CANARY_EXECUTED");
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);
    await Bun.write(f.providerRoot+'/node_modules/@palantirkc/ontology/dist/validation.js',${JSON.stringify(`await Bun.write(${JSON.stringify(canary)},'unsafe');throw Error('TRANSITIVE_CANARY_EXECUTED');`)});
    await proof.verify();await import(f.codeOwnerRoot+'/src/ontology-driven/work-context.ts');`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("Hermes code source changed during packaging");
  expect(await Bun.file(canary).exists()).toBe(false);
});

test("initial mutation of an AJV transitive file cannot be silently recaptured as trusted source", async () => {
  const f = await fixture();
  const path = join(f.providerRoot, "node_modules/ajv/dist/compile/index.js");
  await writeFile(
    path,
    (await readFile(path, "utf8")) + "\nthrow Error('TRANSITIVE_CANARY_EXECUTED');\n",
  );
  const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("EXECUTABLE_DEPENDENCY_CLOSURE_DRIFT");
});

test("dependency closure refuses malformed,missing,extra,foreign and symlinked runtime dependencies", async () => {
  for (const mutation of ["malformed", "missing", "extra", "foreign", "symlink"] as const) {
    const f = await fixture(),
      ajv = join(f.providerRoot, "node_modules/ajv");
    if (mutation === "malformed") await writeFile(join(ajv, "package.json"), "{malformed");
    if (mutation === "missing") await rm(join(ajv, "dist/compile/index.js"));
    if (mutation === "extra")
      await writeFile(join(ajv, "extra.js"), "throw Error('MUST_NOT_IMPORT');");
    if (mutation === "foreign") {
      const foreign = join(f.fixtureRoot, "foreign-ajv");
      await cp(ajv, foreign, { recursive: true });
      await rm(ajv, { recursive: true });
      await symlink(foreign, ajv);
    }
    if (mutation === "symlink") {
      await rm(join(ajv, "dist/compile/index.js"));
      await symlink(join(ajv, "dist/ajv.js"), join(ajv, "dist/compile/index.js"));
    }
    const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain(
      {
        malformed: "SyntaxError",
        missing: "EXECUTABLE_DEPENDENCY_CLOSURE_DRIFT",
        extra: "EXECUTABLE_DEPENDENCY_CLOSURE_DRIFT",
        foreign: "PLATFORM_DEPENDENCY_ESCAPED_OWNER",
        symlink: "UNADMITTED_PLATFORM_DEPENDENCY_SYMLINK",
      }[mutation],
    );
  }
}, 25000);

test("repinning a mutated installed Ontology file cannot replace its immutable release correspondence", async () => {
  const f = await fixture(),
    path = join(f.providerRoot, "node_modules/@palantirkc/ontology/dist/validation.js");
  await writeFile(path, (await readFile(path, "utf8")) + "\nthrow Error('MUST_NOT_IMPORT');\n");
  const lineagePath = join(f.codeOwnerRoot, "source-successor.json");
  const lineage = JSON.parse(await readFile(lineagePath, "utf8"));
  const row = lineage.runtimeDependencies.dependencyPins.find(
    (pin: { path: string }) => pin.path === "node_modules/@palantirkc/ontology/dist/validation.js",
  );
  const bytes = await readFile(path);
  row.sha256 = sha(bytes);
  row.bytes = bytes.length;
  const lineageBytes = JSON.stringify(lineage, null, 2) + "\n";
  await writeFile(lineagePath, lineageBytes);
  const guardPath = join(f.codeOwnerRoot, "src/ontology-driven/provider-source.ts");
  await writeFile(
    guardPath,
    (await readFile(guardPath, "utf8")).replace(
      /const expectedSuccessorSha256 = "[a-f0-9]{64}";/u,
      `const expectedSuccessorSha256 = "${sha(lineageBytes)}";`,
    ),
  );
  const result = await child(f, "await api.observeHermesContextProvider(f.codeOwnerRoot);");
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
});

test("preimported transitive executable dependency requires a fresh source process", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `await import(f.providerRoot+'/node_modules/@palantirkc/ontology/dist/validation.js');
    await api.observeHermesContextProvider(f.codeOwnerRoot);`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("loaded before source observation");
});

test("adding an executable dependency after observation is outside the captured closure", async () => {
  const f = await fixture();
  const result = await child(
    f,
    `const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);
    await Bun.write(f.providerRoot+'/node_modules/ajv/extra.js','throw Error("MUST_NOT_IMPORT");');await proof.verify();`,
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("executable dependency closure changed during read");
});

test("nested dependency lookup directories and links refuse before and after observation", async () => {
  for (const link of [false, true]) {
    for (const after of [false, true]) {
      const f = await fixture();
      const path = join(f.providerRoot, "node_modules/ajv/dist/compile/node_modules");
      const setup = link
        ? `await fs.symlink(f.fixtureRoot,${JSON.stringify(path)});`
        : `await fs.mkdir(${JSON.stringify(path)});`;
      const result = await child(
        f,
        `const fs=await import('node:fs/promises');
        ${after ? "const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);" : ""}
        ${setup}
        ${after ? "await proof.verify();" : "await api.observeHermesContextProvider(f.codeOwnerRoot);"}`,
      );
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain("UNADMITTED_NESTED_DEPENDENCY_ROOT");
    }
  }
}, 25000);

test("all canonical importers refuse package-local Ontology lookup overrides before and after observation", async () => {
  for (const location of ["", "/src/ontology-driven", "/src/adoption"]) {
    for (const after of [false, true]) {
      const f = await fixture();
      const path = join(
        f.providerRoot,
        "packages/native-hermes-adapter" + location,
        "node_modules/@palantirkc/ontology",
      );
      // Inert metadata/code fixtures only: no untrusted module is ever imported.
      const setup = `await fs.mkdir(${JSON.stringify(path)},{recursive:true});
        await fs.writeFile(${JSON.stringify(join(path, "package.json"))},${JSON.stringify(
          JSON.stringify({
            name: "@palantirkc/ontology",
            version: "0.2.0",
            exports: { ".": "./index.js", "./package.json": "./package.json" },
          }),
        )});
        await fs.writeFile(${JSON.stringify(join(path, "index.js"))},'export const sourceFixtureOnly=true;');`;
      const result = await child(
        f,
        `const fs=await import('node:fs/promises');
        ${after ? "const proof=await api.observeHermesContextProvider(f.codeOwnerRoot);" : ""}
        ${setup}
        ${after ? "await proof.verify();" : "await api.observeHermesContextProvider(f.codeOwnerRoot);"}`,
      );
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain("UNADMITTED_ONTOLOGY_LOOKUP_ROOT");
    }
  }
}, 25000);
