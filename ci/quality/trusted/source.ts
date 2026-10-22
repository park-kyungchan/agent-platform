import { readFile, readdir, lstat, realpath, mkdir } from "node:fs/promises";
import { join, relative } from "node:path";
import { deriveReviewedSourceManifest } from "../receipt-validator";
import { verifyCommittedSourceAuthor } from "../../reuse/source-author";
import { fixedProcess } from "./process";
import {
  insist,
  oid,
  object,
  repository,
  sha256,
  trustedDigest,
  safePath,
  same,
  type Subject,
  type Target,
} from "./policy";
import { repoPath, type ProviderReader } from "./provider";

const gitEnv = {
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_NO_REPLACE_OBJECTS: "1",
  GIT_TERMINAL_PROMPT: "0",
  HOME: "/nonexistent",
};
export async function git(root: string, args: string[]): Promise<string> {
  const result = await fixedProcess(
    [
      "/usr/bin/git",
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "safe.directory=" + root,
      "-c",
      "credential.helper=",
      "-c",
      "protocol.file.allow=never",
      "-C",
      root,
      ...args,
    ],
    { cwd: root, env: gitEnv, maxBytes: 8_000_000 },
  );
  insist(result.code === 0 && !result.timedOut, "EXACT_GIT_OBJECT_READ_FAILED");
  return result.stdout.toString().trimEnd();
}
export async function dependencyInputs(root: string): Promise<Record<string, string>> {
  const paths = ["package.json", "bun.lock"];
  for (const e of await readdir(join(root, "packages"), { withFileTypes: true })) {
    insist(e.isDirectory() && !e.isSymbolicLink(), "EXACT_WORKSPACE_DIRECTORY_REQUIRED");
    paths.push("packages/" + e.name + "/package.json");
  }
  const out: Record<string, string> = {};
  for (const path of paths.sort()) {
    safePath(path);
    const f = join(root, path),
      info = await lstat(f);
    insist(
      info.isFile() && info.size < 4_000_000 && (await realpath(f)) === f,
      "EXACT_DEPENDENCY_INPUT_REQUIRED",
    );
    out[path] = sha256(await readFile(f));
  }
  return out;
}
/** Provider objects are independently joined to local immutable Git bytes. */
export async function readSubject(
  reader: ProviderReader,
  root: string,
  head: string,
): Promise<Subject> {
  insist(oid(head), "EXACT_SOURCE_HEAD_REQUIRED");
  const commit = object(await reader.json(repoPath("git/commits/" + head))),
    tree = object(commit.tree).sha;
  insist(
    commit.sha === head && oid(tree) && Array.isArray(commit.parents),
    "ACTUAL_COMMIT_REQUIRED",
  );
  const parents = commit.parents.map((p) => String(object(p).sha));
  insist(
    parents.every(oid) &&
      (await git(root, ["rev-parse", "HEAD"])) === head &&
      (await git(root, ["rev-parse", head + "^{tree}"])) === tree &&
      same(
        (await git(root, ["rev-list", "--parents", "-n", "1", head])).split(" ").slice(1),
        parents,
      ),
    "PROVIDER_GIT_SUBJECT_MISMATCH",
  );
  const response = object(await reader.json(repoPath(`git/trees/${tree}?recursive=1`)));
  const manifest = deriveReviewedSourceManifest(response, tree);
  insist(
    Array.isArray(response.tree) &&
      response.tree.every(
        (e) => object(e).type === "tree" || ["100644", "100755"].includes(String(object(e).mode)),
      ),
    "SYMLINK_OR_SUBMODULE_SOURCE_REFUSED",
  );
  const local = (await git(root, ["ls-tree", "-r", "-t", "-z", head]))
    .split("\0")
    .filter(Boolean)
    .map((line) => {
      const m = /^(040000|100644|100755) (tree|blob) ([a-f0-9]{40})\t(.+)$/.exec(line);
      insist(m, "REGULAR_GIT_SOURCE_REQUIRED");
      return { mode: m[1], type: m[2], sha: m[3], path: m[4] };
    });
  insist(
    deriveReviewedSourceManifest({ sha: tree, truncated: false, tree: local }, tree).sha256 ===
      manifest.sha256,
    "LOCAL_SOURCE_MANIFEST_MISMATCH",
  );
  return {
    commit: head,
    tree,
    parents,
    sourceManifestSha256: manifest.sha256,
    dependencyInputsSha256: trustedDigest(await dependencyInputs(root)),
  };
}
export async function verifyCurrentTarget(reader: ProviderReader, t: Target): Promise<void> {
  const ref = object(await reader.json(repoPath("git/ref/heads/main")));
  insist(
    ref.ref === "refs/heads/main" &&
      object(ref.object).sha === (t.kind === "pull-request" ? t.base : t.tested.commit),
    "CURRENT_MAIN_DRIFT",
  );
  if (t.kind === "pull-request") {
    const pr = object(await reader.json(repoPath("pulls/" + t.pr))),
      head = object(pr.head),
      base = object(pr.base);
    insist(
      pr.number === t.pr &&
        pr.state === "open" &&
        pr.merged === false &&
        head.sha === t.authored.commit &&
        base.sha === t.base &&
        base.ref === "main" &&
        object(head.repo).full_name === repository &&
        object(base.repo).full_name === repository &&
        pr.merge_commit_sha === t.tested.commit,
      "CURRENT_PR_HEAD_BASE_OR_MERGE_DRIFT",
    );
  } else {
    const c = object(await reader.json(repoPath("git/commits/" + t.tested.commit)));
    insist(
      c.sha === t.tested.commit &&
        Array.isArray(c.parents) &&
        same(
          c.parents.map((p) => object(p).sha),
          [t.base, t.authored.commit],
        ),
      "ACTUAL_POSTMERGE_SUBJECT_REQUIRED",
    );
  }
}
export async function readAuthor(root: string, subject: Subject) {
  // Canonical P reader retains complete-history, fresh-parent, project and mechanical-merge checks.
  return verifyCommittedSourceAuthor({
    workspace: root,
    repository,
    projectId: "agent-platform",
    sourceHead: subject.commit,
  });
}
/** Root/P owns this checkout; .git is freshly generated and never copied from candidate storage. */
export async function checkout(root: string, head: string, token: string): Promise<void> {
  insist(oid(head) && (await realpath(root)) === root, "EXACT_CHECKOUT_ROOT_REQUIRED");
  await git(root, ["init", "--quiet", "--template="]);
  const result = await fixedProcess(
    [
      "/usr/bin/git",
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "credential.helper=",
      "-c",
      "protocol.file.allow=never",
      "-C",
      root,
      "fetch",
      "--no-tags",
      "--no-recurse-submodules",
      "https://github.com/" + repository + ".git",
      head,
    ],
    {
      cwd: root,
      maxBytes: 128000,
      timeoutMs: 180000,
      env: {
        ...gitEnv,
        GIT_CONFIG_COUNT: "1",
        GIT_CONFIG_KEY_0: "http.https://github.com/.extraheader",
        GIT_CONFIG_VALUE_0:
          "AUTHORIZATION: basic " + Buffer.from("x-access-token:" + token).toString("base64"),
      },
    },
  );
  insist(result.code === 0 && !result.timedOut, "FETCH_UNKNOWN_RECONCILE_REQUIRED");
  await git(root, ["checkout", "--quiet", "--detach", head]);
  // No custom Git config, credentials, hooks, alternates or shallow history enters the worker.
  insist(
    (await git(root, ["rev-parse", "--is-shallow-repository"])) === "false",
    "COMPLETE_HISTORY_REQUIRED",
  );
  const localConfig = await git(root, ["config", "--local", "--null", "--list"]);
  insist(
    !localConfig
      .split("\0")
      .some((entry) =>
        /^(credential|http|include|filter|merge|alias|url)\./i.test(entry.split("\n")[0]!),
      ),
    "CUSTOM_CHECKOUT_CONFIG_REFUSED",
  );
}
export async function sourceFiles(root: string): Promise<string[]> {
  const paths: string[] = [];
  async function visit(dir: string): Promise<void> {
    for (const e of await readdir(dir, { withFileTypes: true })) {
      if ([".git", "node_modules"].includes(e.name)) continue;
      const f = join(dir, e.name),
        path = relative(root, f);
      safePath(path);
      insist(!e.isSymbolicLink(), "SOURCE_SYMLINK_REFUSED");
      if (e.isDirectory()) await visit(f);
      else {
        const info = await lstat(f);
        insist(
          info.isFile() && info.size <= 100_000_000 && paths.length < 8192,
          "SOURCE_BUDGET_EXCEEDED",
        );
        paths.push(path);
      }
    }
  }
  await visit(root);
  return paths.sort();
}
