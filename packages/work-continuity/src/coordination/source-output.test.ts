import { afterEach, test, expect } from "bun:test";
import {
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  symlink,
  writeFile,
  access,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import {
  compileSourceBootstrap,
  bootstrapInputs,
  type NativeBootstrapBuildOwner,
  type BootstrapBuildRequest,
} from "./source-bootstrap.ts";
import { sourceHash } from "./source-contract.ts";
import { sourceFixture, cleanupSourceFixtures } from "./fixtures/source-owner.ts";
afterEach(cleanupSourceFixtures);

async function buildRequest(outputRoot: string): Promise<BootstrapBuildRequest> {
  const sourceRoot = new URL("../../../../", import.meta.url).pathname.replace(/\/$/u, ""),
    compilerPath = await realpath(process.execPath);
  return {
    sourceRoot,
    outputRoot,
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
}
function ownerWithCheck(
  check: (count: number) => void | Promise<void> = () => {},
): NativeBootstrapBuildOwner {
  return {
    async withBuild(request, consume) {
      let count = 0;
      return consume({
        output: {
          root: request.outputRoot,
          parent: dirname(request.outputRoot),
          exclusiveWriter: true,
        },
        async assertCurrent() {
          await check(++count);
        },
      });
    },
  };
}
test("symlinked bootstrap parent rejects before creating anything in its target", async () => {
  const f = await sourceFixture(),
    target = join(f.root, "physical-target"),
    alias = join(f.root, "alias");
  await mkdir(target);
  await symlink(target, alias);
  const request = await buildRequest(join(alias, "new-build"));
  await expect(compileSourceBootstrap(request, ownerWithCheck())).rejects.toThrow();
  expect(await readdir(target)).toEqual([]);
});
test("held bootstrap output parent detects physical replacement before the first mkdir", async () => {
  const f = await sourceFixture(),
    parent = join(f.root, "output-parent"),
    retained = parent + "-retained";
  await mkdir(parent);
  const request = await buildRequest(join(parent, "build"));
  await expect(
    compileSourceBootstrap(
      request,
      ownerWithCheck(async (count) => {
        if (count === 1) {
          await rename(parent, retained);
          await mkdir(parent);
        }
      }),
    ),
  ).rejects.toThrow("PARENT_OR_DIRECTORY_CHANGED");
  expect(await readdir(parent)).toEqual([]);
  expect(await readdir(retained)).toEqual([]);
});
test("new output directory identity is held before a compiler can write artifacts", async () => {
  const f = await sourceFixture(),
    parent = join(f.root, "output-parent");
  await mkdir(parent);
  const request = await buildRequest(join(parent, "build")),
    retained = request.outputRoot + "-retained";
  await expect(
    compileSourceBootstrap(
      request,
      ownerWithCheck(async (count) => {
        if (count === 2) {
          await rename(request.outputRoot, retained);
          await mkdir(request.outputRoot);
        }
      }),
    ),
  ).rejects.toThrow("INCOMPLETE_RECONCILE_NO_RETRY");
  expect(await readdir(request.outputRoot)).toEqual([]);
  expect(await readdir(retained)).toEqual([]);
});
test("real compiler output remains retained at its held directory when the parent name drifts; no aliased proof write", async () => {
  const f = await sourceFixture(),
    parent = join(f.root, "output-parent"),
    retained = parent + "-retained",
    redirect = join(f.root, "redirect-target");
  await mkdir(parent);
  await mkdir(redirect);
  const request = await buildRequest(join(parent, "build"));
  await expect(
    compileSourceBootstrap(
      request,
      ownerWithCheck(async (count) => {
        if (count === 3) {
          await rename(parent, retained);
          await symlink(redirect, parent);
        }
      }),
    ),
  ).rejects.toThrow("INCOMPLETE_RECONCILE_NO_RETRY");
  expect(await readdir(redirect)).toEqual([]);
  expect(
    (await readFile(join(retained, "build", "source-bootstrap.mjs"))).byteLength,
  ).toBeGreaterThan(0);
  await expect(access(join(retained, "build", "source-bootstrap.proof.json"))).rejects.toThrow();
}, 30000);
test.each(["empty", "incomplete"])(
  "existing %s bootstrap output is never reused or removed",
  async (state) => {
    const f = await sourceFixture(),
      output = join(f.root, "existing-build");
    await mkdir(output);
    if (state === "incomplete")
      await writeFile(join(output, "original-occurrence.txt"), "retain exact original occurrence");
    const before = await readdir(output),
      request = await buildRequest(output);
    await expect(compileSourceBootstrap(request, ownerWithCheck())).rejects.toThrow(
      "EXISTS_RECONCILE_NO_RETRY",
    );
    expect(await readdir(output)).toEqual(before);
    if (state === "incomplete")
      expect(await readFile(join(output, before[0]!), "utf8")).toBe(
        "retain exact original occurrence",
      );
  },
);
test("native build owner must hold the exact exclusive output writer independently of JSON", async () => {
  const f = await sourceFixture(),
    request = await buildRequest(join(f.root, "new-build"));
  const owner: NativeBootstrapBuildOwner = {
    async withBuild(selected, consume) {
      return consume({
        output: {
          root: selected.outputRoot,
          parent: dirname(selected.outputRoot),
          exclusiveWriter: false as unknown as true,
        },
        assertCurrent() {},
      });
    },
  };
  await expect(compileSourceBootstrap(request, owner)).rejects.toThrow(
    "EXCLUSIVE_BUILD_OUTPUT_REQUIRED",
  );
  await expect(access(request.outputRoot)).rejects.toThrow();
});
