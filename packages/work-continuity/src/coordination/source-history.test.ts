import { afterEach, expect, test } from "bun:test";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { sourceFixture, cleanupSourceFixtures } from "./fixtures/source-owner.ts";
import {
  normalizeContent,
  contentDigest,
  locationDigest,
  providerBytes,
  providerDigest,
  sourceHash,
  type SourceBindingInput,
} from "./source-contract.ts";
import {
  materializeSourceDistribution,
  type NativeSourceDistributionOwner,
} from "./source-distribution.ts";
import { withQualifiedSource } from "./source-runtime.ts";
afterEach(cleanupSourceFixtures);
async function git(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-c", "core.hooksPath=/dev/null", "-C", root, ...args], {
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
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [code, out, err] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  if (code) throw Error("SYNTHETIC_SOURCE_GIT_FAILED_" + sourceHash(err));
  return args[0] === "show" ? out : out.trim();
}
test("actual private Git main movement preserves committed audit/dependency release and original source recovery", async () => {
  const f = await sourceFixture();
  await git(f.sourceRoot, "init", "--initial-branch=main");
  await git(f.sourceRoot, "add", "src", "tools");
  await git(f.sourceRoot, "commit", "-m", "Synthetic original source");
  const original = await git(f.sourceRoot, "rev-parse", "HEAD"),
    originalTree = await git(f.sourceRoot, "rev-parse", "HEAD^{tree}");
  async function snapshot() {
    const commit = await git(f.sourceRoot, "rev-parse", "HEAD"),
      tree = await git(f.sourceRoot, "rev-parse", "HEAD^{tree}");
    const files = await Promise.all(
      f.content.files.map(async (pin) => {
        const bytes = await readFile(join(f.sourceRoot, pin.path));
        return { ...pin, bytes: bytes.byteLength, sha256: sourceHash(bytes) };
      }),
    );
    return normalizeContent({ ...f.content, commit, tree, files });
  }
  const owner: NativeSourceDistributionOwner = {
    async withSnapshot(expected, consume) {
      const rootHead = await git(f.sourceRoot, "rev-parse", "HEAD"),
        rootTree = await git(f.sourceRoot, "rev-parse", "HEAD^{tree}");
      return consume({
        sourceRoot: f.sourceRoot,
        targetParent: f.targetParent,
        repositoryId: expected.repositoryId,
        commit: rootHead,
        tree: rootTree,
        async readCommitted(path) {
          return {
            commit: rootHead,
            tree: rootTree,
            path,
            bytes: new TextEncoder().encode(await git(f.sourceRoot, "show", rootHead + ":" + path)),
          };
        },
        async assertCurrent() {
          if (
            (await git(f.sourceRoot, "rev-parse", "HEAD")) !== rootHead ||
            (await git(f.sourceRoot, "rev-parse", "HEAD^{tree}")) !== rootTree
          )
            throw Error("SYNTHETIC_ACTUAL_GIT_DRIFT");
        },
      });
    },
  };
  const contentA = await snapshot(),
    releaseA = await materializeSourceDistribution(contentA, owner);
  const locationA = {
    ...f.binding.location,
    contentDigest: contentDigest(contentA),
    root: releaseA.root,
    kind: "audit" as const,
  };
  const registrationA = {
    ...f.binding.registration,
    contentDigest: contentDigest(contentA),
    locationDigest: locationDigest(locationA),
  };
  const registrationPathA = join(f.root, "bindings", "original-provider.json");
  await writeFile(registrationPathA, providerBytes(registrationA));
  const bindingA: SourceBindingInput = {
    content: contentA,
    location: locationA,
    registration: registrationA,
    registrationPath: registrationPathA,
    mode: "recovery",
    recovery: {
      schema: "common.original-source-recovery.v1",
      providerId: registrationA.id,
      contentDigest: contentDigest(contentA),
      locationDigest: locationDigest(locationA),
      registrationDigest: providerDigest(registrationA),
      originalReceipt: {
        owner: "synthetic-native-source-owner",
        uri: "fixture://retained-source/" + original,
        revision: original,
        sha256: sourceHash(JSON.stringify(releaseA)),
      },
    },
  };
  f.state.recoverySelection = f.selection(bindingA);
  await writeFile(join(f.sourceRoot, "src/api.ts"), 'export const value="source-v2";\n');
  await git(f.sourceRoot, "add", "src/api.ts");
  await git(f.sourceRoot, "commit", "-m", "Synthetic next main");
  const current = await git(f.sourceRoot, "rev-parse", "HEAD");
  expect(current).not.toBe(original);
  const contentB = await snapshot(),
    releaseB = await materializeSourceDistribution(contentB, owner);
  expect(releaseB.contentDigest).not.toBe(releaseA.contentDigest);
  f.state.currentSelection = {
    ...f.state.currentSelection!,
    contentDigest: releaseB.contentDigest,
  };
  f.state.currentGeneration = "new-main-provider";
  expect(
    await withQualifiedSource(
      bindingA,
      f.owner,
      async (source) => (await source.load("api")).value,
    ),
  ).toBe("source-v1");
  expect(bindingA.content.commit).toBe(original);
  expect(bindingA.content.tree).toBe(originalTree);
  const dependency = {
    ...bindingA,
    location: { ...bindingA.location, kind: "dependency" as const },
  };
  dependency.registration = {
    ...dependency.registration,
    locationDigest: locationDigest(dependency.location),
  };
  dependency.registrationPath = join(f.root, "bindings", "original-dependency-provider.json");
  await writeFile(dependency.registrationPath, providerBytes(dependency.registration));
  dependency.recovery = {
    ...dependency.recovery!,
    locationDigest: locationDigest(dependency.location),
    registrationDigest: providerDigest(dependency.registration),
  };
  f.state.recoverySelection = f.selection(dependency);
  expect(
    await withQualifiedSource(
      dependency,
      f.owner,
      async (source) => (await source.load("api")).value,
    ),
  ).toBe("source-v1");
  expect(await git(f.sourceRoot, "rev-parse", "HEAD")).toBe(current);
}, 30000);
