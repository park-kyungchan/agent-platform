import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open, realpath } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";

export const defaultSourceAuthorPath = ".github/source-author.json";
const maxBytes = 16_384;
const oid = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{40}$/.test(value);
const id = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value);
const thread = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value);
const nullableId = (value: unknown): boolean => value === null || id(value);
const hash = (value: Uint8Array): string => createHash("sha256").update(value).digest("hex");

export interface SourceOwner {
  repository: string;
  projectId: string;
  domainId: string;
}
export interface AuthorLogicalWork {
  domainId: string;
  projectId: string;
  workId: string;
  taskId: string;
}
export interface NativeSourceAuthor {
  runtime: "native-codex" | "native-hermes";
  threadId: string | null;
  sessionId: string | null;
  nativeTaskId: string | null;
  nativeRunId: string | null;
  observedVia: "CODEX_THREAD_ID" | "pre_llm_call.session_id";
}
export interface SourceAuthorMetadata {
  schema: "agent-platform.source-author-metadata.v1";
  sourceOwner: SourceOwner;
  sourceBaseSha: string;
  logicalWork: AuthorLogicalWork;
  author: NativeSourceAuthor;
  capturedAtUtc: string;
  nativeAuthorshipAuthenticated: false;
  publisherIdentityInferred: false;
}
export interface SourceAuthorReadback {
  schema: "agent-platform.source-author-readback.v1";
  sourceHead: string;
  sourceTree: string;
  metadataPath: string;
  metadataGitBlob: string;
  metadataSha256: string;
  metadata: SourceAuthorMetadata;
  nativeAuthorshipAuthenticated: false;
  publisherIdentityInferred: false;
}

function record(value: unknown, keys: string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).sort().join("|") !== [...keys].sort().join("|")
  )
    throw Error("Exact source-author fields required");
  return value as Record<string, unknown>;
}

export function sourceAuthorPath(value: string): string {
  if (
    value.length > 256 ||
    !/^[A-Za-z0-9_.\/-]+$/.test(value) ||
    isAbsolute(value) ||
    value.split("/").some((part) => ["", ".", "..", ".git"].includes(part)) ||
    !value.endsWith(".json")
  )
    throw Error("Safe source-author metadata path required");
  return value;
}

/** A closed, generated JSON representation rejects duplicate keys and lossy spellings. */
export function parseSourceAuthorJson(bytes: Uint8Array): unknown {
  if (!bytes.length || bytes.length > maxBytes) throw Error("Bounded source-author JSON required");
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const value: unknown = JSON.parse(text);
  if (JSON.stringify(value, null, 2) + "\n" !== text)
    throw Error("Unambiguous generated source-author JSON required");
  return value;
}

function scope(value: unknown): { sourceOwner: SourceOwner; logicalWork: AuthorLogicalWork } {
  const input = record(value, ["sourceOwner", "logicalWork"]);
  const owner = record(input.sourceOwner, ["repository", "projectId", "domainId"]);
  const logical = record(input.logicalWork, ["domainId", "projectId", "workId", "taskId"]);
  if (
    typeof owner.repository !== "string" ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(owner.repository) ||
    !id(owner.projectId) ||
    !id(owner.domainId) ||
    !Object.values(logical).every(id)
  )
    throw Error("Explicit source owner and logical Work required");
  return structuredClone(input) as { sourceOwner: SourceOwner; logicalWork: AuthorLogicalWork };
}

export function validateSourceAuthorMetadata(value: unknown): SourceAuthorMetadata {
  const input = record(value, [
    "schema",
    "sourceOwner",
    "sourceBaseSha",
    "logicalWork",
    "author",
    "capturedAtUtc",
    "nativeAuthorshipAuthenticated",
    "publisherIdentityInferred",
  ]);
  const selected = scope({ sourceOwner: input.sourceOwner, logicalWork: input.logicalWork });
  const author = record(input.author, [
    "runtime",
    "threadId",
    "sessionId",
    "nativeTaskId",
    "nativeRunId",
    "observedVia",
  ]);
  if (
    input.schema !== "agent-platform.source-author-metadata.v1" ||
    !oid(input.sourceBaseSha) ||
    typeof input.capturedAtUtc !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(input.capturedAtUtc) ||
    !Number.isFinite(Date.parse(input.capturedAtUtc)) ||
    new Date(input.capturedAtUtc).toISOString() !== input.capturedAtUtc ||
    input.nativeAuthorshipAuthenticated !== false ||
    input.publisherIdentityInferred !== false ||
    !nullableId(author.nativeTaskId) ||
    !nullableId(author.nativeRunId) ||
    !(
      (author.runtime === "native-codex" &&
        thread(author.threadId) &&
        author.sessionId === null &&
        author.observedVia === "CODEX_THREAD_ID") ||
      (author.runtime === "native-hermes" &&
        author.threadId === null &&
        id(author.sessionId) &&
        author.observedVia === "pre_llm_call.session_id")
    ) ||
    [author.nativeTaskId, author.nativeRunId].some(
      (native) => native !== null && Object.values(selected.logicalWork).includes(String(native)),
    )
  )
    throw Error("Observed native author and explicit nullable Task/Run required");
  return structuredClone(input) as unknown as SourceAuthorMetadata;
}

/** Runs in the native author's process, never in GHA to invent an original identity. */
export function captureCodexSourceAuthor(
  selection: unknown,
  sourceBaseSha: string,
  env: NodeJS.ProcessEnv,
  capturedAtUtc = new Date().toISOString(),
): SourceAuthorMetadata {
  if (env.GITHUB_ACTIONS === "true" || env.CI === "true" || !thread(env.CODEX_THREAD_ID))
    throw Error("Native Codex source-author environment required before publication");
  const selected = scope(selection);
  return validateSourceAuthorMetadata({
    schema: "agent-platform.source-author-metadata.v1",
    ...selected,
    sourceBaseSha,
    author: {
      runtime: "native-codex",
      threadId: env.CODEX_THREAD_ID,
      sessionId: null,
      nativeTaskId: null,
      nativeRunId: null,
      observedVia: "CODEX_THREAD_ID",
    },
    capturedAtUtc,
    nativeAuthorshipAuthenticated: false,
    publisherIdentityInferred: false,
  });
}

/** Native Hermes owner supplies only these documented hook fields, not a raw event/session. */
export function captureHermesHookSourceAuthor(
  selection: unknown,
  sourceBaseSha: string,
  observedHook: unknown,
  capturedAtUtc = new Date().toISOString(),
  env: NodeJS.ProcessEnv = process.env,
): SourceAuthorMetadata {
  if (env.GITHUB_ACTIONS === "true" || env.CI === "true")
    throw Error("Native Hermes source-author observation required before publication");
  const hook = record(observedHook, ["hook_event_name", "session_id", "extra"]);
  const extra = record(hook.extra, ["task_id", "run_id"]);
  if (hook.hook_event_name !== "pre_llm_call") throw Error("Observed Hermes hook required");
  return validateSourceAuthorMetadata({
    schema: "agent-platform.source-author-metadata.v1",
    ...scope(selection),
    sourceBaseSha,
    author: {
      runtime: "native-hermes",
      threadId: null,
      sessionId: hook.session_id,
      nativeTaskId: extra.task_id,
      nativeRunId: extra.run_id,
      observedVia: "pre_llm_call.session_id",
    },
    capturedAtUtc,
    nativeAuthorshipAuthenticated: false,
    publisherIdentityInferred: false,
  });
}

export async function sourceAuthorGit(root: string, args: string[]): Promise<string> {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "ignore",
    env: { ...process.env, GIT_NO_REPLACE_OBJECTS: "1", GIT_TERMINAL_PROMPT: "0" },
  });
  const [output, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  if (code !== 0 || output.length > 2_000_000)
    throw Error("Source-author Git readback unavailable");
  return output.trimEnd();
}

async function regularBytes(path: string): Promise<Buffer> {
  if ((await realpath(path)) !== path) throw Error("Exact source-author file required");
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat();
    if (!before.isFile() || before.size > maxBytes)
      throw Error("Bounded source-author file required");
    const bytes = await file.readFile();
    const after = await file.stat();
    if (
      before.size !== bytes.length ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs
    )
      throw Error("Source-author metadata changed during readback");
    return bytes;
  } finally {
    await file.close();
  }
}

/** Bind committed attribution to the actual execution/publication head; no committed self-hash. */
export async function verifyCommittedSourceAuthor(input: {
  workspace: string;
  repository: string;
  projectId: string;
  sourceHead: string;
  metadataPath?: string;
}): Promise<SourceAuthorReadback> {
  const root = input.workspace,
    path = sourceAuthorPath(input.metadataPath ?? defaultSourceAuthorPath);
  if (
    !isAbsolute(root) ||
    resolve(root) !== root ||
    (await realpath(root)) !== root ||
    !oid(input.sourceHead) ||
    (await sourceAuthorGit(root, ["rev-parse", "--show-toplevel"])) !== root ||
    (await sourceAuthorGit(root, ["rev-parse", "HEAD"])) !== input.sourceHead
  )
    throw Error("Authoritative source-author checkout required");
  const sourceTree = await sourceAuthorGit(root, ["rev-parse", input.sourceHead + "^{tree}"]);
  const entry = await sourceAuthorGit(root, ["ls-tree", "-z", input.sourceHead, "--", path]);
  const match = /^100644 blob ([a-f0-9]{40})\t([^\u0000]+)\u0000$/.exec(entry);
  if (!oid(sourceTree) || !match || match[2] !== path)
    throw Error("Exactly one committed regular source-author record required");
  const metadataGitBlob = match[1]!;
  const size = Number(await sourceAuthorGit(root, ["cat-file", "-s", metadataGitBlob]));
  if (!Number.isInteger(size) || size < 1 || size > maxBytes)
    throw Error("Bounded committed source-author record required");
  const text = (await sourceAuthorGit(root, ["cat-file", "blob", metadataGitBlob])) + "\n";
  const bytes = Buffer.from(text),
    working = await regularBytes(join(root, path));
  if (bytes.length !== size || !bytes.equals(working))
    throw Error("Working source-author bytes differ from the committed blob");
  const metadata = validateSourceAuthorMetadata(parseSourceAuthorJson(bytes));
  const project: unknown = JSON.parse(
    await sourceAuthorGit(root, ["show", input.sourceHead + ":project.json"]),
  );
  if (
    metadata.sourceOwner.repository !== input.repository ||
    metadata.sourceOwner.projectId !== input.projectId ||
    !project ||
    typeof project !== "object" ||
    !("schemaVersion" in project) ||
    !("kind" in project) ||
    !("projectId" in project) ||
    !("domainId" in project) ||
    project.schemaVersion !== "1.0.0" ||
    project.kind !== "project" ||
    project.projectId !== input.projectId ||
    project.domainId !== metadata.sourceOwner.domainId
  )
    throw Error("Source-author owner differs from the authoritative caller");
  await sourceAuthorGit(root, [
    "merge-base",
    "--is-ancestor",
    metadata.sourceBaseSha,
    input.sourceHead,
  ]);
  if (
    (await sourceAuthorGit(root, ["rev-parse", "HEAD"])) !== input.sourceHead ||
    !(await regularBytes(join(root, path))).equals(bytes)
  )
    throw Error("Source-author checkout changed during readback");
  return {
    schema: "agent-platform.source-author-readback.v1",
    sourceHead: input.sourceHead,
    sourceTree,
    metadataPath: path,
    metadataGitBlob,
    metadataSha256: hash(bytes),
    metadata,
    nativeAuthorshipAuthenticated: false,
    publisherIdentityInferred: false,
  };
}
