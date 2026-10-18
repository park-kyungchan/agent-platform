import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { validateCompatibilityForwarder } from "./compatibility";
import { compareGeneratedTrees, verifyOntologyBuild } from "./ontology-build";
import { readQualityScope, readTrackedMarkdown } from "./scope";
import { captureQualitySources, compareQualitySources } from "./source-snapshot";
import { checkFormatting, runQuality } from "./run";
import { writeQualityProgress, type QualityProgress } from "./progress";

const temporary: string[] = [];
async function scratch(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "agent-platform-ci-boundaries-"));
  temporary.push(path);
  return path;
}
afterEach(async () => {
  for (const path of temporary.splice(0)) await rm(path, { recursive: true, force: true });
});
async function put(root: string, path: string, value: unknown): Promise<void> {
  await mkdir(dirname(join(root, path)), { recursive: true });
  await writeFile(join(root, path), typeof value === "string" ? value : JSON.stringify(value));
}
async function git(root: string, args: string[]): Promise<void> {
  const child = Bun.spawn(["git", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
  const [_stdout, _stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error("Disposable Git fixture failed.");
}

test("compatibility forwarding rejects implementation, side effects and noncanonical exports", () => {
  const canonical = new Set(["@fixture/common", "@fixture/common/handoff"]);
  for (const source of [
    "export * from '@fixture/common/handoff';",
    "export { value } from '@fixture/common';",
  ])
    expect(() =>
      validateCompatibilityForwarder("compatibility/old/src/index.ts", source, canonical),
    ).not.toThrow();
  for (const source of [
    "export const duplicate = 1;",
    "export * from '@fixture/common'; console.log('side effect');",
    "import '@fixture/common'; export * from '@fixture/common';",
    "export * from '../../packages/common/src/index';",
    "export * from '@fixture/common/private';",
    "// empty entrypoint",
  ])
    expect(() =>
      validateCompatibilityForwarder("compatibility/old/src/index.ts", source, canonical),
    ).toThrow("only forward");
});

test("compatibility and tracked Markdown participate in byte snapshots with protected docs excluded", async () => {
  const root = await scratch();
  const source = "packages/common/src/index.ts";
  const forwarder = "compatibility/old/src/index.ts";
  await put(root, "architecture/capability-source-owners.json", {
    schema: "agent-platform.capability-source-owners.v1",
    capabilities: [
      {
        packagePath: "packages/common",
        packageName: "@fixture/common",
        sourcePins: [{ relativePath: source, sha256: "a".repeat(64) }],
      },
    ],
  });
  await put(root, "architecture/quality-policy.json", {
    schema: "agent-platform.architecture-quality-policy.v1",
    stagedSources: [],
    ownedDefinitionInputs: [],
  });
  for (const path of [
    "architecture/common-git-harness.json",
    "architecture/runtime-documentation-routing/index.json",
    ".prettierrc.json",
    "ci/quality/any-baseline.json",
    "package.json",
  ])
    await put(root, path, {});
  await put(root, "packages/common/package.json", {
    name: "@fixture/common",
    exports: { ".": "./src/index.ts" },
  });
  await put(root, source, "export const value = 1;\n");
  await put(root, forwarder, "export * from '@fixture/common';\n");
  await put(root, ".github/workflows/source.yml", "name: Fixture\non: push\njobs: {}\n");
  for (const path of ["bun.lock", ".editorconfig", ".prettierignore"]) await put(root, path, "\n");
  for (const path of [
    "README.md",
    "AGENTS.md",
    "packages/common/AGENTS.md",
    "history/README.md",
    "evidence/README.md",
  ])
    await put(root, path, "Maintained example.\n");
  await put(root, "untracked.md", "Untracked.\n");
  await git(root, ["init", "--quiet"]);
  await git(root, [
    "add",
    "README.md",
    "AGENTS.md",
    "packages/common/AGENTS.md",
    "history/README.md",
    "evidence/README.md",
  ]);
  await git(root, [
    "-c",
    "user.name=Source quality fixture",
    "-c",
    "user.email=source-quality-fixture@example.invalid",
    "commit",
    "--no-gpg-sign",
    "--quiet",
    "-m",
    "Record tracked Markdown fixture",
  ]);
  expect(await readTrackedMarkdown(root)).toEqual([
    "AGENTS.md",
    "README.md",
    "packages/common/AGENTS.md",
  ]);
  const scope = await readQualityScope(root);
  for (const files of [scope.lintFiles, scope.formatFiles, scope.secretFiles])
    expect(files).toContain(forwarder);
  expect(scope.secretFiles).toContain("README.md");
  const progress: QualityProgress[] = [];
  const observe = async (value: QualityProgress): Promise<void> => {
    progress.push(value);
    await writeQualityProgress(join(root, "quality-progress.json"), value);
  };
  const unformatted = await runQuality(root, "format", observe);
  expect(unformatted.status).toBe("fail");
  expect(progress.find((value) => value.currentCheck === "format")?.checks).not.toContainEqual(
    expect.objectContaining({ id: "format" }),
  );
  expect(progress.at(-1)).toMatchObject({
    status: "complete",
    currentCheck: null,
    checks: expect.arrayContaining([expect.objectContaining({ id: "format", status: "fail" })]),
  });
  expect(JSON.stringify(progress)).not.toContain("details");
  expect(JSON.parse(await readFile(join(root, "quality-progress.json"), "utf8"))).toEqual(
    progress.at(-1),
  );
  await checkFormatting(scope, true);
  const formatted = await runQuality(root, "format", observe);
  expect(formatted.status).toBe("pass");
  expect(progress.at(-1)?.checks.every((check) => check.status === "pass")).toBe(true);
  const before = await captureQualitySources(scope);
  await put(root, forwarder, "export { value } from '@fixture/common';\n");
  await put(root, "README.md", "Changed maintained document.\n");
  const after = await captureQualitySources(await readQualityScope(root));
  expect(compareQualitySources(before, after).changedPaths).toEqual(["README.md", forwarder]);
  await put(root, forwarder, "export const duplicate = 1;\n");
  await expect(readQualityScope(root)).rejects.toThrow("only forward");
});

test("a progress sink failure refuses verification before work can be reported as passed", async () => {
  await expect(
    runQuality(await scratch(), "format", () => Promise.reject(Error("Progress sink failed"))),
  ).rejects.toThrow("Progress sink failed");
});

test("isolated build fails stale, missing and extra reviewed generated files without repairing bytes", async () => {
  const root = await scratch();
  await put(root, "packages/ontology/package.json", { scripts: { build: "bun build.ts" } });
  await put(
    root,
    "packages/ontology/build.ts",
    "await Bun.write('dist/index.js', 'fresh generated bytes');\n",
  );
  const reviewed = "packages/ontology/dist/index.js";
  await put(root, reviewed, "stale reviewed bytes");
  const result = await verifyOntologyBuild(root);
  expect(result.command.exitCode).toBe(0);
  expect(result.status).toBe("fail");
  expect(result.generated.changed).toEqual(["index.js"]);
  expect(await readFile(join(root, reviewed), "utf8")).toBe("stale reviewed bytes");
  const built = new Map([["index.js", "new"]]);
  expect(compareGeneratedTrees(new Map(), built)).toMatchObject({
    status: "fail",
    missing: ["index.js"],
  });
  expect(
    compareGeneratedTrees(
      new Map([
        ["index.js", "new"],
        ["surprise.js", "extra"],
      ]),
      built,
    ),
  ).toMatchObject({ status: "fail", extra: ["surprise.js"] });
  expect(compareGeneratedTrees(new Map(built), built).status).toBe("pass");
});

test("authored head and prospective merge both run full quality while native review remains exact-head", async () => {
  const root = resolve(import.meta.dir, "../..");
  const workflow = Bun.YAML.parse(
    await readFile(join(root, ".github/workflows/source.yml"), "utf8"),
  ) as {
    jobs: Record<
      string,
      {
        steps: {
          uses?: string;
          run?: string;
          with?: { ref?: string; "fetch-depth"?: number };
          env?: Record<string, string>;
        }[];
      }
    >;
  };
  for (const name of ["quality", "merge-integration"])
    expect(workflow.jobs[name]!.steps.some((step) => step.run === "bun run check:quality")).toBe(
      true,
    );
  const checkout = workflow.jobs["merge-integration"]!.steps.find((step) =>
    step.uses?.startsWith("actions/checkout@"),
  );
  expect(checkout?.with?.ref).toBeUndefined();
  for (const name of ["source", "merge-integration"]) {
    const job = workflow.jobs[name]!;
    const nativeCheckout = job.steps.find((step) => step.uses?.startsWith("actions/checkout@"));
    expect(nativeCheckout?.with?.["fetch-depth"]).toBe(0);
    const sourceCheck = job.steps.find((step) => step.run === "bun ci/source-check.ts");
    expect(sourceCheck?.env?.COMMON_BASE_SHA).toBe("${{ github.event.pull_request.base.sha }}");
    expect(
      job.steps.some((step) => step.run?.split("\n").includes("bun ci/verify-runtime-layout.ts")),
    ).toBe(true);
  }
  expect(
    workflow.jobs["quality"]!.steps.find((step) => step.uses?.startsWith("actions/checkout@"))
      ?.with?.["fetch-depth"],
  ).toBeUndefined();
  expect(
    workflow.jobs["native-review"]!.steps.some(
      (step) => step.run === "bun ci/source-check.ts --review",
    ),
  ).toBe(true);
  expect(JSON.stringify(workflow)).not.toContain("Probe independent review status availability");
});
