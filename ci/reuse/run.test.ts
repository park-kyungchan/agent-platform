import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { captureCodexSourceAuthor, sourceAuthorGit } from "./source-author";

const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});

async function fixture(install = false) {
  const base = await mkdtemp(join(tmpdir(), "caller-source-test-"));
  roots.push(base);
  const owner = join(base, "owner"),
    decoy = join(base, "common-source");
  await mkdir(owner);
  await mkdir(decoy);
  await writeFile(
    join(owner, "project.json"),
    JSON.stringify({
      schemaVersion: "1.0.0",
      kind: "project",
      projectId: "fixture-owner",
      domainId: "fixture-domain",
    }),
  );
  await writeFile(
    join(owner, "package.json"),
    JSON.stringify({
      name: "fixture-owner",
      private: true,
      ...(install ? { dependencies: { "fixture-dependency": "file:./dependency" } } : {}),
      scripts: {
        check: "bun check.ts",
        fail: "bun fail.ts",
        mutate: "bun mutate.ts",
        postinstall: "bun postinstall.ts",
      },
    }),
  );
  await writeFile(join(owner, ".gitignore"), ".result.json\n.postinstall.json\nnode_modules/\n");
  await writeFile(
    join(owner, "check.ts"),
    'await Bun.write(".result.json", JSON.stringify({ cwd: process.cwd(), version: Bun.version }));\n',
  );
  await writeFile(join(owner, "fail.ts"), "process.exit(7);\n");
  await writeFile(
    join(owner, "mutate.ts"),
    'import { appendFile } from "node:fs/promises"; await appendFile("project.json", "\\n");\n',
  );
  await writeFile(
    join(owner, "postinstall.ts"),
    'await Bun.write(".postinstall.json", "unexpected install lifecycle");\n',
  );
  if (install) {
    await mkdir(join(owner, "dependency"));
    await writeFile(
      join(owner, "dependency/package.json"),
      JSON.stringify({ name: "fixture-dependency", version: "1.0.0" }),
    );
    const child = Bun.spawn(
      [process.execPath, "--no-env-file", "install", "--lockfile-only", "--ignore-scripts"],
      { cwd: owner, stdout: "pipe", stderr: "pipe" },
    );
    if ((await child.exited) !== 0) throw Error("Fixture frozen lock preparation failed");
  }
  for (const args of [
    ["init", "-q"],
    ["add", "."],
    [
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit",
      "-qm",
      "Synthetic owner source",
    ],
  ]) {
    const child = Bun.spawn(["git", "-C", owner, ...args], { stdout: "pipe", stderr: "pipe" });
    if ((await child.exited) !== 0) throw Error("Fixture Git preparation failed");
  }
  const readHead = async () => {
    const git = Bun.spawn(["git", "-C", owner, "rev-parse", "HEAD"], { stdout: "pipe" });
    const head = (await new Response(git.stdout).text()).trim();
    if ((await git.exited) !== 0) throw Error("Fixture source identity failed");
    return head;
  };
  const metadata = captureCodexSourceAuthor(
    {
      sourceOwner: {
        repository: "fixture/owner",
        projectId: "fixture-owner",
        domainId: "fixture-domain",
      },
      logicalWork: {
        domainId: "fixture-domain",
        projectId: "fixture-owner",
        workId: "fictional-work",
        taskId: "fictional-task",
      },
    },
    await readHead(),
    { CODEX_THREAD_ID: "00000000-0000-4000-8000-000000000001" },
  );
  await mkdir(join(owner, ".github"));
  await writeFile(
    join(owner, ".github/source-author.json"),
    JSON.stringify(metadata, null, 2) + "\n",
  );
  for (const args of [
    ["add", ".github/source-author.json"],
    [
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit",
      "-qm",
      "Fictional native-author source record",
    ],
  ]) {
    const child = Bun.spawn(["git", "-C", owner, ...args], { stdout: "ignore", stderr: "pipe" });
    if ((await child.exited) !== 0) throw Error("Fixture metadata commit failed");
  }
  const head = await readHead();
  const env = {
    ...process.env,
    GITHUB_WORKSPACE: owner,
    GITHUB_REPOSITORY: "fixture/owner",
    COMMON_CI_REPOSITORY: "fixture/owner",
    COMMON_CI_PROJECT_ID: "fixture-owner",
    COMMON_CI_SOURCE_SHA: head,
    COMMON_CI_BUN_VERSION: Bun.version,
    COMMON_CI_SCRIPTS: '["check"]',
    COMMON_CI_INSTALL: "false",
  };
  async function run(extra: NodeJS.ProcessEnv = {}) {
    const child = Bun.spawn(
      [process.execPath, "--no-env-file", "--no-install", join(import.meta.dir, "run.ts")],
      { cwd: decoy, env: { ...env, ...extra }, stdout: "pipe", stderr: "pipe" },
    );
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    return { stdout, stderr, exitCode };
  }
  return { base, owner, decoy, run };
}

test("downloaded common runner executes the caller's script in its exact Git root", async () => {
  const f = await fixture();
  expect((await f.run()).exitCode).toBe(0);
  expect(JSON.parse(await readFile(join(f.owner, ".result.json"), "utf8"))).toEqual({
    cwd: f.owner,
    version: Bun.version,
  });
  expect(await Bun.file(join(f.decoy, ".result.json")).exists()).toBe(false);
});

test("frozen dependency setup preserves the lock and disables install lifecycle scripts", async () => {
  const f = await fixture(true);
  const lock = await readFile(join(f.owner, "bun.lock"), "utf8");
  expect((await f.run({ COMMON_CI_INSTALL: "true" })).exitCode).toBe(0);
  expect(await readFile(join(f.owner, "bun.lock"), "utf8")).toBe(lock);
  expect(await Bun.file(join(f.owner, ".postinstall.json")).exists()).toBe(false);
  expect(await Bun.file(join(f.owner, ".result.json")).exists()).toBe(true);
});

test("wrong owner, source, runtime and unsafe script inputs refuse before execution", async () => {
  const f = await fixture();
  const alias = join(f.base, "owner-alias");
  await symlink(f.owner, alias);
  for (const extra of [
    { GITHUB_REPOSITORY: "foreign/owner" },
    { COMMON_CI_PROJECT_ID: "foreign-project" },
    { COMMON_CI_SOURCE_SHA: "a".repeat(40) },
    { COMMON_CI_BUN_VERSION: "0.0.1" },
    { COMMON_CI_BUN_VERSION: "latest" },
    { COMMON_CI_SCRIPTS: '["missing"]' },
    { COMMON_CI_SCRIPTS: '["check;echo injected"]' },
    { COMMON_CI_SCRIPTS: '["check","check"]' },
    { COMMON_CI_SCRIPTS: "[]" },
    { COMMON_CI_INSTALL: "yes" },
    { GITHUB_WORKSPACE: alias },
    { GITHUB_WORKSPACE: f.decoy },
  ]) {
    const result = await f.run(extra);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("Caller source checks refused");
    expect(await Bun.file(join(f.owner, ".result.json")).exists()).toBe(false);
  }
});

test("owner failure keeps its exit code and does not run a later script", async () => {
  const f = await fixture();
  expect((await f.run({ COMMON_CI_SCRIPTS: '["fail","check"]' })).exitCode).toBe(7);
  expect(await Bun.file(join(f.owner, ".result.json")).exists()).toBe(false);
});

test("missing or unsafe required author metadata refuses before the caller script", async () => {
  const f = await fixture();
  for (const path of [".github/missing.json", "../source-author.json", ""]) {
    expect((await f.run({ COMMON_CI_SOURCE_AUTHOR_METADATA: path })).exitCode).toBe(1);
    expect(await Bun.file(join(f.owner, ".result.json")).exists()).toBe(false);
  }
});

test("stale author metadata refuses before dependency installation or any owner script", async () => {
  const f = await fixture(true);
  await writeFile(join(f.owner, "later.ts"), "export const anotherNativeAttempt = true;\n");
  await sourceAuthorGit(f.owner, ["add", "."]);
  await sourceAuthorGit(f.owner, [
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "commit",
    "-qm",
    "Fictional later native author without recapture",
  ]);
  const sourceHead = await sourceAuthorGit(f.owner, ["rev-parse", "HEAD"]);
  const result = await f.run({ COMMON_CI_SOURCE_SHA: sourceHead, COMMON_CI_INSTALL: "true" });
  expect({
    exitCode: result.exitCode,
    installed: await Bun.file(
      join(f.owner, "node_modules/fixture-dependency/package.json"),
    ).exists(),
    ownerExecuted: await Bun.file(join(f.owner, ".result.json")).exists(),
  }).toEqual({ exitCode: 1, installed: false, ownerExecuted: false });
  expect(result.stderr).toContain("Caller source checks refused");
});

test("tracked source mutation refuses continuation and preserves the changed bytes", async () => {
  const f = await fixture();
  const before = await readFile(join(f.owner, "project.json"), "utf8");
  expect((await f.run({ COMMON_CI_SCRIPTS: '["mutate","check"]' })).exitCode).toBe(1);
  expect(await readFile(join(f.owner, "project.json"), "utf8")).toBe(before + "\n");
  expect(await Bun.file(join(f.owner, ".result.json")).exists()).toBe(false);
});
