import { test, expect } from "bun:test";
import { cp, lstat, mkdtemp, mkdir, readFile, writeFile, rm, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  captureHermesPlatformSource,
  observeHermesPlatformSource,
  platformSourcePaths,
  sourceDigest,
} from "./provider.ts";
import { buildHermesPlatformSource } from "./build.ts";
import { syntheticMemoryOwner } from "./tests/memory-caller.fixture.ts";
import * as commonContext from "../../../work-continuity/src/context/index.ts";
import * as commonWorkMethod from "../../../work-continuity/src/work-method.ts";

const source = resolve(import.meta.dir, "../../../..");
test("existing common context API forwards the single canonical work-method implementation", () => {
  for (const name of Object.keys(commonWorkMethod) as (keyof typeof commonWorkMethod)[])
    expect(commonContext[name]).toBe(commonWorkMethod[name]);
});
async function fixture(importMarker = false, memoryImportDrift = false) {
  const root = await mkdtemp(join(tmpdir(), "hermes-platform-source-"));
  for (const path of platformSourcePaths) {
    await mkdir(dirname(join(root, path)), { recursive: true });
    await cp(join(source, path), join(root, path));
  }
  const markerPath = join(root, "synthetic-implementation-imported");
  if (importMarker) {
    const implementation = join(
      root,
      "packages/native-hermes-adapter/src/platform-current-work/hook-context.ts",
    );
    await writeFile(
      implementation,
      (await readFile(implementation, "utf8")) +
        `\nawait Bun.write(${JSON.stringify(markerPath)}, 'implementation imported');\n`,
    );
  }
  if (memoryImportDrift) {
    const implementation = join(root, "packages/work-continuity/src/workspace/recall-request.ts");
    const changed = join(root, "packages/work-continuity/src/workspace/qualification.ts");
    await writeFile(
      implementation,
      (await readFile(implementation, "utf8")) +
        `\nawait Bun.write(${JSON.stringify(markerPath)}, 'synthetic Memory imported');` +
        `\nawait Bun.write(${JSON.stringify(changed)}, ${JSON.stringify((await readFile(changed, "utf8")) + "\n// Synthetic import-time source drift.\n")});\n`,
    );
  }
  await cp(join(source, "node_modules"), join(root, "node_modules"), {
    recursive: true,
    dereference: false,
    verbatimSymlinks: true,
  });
  await cp(
    join(source, "packages/native-hermes-adapter/node_modules"),
    join(root, "packages/native-hermes-adapter/node_modules"),
    { recursive: true, dereference: false, verbatimSymlinks: true },
  );
  if (memoryImportDrift) {
    const canonical = join(root, "packages/work-continuity/src/workspace/recall-request.ts");
    const installed = await realpath(
      Bun.resolveSync(
        "@palantirkc/work-continuity/memory/recall-request",
        join(root, "packages/native-hermes-adapter/src/platform-current-work"),
      ),
    );
    if (installed !== canonical) await cp(canonical, installed);
  }
  const registration = await captureHermesPlatformSource(root),
    path = join(root, "source-registration.json"),
    raw = JSON.stringify(registration, null, 2) + "\n";
  await writeFile(path, raw);
  return {
    root,
    registration,
    path,
    sha256: sourceDigest(raw),
    markerPath,
    cleanup: () => rm(root, { recursive: true, force: true }),
  };
}
test("v2 platform denominator includes transitive contracts, generator, dependencies and separate RuntimeIndex", async () => {
  const f = await fixture();
  try {
    for (const path of [
      "packages/work-continuity/src/runtime-index.ts",
      "packages/work-continuity/src/work-method.ts",
      "packages/work-continuity/src/context/index.ts",
      "packages/work-continuity/src/context/validation.ts",
      "packages/work-continuity/src/context/store.ts",
      "packages/work-continuity/src/context/context.ts",
      "packages/work-continuity/src/context/boundary.ts",
      "packages/native-hermes-adapter/src/platform-current-work/work-method.ts",
      "packages/native-hermes-adapter/src/platform-current-work/read-work-method.ts",
      "packages/work-continuity/src/workspace/recall-request.ts",
      "packages/work-continuity/src/workspace/lexical-recall.ts",
      "packages/work-continuity/src/workspace/qualification.ts",
      "packages/work-continuity/src/context/source.ts",
      "packages/work-continuity/src/context/types.ts",
      "packages/native-hermes-adapter/src/platform-current-work/read-memory.ts",
      "packages/work-continuity/src/git-work/source.ts",
      "packages/work-continuity/src/handoff.ts",
      "packages/native-hermes-adapter/src/platform-rearchitecture/work-identity-contracts.ts",
      "packages/native-hermes-adapter/src/platform-current-work/build.ts",
    ])
      expect(f.registration.sourcePins.some((pin) => pin.path === path)).toBe(true);
    expect(
      f.registration.dependencies.some(
        (p) => p.name === "@palantirkc/ontology" && p.version === "0.2.0",
      ),
    ).toBe(true);
    expect(f.registration.dependencies.some((p) => p.name === "ajv")).toBe(true);
    const expected = { providerRoot: f.root, registrationSha256: f.sha256 };
    const observed = await observeHermesPlatformSource(f.path, expected);
    await observed.verify();
    expect(observed.nativePermissionGranted).toBe(false);
    expect(Object.isFrozen(observed.registration.sourcePins)).toBe(true);
    expect(() => {
      observed.registration.sourcePins[0]!.sha256 = "0".repeat(64);
    }).toThrow();
    expected.registrationSha256 = "0".repeat(64);
    await observed.verify();
    const changed = join(
      f.root,
      "packages/native-hermes-adapter/src/platform-current-work/context.ts",
    );
    await writeFile(
      changed,
      (await readFile(changed, "utf8")) + "\n// Synthetic post-admission drift.\n",
    );
    await expect(observed.verify()).rejects.toThrow("CLOSURE_CHANGED");
    await expect(
      observeHermesPlatformSource(f.path, { providerRoot: f.root, registrationSha256: f.sha256 }),
    ).rejects.toThrow("CLOSURE_DRIFT");
  } finally {
    await f.cleanup();
  }
}, 30000);
test("current Work source guard refuses an omitted or changed common work-method core", async () => {
  const f = await fixture();
  try {
    const path = "packages/work-continuity/src/work-method.ts";
    expect(f.registration.sourcePins.filter((pin) => pin.path === path)).toHaveLength(1);
    const observed = await observeHermesPlatformSource(f.path, {
      providerRoot: f.root,
      registrationSha256: f.sha256,
    });
    const reduced = {
      ...f.registration,
      sourcePins: f.registration.sourcePins.filter((pin) => pin.path !== path),
    };
    const reducedBytes = JSON.stringify(reduced, null, 2) + "\n";
    await writeFile(f.path, reducedBytes);
    await expect(
      observeHermesPlatformSource(f.path, {
        providerRoot: f.root,
        registrationSha256: sourceDigest(reducedBytes),
      }),
    ).rejects.toThrow("CLOSURE_DRIFT");
    await writeFile(f.path, JSON.stringify(f.registration, null, 2) + "\n");
    await writeFile(
      join(f.root, path),
      (await readFile(join(f.root, path), "utf8")) + "\n// Synthetic core drift.\n",
    );
    const installedProjection = f.registration.dependencies.some(
      (dependency) => dependency.name === "@palantirkc/work-continuity",
    );
    await expect(observed.verify()).rejects.toThrow(
      installedProjection ? "INSTALLED_WORKSPACE_PROJECTION_DRIFT" : "CLOSURE_CHANGED",
    );
    await expect(
      observeHermesPlatformSource(f.path, {
        providerRoot: f.root,
        registrationSha256: f.sha256,
      }),
    ).rejects.toThrow(
      installedProjection ? "INSTALLED_WORKSPACE_PROJECTION_DRIFT" : "CLOSURE_DRIFT",
    );
  } finally {
    await f.cleanup();
  }
}, 30000);
test("a real FIFO compiler is refused without waiting for a writer", async () => {
  const f = await fixture();
  try {
    const compiler = join(f.root, "synthetic-compiler.fifo");
    const created = Bun.spawnSync(["mkfifo", "--", compiler], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(created.exitCode).toBe(0);
    const request = {
      registrationPath: f.path,
      registrationSha256: f.sha256,
      providerRoot: f.root,
      compiler: { path: compiler, sha256: "0".repeat(64) },
      legacyManifestSha256: "1".repeat(64),
      runtimeProviderRoot: f.root,
      runtimeRegistrationPath: join(f.root, "runtime-registration.json"),
    };
    const driver = join(f.root, "fifo-compiler-probe.ts");
    await writeFile(
      driver,
      `import {buildHermesPlatformSource} from ${JSON.stringify(join(source, "packages/native-hermes-adapter/src/platform-current-work/build.ts"))};\ntry {await buildHermesPlatformSource(${JSON.stringify(request)});process.exitCode=1;} catch(error) {if(!(error instanceof Error)||error.message!=="BOUNDED_INSTALLED_COMPILER_REQUIRED")throw error;console.log("nonregular_compiler_refused");}\n`,
    );
    const child = Bun.spawn([process.execPath, "--no-env-file", driver], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, 5000);
    try {
      const [exitCode, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect(timedOut).toBe(false);
      expect(exitCode).toBe(0);
      expect(stdout).toBe("nonregular_compiler_refused\n");
      expect(stderr).toBe("");
    } finally {
      clearTimeout(timer);
      if (child.exitCode === null) {
        child.kill("SIGKILL");
        await child.exited;
      }
    }
  } finally {
    await f.cleanup();
  }
}, 30000);
test("first-import loader denies source/dependency drift and a forged reduced denominator", async () => {
  const f = await fixture();
  try {
    const reduced = { ...f.registration, sourcePins: f.registration.sourcePins.slice(1) },
      bytes = JSON.stringify(reduced, null, 2) + "\n";
    await writeFile(f.path, bytes);
    await expect(
      observeHermesPlatformSource(f.path, {
        providerRoot: f.root,
        registrationSha256: sourceDigest(bytes),
      }),
    ).rejects.toThrow("CLOSURE_DRIFT");
    await writeFile(f.path, JSON.stringify(f.registration, null, 2) + "\n");
    const dependency = f.registration.dependencyPins.find((p) => p.path.endsWith("package.json"))!;
    await writeFile(
      join(f.root, dependency.path),
      (await readFile(join(f.root, dependency.path), "utf8")) + "\n",
    );
    await expect(
      observeHermesPlatformSource(f.path, { providerRoot: f.root, registrationSha256: f.sha256 }),
    ).rejects.toThrow("CLOSURE_DRIFT");
  } finally {
    await f.cleanup();
  }
}, 30000);
test("finite bundle compiler covers actual metafile inputs and emits no PC-specific source roots", async () => {
  const f = await fixture();
  try {
    const compiler = await realpath(process.execPath),
      compilerSha = sourceDigest(await readFile(compiler));
    const bindingsPath = join(f.root, "current-work-bindings.json"),
      bindingsBytes = JSON.stringify({
        schema: "hermes.current-git-work-hook-bindings.v1",
        selections: [],
        runtimeIndex: { schema: "native-harness.runtime-index.v1", entries: [] },
      });
    await writeFile(bindingsPath, bindingsBytes);
    const request = {
      registrationPath: f.path,
      registrationSha256: f.sha256,
      providerRoot: f.root,
      compiler: { path: compiler, sha256: compilerSha },
      legacyManifestSha256: "1".repeat(64),
      runtimeProviderRoot: f.root,
      runtimeRegistrationPath: join(f.root, "runtime-registration.json"),
      currentWorkBindings: { runtimePath: bindingsPath, sha256: sourceDigest(bindingsBytes) },
      currentWorkOutputVersion: "v2" as const,
    };
    await expect(
      buildHermesPlatformSource({ ...request, currentWorkOutputVersion: "unknown" as "v2" }),
    ).rejects.toThrow("EXPLICIT_CURRENT_WORK_OUTPUT_VERSION_REQUIRED");
    await expect(
      buildHermesPlatformSource({ ...request, currentWorkMaxChars: 12001 }),
    ).rejects.toThrow("INVALID_CURRENT_WORK_OUTPUT_BUDGET");
    await expect(
      buildHermesPlatformSource({
        ...request,
        currentWorkOutputVersion: "v1",
        currentWorkMetadataEncoding: "metadata.v1",
      }),
    ).rejects.toThrow("EXPLICIT_CURRENT_WORK_METADATA_ENCODING_REQUIRED");
    await expect(
      buildHermesPlatformSource({
        ...request,
        currentWorkOutputVersion: "v1",
        currentWorkMaxChars: 6001,
      }),
    ).rejects.toThrow("INVALID_CURRENT_WORK_OUTPUT_BUDGET");
    await expect(
      buildHermesPlatformSource({
        ...request,
        compiler: { ...request.compiler, sha256: "0".repeat(64) },
      }),
    ).rejects.toThrow("COMPILER_PIN_DRIFT");
    await expect(
      buildHermesPlatformSource({ ...request, runtimeProviderRoot: "/home/synthetic-owner" }),
    ).rejects.toThrow("PC_INDEPENDENT");
    const built = await buildHermesPlatformSource(request);
    expect(built.bundles).toHaveLength(6);
    expect(built.bundles.some((bundle) => bundle.name === "read-work-method.js")).toBe(true);
    expect(built.transitiveCompilerInputsVerified).toBe(true);
    expect(built.sourceClosureVerifiedBeforeAndAfter).toBe(true);
    expect(built.nativeActivation).toBe(false);
    expect(built.currentWorkOutputVersion).toBe("v2");
    expect(built.currentWorkOutputPolicy).toEqual({
      policyVersion: "hermes.current-work-output.v2",
      maxChars: 12000,
      capChars: 12000,
    });
    expect(built.compilerExecution).toBe("held-parent-file-descriptor");
    const metadataBuilt = await buildHermesPlatformSource({
      ...request,
      currentWorkMetadataEncoding: "metadata.v1",
    });
    expect(metadataBuilt.currentWorkMetadataEncoding).toBe("metadata.v1");
    expect(metadataBuilt.currentWorkOutputPolicy.capChars).toBe(12000);
    expect(metadataBuilt.sourceClosureVerifiedBeforeAndAfter).toBe(true);
    expect(metadataBuilt.nativeActivation).toBe(false);
    const admitted = new Set(
      [...f.registration.sourcePins, ...f.registration.dependencyPins].map((p) => p.path),
    );
    for (const bundle of built.bundles) {
      expect(bundle.inputs.every((path) => admitted.has(path))).toBe(true);
      expect(Buffer.from(bundle.bytes).includes("/home/")).toBe(false);
      expect(Buffer.from(bundle.bytes).includes("/mnt/")).toBe(false);
      await writeFile(join(f.root, bundle.name), bundle.bytes);
    }
    await writeFile(request.runtimeRegistrationPath, built.nativeRegistration.bytes);
    const memory = await syntheticMemoryOwner();
    const invokeMemory = async (payload: unknown) => {
      const child = Bun.spawn([compiler, "--no-env-file", join(f.root, "read-memory.js")], {
        stdin: new Blob([JSON.stringify(payload)]),
        stdout: "pipe",
        stderr: "pipe",
      });
      const [code, out, error] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      return { code, out, error };
    };
    try {
      const actual = await invokeMemory(memory.payload);
      expect(actual.code).toBe(0);
      expect(actual.error).toBe("");
      const recalled = JSON.parse(actual.out);
      expect(recalled.schema).toBe(
        "workspace-architecture.lexical-recall-git-dataset-reference-result.v1",
      );
      expect(recalled.hits).toHaveLength(1);
      expect(recalled.hits[0].text).toBe("Use Git");
      expect(recalled.hits[0].status).toBe("unconfirmed-source-attributed-inference");
      expect(recalled.selectedValue).toBe("Use Git");
      expect(recalled.selectionClass).toBe("current-authoritative-record");
      expect(recalled.gitWork.admittedCommit).toBe(memory.payload.snapshot.commit);
      expect(recalled.gitWork.observedHeadAfter).toBe(memory.payload.snapshot.commit);
      expect(recalled.gitWork.policyRevision).toBe("synthetic-policy");
      expect(recalled.gitWork.activeGenerationId).toBe("synthetic-generation");
      expect(recalled.nativePermissionGranted).toBe(false);
      expect(recalled.index).toEqual({
        kind: "sqlite-fts5",
        lifetime: "one-in-memory-query",
        disposed: true,
      });
      for (const override of [
        { state: "unknown" },
        { policyRevision: "stale-policy" },
        { activeGenerationId: "stale-generation" },
      ]) {
        const rejected = await invokeMemory({
          ...memory.payload,
          ownerSelection: { ...memory.payload.ownerSelection, ...override },
        });
        expect(rejected.code).toBe(1);
        expect(rejected.out).toBe("");
        expect(JSON.parse(rejected.error)).toEqual({
          error: "MEMORY_INPUT_OR_SOURCE_FAILED",
          stage: "memory",
          reason: "MEMORY_QUERY_FAILED",
        });
      }
      const badPin = await invokeMemory({
        ...memory.payload,
        requestRef: { ...memory.payload.requestRef, sha256: "0".repeat(64) },
      });
      expect(badPin.code).toBe(1);
      expect(badPin.out).toBe("");
      expect(JSON.parse(badPin.error).stage).toBe("memory");
      const unknownField = await invokeMemory({
        ...memory.payload,
        token: "SYNTHETIC_PRIVATE_MEMORY_VALUE",
      });
      expect(unknownField.code).toBe(1);
      expect(unknownField.out).toBe("");
      expect(JSON.parse(unknownField.error).stage).toBe("input");
      expect(unknownField.error).not.toContain("SYNTHETIC_PRIVATE_MEMORY_VALUE");
      await memory.advance();
      const advanced = await invokeMemory(memory.payload);
      expect(advanced.code).toBe(1);
      expect(advanced.out).toBe("");
      expect(JSON.parse(advanced.error).stage).toBe("memory");
    } finally {
      await memory.cleanup();
    }
    const marker = "SYNTHETIC_PRIVATE_NATIVE_HOOK_79213";
    for (const name of [
      "native-hook.js",
      "read-current-work.js",
      "current-work-native-hook.js",
      "read-memory.js",
      "read-work-method.js",
    ]) {
      const p = Bun.spawn([compiler, "--no-env-file", join(f.root, name)], {
        stdin: new Blob([marker]),
        stdout: "pipe",
        stderr: "pipe",
      });
      const [code, out, error] = await Promise.all([
        p.exited,
        new Response(p.stdout).text(),
        new Response(p.stderr).text(),
      ]);
      expect(out + error).not.toContain(marker);
      if (name === "native-hook.js") {
        expect(code).toBe(0);
        expect(JSON.parse(out)).toEqual({});
        expect(JSON.parse(error).error).toBe("NATIVE_BOUNDARY_INPUT_OR_SOURCE_FAILED");
      } else if (name === "read-work-method.js") {
        expect(code).toBe(1);
        expect(JSON.parse(out)).toEqual({
          schema: "hermes.work-method-owner-reader-held.v1",
          status: "held",
          holdReasons: ["WORK_METHOD_INPUT_OR_SOURCE_HELD"],
          nativeAuthenticationEstablished: false,
          nativePermissionGranted: false,
          executionAuthorityGranted: false,
        });
        expect(JSON.parse(error)).toEqual({
          error: "CURRENT_WORK_INPUT_OR_SOURCE_FAILED",
          stage: "input",
          reason: "REQUEST_UNAVAILABLE_OR_INVALID",
        });
      } else if (name === "read-current-work.js" || name === "read-memory.js") {
        expect(code).toBe(1);
        expect(JSON.parse(error)).toEqual({
          error:
            name === "read-memory.js"
              ? "MEMORY_INPUT_OR_SOURCE_FAILED"
              : "CURRENT_WORK_INPUT_OR_SOURCE_FAILED",
          stage: "input",
          reason: "REQUEST_UNAVAILABLE_OR_INVALID",
        });
      } else {
        expect(code).toBe(0);
        expect(JSON.parse(out)).toEqual({});
        expect(JSON.parse(error)).toEqual({
          error: "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
          stage: "input",
          reason: "REQUEST_UNAVAILABLE_OR_INVALID",
        });
      }
    }
    const invokeNative = async (payloadOverride: Record<string, unknown> = {}) => {
      const p = Bun.spawn(
        [compiler, "--no-env-file", join(f.root, "current-work-native-hook.js")],
        {
          stdin: new Blob([
            JSON.stringify({
              hook_event_name: "pre_llm_call",
              profile: "synthetic-profile",
              session_id: "synthetic-session",
              cwd: "/synthetic/cwd",
              extra: { task_id: "actual-native-task", user_message: marker },
              ...payloadOverride,
            }),
          ]),
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      return {
        code: await p.exited,
        stdout: await new Response(p.stdout).text(),
        stderr: await new Response(p.stderr).text(),
      };
    };
    const unselected = await invokeNative();
    expect(unselected.code).toBe(0);
    expect(JSON.parse(unselected.stdout)).toEqual({});
    expect(unselected.stderr).toBe("");
    const invalidProtocol = await invokeNative({ profile: "", session_id: marker });
    expect(invalidProtocol.code).toBe(0);
    expect(JSON.parse(invalidProtocol.stdout)).toEqual({});
    expect(JSON.parse(invalidProtocol.stderr)).toEqual({
      error: "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
      stage: "context",
      reason: "CONTEXT_PROJECTION_FAILED",
    });
    expect(invalidProtocol.stdout + invalidProtocol.stderr).not.toContain(marker);
    await writeFile(bindingsPath, bindingsBytes + "\n");
    const mutatedBindings = await invokeNative();
    expect(mutatedBindings.code).toBe(0);
    expect(JSON.parse(mutatedBindings.stdout)).toEqual({});
    expect(JSON.parse(mutatedBindings.stderr)).toEqual({
      error: "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
      stage: "bindings",
      reason: "BINDINGS_UNAVAILABLE_OR_INVALID",
    });
    expect(mutatedBindings.stdout + mutatedBindings.stderr).not.toContain(marker);
    await writeFile(bindingsPath, bindingsBytes);
    const held = Bun.spawn([compiler, "--no-env-file", join(f.root, "read-current-work.js")], {
      stdin: new Blob([JSON.stringify({ event: null, selection: null, runtimeIndex: null })]),
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, out, error] = await Promise.all([
      held.exited,
      new Response(held.stdout).text(),
      new Response(held.stderr).text(),
    ]);
    expect(code).toBe(0);
    expect(error).toBe("");
    expect(JSON.parse(out).status).toBe("held");
    expect(JSON.parse(out).holdReasons).toContain("EXPLICIT_NATIVE_WORK_SELECTION_REQUIRED");
    expect(JSON.parse(out).schema).toBe("hermes.current-git-work-context.v1");
    const explicitV2 = Bun.spawn(
      [compiler, "--no-env-file", join(f.root, "read-current-work.js")],
      {
        stdin: new Blob([
          JSON.stringify({ event: null, selection: null, runtimeIndex: null, outputVersion: "v2" }),
        ]),
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const [v2Code, v2Out, v2Error] = await Promise.all([
      explicitV2.exited,
      new Response(explicitV2.stdout).text(),
      new Response(explicitV2.stderr).text(),
    ]);
    expect(v2Code).toBe(0);
    expect(v2Error).toBe("");
    expect(JSON.parse(v2Out).schema).toBe("hermes.current-git-work-context.v2");
    expect(JSON.parse(v2Out).context).toBeNull();
    await writeFile(
      join(f.root, "packages/work-continuity/src/runtime-index.ts"),
      "// Synthetic source replacement after compile\n",
    );
    const stale = Bun.spawn([compiler, "--no-env-file", join(f.root, "read-current-work.js")], {
      stdin: new Blob([JSON.stringify({ event: null, selection: null, runtimeIndex: null })]),
      stdout: "pipe",
      stderr: "pipe",
    });
    const [staleCode, staleOut, staleError] = await Promise.all([
      stale.exited,
      new Response(stale.stdout).text(),
      new Response(stale.stderr).text(),
    ]);
    expect(staleCode).toBe(1);
    expect(staleOut).toBe("");
    expect(JSON.parse(staleError)).toEqual({
      error: "CURRENT_WORK_INPUT_OR_SOURCE_FAILED",
      stage: "source",
      reason: "SOURCE_CLOSURE_UNAVAILABLE_OR_DRIFTED",
    });
    const staleMemory = await invokeMemory({
      snapshot: null,
      requestRef: null,
      ownerSelection: null,
    });
    expect(staleMemory.code).toBe(1);
    expect(staleMemory.out).toBe("");
    expect(JSON.parse(staleMemory.error)).toEqual({
      error: "MEMORY_INPUT_OR_SOURCE_FAILED",
      stage: "source",
      reason: "SOURCE_CLOSURE_UNAVAILABLE_OR_DRIFTED",
    });
    const staleNative = await invokeNative();
    expect(staleNative.code).toBe(0);
    expect(JSON.parse(staleNative.stdout)).toEqual({});
    expect(JSON.parse(staleNative.stderr)).toEqual({
      error: "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
      stage: "source",
      reason: "SOURCE_CLOSURE_UNAVAILABLE_OR_DRIFTED",
    });
  } finally {
    await f.cleanup();
  }
}, 60000);

test("compiled Memory caller brackets implementation import and query with exact closure proof", async () => {
  const f = await fixture(false, true);
  const memory = await syntheticMemoryOwner();
  try {
    const compiler = await realpath(process.execPath);
    const built = await buildHermesPlatformSource({
      registrationPath: f.path,
      registrationSha256: f.sha256,
      providerRoot: f.root,
      compiler: { path: compiler, sha256: sourceDigest(await readFile(compiler)) },
      legacyManifestSha256: "1".repeat(64),
      runtimeProviderRoot: f.root,
      runtimeRegistrationPath: join(f.root, "runtime-registration.json"),
    });
    const bundle = built.bundles.find((item) => item.name === "read-memory.js")!;
    await writeFile(join(f.root, bundle.name), bundle.bytes);
    await writeFile(join(f.root, "runtime-registration.json"), built.nativeRegistration.bytes);
    const invoke = async () => {
      const child = Bun.spawn([compiler, "--no-env-file", join(f.root, bundle.name)], {
        stdin: new Blob([JSON.stringify(memory.payload)]),
        stdout: "pipe",
        stderr: "pipe",
      });
      const [code, out, error] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      return { code, out, error };
    };
    // First guard passes. The admitted synthetic implementation mutates a pinned
    // source at import; a valid query still cannot publish its result afterward.
    const after = await invoke();
    expect(after.code).toBe(1);
    expect(after.out).toBe("");
    expect(JSON.parse(after.error)).toEqual({
      error: "MEMORY_INPUT_OR_SOURCE_FAILED",
      stage: "source-readback",
      reason: "SOURCE_CLOSURE_READBACK_FAILED",
    });
    expect(await readFile(f.markerPath, "utf8")).toBe("synthetic Memory imported");
    await rm(f.markerPath);
    // The same changed closure is now refused before implementation import.
    const before = await invoke();
    expect(before.code).toBe(1);
    expect(before.out).toBe("");
    expect(JSON.parse(before.error).stage).toBe("source");
    expect(await Bun.file(f.markerPath).exists()).toBe(false);
  } finally {
    await memory.cleanup();
    await f.cleanup();
  }
}, 60000);

test("compiled native hook refuses a changed registration before the current Work implementation executes", async () => {
  const f = await fixture(true);
  try {
    const compiler = await realpath(process.execPath),
      bindingsPath = join(f.root, "native-bindings.json"),
      bindingsBytes = JSON.stringify({
        schema: "hermes.current-git-work-hook-bindings.v1",
        selections: [],
        runtimeIndex: { schema: "native-harness.runtime-index.v1", entries: [] },
      });
    await writeFile(bindingsPath, bindingsBytes);
    const runtimeRegistrationPath = join(f.root, "native-registration.json");
    const built = await buildHermesPlatformSource({
      registrationPath: f.path,
      registrationSha256: f.sha256,
      providerRoot: f.root,
      compiler: { path: compiler, sha256: sourceDigest(await readFile(compiler)) },
      legacyManifestSha256: "1".repeat(64),
      runtimeProviderRoot: f.root,
      runtimeRegistrationPath,
      currentWorkBindings: { runtimePath: bindingsPath, sha256: sourceDigest(bindingsBytes) },
    });
    const native = built.bundles.find((bundle) => bundle.name === "current-work-native-hook.js")!;
    const path = join(f.root, native.name);
    await writeFile(path, native.bytes);
    await writeFile(
      runtimeRegistrationPath,
      Buffer.from(built.nativeRegistration.bytes).toString() + "\n",
    );
    const p = Bun.spawn([compiler, "--no-env-file", path], {
      stdin: new Blob([
        JSON.stringify({
          hook_event_name: "pre_llm_call",
          profile: "synthetic",
          session_id: "synthetic",
          cwd: "/synthetic",
        }),
      ]),
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(await p.exited).toBe(0);
    expect(JSON.parse(await new Response(p.stdout).text())).toEqual({});
    expect(JSON.parse(await new Response(p.stderr).text())).toEqual({
      error: "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
      stage: "source",
      reason: "SOURCE_CLOSURE_UNAVAILABLE_OR_DRIFTED",
    });
    expect(await Bun.file(f.markerPath).exists()).toBe(false);
  } finally {
    await f.cleanup();
  }
}, 60000);

test("actual native Work importer rejects a stale same-export projection before compiler inspection", async () => {
  const f = await fixture();
  try {
    const dependency = f.registration.dependencies.find(
      (entry) => entry.name === "@palantirkc/work-continuity",
    );
    expect(dependency).toBeDefined();
    const imported = await realpath(
      Bun.resolveSync(
        "@palantirkc/work-continuity/git-work",
        join(f.root, "packages/native-hermes-adapter/src/platform-current-work"),
      ),
    );
    expect(imported).toBe(join(f.root, dependency!.root, "src/git-work/index.ts"));
    const installed = join(f.root, dependency!.root, "src/git-work/context.ts");
    expect((await lstat(installed)).nlink).toBe(1);
    const fresh = await readFile(installed, "utf8");
    expect(fresh).toContain("criteria: record.criteria");
    // Same exported API, different normative behavior: stale installed code silently drops criteria.
    await writeFile(installed, fresh.replace("criteria: record.criteria", "criteria: []"));
    expect(
      await readFile(join(f.root, "packages/work-continuity/src/git-work/context.ts"), "utf8"),
    ).toBe(fresh);
    await expect(captureHermesPlatformSource(f.root)).rejects.toThrow(
      "INSTALLED_WORKSPACE_PROJECTION_DRIFT",
    );
    await expect(
      buildHermesPlatformSource({
        registrationPath: f.path,
        registrationSha256: f.sha256,
        providerRoot: f.root,
        compiler: { path: join(f.root, "compiler-must-not-be-inspected"), sha256: "0".repeat(64) },
        legacyManifestSha256: "1".repeat(64),
        runtimeProviderRoot: f.root,
        runtimeRegistrationPath: join(f.root, "runtime-registration.json"),
      }),
    ).rejects.toThrow("INSTALLED_WORKSPACE_PROJECTION_DRIFT");
  } finally {
    await f.cleanup();
  }
}, 30000);

test("native importer readback rejects an unadmitted lookup root even after resolver priming", async () => {
  const f = await fixture();
  try {
    const importedFrom = join(f.root, "packages/native-hermes-adapter/src/platform-current-work");
    const dependency = f.registration.dependencies.find(
      (entry) => entry.name === "@palantirkc/work-continuity",
    )!;
    Bun.resolveSync("@palantirkc/work-continuity/git-work", importedFrom);
    const shadow = join(importedFrom, "node_modules/@palantirkc/work-continuity");
    await mkdir(dirname(shadow), { recursive: true });
    await cp(join(f.root, dependency.root), shadow, {
      recursive: true,
      dereference: false,
      verbatimSymlinks: true,
    });
    await expect(captureHermesPlatformSource(f.root)).rejects.toThrow(
      "UNADMITTED_WORKSPACE_LOOKUP_ROOT",
    );
  } finally {
    await f.cleanup();
  }
}, 30000);
