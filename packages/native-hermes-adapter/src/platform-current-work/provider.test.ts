import { test, expect } from "bun:test";
import { cp, mkdtemp, mkdir, readFile, writeFile, rm, realpath } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  captureHermesPlatformSource,
  observeHermesPlatformSource,
  platformSourcePaths,
  sourceDigest,
} from "./provider.ts";
import { buildHermesPlatformSource } from "./build.ts";

const source = resolve(import.meta.dir, "../../../..");
async function fixture(importMarker = false) {
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
    };
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
    expect(built.bundles).toHaveLength(4);
    expect(built.transitiveCompilerInputsVerified).toBe(true);
    expect(built.sourceClosureVerifiedBeforeAndAfter).toBe(true);
    expect(built.nativeActivation).toBe(false);
    expect(built.compilerExecution).toBe("held-parent-file-descriptor");
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
    const marker = "SYNTHETIC_PRIVATE_NATIVE_HOOK_79213";
    for (const name of ["native-hook.js", "read-current-work.js", "current-work-native-hook.js"]) {
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
      } else if (name === "read-current-work.js") {
        expect(code).toBe(1);
        expect(error).toBe("CURRENT_WORK_INPUT_OR_SOURCE_FAILED\n");
      } else {
        expect(code).toBe(0);
        expect(JSON.parse(out)).toEqual({});
        expect(error).toBe("NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD\n");
      }
    }
    const invokeNative = async () => {
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
    await writeFile(bindingsPath, bindingsBytes + "\n");
    const mutatedBindings = await invokeNative();
    expect(mutatedBindings.code).toBe(0);
    expect(JSON.parse(mutatedBindings.stdout)).toEqual({});
    expect(mutatedBindings.stderr).toBe("NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD\n");
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
    expect(staleError).toBe("CURRENT_WORK_INPUT_OR_SOURCE_FAILED\n");
    const staleNative = await invokeNative();
    expect(staleNative.code).toBe(0);
    expect(JSON.parse(staleNative.stdout)).toEqual({});
    expect(staleNative.stderr).toBe("NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD\n");
  } finally {
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
    expect(await new Response(p.stderr).text()).toBe("NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD\n");
    expect(await Bun.file(f.markerPath).exists()).toBe(false);
  } finally {
    await f.cleanup();
  }
}, 60000);
