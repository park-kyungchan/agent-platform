import { lstat, readFile, realpath } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";

export interface CallerSourceInput {
  workspace: string;
  repository: string;
  expectedRepository: string;
  projectId: string;
  sourceSha: string;
  bunVersion: string;
  scripts: string[];
  installDependencies: boolean;
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/** Input selects existing owner scripts; it never becomes shell source or a quality policy. */
export function parseCallerSourceInput(env: NodeJS.ProcessEnv): CallerSourceInput {
  const workspace = env.GITHUB_WORKSPACE ?? "";
  const repository = env.GITHUB_REPOSITORY ?? "";
  const expectedRepository = env.COMMON_CI_REPOSITORY ?? "";
  const projectId = env.COMMON_CI_PROJECT_ID ?? "";
  const sourceSha = env.COMMON_CI_SOURCE_SHA ?? "";
  const bunVersion = env.COMMON_CI_BUN_VERSION ?? "";
  if (
    !isAbsolute(workspace) ||
    resolve(workspace) !== workspace ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository) ||
    repository !== expectedRepository ||
    !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(projectId) ||
    !/^[a-f0-9]{40}$/.test(sourceSha) ||
    !/^\d+\.\d+\.\d+$/.test(bunVersion) ||
    !["true", "false"].includes(env.COMMON_CI_INSTALL ?? "") ||
    !env.COMMON_CI_SCRIPTS ||
    Buffer.byteLength(env.COMMON_CI_SCRIPTS) > 4096
  )
    throw Error("Explicit caller root, repository, project, commit and runtime are required");
  const scripts: unknown = JSON.parse(env.COMMON_CI_SCRIPTS);
  if (
    !Array.isArray(scripts) ||
    scripts.length < 1 ||
    scripts.length > 16 ||
    scripts.some(
      (name) => typeof name !== "string" || !/^[A-Za-z0-9][A-Za-z0-9:_-]{0,127}$/.test(name),
    ) ||
    new Set(scripts).size !== scripts.length
  )
    throw Error("Select a bounded nonempty list of distinct caller package scripts");
  return {
    workspace,
    repository,
    expectedRepository,
    projectId,
    sourceSha,
    bunVersion,
    scripts: scripts as string[],
    installDependencies: env.COMMON_CI_INSTALL === "true",
  };
}

async function jsonFile(root: string, path: string): Promise<unknown> {
  const file = join(root, path),
    info = await lstat(file);
  if (
    !info.isFile() ||
    info.isSymbolicLink() ||
    info.size > 2_000_000 ||
    (await realpath(file)) !== file
  )
    throw Error("Caller metadata must be an exact bounded regular file");
  return JSON.parse(await readFile(file, "utf8")) as unknown;
}

async function git(root: string, args: string[]): Promise<string> {
  const child = Bun.spawn(["git", "-C", root, ...args], { stdout: "pipe", stderr: "pipe" });
  const [output, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  if (code !== 0) throw Error("Caller Git source identity is unavailable");
  return output.trim();
}

async function verifyCheckout(input: CallerSourceInput): Promise<void> {
  if (
    (await realpath(input.workspace)) !== input.workspace ||
    (await git(input.workspace, ["rev-parse", "--show-toplevel"])) !== input.workspace ||
    (await git(input.workspace, ["rev-parse", "HEAD"])) !== input.sourceSha ||
    (await git(input.workspace, ["status", "--porcelain=v1", "--untracked-files=no"]))
  )
    throw Error("Caller checkout differs from its exact clean source commit");
}

/** Uses GITHUB_WORKSPACE, never this common action's own downloaded source root. */
export async function runCallerSource(input: CallerSourceInput): Promise<number> {
  if (Bun.version !== input.bunVersion)
    throw Error("Caller-selected Bun version differs from execution");
  await verifyCheckout(input);
  const project = await jsonFile(input.workspace, "project.json");
  const manifest = await jsonFile(input.workspace, "package.json");
  if (
    !object(project) ||
    project.schemaVersion !== "1.0.0" ||
    project.kind !== "project" ||
    project.projectId !== input.projectId
  )
    throw Error("Caller project identity differs from the explicit owner input");
  if (!object(manifest) || !object(manifest.scripts))
    throw Error("Caller package scripts are unavailable");
  for (const name of input.scripts)
    if (
      !Object.hasOwn(manifest.scripts, name) ||
      typeof manifest.scripts[name] !== "string" ||
      !manifest.scripts[name].trim()
    )
      throw Error("Selected caller package script is unavailable");
  const commands = [
    ...(input.installDependencies
      ? [[process.execPath, "--no-env-file", "install", "--frozen-lockfile", "--ignore-scripts"]]
      : []),
    ...input.scripts.map((name) => [process.execPath, "--no-env-file", "run", name]),
  ];
  for (const argv of commands) {
    const child = Bun.spawn(argv, {
      cwd: input.workspace,
      stdin: "ignore",
      stdout: "inherit",
      stderr: "inherit",
    });
    const exitCode = await child.exited;
    if (exitCode !== 0) return exitCode;
    await verifyCheckout(input);
  }
  console.log(
    `Caller source scripts passed: ${input.projectId} ${input.sourceSha} Bun ${input.bunVersion}`,
  );
  return 0;
}

if (import.meta.main) {
  try {
    process.exitCode = await runCallerSource(parseCallerSourceInput(process.env));
  } catch {
    console.error(
      "Caller source checks refused: inspect the explicit inputs, checkout and owner commands",
    );
    process.exitCode = 1;
  }
}
