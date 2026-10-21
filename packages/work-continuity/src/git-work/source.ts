import { createHash } from "node:crypto";
import { realpath } from "node:fs/promises";
import type {
  GitWorkReader,
  GitWorkRecord,
  GitWorkSelection,
  JsonValue,
  RecordKind,
  RecordReference,
  ResolvedSource,
  SourceReference,
} from "./types.ts";
import { assertRecord, assertRecordReference } from "./records.ts";

const directories: Record<RecordKind, string> = {
  work: "",
  spec: "specs",
  plan: "plans",
  task: "tasks",
  decision: "decisions",
  relation: "relations",
  effect: "effects",
  "effect-fence": "fences",
  evidence: "evidence",
  input: "inputs",
};
export function encodedId(id: string): string {
  if (
    typeof id !== "string" ||
    !id.trim() ||
    id !== id.trim() ||
    id.length > 1024 ||
    id === "." ||
    id === ".." ||
    /[\u0000-\u001f]/.test(id)
  )
    throw Error("INVALID_RECORD_ID");
  return encodeURIComponent(id);
}
export function gitWorkPath(projectId: string, workId: string): string {
  return `work/${encodedId(projectId)}/${encodedId(workId)}`;
}
export function gitRecordPath(
  projectId: string,
  workId: string,
  reference: RecordReference,
): string {
  if (!Object.hasOwn(directories, reference.kind)) throw Error("INVALID_RECORD_KIND");
  assertRecordReference(reference);
  const root = gitWorkPath(projectId, workId);
  return reference.kind === "work"
    ? `${root}/work.json`
    : `${root}/${directories[reference.kind]}/${encodedId(reference.id)}${reference.revision === undefined ? "" : `@${reference.revision}`}.json`;
}
export function assertGitPath(path: string): void {
  if (
    typeof path !== "string" ||
    !path ||
    path.length > 4096 ||
    path.includes("\\") ||
    path.includes(":") ||
    /[\u0000-\u001f]/.test(path) ||
    path.split("/").some((part) => !part || part === "." || part === "..")
  )
    throw Error("UNSAFE_GIT_PATH");
}
export function assertCommit(commit: string): void {
  if (typeof commit !== "string" || !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(commit))
    throw Error("IMMUTABLE_COMMIT_REQUIRED");
}
export function selectFragment(value: JsonValue, pointer = "#"): JsonValue {
  if (pointer === "#") return value;
  if (!pointer.startsWith("#/")) throw Error("INVALID_SOURCE_FRAGMENT");
  let selected = value;
  for (const escaped of pointer.slice(2).split("/")) {
    if (/~(?![01])/.test(escaped)) throw Error("INVALID_SOURCE_FRAGMENT");
    const key = escaped.replaceAll("~1", "/").replaceAll("~0", "~");
    if (selected === null || typeof selected !== "object" || !Object.hasOwn(selected, key))
      throw Error("SOURCE_FRAGMENT_MISSING");
    selected = (selected as { [key: string]: JsonValue })[key]!;
  }
  return selected;
}
const digest = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
function limit(value: number | undefined, fallback: number, maximum: number): number {
  const actual = value ?? fallback;
  if (!Number.isSafeInteger(actual) || actual < 1 || actual > maximum)
    throw Error("INVALID_GIT_READ_LIMIT");
  return actual;
}
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object") {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

// Deliberately internal: metadata, serialized proofs and object copies are not byte receipts.
const originalBytes = new WeakMap<
  ResolvedSource,
  {
    bytes: Uint8Array;
    serialized: string;
    objects: object[];
    descriptors: PropertyDescriptorMap[];
    prototypes: object[];
  }
>();
function objectGraph(value: unknown, result: object[] = [], seen = new Set<object>()): object[] {
  if (value !== null && typeof value === "object" && !seen.has(value)) {
    seen.add(value);
    result.push(value);
    for (const descriptor of Object.values(Object.getOwnPropertyDescriptors(value)))
      if ("value" in descriptor) objectGraph(descriptor.value, result, seen);
  }
  return result;
}
function enrollOriginal(result: ResolvedSource, bytes: Uint8Array): ResolvedSource {
  const objects = objectGraph(result);
  originalBytes.set(result, {
    bytes: bytes.slice(),
    serialized: JSON.stringify(result),
    objects,
    descriptors: objects.map((value) => Object.getOwnPropertyDescriptors(value)),
    prototypes: objects.map((value) => Object.getPrototypeOf(value)),
  });
  return result;
}
/** Same-process canonical reader receipt only, with defensive full-blob bytes. */
export function verifiedOriginalBytes(source: ResolvedSource): Uint8Array {
  const proof = originalBytes.get(source);
  if (!proof) throw Error("VERIFIED_ORIGINAL_RECEIPT_REQUIRED");
  const objects = objectGraph(source);
  if (
    objects.length !== proof.objects.length ||
    objects.some((value, index) => {
      if (
        value !== proof.objects[index] ||
        Object.getPrototypeOf(value) !== proof.prototypes[index]
      )
        return true;
      const descriptors = Object.getOwnPropertyDescriptors(value),
        expected = proof.descriptors[index]!;
      return (
        Reflect.ownKeys(descriptors).length !== Reflect.ownKeys(expected).length ||
        Reflect.ownKeys(expected).some((key) => {
          const actual = Object.getOwnPropertyDescriptor(value, key),
            prior = Object.getOwnPropertyDescriptor(proof.objects[index]!, key);
          const saved = expected[key as string];
          return (
            !actual ||
            !prior ||
            !saved ||
            ["value", "get", "set", "writable", "enumerable", "configurable"].some(
              (field) =>
                actual[field as keyof PropertyDescriptor] !==
                saved[field as keyof PropertyDescriptor],
            )
          );
        })
      );
    }) ||
    JSON.stringify(source) !== proof.serialized
  )
    throw Error("VERIFIED_ORIGINAL_RECEIPT_REQUIRED");
  return proof.bytes.slice();
}

export function gitReadEnvironment(): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    GIT_TERMINAL_PROMPT: "0",
    GIT_OPTIONAL_LOCKS: "0",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_NO_LAZY_FETCH: "1",
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

/** Object reads never invoke a shell, filters, network, checkout mutation or hooks. */
async function git(root: string, args: string[], maxBytes: number): Promise<Uint8Array> {
  const env = gitReadEnvironment();
  const child = Bun.spawn(
    ["git", "--no-replace-objects", "--no-pager", "--literal-pathspecs", "-C", root, ...args],
    { env, stdout: "pipe", stderr: "ignore" },
  );
  const stream = child.stdout.getReader();
  const chunks: Uint8Array[] = [];
  let count = 0;
  const timer = setTimeout(() => child.kill(), 10_000);
  try {
    while (true) {
      const result = await stream.read();
      if (result.done) break;
      count += result.value.byteLength;
      if (count > maxBytes) {
        child.kill();
        throw Error("GIT_OUTPUT_LIMIT");
      }
      chunks.push(result.value);
    }
    if ((await child.exited) !== 0) throw Error("GIT_SOURCE_UNAVAILABLE");
    const bytes = new Uint8Array(count);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  } finally {
    clearTimeout(timer);
    stream.releaseLock();
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}
const decode = (bytes: Uint8Array) => new TextDecoder("utf-8", { fatal: true }).decode(bytes);

/** Resolve one bounded local ref to an immutable snapshot without fetching or changing Git. */
export async function resolveGitWorkSelection(value: unknown): Promise<GitWorkSelection> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("Explicit Git Work selection must be an object");
  const request = value as Record<string, unknown>;
  if (
    typeof request.repositoryRoot !== "string" ||
    typeof request.commit !== "string" ||
    !request.commit ||
    request.commit.length > 256 ||
    request.commit.includes("\0")
  )
    throw Error("Explicit repository root and bounded local commit/ref required");
  const commit = decode(
    await git(
      request.repositoryRoot,
      ["rev-parse", "--verify", "--end-of-options", request.commit + "^{commit}"],
      256,
    ),
  ).trim();
  assertCommit(commit);
  return { ...request, commit } as unknown as GitWorkSelection;
}

export async function openGitSourceSnapshot(input: GitWorkSelection) {
  input = freeze(structuredClone(input));
  assertCommit(input.commit);
  encodedId(input.repositoryId);
  const selection = input;
  const maxBlobBytes = limit(input.maxBlobBytes, 2_097_152, 16_777_216);
  const maxRecords = limit(input.maxRecords, 2048, 10_000);
  const maxTotalBytes = limit(input.maxTotalBytes, 33_554_432, 268_435_456);
  let retainedBytes = 0;
  const root = await realpath(input.repositoryRoot);
  const top = decode(await git(root, ["rev-parse", "--show-toplevel"], 8192)).trim();
  if ((await realpath(top)) !== root) throw Error("EXACT_REPOSITORY_ROOT_REQUIRED");
  const workPath = gitWorkPath(input.projectId, input.workId);
  const repositories = new Map(Object.entries(input.repositories ?? {}));
  repositories.set(input.repositoryId, root);
  const checkedRoots = new Set([root]);
  const commits = new Set<string>();
  const sourceCache = new Map<
    string,
    { document: JsonValue; blobOid: string; bytes: Uint8Array }
  >();
  async function source(reference: SourceReference): Promise<ResolvedSource> {
    reference = freeze(structuredClone(reference));
    if (reference.kind === "native") {
      if (
        ![reference.ownerId, reference.locator].every(
          (value) =>
            typeof value === "string" &&
            value.trim().length > 0 &&
            value.length <= 12_000 &&
            !value.includes("\0"),
        ) ||
        !reference.receipt ||
        !["snapshot", "git"].includes(reference.receipt.kind)
      )
        throw Error("INVALID_NATIVE_SOURCE_REFERENCE");
      const receipt = await source(reference.receipt);
      return enrollOriginal(
        { ...receipt, declaredNativeSource: reference },
        verifiedOriginalBytes(receipt),
      );
    }
    if (!["snapshot", "git"].includes(reference.kind)) throw Error("INVALID_SOURCE_REFERENCE");
    assertGitPath(reference.path);
    const repositoryId = reference.kind === "git" ? reference.repositoryId : input.repositoryId;
    const commit = reference.kind === "git" ? reference.commit : input.commit;
    assertCommit(commit);
    const repositoryRoot = repositories.get(repositoryId);
    if (!repositoryRoot) throw Error("SOURCE_REPOSITORY_NOT_ADMITTED");
    const canonical = await realpath(repositoryRoot);
    if (!checkedRoots.has(canonical)) {
      const externalTop = decode(
        await git(canonical, ["rev-parse", "--show-toplevel"], 8192),
      ).trim();
      if ((await realpath(externalTop)) !== canonical)
        throw Error("EXACT_REPOSITORY_ROOT_REQUIRED");
      checkedRoots.add(canonical);
    }
    const commitKey = JSON.stringify([repositoryId, canonical, commit]);
    if (!commits.has(commitKey)) {
      const actual = decode(
        await git(
          canonical,
          ["rev-parse", "--verify", "--end-of-options", `${commit}^{commit}`],
          256,
        ),
      ).trim();
      if (actual !== commit) throw Error("SOURCE_IS_NOT_EXACT_COMMIT");
      commits.add(commitKey);
    }
    const key = JSON.stringify([commitKey, reference.path]);
    let cached = sourceCache.get(key);
    if (!cached) {
      const entry = decode(
        await git(canonical, ["ls-tree", "-z", commit, "--", reference.path], 16_384),
      );
      const match = /^(100644|100755) blob ([a-f0-9]{40}|[a-f0-9]{64})\t([^\0]+)\0$/.exec(entry);
      if (!match || match[3] !== reference.path) throw Error("REGULAR_GIT_BLOB_REQUIRED");
      const blobOid = match[2]!;
      const size = Number(decode(await git(canonical, ["cat-file", "-s", blobOid], 128)).trim());
      if (!Number.isSafeInteger(size) || size > maxBlobBytes) throw Error("SELECTED_BLOB_LIMIT");
      if (retainedBytes + size > maxTotalBytes) throw Error("SELECTED_TOTAL_BYTES_LIMIT");
      const bytes = await git(canonical, ["cat-file", "blob", blobOid], maxBlobBytes);
      if (bytes.byteLength !== size) throw Error("GIT_BLOB_LENGTH_MISMATCH");
      if (
        createHash(blobOid.length === 64 ? "sha256" : "sha1")
          .update(`blob ${bytes.byteLength}\0`)
          .update(bytes)
          .digest("hex") !== blobOid
      )
        throw Error("GIT_BLOB_IDENTITY_MISMATCH");
      const text = decode(bytes);
      let document: JsonValue;
      try {
        document = JSON.parse(text) as JsonValue;
      } catch {
        document = text;
      }
      cached = { document, bytes, blobOid };
      // Bound retained content independently of the number of tree entries.
      if (sourceCache.size >= maxRecords) throw Error("SOURCE_CACHE_RECORD_LIMIT");
      retainedBytes += bytes.byteLength;
      sourceCache.set(key, cached);
    }
    const sha256 = digest(cached.bytes);
    if (reference.sha256 !== undefined && reference.sha256 !== sha256)
      throw Error("SOURCE_CONTENT_MISMATCH");
    return enrollOriginal(
      {
        reference: structuredClone(reference),
        repositoryId,
        commit,
        path: reference.path,
        blobOid: cached.blobOid,
        byteLength: cached.bytes.byteLength,
        sha256,
        document: structuredClone(cached.document),
        value: structuredClone(selectFragment(cached.document, reference.fragment)),
        verificationClass: "git-object-source-only",
        nativeAuthorshipVerified: false,
      },
      cached.bytes,
    );
  }
  return Object.freeze({ selection, commit: input.commit, workPath, maxRecords, readSource: source,
    async listPaths(prefix: string): Promise<string[]> {
      assertGitPath(prefix.endsWith("/") ? prefix.slice(0, -1) : prefix);
      const paths = decode(await git(root, ["ls-tree", "-r", "-z", "--name-only", input.commit, "--", prefix], Math.min(maxRecords * 8192, 16_777_216))).split("\0").filter(Boolean);
      if (paths.length > maxRecords) throw Error("SOURCE_SNAPSHOT_RECORD_COUNT_LIMIT");
      return paths;
    },
  });
}

export async function openGitWork(input: GitWorkSelection): Promise<GitWorkReader> {
  const snapshot = await openGitSourceSnapshot(input);
  const { selection, workPath, maxRecords, readSource: source } = snapshot;
  input = selection;
  const root = input.repositoryRoot;
  async function readRecord(reference: RecordReference): Promise<GitWorkRecord> {
    reference = freeze(structuredClone(reference));
    const resolved = await source({
      kind: "snapshot",
      path: gitRecordPath(input.projectId, input.workId, reference),
    });
    const record = assertRecord(resolved.document);
    if (
      record.kind !== reference.kind ||
      record.id !== reference.id ||
      record.projectId !== input.projectId ||
      record.workId !== input.workId ||
      ((record.kind === "spec" || record.kind === "plan") && record.revision !== reference.revision)
    )
      throw Error("RECORD_IDENTITY_OR_SCOPE_MISMATCH");
    return record;
  }
  const work = await readRecord({ kind: "work", id: input.workId });
  if (work.kind !== "work" || work.repositoryId !== input.repositoryId)
    throw Error("WORK_REPOSITORY_MISMATCH");
  async function listRecords(): Promise<RecordReference[]> {
    const entries = decode(
      await git(
        root,
        ["ls-tree", "-r", "-z", "--name-only", input.commit, "--", `${workPath}/`],
        Math.min(maxRecords * 8192, 16_777_216),
      ),
    )
      .split("\0")
      .filter(Boolean);
    if (entries.length > maxRecords) throw Error("WORK_RECORD_COUNT_LIMIT");
    const records: RecordReference[] = [];
    for (const path of entries) {
      if (path === `${workPath}/work.json`) {
        records.push({ kind: "work", id: input.workId });
        continue;
      }
      const suffix = path.slice(workPath.length + 1).split("/");
      if (suffix.length !== 2 || !suffix[1]!.endsWith(".json"))
        throw Error("UNRECOGNIZED_WORK_RECORD_PATH");
      const kind = Object.entries(directories).find(
        ([, directory]) => directory === suffix[0],
      )?.[0] as RecordKind | undefined;
      if (!kind || kind === "work") throw Error("UNRECOGNIZED_WORK_RECORD_PATH");
      let id: string;
      const filename = suffix[1]!.slice(0, -5);
      const contract = kind === "spec" || kind === "plan";
      const versioned = contract ? /^(.*)@([1-9][0-9]*)$/.exec(filename) : null;
      if (contract && !versioned) throw Error("CONTRACT_REVISION_FILENAME_REQUIRED");
      try {
        id = decodeURIComponent(versioned ? versioned[1]! : filename);
      } catch {
        throw Error("INVALID_RECORD_FILENAME");
      }
      const reference: RecordReference = {
        kind,
        id,
        ...(versioned ? { revision: Number(versioned[2]) } : {}),
      };
      if (gitRecordPath(input.projectId, input.workId, reference) !== path)
        throw Error("NONCANONICAL_RECORD_FILENAME");
      records.push(reference);
    }
    return records;
  }
  return Object.freeze({
    selection,
    commit: input.commit,
    workPath,
    work: freeze(work),
    readRecord,
    listRecords,
    readSource: source,
  });
}
