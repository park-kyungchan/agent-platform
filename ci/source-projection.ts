import { strict as assert } from "node:assert";
import { createHash } from "node:crypto";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { validateSourceProviderProjections } from "./verify-runtime-layout";

type RecordValue = Record<string, unknown>;
const record = (value: unknown): RecordValue => {
  assert(
    value && typeof value === "object" && !Array.isArray(value),
    "Source projection record required",
  );
  return value as RecordValue;
};
const hash = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const write = async (path: string, bytes: Uint8Array | string) => {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, bytes);
};

/** Source agreement only. Neither declarations nor fictional QA roots authenticate a native caller. */
export function assertSourceProjectionAgreement(
  policyValue: unknown,
  contextValue: unknown,
  nativeValue: unknown,
): void {
  const policy = record(policyValue),
    context = record(contextValue),
    native = record(nativeValue);
  validateSourceProviderProjections(policy.sourceProviderProjections);
  assert.equal(policy.nativePermissionGranted, false);
  assert.equal(policy.verificationClass, "source-layout-only");
  const projection = record((policy.sourceProviderProjections as unknown[])[0]);
  assert.equal(context.schema, "codex.context-provider-source-closure.v5");
  assert.equal(native.schema, "codex.native-adapter-source-closure.v2");
  for (const registration of [context, native]) {
    assert.equal(registration.consumerRoot, projection.canonicalConsumerRoot);
    assert.equal(registration.providerRoot, projection.canonicalProviderRoot);
    assert.equal(registration.authority, "none");
    assert.equal(registration.nativePermissionGranted, false);
    assert.deepEqual(registration.sourceProjections, policy.sourceProviderProjections);
  }
}

export async function verifyLiveSourceProjectionAgreement(
  common: string,
  owner: string,
): Promise<void> {
  const paths = [
    join(common, "architecture/runtime-filesystem-contract.json"),
    join(owner, "architecture/context-provider.json"),
    join(owner, "architecture/native-codex-provider.json"),
  ];
  const values: unknown[] = [];
  for (const path of paths) {
    const stat = await lstat(path);
    assert(
      (await realpath(path)) === path && stat.isFile() && stat.size <= 2_097_152,
      "Exact bounded source projection metadata required",
    );
    values.push(JSON.parse(await readFile(path, "utf8")) as unknown);
  }
  assertSourceProjectionAgreement(values[0], values[1], values[2]);
}

export interface ProjectionQaInput {
  common: string;
  ownerFixture: string;
  contextPaths: readonly string[];
  nativePaths: readonly string[];
  contextModules: readonly string[];
}

/** Temporary source-only exercise of the real finite guards, retaining a fictional canonical owner. */
export async function verifyProjectedSourceQa(input: ProjectionQaInput) {
  const base = await mkdtemp(join(tmpdir(), "common-source-projection-"));
  try {
    const canonical = join(base, "projects/codex-harness-engineering"),
      canonicalProvider = join(base, "projects/agent-platform"),
      projected = join(base, "code/shared/codex-harness-engineering"),
      projectedProvider = join(base, "code/shared/agent-platform");
    const policy = record(
      JSON.parse(
        await readFile(join(input.common, "architecture/runtime-filesystem-contract.json"), "utf8"),
      ),
    );
    validateSourceProviderProjections(policy.sourceProviderProjections);
    const projection: RecordValue = {
      ...record((policy.sourceProviderProjections as unknown[])[0]),
      canonicalConsumerRoot: canonical,
      canonicalProviderRoot: canonicalProvider,
      consumerRoot: projected,
      providerRoot: projectedProvider,
    };
    const ownerPaths = [
      "src/legacy/harness/owner-context-loader.ts",
      "src/legacy/harness/provider-closure.ts",
      "src/native/provider-closure.ts",
      "src/native/hook-cli.ts",
      ...["hooks", "home-context", "evaluation-ledger"].map((name) => `src/native/${name}.ts`),
    ];
    const declarations = new Map<string, { context: RecordValue; native: RecordValue }>();
    for (const root of [canonical, projected]) {
      const provider = resolve(root, "../agent-platform");
      for (const path of new Set([...input.contextPaths, ...input.nativePaths]))
        await write(join(provider, path), await readFile(join(input.common, path)));
      const project = record(JSON.parse(await readFile(join(provider, "project.json"), "utf8")));
      record(record(project.payload).details).sourceOwnerRoot = canonicalProvider;
      await write(join(provider, "project.json"), JSON.stringify(project));
      for (const path of ownerPaths)
        await write(join(root, path), await readFile(join(input.ownerFixture, path)));
      await write(
        join(root, "package.json"),
        JSON.stringify({
          type: "module",
          dependencies: {
            "@palantirkc/work-continuity": "file:../agent-platform/packages/work-continuity",
            "@palantirkc/native-codex-adapter":
              "file:../agent-platform/packages/native-codex-adapter",
          },
        }),
      );
      await write(join(root, "bun.lock"), "fictional-source-only-owner-lock\n");
      await mkdir(join(root, "node_modules/@palantirkc"), { recursive: true });
      for (const name of ["work-continuity", "native-codex-adapter"])
        await symlink(
          join(provider, "packages", name),
          join(root, "node_modules/@palantirkc", name),
        );
      await mkdir(join(provider, "node_modules/@palantirkc"), { recursive: true });
      // Reuse the already installed immutable dependency and its transitive closure; no QA install or broad dependency copy.
      await symlink(
        join(input.common, "node_modules/@palantirkc/ontology"),
        join(provider, "node_modules/@palantirkc/ontology"),
      );
      const pins = async (paths: readonly string[]) =>
        Promise.all(
          paths.map(async (path) => ({ path, sha256: hash(await readFile(join(provider, path))) })),
        );
      const context = {
        schema: "codex.context-provider-source-closure.v5",
        consumerRoot: canonical,
        providerRoot: canonicalProvider,
        providerProjectId: "agent-platform",
        packageName: "@palantirkc/work-continuity",
        modulePath: "packages/work-continuity/src/context/index.ts",
        gitWorkModulePath: "packages/work-continuity/src/git-work/index.ts",
        sourcePins: await pins(input.contextPaths),
        sourceProjections: [projection],
        authority: "none",
        nativePermissionGranted: false,
      };
      const native = {
        schema: "codex.native-adapter-source-closure.v2",
        consumerRoot: canonical,
        providerRoot: canonicalProvider,
        packageName: "@palantirkc/native-codex-adapter",
        sourcePins: await pins(input.nativePaths),
        sourceProjections: [projection],
        authority: "none",
        nativePermissionGranted: false,
      };
      for (const name of input.contextModules.filter((name) => name !== "index"))
        await write(
          join(root, `src/continuity/${name}.ts`),
          `export * from '@palantirkc/work-continuity/context/${name}';\n`,
        );
      declarations.set(root, { context, native });
      await publish(root, context, native);
    }
    async function publish(root: string, context: RecordValue, native: RecordValue) {
      const cb = JSON.stringify(context) + "\n",
        nb = JSON.stringify(native) + "\n";
      await write(join(root, "architecture/context-provider.json"), cb);
      await write(
        join(root, "src/continuity/index.ts"),
        `// codex-context-provider-registration-sha256: ${hash(cb)}\nexport * from '@palantirkc/work-continuity/context';\n`,
      );
      await write(join(root, "architecture/native-codex-provider.json"), nb);
      const hooks = await readFile(join(input.ownerFixture, "src/native/hooks.ts"), "utf8");
      await write(
        join(root, "src/native/hooks.ts"),
        hooks.replace(
          /^\/\/ codex-native-provider-registration-sha256: [a-f0-9]{64}/,
          "// codex-native-provider-registration-sha256: " + hash(nb),
        ),
      );
    }
    function probe(root: string, preimport = false) {
      const script = `${preimport ? `await import(${JSON.stringify(resolve(root, "../agent-platform/packages/work-continuity/src/context/source.ts"))});` : ""}
const {readNativeAdapterSourceClosure}=await import(${JSON.stringify(join(root, "src/native/provider-closure.ts"))});
const native=await readNativeAdapterSourceClosure();await native.verify();
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(root, "src/legacy/harness/owner-context-loader.ts"))});
const context=await loadCodexOwnerContext();await context.verifyCurrentSourceClosure();
console.log(JSON.stringify({providerRoot:native.providerRoot,canonicalProviderRoot:native.canonicalProviderRoot,sourceLocationId:native.sourceLocationId,contextPins:context.sourcePins.length,nativePins:native.pins.length}));`;
      return Bun.spawnSync([process.execPath, "--no-env-file", "-e", script], {
        cwd: base,
        env: {
          ...process.env,
          HOME: join(base, "unrelated-fictional-home"),
          BUN_OPTIONS: "",
          NODE_OPTIONS: "",
        },
        stdout: "pipe",
        stderr: "pipe",
      });
    }
    const valid = [];
    for (const root of [canonical, projected]) {
      const result = probe(root);
      assert.equal(result.exitCode, 0, result.stderr.toString());
      const value = record(JSON.parse(result.stdout.toString()));
      assert.equal(value.providerRoot, resolve(root, "../agent-platform"));
      assert.equal(value.canonicalProviderRoot, canonicalProvider);
      assert.equal(
        value.sourceLocationId,
        root === canonical ? "canonical-host-source" : projection.id,
      );
      assert.equal(value.contextPins, 34);
      valid.push(value);
    }
    for (const file of ["context-provider.json", "native-codex-provider.json"])
      assert.equal(
        await readFile(join(canonical, "architecture", file), "utf8"),
        await readFile(join(projected, "architecture", file), "utf8"),
      );
    const original = declarations.get(projected)!;
    const mutations: ((value: RecordValue) => void)[] = [
      (value) => {
        value.sourceProjections = [];
      },
      (value) => {
        const rows = value.sourceProjections as unknown[];
        rows.push(structuredClone(rows[0]));
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).id = "automatic-home-fallback";
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).providerRoot = join(
          base,
          "foreign/agent-platform",
        );
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).canonicalProviderRoot = projectedProvider;
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).nativePermissionGranted = true;
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).authority = "execute";
      },
      (value) => {
        record((value.sourceProjections as unknown[])[0]).extra = true;
      },
    ];
    let rejected = 0;
    for (const kind of ["context", "native"] as const)
      for (const mutate of mutations) {
        const next = structuredClone(original);
        mutate(next[kind]);
        await publish(projected, next.context, next.native);
        const result = probe(projected);
        assert.notEqual(result.exitCode, 0);
        assert.match(result.stderr.toString(), /Unregistered source provider location/);
        rejected++;
      }
    await publish(projected, original.context, original.native);
    const foreign = join(base, "foreign/codex-harness-engineering");
    await cp(projected, foreign, { recursive: true, dereference: false });
    let result = probe(foreign);
    assert.notEqual(result.exitCode, 0);
    assert.match(result.stderr.toString(), /Unregistered source provider location/);
    rejected++;
    const projectPath = join(projectedProvider, "project.json"),
      before = await readFile(projectPath);
    const project = record(JSON.parse(before.toString()));
    record(record(project.payload).details).sourceOwnerRoot = projectedProvider;
    await writeFile(projectPath, JSON.stringify(project));
    const changed = structuredClone(original),
      pins = changed.context.sourcePins as { path: string; sha256: string }[];
    pins.find((pin) => pin.path === "project.json")!.sha256 = hash(await readFile(projectPath));
    await publish(projected, changed.context, changed.native);
    result = probe(projected);
    assert.notEqual(result.exitCode, 0);
    assert.match(result.stderr.toString(), /immutable owner\/package\/Ontology/);
    rejected++;
    await writeFile(projectPath, before);
    await publish(projected, original.context, original.native);
    result = probe(projected, true);
    assert.notEqual(result.exitCode, 0);
    assert.match(result.stderr.toString(), /fresh process/);
    rejected++;
    const source = join(projectedProvider, "packages/native-codex-adapter/src/native/hooks.ts"),
      outside = join(base, "foreign.ts");
    await write(outside, await readFile(source));
    await unlink(source);
    await symlink(outside, source);
    result = probe(projected);
    assert.notEqual(result.exitCode, 0);
    assert.match(result.stderr.toString(), /EXACT_NATIVE_SOURCE_REQUIRED/);
    rejected++;
    await unlink(source);
    await writeFile(source, await readFile(outside));
    const driftScript = `import {strict as assert} from 'node:assert';
import {readFile,writeFile} from 'node:fs/promises';
const {readNativeAdapterSourceClosure}=await import(${JSON.stringify(join(projected, "src/native/provider-closure.ts"))});
const native=await readNativeAdapterSourceClosure();
const {loadCodexOwnerContext}=await import(${JSON.stringify(join(projected, "src/legacy/harness/owner-context-loader.ts"))});
const context=await loadCodexOwnerContext();
for(const [path,verify] of [[${JSON.stringify(join(projectedProvider, "packages/work-continuity/src/git-work/source.ts"))},()=>context.verifyCurrentSourceClosure()],
[${JSON.stringify(join(projectedProvider, "packages/native-codex-adapter/src/native/home-context.ts"))},()=>native.verify()]]){
 const bytes=await readFile(path);try{await writeFile(path,Buffer.concat([bytes,Buffer.from('\\n')]));await assert.rejects(verify,/changed|CHANGED/);}finally{await writeFile(path,bytes);}await verify();
}`;
    const drift = Bun.spawnSync([process.execPath, "--no-env-file", "-e", driftScript], {
      cwd: base,
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, BUN_OPTIONS: "", NODE_OPTIONS: "" },
    });
    assert.equal(drift.exitCode, 0, drift.stderr.toString());
    rejected += 2;
    return {
      recordClass: "fictional-source-only-v5-v2-projection-guard-proof",
      valid,
      rejectedCases: rejected,
      canonicalOwnerDistinctFromProjectedPhysicalRoot: canonicalProvider !== projectedProvider,
      identicalSealedRegistrations: true,
      nativePermissionGranted: false,
      nativeModelCalls: 0,
      actualNativeTaskId: null,
      actualNativeRunId: null,
    };
  } finally {
    await rm(base, { recursive: true, force: true });
  }
}
