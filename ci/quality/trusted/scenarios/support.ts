import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile, readFile, rm, lstat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { fixedProcess } from "../process";
import { canonical } from "../policy";
export { canonical } from "../policy";
export const hash = (bytes: string | Uint8Array): string =>
  createHash("sha256").update(bytes).digest("hex");
export const bytes = (value: unknown): string => canonical(value) + "\n";
export async function write(root: string, path: string, value: string | Uint8Array): Promise<void> {
  await mkdir(dirname(join(root, path)), { recursive: true });
  await writeFile(join(root, path), value);
}
export async function scratch<T>(name: string, use: (root: string) => Promise<T>): Promise<T> {
  const root = await mkdtemp(join(tmpdir(), "trusted-" + name + "-"));
  try {
    return await use(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
export async function capture(use: () => unknown | Promise<unknown>) {
  try {
    const value = await use();
    return { returned: value === undefined ? { kind: "undefined" } : value, error: null };
  } catch (error) {
    if (!(error instanceof Error)) throw error;
    const code = Object.getOwnPropertyDescriptor(error, "code")?.value;
    return {
      returned: null,
      error: {
        name: error.name,
        message: error.message,
        code: typeof code === "string" ? code : null,
      },
    };
  }
}
export async function git(root: string, args: string[], stdin?: string): Promise<string> {
  const result = await fixedProcess(
    [
      "/usr/bin/git",
      "--no-replace-objects",
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "commit.gpgsign=false",
      "-C",
      root,
      ...args,
    ],
    {
      cwd: root,
      env: {
        GIT_CONFIG_NOSYSTEM: "1",
        GIT_CONFIG_GLOBAL: "/dev/null",
        GIT_AUTHOR_NAME: "Fictional P scenario",
        GIT_AUTHOR_EMAIL: "scenario@example.invalid",
        GIT_COMMITTER_NAME: "Fictional P scenario",
        GIT_COMMITTER_EMAIL: "scenario@example.invalid",
      },
      stdin: stdin === undefined ? undefined : Buffer.from(stdin),
      timeoutMs: 10000,
      maxBytes: 1000000,
    },
  );
  if (result.code || result.timedOut) throw Error("SCENARIO_GIT_FIXTURE_FAILED");
  return result.stdout.toString().trim();
}
export async function commit(root: string): Promise<string> {
  await git(root, ["add", "."]);
  await git(root, ["commit", "--allow-empty", "-m", "Fictional immutable source"]);
  return git(root, ["rev-parse", "HEAD"]);
}
export async function fileState(path: string) {
  try {
    const stat = await lstat(path);
    return {
      kind: stat.isFile() ? "file" : "other",
      sha256: stat.isFile() ? hash(await readFile(path)) : null,
    };
  } catch (error) {
    if (
      (error as NodeJS.ErrnoException).code === "ENOENT" ||
      (error as NodeJS.ErrnoException).code === "ENOTDIR"
    )
      return { kind: "absent", sha256: null };
    throw error;
  }
}
export async function inputFixture() {
  return JSON.parse(
    await readFile(join(import.meta.dir, "../fixtures/scenario-inputs.json"), "utf8"),
  ) as {
    graph: import("../../../../packages/work-continuity/src/git-work/generated/v3").Graph;
    task: import("../../../../packages/work-continuity/src/git-work/generated/v3").TaskRef;
  };
}
