import { expect, test } from "bun:test";
import { join } from "node:path";

test("metadata data reads disable a configured filesystem-monitor executable", async () => {
  const { mkdtemp, writeFile, readFile, access, rm } = await import("node:fs/promises"),
    { tmpdir } = await import("node:os"),
    { sourceAuthorGit } = await import("./source-author");
  const root = await mkdtemp(join(tmpdir(), "review-git-hooks-")),
    marker = join(root, "monitor-ran"),
    monitor = join(root, "monitor.sh");
  function git(args: string[]) {
    const r = Bun.spawnSync(["git", "-C", root, ...args], { stdout: "pipe", stderr: "pipe" });
    expect(r.exitCode).toBe(0);
    return r.stdout.toString();
  }
  try {
    git(["init", "-q"]);
    await writeFile(monitor, '#!/bin/sh\nprintf called > "' + marker + '"\nprintf "token\\0"\n', {
      mode: 0o700,
    });
    git(["config", "core.fsmonitor", monitor]);
    git(["status", "--porcelain=v1"]);
    await expect(readFile(marker, "utf8")).resolves.toBe("called");
    await rm(marker);
    await sourceAuthorGit(root, ["status", "--porcelain=v1"]);
    await expect(access(marker)).rejects.toThrow();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
