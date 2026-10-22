/** One fresh, isolated synthetic occurrence. Never run on the author Host. */
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir, readdir, access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { sourceFixture, cleanupSourceFixtures } from "./source-owner.ts";
import {
  normalizeContent,
  sourceHash,
  contentDigest,
  locationDigest,
  providerBytes,
  type SourceFilePin,
  type SourceInstalledSelection,
} from "../source-contract.ts";
import {
  materializeSourceDistribution,
  type NativeSourceDistributionOwner,
} from "../source-distribution.ts";
import { withQualifiedSource } from "../source-runtime.ts";

async function git(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-c", "core.hooksPath=/dev/null", "-C", root, ...args], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    env: {
      PATH: "/usr/local/bin:/usr/bin:/bin",
      HOME: root,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Synthetic",
      GIT_AUTHOR_EMAIL: "synthetic@example.invalid",
      GIT_COMMITTER_NAME: "Synthetic",
      GIT_COMMITTER_EMAIL: "synthetic@example.invalid",
    },
  });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  assert.equal(code, 0, "SYNTHETIC_GIT_" + sourceHash(stderr));
  return ["show", "ls-tree"].includes(args[0]!) ? stdout : stdout.trim();
}
const occurrence = process.argv[2]!;
try {
  const f = await sourceFixture();
  const canary = join(f.root, "outside-canary.txt"),
    outside = join(f.targetParent, "outside.cjs");
  const canaryProgram = `const fs=require("node:fs");fs.appendFileSync(${JSON.stringify(canary)},"loaded\\n");Object.defineProperty(exports,"value",{get(){fs.appendFileSync(${JSON.stringify(canary)},"used\\n");return "outside"}});`;
  const dependency =
    occurrence.includes("dependency") ||
    ["dirty-declared", "foreign-root", "overlap-root", "wrong-installed"].includes(occurrence);
  let entry = 'export const value="first-party";';
  const values = new Map<string, string>([["tools/loader.bin", "synthetic-observed-loader-image"]]);
  if (["loader-alias", "loader-precache"].includes(occurrence))
    entry =
      'import { createRequire } from "node:module"; const loadOther=createRequire(import.meta.url); export const value=loadOther("../../outside.cjs").value;';
  if (occurrence.startsWith("loader-comment-")) {
    const kind = occurrence.slice("loader-comment-".length).replace("-precache", "");
    const terminators: Record<string, string> = { cr: "\r", ls: "\u2028", ps: "\u2029" };
    const terminator = terminators[kind];
    assert(terminator, "Explicit line-comment case required");
    entry =
      "// synthetic comment" +
      terminator +
      'import { createRequire } from "node:module"; const loadOther=createRequire(import.meta.url); export const value=loadOther("../../outside.cjs").value;';
  }
  if (occurrence === "loader-namespace")
    entry =
      'import * as capability from "node:module"; const loadOther=capability.createRequire(import.meta.url); export const value=loadOther("../../outside.cjs").value;';
  if (occurrence === "loader-prefix")
    entry = 'const suffix="outside.cjs"; export const value=(await import("../../"+suffix)).value;';
  if (occurrence === "reflection")
    entry =
      'const key="con"+"structor"; const build=({})[key][key]; export const value=build("return 1")();';
  if (occurrence === "vm-alias")
    entry = 'import {runInThisContext as run} from "node:vm"; export const value=run("1");';
  if (occurrence === "free-global")
    entry = 'export const value=ambientNativeLoader("../../outside.cjs").value;';
  if (occurrence.startsWith("positive-static") || occurrence === "dirty-orphan") {
    entry =
      'import { part } from "./helper.ts"; export const value=part; export const metadata={paren:"(",word:"import"}; export function add(a,b){return a+b;}';
    values.set(
      "src/helper.ts",
      'import { basename } from "node:path"; export const part=basename("/synthetic/declared");',
    );
  }
  if (occurrence === "positive-static-comment-lf" || occurrence === "positive-static-comment-crlf")
    entry = "// legitimate static comment" + (occurrence.endsWith("crlf") ? "\r\n" : "\n") + entry;
  if (dependency) {
    entry = 'import { part } from "fixture-dep"; export const value=part;';
    values.set(
      "node_modules/fixture-dep/package.json",
      JSON.stringify({
        name: "fixture-dep",
        version: "1.0.0",
        type: "module",
        exports: "./index.js",
      }),
    );
    values.set("node_modules/fixture-dep/index.js", 'export const part="installed-external";');
  }
  if (occurrence === "positive-tool")
    values.set("tools/external.bin", "independent installed synthetic tool");
  if (occurrence === "orphan-tool") values.set("tools/orphan.bin", "unowned tool label");
  values.set("src/api.ts", entry);
  const installedRoot = join(f.root, "independent-installation");
  for (const [path, text] of values) {
    await mkdir(dirname(join(f.sourceRoot, path)), { recursive: true });
    await writeFile(join(f.sourceRoot, path), text);
    if (path.startsWith("node_modules/") || path === "tools/external.bin") {
      await mkdir(dirname(join(installedRoot, path)), { recursive: true });
      await writeFile(join(installedRoot, path), text);
    }
  }
  await git(f.sourceRoot, "init", "--initial-branch=main");
  for (const path of values.keys())
    if (
      (!path.startsWith("node_modules/") || occurrence === "dirty-declared") &&
      path !== "tools/external.bin"
    )
      await git(f.sourceRoot, "add", "--", path);
  await git(f.sourceRoot, "commit", "-m", "Synthetic source baseline");
  const commit = await git(f.sourceRoot, "rev-parse", "HEAD"),
    tree = await git(f.sourceRoot, "rev-parse", "HEAD^{tree}");
  if (occurrence === "dirty-orphan") {
    values.set("src/helper.ts", 'export const part="dirty-uncommitted";');
    await writeFile(join(f.sourceRoot, "src/helper.ts"), values.get("src/helper.ts")!);
  }
  if (occurrence === "dirty-declared") {
    values.set("node_modules/fixture-dep/index.js", 'export const part="dirty-uncommitted";');
    await writeFile(
      join(f.sourceRoot, "node_modules/fixture-dep/index.js"),
      values.get("node_modules/fixture-dep/index.js")!,
    );
  }
  if (occurrence === "dirty-tool") {
    values.set("tools/loader.bin", "dirty selected-tree tool");
    await writeFile(join(f.sourceRoot, "tools/loader.bin"), values.get("tools/loader.bin")!);
  }
  const files = [...values].map(([path, text]): SourceFilePin => ({
    path,
    bytes: Buffer.byteLength(text),
    sha256: sourceHash(text),
    mode: 0o444,
    class:
      path.startsWith("node_modules/") ||
      (occurrence === "dirty-orphan" && path === "src/helper.ts")
        ? "dependency"
        : path.startsWith("tools/")
          ? "tool"
          : "source",
  }));
  const input = {
    ...f.content,
    commit,
    tree,
    files,
    dependencies: dependency
      ? [
          {
            name: "fixture-dep",
            version: "1.0.0",
            root: "node_modules/fixture-dep",
            manifest: "node_modules/fixture-dep/package.json",
          },
        ]
      : [],
    tools: [
      ...f.content.tools,
      ...(occurrence === "positive-tool"
        ? [{ id: "external-tool", version: "1", path: "tools/external.bin" }]
        : []),
    ],
  };
  if (occurrence === "foreign-root") input.dependencies[0]!.root = "node_modules/foreign";
  if (occurrence === "overlap-root")
    input.dependencies.push({
      name: "nested",
      version: "1",
      root: "node_modules/fixture-dep/nested",
      manifest: "node_modules/fixture-dep/nested/package.json",
    });
  const owner: NativeSourceDistributionOwner = {
    async withSnapshot(expected, consume) {
      return consume({
        sourceRoot: f.sourceRoot,
        targetParent: f.targetParent,
        repositoryId: expected.repositoryId,
        commit,
        tree,
        async assertCurrent() {
          assert.equal(await git(f.sourceRoot, "rev-parse", "HEAD"), commit);
        },
        async readCommitted(path) {
          if (!(await git(f.sourceRoot, "ls-tree", commit, "--", path))) return null;
          return {
            commit,
            tree,
            path,
            bytes: new TextEncoder().encode(await git(f.sourceRoot, "show", commit + ":" + path)),
          };
        },
        async readInstalled(selected) {
          const root = selected.kind === "dependency" ? "node_modules/fixture-dep" : "tools";
          const id = selected.kind === "dependency" ? "fixture-dep" : "external-tool";
          const version = selected.kind === "dependency" ? "1.0.0" : "1";
          const paths = [...values.keys()]
            .filter((p) =>
              selected.kind === "dependency"
                ? p.startsWith(root + "/")
                : p === "tools/external.bin",
            )
            .sort();
          assert(paths.includes(selected.path));
          const inventory = await Promise.all(
            paths.map(async (path): Promise<SourceFilePin> => {
              const bytes = await readFile(join(installedRoot, path));
              return {
                path,
                bytes: bytes.byteLength,
                sha256: sourceHash(bytes),
                mode: 0o444,
                class: selected.kind,
              };
            }),
          );
          const observed: SourceInstalledSelection = {
            kind: selected.kind,
            id,
            version,
            root,
            inventoryDigest: sourceHash(
              JSON.stringify(
                inventory.map((pin) => [pin.path, pin.sha256, pin.bytes, pin.mode, pin.class]),
              ),
            ),
            path: selected.path,
          };
          if (occurrence === "wrong-installed") observed.id = "another-installed-owner";
          return {
            selection: observed,
            provenance: {
              owner: "synthetic-installed-owner",
              uri: "fixture://installation/" + id,
              revision: version,
              sha256: observed.inventoryDigest,
            },
            bytes: await readFile(join(installedRoot, selected.path)),
          };
        },
      });
    },
  };
  const prior = (await readdir(f.targetParent)).sort();
  if (
    [
      "dirty-orphan",
      "dirty-declared",
      "foreign-root",
      "overlap-root",
      "wrong-installed",
      "orphan-tool",
      "dirty-tool",
    ].includes(occurrence)
  ) {
    await assert.rejects(() => materializeSourceDistribution(input, owner));
    assert.deepEqual(
      (await readdir(f.targetParent)).sort(),
      prior,
      "No output or incomplete stage before provenance admission",
    );
  } else {
    const content = normalizeContent(input),
      delivered = await materializeSourceDistribution(content, owner);
    assert.equal(delivered.commit, commit);
    assert.equal(delivered.tree, tree);
    if (occurrence === "positive-dependency" || occurrence === "positive-tool") {
      assert.equal(delivered.externalSources.length, 1);
      assert.equal(delivered.externalSources[0]!.provenance.owner, "synthetic-installed-owner");
      assert.equal(
        delivered.externalSources[0]!.selection.kind,
        dependency ? "dependency" : "tool",
      );
    }
    if (occurrence !== "positive-tool") {
      const location = {
        ...f.binding.location,
        root: delivered.root,
        contentDigest: contentDigest(content),
      };
      const registration = {
        ...f.binding.registration,
        contentDigest: contentDigest(content),
        locationDigest: locationDigest(location),
      };
      const binding = { ...f.binding, content, location, registration };
      await writeFile(binding.registrationPath, providerBytes(registration));
      f.state.currentSelection = f.selection(binding);
      await writeFile(outside, canaryProgram);
      if (occurrence.includes("precache")) createRequire(import.meta.url)(outside);
      const before = await readFile(canary, "utf8").catch(() => "");
      if (occurrence.startsWith("positive")) {
        const value = await withQualifiedSource(binding, f.owner, async (view) => {
          const module = await view.load("api");
          if (occurrence.startsWith("positive-static")) {
            assert.equal((module.add as (a: number, b: number) => number)(2, 3), 5);
            assert.deepEqual(module.metadata, { paren: "(", word: "import" });
          }
          return module.value;
        });
        assert.equal(value, dependency ? "installed-external" : "declared");
      } else {
        await assert.rejects(() =>
          withQualifiedSource(binding, f.owner, (view) => view.load("api")),
        );
        assert.equal(
          await readFile(canary, "utf8").catch(() => ""),
          before,
          "Outside code was neither loaded nor used",
        );
        if (!occurrence.includes("precache")) await assert.rejects(() => access(canary));
      }
    }
  }
  console.log(
    JSON.stringify({ schema: "synthetic.source-correction-case.v1", occurrence, status: "pass" }),
  );
} finally {
  await cleanupSourceFixtures();
}
