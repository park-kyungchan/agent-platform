import { afterEach, test, expect } from "bun:test";
import { readFile, realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";
import {
  compileSourceBootstrap,
  bootstrapInputs,
  type NativeBootstrapBuildOwner,
} from "./source-bootstrap.ts";
import { prepareSourceProviderSuccessors } from "./source-successors.ts";
import { sourceHash } from "./source-contract.ts";
import { sourceFixture, cleanupSourceFixtures } from "./fixtures/source-owner.ts";
afterEach(cleanupSourceFixtures);

test("strict source contracts, native adapters and both owner command consumers typecheck together", () => {
  const root = new URL("../../../../", import.meta.url).pathname,
    config = ts.readConfigFile(join(root, "ci/quality/tsconfig.json"), ts.sys.readFile),
    parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
  const paths = [
    ...ts.sys.readDirectory(
      join(root, "packages/work-continuity/src/coordination"),
      [".ts"],
      [],
      ["**/*.ts"],
    ),
    join(root, "packages/native-codex-adapter/src/native/source-v1.ts"),
    join(root, "packages/native-hermes-adapter/src/source-provider/binding-v1.ts"),
    join(root, "scripts/source-distribution.ts"),
    join(root, "scripts/source-binding.ts"),
  ];
  const diagnostics = ts
    .getPreEmitDiagnostics(ts.createProgram(paths, parsed.options))
    .filter((row) => row.category === ts.DiagnosticCategory.Error);
  for (const row of diagnostics.slice(0, 32)) {
    const pos =
      row.file && row.start !== undefined
        ? row.file.getLineAndCharacterOfPosition(row.start)
        : undefined;
    console.error(
      `SOURCE_BINDING_TYPECHECK ${row.file?.fileName ?? "unknown"}:${(pos?.line ?? 0) + 1}:${(pos?.character ?? 0) + 1} TS${row.code}`,
    );
  }
  expect(diagnostics.length).toBe(0);
}, 30000);
test("fixed standalone bootstrap is compiled from canonical input bytes and finite compiler closure", async () => {
  const f = await sourceFixture(),
    sourceRoot = new URL("../../../../", import.meta.url).pathname.replace(/\/$/u, ""),
    compilerPath = await realpath(process.execPath);
  const request = {
    sourceRoot,
    outputRoot: join(f.root, "bootstrap"),
    compiler: {
      path: compilerPath,
      sha256: sourceHash(await readFile(compilerPath)),
      version: Bun.version,
    },
    inputs: await Promise.all(
      bootstrapInputs.map(async (path) => ({
        path,
        sha256: sourceHash(await readFile(join(sourceRoot, path))),
      })),
    ),
  };
  const owner: NativeBootstrapBuildOwner = {
    async withBuild(selected, consume) {
      expect(selected).toEqual(request);
      return consume({
        output: {
          root: selected.outputRoot,
          parent: join(selected.outputRoot, ".."),
          exclusiveWriter: true,
        },
        assertCurrent() {},
      });
    },
  };
  const proof = await compileSourceBootstrap(request, owner);
  expect(proof.output.sha256).toBe(
    sourceHash(await readFile(join(request.outputRoot, proof.output.path))),
  );
  const module = await import(pathToFileURL(join(request.outputRoot, proof.output.path)).href);
  expect(typeof module.withQualifiedSource).toBe("function");
  expect(typeof module.normalizeBinding).toBe("function");
  expect(proof.nativePermissionGranted).toBe(false);
  await expect(compileSourceBootstrap(request, owner)).rejects.toThrow();
}, 30000);
test("all three source registration and marker successors form one exact content set with separate locations", async () => {
  const ids = ["codex-context", "codex-native-source", "hermes-context-source"] as const,
    fixtures = await Promise.all(ids.map((provider) => sourceFixture({ provider })));
  const before = fixtures.map((f, i) => {
    const utf8 = "// provider-registration-sha256: " + "a".repeat(64) + "\n";
    return {
      id: ids[i]!,
      registration: { path: f.binding.registrationPath, sha256: "a".repeat(64) },
      markers: [{ path: join(f.root, "source-marker.ts"), sha256: sourceHash(utf8), utf8 }],
    };
  });
  const proposal = prepareSourceProviderSuccessors(
    fixtures.map((f) => f.binding),
    before,
  );
  expect(proposal.providers).toHaveLength(3);
  expect(new Set(proposal.providers.map((row) => row.contentDigest)).size).toBe(1);
  expect(new Set(proposal.providers.map((row) => row.locationDigest)).size).toBe(3);
  for (const provider of proposal.providers) {
    expect(sourceHash(provider.registration.utf8)).toBe(provider.registration.nextSha256);
    expect(sourceHash(provider.markers[0]!.utf8)).toBe(provider.markers[0]!.nextSha256);
  }
  expect(proposal.adoptionEstablished).toBe(false);
  expect(() => prepareSourceProviderSuccessors([fixtures[0]!.binding], before)).toThrow(
    "EXACT_THREE",
  );
  const wrong = structuredClone(before);
  wrong[0]!.markers[0]!.utf8 += "unqualified";
  expect(() =>
    prepareSourceProviderSuccessors(
      fixtures.map((f) => f.binding),
      wrong,
    ),
  ).toThrow("PREDECESSOR_MISMATCH");
});
test("fresh public routes expose current immutable-source APIs without expanding historical facades", async () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const script =
    "import {strict as assert} from 'node:assert';import * as source from '@palantirkc/work-continuity/source/v1';import * as codex from '@palantirkc/native-codex-adapter/source/v1';import * as hermes from '@palantirkc/native-hermes-adapter/source-provider/v1';import * as old from '@palantirkc/work-continuity/git-work';assert.equal(typeof source.withQualifiedSource,'function');assert.equal(typeof codex.withCodexSource,'function');assert.equal(typeof hermes.withHermesSource,'function');assert.equal('withQualifiedSource' in old,false);console.log('source-public-route-pass');";
  const child = Bun.spawn(
    [process.execPath, "--no-env-file", "--no-install", "--no-macros", "--no-addons", "-e", script],
    { cwd: root, stdin: "ignore", stdout: "pipe", stderr: "pipe" },
  );
  const out = await new Response(child.stdout).text(),
    err = await new Response(child.stderr).text();
  expect(await child.exited, sourceHash(err)).toBe(0);
  expect(out.trim()).toBe("source-public-route-pass");
});
test("command JSON alone never supplies a native source read/write/build owner", async () => {
  const { runSourceDistribution } = await import("../../../../scripts/source-distribution.ts"),
    { runSourceBinding } = await import("../../../../scripts/source-binding.ts");
  await expect(runSourceDistribution(["materialize", "not-read.json"])).rejects.toThrow(
    "NATIVE_OWNER_REQUIRED",
  );
  await expect(runSourceDistribution(["bootstrap", "not-read.json"])).rejects.toThrow(
    "NATIVE_OWNER_REQUIRED",
  );
  await expect(runSourceBinding(["verify", "not-read.json"])).rejects.toThrow(
    "NATIVE_OWNER_REQUIRED",
  );
});
