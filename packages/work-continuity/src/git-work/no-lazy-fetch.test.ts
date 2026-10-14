import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, readdir, realpath, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  gitRecordPath,
  openGitWork,
  resolveGitWorkSelection,
  type GitWorkSelection,
} from "./index.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
async function fixtureGit(root: string, ...args: string[]) {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    GIT_TERMINAL_PROMPT: "0",
    GIT_OPTIONAL_LOCKS: "0",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_NO_LAZY_FETCH: "1",
    GIT_AUTHOR_NAME: "Synthetic",
    GIT_AUTHOR_EMAIL: "fixture@example.invalid",
    GIT_COMMITTER_NAME: "Synthetic",
    GIT_COMMITTER_EMAIL: "fixture@example.invalid",
  };
  for (const key of [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_TRACE2_EVENT",
  ])
    delete env[key];
  const child = Bun.spawn(["git", "--no-replace-objects", "-C", root, ...args], {
    env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error(`Synthetic fixture Git failed: ${stderr}`);
  return stdout.trim();
}
async function objectSnapshot(root: string): Promise<{ path: string; sha256: string }[]> {
  const rows: { path: string; sha256: string }[] = [];
  async function walk(relative: string) {
    for (const entry of await readdir(join(root, relative), { withFileTypes: true })) {
      const path = join(relative, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.isFile()) rows.push({ path, sha256: hash(await readFile(join(root, path))) });
      else throw Error("Synthetic Git object store must contain only files/directories");
    }
  }
  await walk("");
  return rows.sort((a, b) => a.path.localeCompare(b.path));
}
async function fixture(missing: "commit" | "tree" | "blob") {
  const root = await realpath(await mkdtemp(join(tmpdir(), "git-work-no-lazy-fetch-")));
  roots.push(root);
  const origin = join(root, "origin"),
    selected = join(root, "selected"),
    trace = join(root, "read-trace.jsonl");
  await mkdir(origin);
  await mkdir(selected);
  await fixtureGit(origin, "init", "-q", "-b", "main");
  const workPath = gitRecordPath("fixture", "work", { kind: "work", id: "work" }),
    originalPath = "work/fixture/work/inputs/original.json";
  await mkdir(dirname(join(origin, workPath)), { recursive: true });
  await writeFile(
    join(origin, workPath),
    JSON.stringify({
      schema: "common.git-work.v1",
      domainId: "native-harness",
      projectId: "fixture",
      workId: "work",
      kind: "work",
      id: "work",
      repositoryId: "synthetic/promisor",
      issueId: "issue",
      purpose: {
        objective: "Synthetic local-only Work",
        successCriteria: ["No transport or object-store effects"],
        inScope: ["Source reads"],
        outOfScope: ["Native permission"],
      },
      spec: { id: "spec", revision: 1 },
      plan: { id: "plan", revision: 1 },
      governingDecisionIds: [],
      historicalFenceIds: [],
    }),
  );
  await mkdir(dirname(join(origin, originalPath)), { recursive: true });
  await writeFile(
    join(origin, originalPath),
    JSON.stringify({ text: "Synthetic original source" }),
  );
  await fixtureGit(origin, "add", ".");
  await fixtureGit(origin, "commit", "-qm", "Synthetic promisor source");
  const commit = await fixtureGit(origin, "rev-parse", "HEAD"),
    blob = await fixtureGit(origin, "rev-parse", `${commit}:${originalPath}`);
  await fixtureGit(selected, "init", "-q", "-b", "main");
  await fixtureGit(selected, "config", "core.repositoryformatversion", "1");
  await fixtureGit(selected, "config", "extensions.partialclone", "origin");
  await fixtureGit(selected, "config", "remote.origin.url", pathToFileURL(origin).href);
  await fixtureGit(selected, "config", "remote.origin.promisor", "true");
  await fixtureGit(selected, "config", "remote.origin.partialclonefilter", "blob:none");
  // Copy only chosen existing local objects, never fetch/clone to prepare the
  // negative case. The missing commit/tree/blob exists solely in the owned origin.
  const objects =
    missing === "commit"
      ? []
      : missing === "tree"
        ? [commit]
        : (await fixtureGit(origin, "rev-list", "--objects", "--all"))
            .split("\n")
            .map((row) => row.split(" ")[0]!)
            .filter((oid) => oid !== blob);
  for (const oid of objects) {
    const relative = join(oid.slice(0, 2), oid.slice(2)),
      target = join(selected, ".git/objects", relative);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, await readFile(join(origin, ".git/objects", relative)));
  }
  const selection: GitWorkSelection = {
    repositoryId: "synthetic/promisor",
    repositoryRoot: selected,
    commit,
    projectId: "fixture",
    workId: "work",
  };
  return { selected, trace, selection, originalPath };
}

for (const ambient of [undefined, "0"] as const)
  for (const missing of ["commit", "tree", "blob"] as const) {
    test(`local promisor ${missing} read refuses transport and object writes with ambient no-lazy-fetch ${ambient ?? "unset"}`, async () => {
      const f = await fixture(missing),
        objectsPath = join(f.selected, ".git/objects"),
        before = await objectSnapshot(objectsPath),
        previousFlag = process.env.GIT_NO_LAZY_FETCH,
        previousTrace = process.env.GIT_TRACE2_EVENT;
      let failure: string | undefined;
      try {
        if (ambient === undefined) delete process.env.GIT_NO_LAZY_FETCH;
        else process.env.GIT_NO_LAZY_FETCH = ambient;
        process.env.GIT_TRACE2_EVENT = f.trace;
        try {
          const resolved = await resolveGitWorkSelection(f.selection);
          if (missing !== "commit") {
            const reader = await openGitWork(resolved);
            if (missing === "blob")
              await reader.readSource({ kind: "snapshot", path: f.originalPath });
          }
        } catch (error) {
          failure = error instanceof Error ? error.message : String(error);
        }
      } finally {
        if (previousFlag === undefined) delete process.env.GIT_NO_LAZY_FETCH;
        else process.env.GIT_NO_LAZY_FETCH = previousFlag;
        if (previousTrace === undefined) delete process.env.GIT_TRACE2_EVENT;
        else process.env.GIT_TRACE2_EVENT = previousTrace;
      }
      const events = (await readFile(f.trace, "utf8"))
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line) as { event?: string; argv?: string[] });
      expect(events.some((event) => event.event === "start")).toBe(true);
      // Lazy fetch spawns fetch/upload-pack children even when the only configured
      // transport is this disposable file:// origin. Read commands need none.
      expect(events.filter((event) => event.event === "child_start")).toEqual([]);
      const after = await objectSnapshot(objectsPath);
      expect(after).toEqual(before);
      expect(after.filter((row) => row.path.startsWith("pack/"))).toEqual([]);
      expect(failure).toBe("GIT_SOURCE_UNAVAILABLE");
    });
  }
