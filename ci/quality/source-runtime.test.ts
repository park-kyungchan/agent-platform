import { describe, expect, test } from "bun:test";
import { chmod, link, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  needsSourceRuntime,
  sourceRuntimePaths,
  verifySourceRuntimeArtifact,
  type SourceCiRole,
} from "./source-runtime";

const sourceEnv = {
  GITHUB_ACTIONS: "true",
  GITHUB_JOB: "source",
  RUNNER_OS: "Linux",
  RUNNER_ARCH: "X64",
};
describe("finite source CI runtime preparation", () => {
  test("current Bun requires no runner paths, environment mutation, or bootstrap", () => {
    expect(needsSourceRuntime("source-check", "1.4.3", {}, "linux", "x64")).toBe(false);
    expect(needsSourceRuntime("all", "1.4.3", {}, "darwin", "arm64")).toBe(false);
  });
  test("only exact old runtime and explicit source jobs prepare", () => {
    for (const job of ["source", "merge-integration"])
      expect(
        needsSourceRuntime(
          "source-check",
          "1.3.14",
          { ...sourceEnv, GITHUB_JOB: job },
          "linux",
          "x64",
        ),
      ).toBe(true);
    for (const job of ["quality", "merge-integration"])
      for (const role of ["all", "format", "lint", "workflows", "architecture"] as const)
        expect(
          needsSourceRuntime(role, "1.3.14", { ...sourceEnv, GITHUB_JOB: job }, "linux", "x64"),
        ).toBe(true);
    for (const version of ["1.3.13", "1.4.2", "1.4.4", "1.4.3-canary"])
      expect(() => needsSourceRuntime("source-check", version, sourceEnv, "linux", "x64")).toThrow(
        "require Bun 1.4.3",
      );
    for (const change of [
      { GITHUB_ACTIONS: "false" },
      { GITHUB_JOB: "native-review" },
      { GITHUB_JOB: "quality" },
      { RUNNER_OS: "macOS" },
      { RUNNER_ARCH: "ARM64" },
    ])
      expect(() =>
        needsSourceRuntime("source-check", "1.3.14", { ...sourceEnv, ...change }, "linux", "x64"),
      ).toThrow();
    expect(() => needsSourceRuntime("source-check", "1.3.14", {}, "linux", "x64")).toThrow();
    expect(() =>
      needsSourceRuntime("source-check", "1.3.14", sourceEnv, "darwin", "x64"),
    ).toThrow();
    expect(() =>
      needsSourceRuntime("source-check", "1.3.14", sourceEnv, "linux", "arm64"),
    ).toThrow();
    for (const role of ["review", "format:write", "baseline:any", "--refresh-owner-fixtures"])
      expect(() => needsSourceRuntime(role as SourceCiRole, "1.3.14", sourceEnv)).toThrow(
        "Unknown",
      );
  });
  test("preload options cannot enter a prepared process", () => {
    for (const key of ["NODE_OPTIONS", "BUN_OPTIONS"])
      expect(() =>
        needsSourceRuntime(
          "source-check",
          "1.3.14",
          { ...sourceEnv, [key]: "--preload untrusted.ts" },
          "linux",
          "x64",
        ),
      ).toThrow("preload");
  });
  test("short and same-size corrupted release artifacts fail closed", () => {
    for (const kind of ["archive", "binary"] as const) {
      expect(() => verifySourceRuntimeArtifact(kind, new Uint8Array(1))).toThrow("size or SHA256");
      expect(() =>
        verifySourceRuntimeArtifact(kind, new Uint8Array(kind === "archive" ? 42015139 : 88979072)),
      ).toThrow("size or SHA256");
    }
  });
  test("runner paths require owned regular files and reject escapes, links, and writable shared state", async () => {
    const temporary = await mkdtemp(join(tmpdir(), "source-runtime-path-test-"));
    const commandFile = join(temporary, "github-path");
    try {
      await writeFile(commandFile, "", { mode: 0o600 });
      const env = { RUNNER_TEMP: temporary, GITHUB_PATH: commandFile };
      expect(await sourceRuntimePaths(env)).toEqual({ temporary, commandFile });
      for (const change of [
        { RUNNER_TEMP: "/" },
        { RUNNER_TEMP: "relative" },
        { RUNNER_TEMP: temporary + "/../" },
        { GITHUB_PATH: join(temporary, "../outside") },
        { GITHUB_PATH: commandFile + "\nextra" },
      ])
        await expect(sourceRuntimePaths({ ...env, ...change })).rejects.toThrow();
      const alias = join(temporary, "alias");
      await symlink(commandFile, alias);
      await expect(sourceRuntimePaths({ ...env, GITHUB_PATH: alias })).rejects.toThrow();
      const linked = join(temporary, "linked");
      await link(commandFile, linked);
      await expect(sourceRuntimePaths(env)).rejects.toThrow();
      await rm(linked);
      await chmod(commandFile, 0o666);
      await expect(sourceRuntimePaths(env)).rejects.toThrow();
      await chmod(commandFile, 0o600);
      await chmod(temporary, 0o777);
      await expect(sourceRuntimePaths(env)).rejects.toThrow();
      await chmod(temporary, 0o700);
      await writeFile(commandFile, "x".repeat(65537));
      await expect(sourceRuntimePaths(env)).rejects.toThrow();
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  });
  test("native-review CLI keeps its authenticated mocked receipt path without runtime preparation", async () => {
    const temporary = await mkdtemp(join(tmpdir(), "source-runtime-review-test-"));
    try {
      const preload = join(temporary, "mock-review.ts");
      await writeFile(
        preload,
        `
const head = "a".repeat(40), tree = "b".repeat(40), receiptCommit = "c".repeat(40);
const receipt = { schema: "native-independent-review.v1", headSha: head, reviewedTreeSha: tree,
 reviewer: { runtime: "native-codex", taskId: "reviewer" }, author: { taskId: "author" },
 conclusion: "pass", unresolvedFindings: 0,
 provenance: { owner: "hermes-harness-engineering", resultRef: "historical-native-result", sha256: "1".repeat(64) } };
let reads = 0;
globalThis.fetch = async (url, options) => {
 if (!String(url).startsWith("https://api.github.com/repos/owner/platform/") ||
     options?.headers?.Authorization !== "Bearer fictional-test-token") throw Error("Unexpected network or auth");
 reads++;
 if (String(url).includes("/statuses?")) return Response.json([{ context: "native-independent-review", state: "success",
   creator: { id: 42, login: "fictional-reviewer" }, target_url: "https://github.com/owner/platform/blob/" + receiptCommit + "/native-reviews/" + head + ".json" }]);
 if (String(url).includes("/contents/")) return Response.json({ type: "file", encoding: "base64",
   size: JSON.stringify(receipt).length, content: Buffer.from(JSON.stringify(receipt)).toString("base64") });
 if (String(url).endsWith("/git/commits/" + head)) return Response.json({ sha: head, tree: { sha: tree } });
 throw Error("Unexpected evidence read");
};
process.on("exit", code => { if (code === 0 && reads !== 3) throw Error("Receipt reads missing"); });
`,
      );
      const env: NodeJS.ProcessEnv = {
        ...process.env,
        ...sourceEnv,
        GITHUB_JOB: "native-review",
        GITHUB_REPOSITORY: "owner/platform",
        REVIEW_HEAD_SHA: "a".repeat(40),
        GITHUB_TOKEN: "fictional-test-token",
        NATIVE_REVIEW_STATUS_CREATOR_ID: "42",
        NATIVE_REVIEW_STATUS_CREATOR_LOGIN: "fictional-reviewer",
      };
      delete env.RUNNER_TEMP;
      delete env.GITHUB_PATH;
      delete env.NODE_OPTIONS;
      delete env.BUN_OPTIONS;
      const child = Bun.spawn(
        [
          process.execPath,
          "--preload",
          preload,
          resolve(import.meta.dir, "../source-check.ts"),
          "--review",
        ],
        { env, stdin: "ignore", stdout: "pipe", stderr: "pipe" },
      );
      const output = await new Response(child.stdout).text(),
        error = await new Response(child.stderr).text();
      expect(await child.exited).toBe(0);
      expect(error).toBe("");
      expect(JSON.parse(output).status).toBe("pass");
      expect(output).not.toContain("verified-source-ci-runtime");
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  });
});
