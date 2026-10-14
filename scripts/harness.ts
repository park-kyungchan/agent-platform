import { readFile } from "node:fs/promises";
import {
  openGitWork,
  qualifyGitWorkStart,
  readGitWorkContext,
  validateGitWorkRecords,
  validateGitWorkTransition,
  type ContextRequest,
  type GitWorkSelection,
  type StartRequest,
} from "../packages/work-continuity/src/git-work/index.ts";
import { readDocumentationRegistry } from "../packages/work-continuity/src/runtime-docs/index.ts";

const usage =
  "bun run harness context|validate|start|docs <request.json>; transition <before.json> <after.json>; source <snapshot.json> <relative-path>";
async function readRequest(path: string | undefined): Promise<unknown> {
  if (!path) throw Error(usage);
  const bytes = await readFile(path);
  if (bytes.length > 1_048_576) throw Error("Request exceeds the bounded input size");
  return JSON.parse(bytes.toString("utf8")) as unknown;
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("Request must be a JSON object");
  return value as Record<string, unknown>;
}
export function gitReadEnvironment(): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    GIT_OPTIONAL_LOCKS: "0",
    GIT_TERMINAL_PROMPT: "0",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_CONFIG_GLOBAL: "/dev/null",
  };
  for (const key of [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
  ])
    delete env[key];
  return env;
}
export async function resolveGitWorkSelection(value: unknown): Promise<GitWorkSelection> {
  const request = object(value);
  if (
    typeof request.repositoryRoot !== "string" ||
    typeof request.commit !== "string" ||
    !request.commit ||
    request.commit.length > 256 ||
    request.commit.includes("\0")
  )
    throw Error("Explicit repository root and bounded local commit/ref required");
  const child = Bun.spawn(
    [
      "git",
      "--no-replace-objects",
      "-C",
      request.repositoryRoot,
      "rev-parse",
      "--verify",
      "--end-of-options",
      request.commit + "^{commit}",
    ],
    { stdout: "pipe", stderr: "ignore", env: gitReadEnvironment() },
  );
  const commit = (await new Response(child.stdout).text()).trim();
  if ((await child.exited) !== 0 || !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(commit))
    throw Error("Selected local commit/ref unavailable");
  return { ...request, commit } as unknown as GitWorkSelection;
}
export async function runHarness(argv: string[]): Promise<unknown> {
  const [command, path, extra] = argv;
  if (argv.length !== (command === "transition" || command === "source" ? 3 : 2))
    throw Error(usage);
  const request = object(await readRequest(path));
  switch (command) {
    case "context": {
      const reader = await openGitWork(await resolveGitWorkSelection(request.snapshot));
      return readGitWorkContext(reader, object(request.context) as unknown as ContextRequest);
    }
    case "validate":
      return validateGitWorkRecords(await openGitWork(await resolveGitWorkSelection(request)));
    case "transition":
      return validateGitWorkTransition(
        await openGitWork(await resolveGitWorkSelection(request)),
        await openGitWork(await resolveGitWorkSelection(await readRequest(extra))),
      );
    case "source": {
      if (!extra) throw Error(usage);
      return (await openGitWork(await resolveGitWorkSelection(request))).readSource({
        kind: "snapshot",
        path: extra,
      });
    }
    case "start":
      return qualifyGitWorkStart(request as unknown as StartRequest);
    case "docs":
      return (await readDocumentationRegistry()).select(request);
    default:
      throw Error(usage);
  }
}
if (import.meta.main) {
  try {
    const result = await runHarness(process.argv.slice(2));
    console.log(JSON.stringify(result, null, 2));
    if (result && typeof result === "object") {
      const value = result as Record<string, unknown>;
      if (value.valid === false || value.status === "held") process.exitCode = 1;
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Common Harness request failed");
    process.exitCode = 1;
  }
}
