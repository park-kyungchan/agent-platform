import { isDeepStrictEqual } from "node:util";
import {
  openGitWork,
  resolveGitWorkSelection,
  gitRecordPath,
  verifiedOriginalBytes,
} from "../git-work/source.ts";
import {
  assertRecord,
  recordKey,
  recordDependencies,
  effectInventory,
  validateRecordSources,
  validateRecordSet,
  dependencyFindings,
} from "../git-work/records.ts";
import type {
  GitWorkReader,
  GitWorkRecord,
  RecordReference,
  ResolvedSource,
  SourceReference,
  ExternalGitSource,
  SnapshotSource,
} from "../git-work/types.ts";
import { constants } from "node:fs";
import { open, realpath } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";
import { sha256 } from "../context/source";
import { lexicalRecall, type LexicalRecallResult } from "./lexical-recall";
import {
  qualifySourceClosure,
  qualifyMemory,
  sourceKey,
  type AuthoritativeClaim,
  type AuthoritativeClaimReceipt,
  type DerivationReceipt,
  type DerivedMemory,
  type Episode,
  type MemoryPolicy,
  type PortableSource,
  type SourceObservation,
} from "./qualification";

const maxFileBytes = 2_097_152,
  maxOriginalBytes = 16_777_216,
  maxItems = 1000;
type ObjectValue = Record<string, unknown>;
interface FilePin {
  path: string;
  sha256: string;
}
interface RecallRequest {
  schema: "codex.local-memory-recall.request.v1";
  projectId: string;
  workId: string;
  root: string;
  dataset: FilePin;
  policy: FilePin;
  query: string;
  claimKey: string;
  limit?: number;
  maxChars?: number;
}
interface Dataset {
  schema: "codex.local-memory-dataset.v1";
  projectId: string;
  workId: string;
  episodes: Episode[];
  memories: DerivedMemory[];
  receipts: DerivationReceipt[];
  authoritativeClaims: AuthoritativeClaim[];
  authoritativeReceipts: AuthoritativeClaimReceipt[];
  sourceBindings: Array<{ source: PortableSource; path: string }>;
}
interface PolicyFile {
  schema: "codex.local-memory-policy.v1";
  projectId: string;
  workId: string;
  activeGenerationId: string;
  policy: MemoryPolicy;
}
function invalid(label: string): never {
  throw Error(`Invalid local Recall ${label}`);
}
function object(value: unknown, keys: readonly string[], label: string): ObjectValue {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).some((key) => !keys.includes(key))
  )
    return invalid(label);
  return value as ObjectValue;
}
function text(value: unknown, label: string, max = 256): string {
  if (typeof value !== "string" || !value.trim() || value.length > max) return invalid(label);
  return value;
}
function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value) || value.length > maxItems) return invalid(label);
  return value;
}
function strings(value: unknown, label: string): string[] {
  return array(value, label).map((row) => text(row, label, 12_000));
}
function choice<T extends string>(value: unknown, allowed: readonly T[], label: string): T {
  if (typeof value !== "string" || !allowed.includes(value as T)) return invalid(label);
  return value as T;
}
function digest(value: unknown, label: string): string {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/.test(value)) return invalid(label);
  return value;
}
function exactPath(value: unknown, root: string): string {
  const path = text(value, "owner path", 12_000),
    part = relative(root, path);
  if (
    !isAbsolute(path) ||
    resolve(path) !== path ||
    part === ".." ||
    part.startsWith("../") ||
    isAbsolute(part)
  )
    return invalid("owner path");
  return path;
}
function filePin(value: unknown, root: string): FilePin {
  const row = object(value, ["path", "sha256"], "file pin");
  return { path: exactPath(row.path, root), sha256: digest(row.sha256, "file pin hash") };
}
function portable(value: unknown): PortableSource {
  const row = object(
    value,
    ["repositoryId", "revision", "relativePath", "sha256", "byteLength"],
    "portable source",
  );
  const path = text(row.relativePath, "portable source path", 12_000);
  if (
    path.includes("\\") ||
    /[\u0000-\u001f]/.test(path) ||
    /^[A-Za-z]:/.test(path) ||
    path.split("/").some((part) => !part || part === "." || part === "..")
  )
    return invalid("portable source path");
  if (
    !Number.isSafeInteger(row.byteLength) ||
    Number(row.byteLength) < 0 ||
    Number(row.byteLength) > maxFileBytes
  )
    return invalid("portable source length");
  return {
    repositoryId: text(row.repositoryId, "repository identity"),
    revision: text(row.revision, "source revision"),
    relativePath: path,
    sha256: digest(row.sha256, "source hash"),
    byteLength: Number(row.byteLength),
  };
}
function claim(value: unknown): DerivedMemory["claim"] {
  const row = object(value, ["key", "value"], "claim");
  return { key: text(row.key, "claim key"), value: text(row.value, "claim value", 12_000) };
}
function scope(value: unknown): DerivedMemory["scope"] {
  const row = object(value, ["kind", "repositoryId"], "scope");
  if (row.kind === "common" && row.repositoryId === undefined) return { kind: "common" };
  if (row.kind === "repository")
    return { kind: "repository", repositoryId: text(row.repositoryId, "scope repository") };
  return invalid("scope");
}
function episode(value: unknown): Episode {
  const row = object(value, ["id", "provenance", "sources"], "Episode");
  return {
    id: text(row.id, "Episode identity"),
    provenance: choice(row.provenance, ["source-linked", "unknown"], "Episode provenance"),
    sources: array(row.sources, "Episode sources").map((value) => {
      const item = object(value, ["source", "fragmentIds"], "Episode source");
      return {
        source: portable(item.source),
        fragmentIds: strings(item.fragmentIds, "fragment identities"),
      };
    }),
  };
}
function memory(value: unknown): DerivedMemory {
  const row = object(
    value,
    [
      "id",
      "episodeId",
      "generationId",
      "policyRevision",
      "text",
      "contentHash",
      "sourceKeys",
      "scope",
      "claim",
    ],
    "memory",
  );
  return {
    id: text(row.id, "memory identity"),
    episodeId: text(row.episodeId, "memory Episode"),
    generationId: text(row.generationId, "memory generation"),
    policyRevision: text(row.policyRevision, "memory policy revision"),
    text: text(row.text, "memory text", 12_000),
    contentHash: digest(row.contentHash, "memory content hash"),
    sourceKeys: strings(row.sourceKeys, "memory sources"),
    scope: scope(row.scope),
    claim: claim(row.claim),
  };
}
function receipt(value: unknown): DerivationReceipt {
  const row = object(
    value,
    [
      "memoryId",
      "episodeId",
      "generationId",
      "policyRevision",
      "contentHash",
      "sourceKeys",
      "provenance",
      "claim",
      "scope",
      "episodeFingerprint",
    ],
    "derivation receipt",
  );
  return {
    memoryId: text(row.memoryId, "receipt memory"),
    episodeId: text(row.episodeId, "receipt Episode"),
    generationId: text(row.generationId, "receipt generation"),
    policyRevision: text(row.policyRevision, "receipt policy"),
    contentHash: digest(row.contentHash, "receipt content hash"),
    sourceKeys: strings(row.sourceKeys, "receipt sources"),
    provenance: choice(row.provenance, ["owner-verified", "unknown"], "receipt provenance"),
    claim: claim(row.claim),
    scope: scope(row.scope),
    episodeFingerprint: digest(row.episodeFingerprint, "receipt Episode fingerprint"),
  };
}
function authoritativeClaim(value: unknown): AuthoritativeClaim {
  const row = object(
    value,
    ["key", "value", "recordClass", "status", "source"],
    "authoritative claim",
  );
  return {
    ...claim({ key: row.key, value: row.value }),
    recordClass: choice(row.recordClass, ["decision", "spec"], "record class"),
    status: choice(row.status, ["adopted-current", "proposed", "superseded"], "record status"),
    source: portable(row.source),
  };
}
function authoritativeReceipt(value: unknown): AuthoritativeClaimReceipt {
  const row = object(
    value,
    ["sourceKey", "recordClass", "status", "key", "value", "provenance"],
    "authoritative receipt",
  );
  return {
    sourceKey: text(row.sourceKey, "authoritative receipt source", 12_000),
    ...claim({ key: row.key, value: row.value }),
    recordClass: choice(row.recordClass, ["decision", "spec"], "receipt record class"),
    status: choice(
      row.status,
      ["adopted-current", "proposed", "superseded"],
      "receipt record status",
    ),
    provenance: choice(
      row.provenance,
      ["owner-verified", "unknown"],
      "authoritative receipt provenance",
    ),
  };
}
function identities(rows: unknown[], key: (row: any) => string): void {
  if (new Set(rows.map(key)).size !== rows.length) invalid("ambiguous identities");
}
function dataset(value: unknown, root: string, projectId: string, workId: string): Dataset {
  const row = object(
    value,
    [
      "schema",
      "projectId",
      "workId",
      "episodes",
      "memories",
      "receipts",
      "authoritativeClaims",
      "authoritativeReceipts",
      "sourceBindings",
    ],
    "dataset",
  );
  if (
    row.schema !== "codex.local-memory-dataset.v1" ||
    row.projectId !== projectId ||
    row.workId !== workId
  )
    return invalid("dataset identity");
  const result: Dataset = {
    schema: row.schema,
    projectId,
    workId,
    episodes: array(row.episodes, "Episodes").map(episode),
    memories: array(row.memories, "memories").map(memory),
    receipts: array(row.receipts, "receipts").map(receipt),
    authoritativeClaims: array(row.authoritativeClaims, "authoritative claims").map(
      authoritativeClaim,
    ),
    authoritativeReceipts: array(row.authoritativeReceipts, "authoritative receipts").map(
      authoritativeReceipt,
    ),
    sourceBindings: array(row.sourceBindings, "source bindings").map((value) => {
      const item = object(value, ["source", "path"], "source binding"),
        source = portable(item.source),
        path = exactPath(item.path, root);
      if (path !== resolve(root, source.relativePath)) return invalid("source binding locator");
      return { source, path };
    }),
  };
  if (
    result.episodes.length +
      result.memories.length +
      result.receipts.length +
      result.authoritativeClaims.length +
      result.authoritativeReceipts.length +
      result.sourceBindings.length >
    maxItems
  )
    return invalid("aggregate dataset item budget");
  identities(result.episodes, (row) => row.id);
  identities(result.memories, (row) => row.id);
  identities(result.receipts, (row) => row.memoryId);
  identities(result.sourceBindings, (row) => row.path);
  identities(result.authoritativeReceipts, (row) => JSON.stringify([row.sourceKey, row.key]));
  const required = [
    ...result.episodes.flatMap((row) => row.sources.map((row) => row.source)),
    ...result.authoritativeClaims.map((row) => row.source),
  ];
  if (
    required.some(
      (source) =>
        !result.sourceBindings.some(
          (binding) =>
            sourceKey(binding.source) === sourceKey(source) &&
            binding.source.byteLength === source.byteLength,
        ),
    ) ||
    result.sourceBindings.some(
      (binding) =>
        !required.some(
          (source) =>
            sourceKey(source) === sourceKey(binding.source) &&
            source.byteLength === binding.source.byteLength,
        ),
    )
  )
    return invalid("source reference closure");
  return result;
}
function policy(value: unknown, projectId: string, workId: string): PolicyFile {
  const outer = object(
    value,
    ["schema", "projectId", "workId", "activeGenerationId", "policy"],
    "policy file",
  );
  if (
    outer.schema !== "codex.local-memory-policy.v1" ||
    outer.projectId !== projectId ||
    outer.workId !== workId
  )
    return invalid("policy identity");
  const row = object(
    outer.policy,
    ["revision", "excludedSourceHashes", "excludedFragmentIds", "aliases", "commonAdmissions"],
    "policy",
  );
  const result: MemoryPolicy = {
    revision: text(row.revision, "policy revision"),
    excludedSourceHashes: array(row.excludedSourceHashes, "excluded source hashes").map((value) =>
      digest(value, "excluded source hash"),
    ),
    excludedFragmentIds: strings(row.excludedFragmentIds, "excluded fragments"),
    aliases: array(row.aliases, "fragment aliases").map((value) => {
      const item = object(value, ["from", "to"], "fragment alias");
      return { from: text(item.from, "alias origin"), to: text(item.to, "alias target") };
    }),
    commonAdmissions: array(row.commonAdmissions, "common admissions").map((value) => {
      const item = object(
        value,
        ["memoryId", "episodeId", "contentHash", "claim", "applicability", "status"],
        "common admission",
      );
      return {
        memoryId: text(item.memoryId, "common memory"),
        episodeId: text(item.episodeId, "common Episode"),
        contentHash: digest(item.contentHash, "common content hash"),
        claim: claim(item.claim),
        applicability: text(item.applicability, "common applicability"),
        status: choice(item.status, ["accepted", "proposed", "withdrawn"], "common status"),
      };
    }),
  };
  if (
    result.excludedSourceHashes.length +
      result.excludedFragmentIds.length +
      result.aliases.length +
      result.commonAdmissions.length >
    maxItems
  )
    return invalid("aggregate policy item budget");
  return {
    schema: outer.schema,
    projectId,
    workId,
    activeGenerationId: text(outer.activeGenerationId, "active generation"),
    policy: result,
  };
}
function parse(bytes: Uint8Array): unknown {
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return invalid("JSON");
  }
}
function request(
  value: unknown,
  options: { root: string; projectId: string; workId: string },
): RecallRequest {
  const row = object(
    value,
    [
      "schema",
      "projectId",
      "workId",
      "root",
      "dataset",
      "policy",
      "query",
      "claimKey",
      "limit",
      "maxChars",
    ],
    "request",
  );
  if (
    row.schema !== "codex.local-memory-recall.request.v1" ||
    row.root !== options.root ||
    row.projectId !== options.projectId ||
    row.workId !== options.workId
  )
    return invalid("request identity");
  if (
    row.limit !== undefined &&
    (!Number.isSafeInteger(row.limit) || Number(row.limit) < 1 || Number(row.limit) > 20)
  )
    return invalid("result count budget");
  if (
    row.maxChars !== undefined &&
    (!Number.isSafeInteger(row.maxChars) ||
      Number(row.maxChars) < 1024 ||
      Number(row.maxChars) > 16000)
  )
    return invalid("output character budget");
  return {
    schema: row.schema,
    root: options.root,
    projectId: options.projectId,
    workId: options.workId,
    dataset: filePin(row.dataset, options.root),
    policy: filePin(row.policy, options.root),
    query: text(row.query, "query", 500),
    claimKey: text(row.claimKey, "claim key"),
    ...(row.limit === undefined ? {} : { limit: Number(row.limit) }),
    ...(row.maxChars === undefined ? {} : { maxChars: Number(row.maxChars) }),
  };
}
/** Actual file bytes only; no serialized observations, access grants or provider clients. */
async function readOwned(path: string, root: string): Promise<Uint8Array> {
  exactPath(path, root);
  if ((await realpath(root)) !== root || (await realpath(path)) !== path)
    return invalid("canonical owner path");
  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const metadata = await handle.stat();
    if (!metadata.isFile() || metadata.size > maxFileBytes) return invalid("file byte budget");
    const buffer = Buffer.alloc(metadata.size + 1);
    let length = 0;
    while (length < buffer.length) {
      const read = await handle.read(buffer, length, buffer.length - length, length);
      if (!read.bytesRead) break;
      length += read.bytesRead;
    }
    if (length !== metadata.size || (await realpath(path)) !== path)
      return invalid("changing source file");
    return buffer.subarray(0, length);
  } finally {
    await handle.close();
  }
}
function failure(error: unknown): "missing" | "denied" {
  return error && typeof error === "object" && "code" in error && error.code === "ENOENT"
    ? "missing"
    : "denied";
}
/** Current-project-only observed Recall. Receipts remain conditional source-owner references. */
export async function readLocalRecallRequest(
  path: string,
  options: { root: string; projectId: string; workId: string },
): Promise<LexicalRecallResult> {
  try {
    text(options.projectId, "selected project");
    text(options.workId, "selected Work");
    exactPath(options.root, options.root);
    exactPath(path, options.root);
    const initial = new Map<string, Uint8Array>();
    const selected = async (path: string, expected?: string) => {
      const bytes = await readOwned(path, options.root);
      if (expected !== undefined && sha256(bytes) !== expected) return invalid("pinned file hash");
      const prior = initial.get(path);
      if (prior && sha256(prior) !== sha256(bytes)) return invalid("changing selected source");
      initial.set(path, bytes);
      return bytes;
    };
    const input = request(parse(await selected(path)), options);
    const data = dataset(
      parse(await selected(input.dataset.path, input.dataset.sha256)),
      options.root,
      options.projectId,
      options.workId,
    );
    const current = policy(
      parse(await selected(input.policy.path, input.policy.sha256)),
      options.projectId,
      options.workId,
    );
    const observe = async (): Promise<SourceObservation[]> => {
      const rows: SourceObservation[] = [];
      let total = 0;
      for (const binding of data.sourceBindings) {
        const identity = {
          repositoryId: binding.source.repositoryId,
          revision: binding.source.revision,
          relativePath: binding.source.relativePath,
        };
        if (
          binding.source.repositoryId !== options.projectId ||
          current.policy.excludedSourceHashes.includes(binding.source.sha256)
        ) {
          rows.push({ ...identity, status: "denied" });
          continue;
        }
        try {
          const bytes = await readOwned(binding.path, options.root);
          total += bytes.byteLength;
          if (total > maxOriginalBytes) return invalid("aggregate original byte budget");
          rows.push({
            ...identity,
            status: "available",
            bytes,
            canonicalPathVerified: true,
            access: "original",
          });
        } catch (error) {
          if (
            error instanceof Error &&
            error.message === "Invalid local Recall aggregate original byte budget"
          )
            throw error;
          rows.push({ ...identity, status: failure(error) });
        }
      }
      return rows;
    };
    const observations = await observe(),
      originalRepositoryGrants = new Set([options.projectId]);
    // This local wrapper requires current verified originals. General common
    // derivative access without private originals remains a separate contract.
    const readableMemoryIds = new Set(
      data.memories
        .filter((memory) => {
          if (memory.scope.kind !== "repository" || memory.scope.repositoryId !== options.projectId)
            return false;
          const episode = data.episodes.find((episode) => episode.id === memory.episodeId);
          return (
            episode !== undefined &&
            qualifySourceClosure(
              episode.sources.map((row) => row.source),
              observations,
              originalRepositoryGrants,
            ).status === "verified"
          );
        })
        .map((memory) => memory.id),
    );
    const result = lexicalRecall(
      {
        memories: data.memories,
        episodes: data.episodes,
        receipts: data.receipts,
        authoritativeClaims: data.authoritativeClaims,
        authoritativeReceipts: data.authoritativeReceipts,
        claimKey: input.claimKey,
        policy: current.policy,
        context: {
          repositoryId: options.projectId,
          explicitlySharedRepositoryIds: new Set(),
          commonEnabled: false,
          applicability: options.workId,
          currentPolicyRevision: current.policy.revision,
          policyHead: "current",
          activeGenerationId: current.activeGenerationId,
          readableMemoryIds,
        },
        observations,
        originalRepositoryGrants,
      },
      {
        query: input.query,
        ...(input.limit === undefined ? {} : { limit: input.limit }),
        ...(input.maxChars === undefined ? {} : { maxChars: input.maxChars }),
      },
    );
    const verifySelected = async () => {
      for (const [selectedPath, bytes] of initial)
        if (sha256(await readOwned(selectedPath, options.root)) !== sha256(bytes))
          return invalid("selected source changed during query");
    };
    await verifySelected();
    const state = (rows: SourceObservation[]) =>
      rows.map((row) => (row.status === "available" ? { ...row, bytes: sha256(row.bytes) } : row));
    if (JSON.stringify(state(await observe())) !== JSON.stringify(state(observations)))
      return invalid("original source changed during query");
    // Bracket the original scan so it cannot hide a policy/request successor.
    // This is optimistic source observation, not a multi-file filesystem lock.
    await verifySelected();
    return result;
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("Invalid local Recall ")) throw error;
    throw Error("Local Recall request is unavailable or invalid");
  }
}

// Git Work binding. Code selection remains the native owner's fixed provider;
// these addresses select data only. All review inputs remain conditional owner declarations.
/** A bounded source-owner preference view; every original is explicitly pinned. */
export interface GitPreferenceDescriptor {
  schema: "common.git-work-standing-preference.v1";
  applicability: string;
  revision: string;
  inputId: string;
  nodeIds: string[];
  nodes: SnapshotSource;
  relations: SnapshotSource;
  sources: Array<{ id: string; source: SnapshotSource | ExternalGitSource }>;
}
/** Explicit native-owner selection, not a caller flag or a grant to the consumer's Work. */
export interface GitCommonPreferenceSelection {
  topicId: string;
  claimKey: string;
  applicability: string;
  descriptorSha256: string;
  consumer: { repositoryId: string; workId?: string };
}
interface GitMemoryDescriptor {
  topicId: string;
  generationId: string;
  taskId?: string;
  receiptTaskIds: string[];
  claims: Array<{ key: string; record: RecordReference }>;
  inputIds: { request: string; dataset: string; policy: string };
  query: string;
  claimKey: string;
  limit?: number;
  maxChars?: number;
  policy: PolicyFile;
  preference?: GitPreferenceDescriptor;
}
interface GitSourceBinding {
  source: PortableSource;
  origin: ExternalGitSource;
  blobOid: string;
  declarations: SourceReference[];
}
function gitDescriptor(value: unknown, reader: GitWorkReader): GitMemoryDescriptor {
  const row = object(
    value,
    [
      "topicId",
      "generationId",
      "taskId",
      "receiptTaskIds",
      "claims",
      "inputIds",
      "query",
      "claimKey",
      "policy",
      "limit",
      "maxChars",
      "preference",
    ],
    "Git descriptor",
  );
  const ids = object(row.inputIds, ["request", "dataset", "policy"], "Git Input identities");
  for (const [key, minimum, maximum] of [
    ["limit", 1, 20],
    ["maxChars", 1024, 16000],
  ] as const)
    if (
      row[key] !== undefined &&
      (!Number.isSafeInteger(row[key]) || Number(row[key]) < minimum || Number(row[key]) > maximum)
    )
      invalid("Git Recall budget");
  const result: GitMemoryDescriptor = {
    topicId: text(row.topicId, "topic"),
    generationId: text(row.generationId, "generation"),
    ...(row.preference === undefined ? { taskId: text(row.taskId, "selected Task") } : {}),
    receiptTaskIds: strings(row.receiptTaskIds, "receipt Tasks"),
    claims: array(row.claims, "selected claims").map((value) => {
      const item = object(value, ["key", "record"], "selected claim");
      const ref = object(item.record, ["kind", "id", "revision"], "claim record");
      const reference = { ...ref } as unknown as RecordReference;
      gitRecordPath(reader.selection.projectId, reader.selection.workId, reference);
      if (!["decision", "spec"].includes(reference.kind)) invalid("claim record class");
      return { key: text(item.key, "claim key"), record: reference };
    }),
    inputIds: {
      request: text(ids.request, "request Input"),
      dataset: text(ids.dataset, "dataset Input"),
      policy: text(ids.policy, "policy Input"),
    },
    query: text(row.query, "query", 500),
    claimKey: text(row.claimKey, "claim key"),
    ...(row.limit === undefined ? {} : { limit: Number(row.limit) }),
    ...(row.maxChars === undefined ? {} : { maxChars: Number(row.maxChars) }),
    policy: policy(row.policy, reader.selection.projectId, reader.selection.workId),
    ...(row.preference === undefined
      ? {}
      : { preference: gitPreferenceDescriptor(row.preference, reader) }),
  };
  if (
    !result.claims.length ||
    (!result.preference && !result.receiptTaskIds.length) ||
    result.policy.activeGenerationId !== result.generationId ||
    (!result.preference && result.policy.policy.commonAdmissions.length) ||
    new Set(Object.values(result.inputIds)).size !== 3
  )
    invalid("Git descriptor closure");
  identities(result.claims, (row) => row.key);
  identities(result.receiptTaskIds, (row) => row);
  if (!result.claims.some((row) => row.key === result.claimKey)) invalid("selected claim key");
  if (
    result.preference &&
    (row.taskId !== undefined ||
      result.receiptTaskIds.length ||
      result.claims.length !== 1 ||
      result.claims[0]!.record.kind !== "decision")
  )
    invalid("Git preference-only descriptor");
  return result;
}

function preferenceSource(value: unknown): SnapshotSource | ExternalGitSource {
  const row = object(
    value,
    ["kind", "path", "fragment", "sha256", "repositoryId", "commit"],
    "preference source",
  );
  const common = {
    path: text(row.path, "preference source path", 4096),
    fragment: text(row.fragment, "preference source fragment", 1024),
    sha256: digest(row.sha256, "preference source hash"),
  };
  if (!common.fragment.startsWith("#/")) invalid("preference source fragment");
  if (row.kind === "snapshot" && row.repositoryId === undefined && row.commit === undefined)
    return { kind: "snapshot", ...common };
  if (
    row.kind !== "git" ||
    typeof row.commit !== "string" ||
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(row.commit)
  )
    invalid("preference source address");
  return {
    kind: "git",
    ...common,
    repositoryId: text(row.repositoryId, "preference source repository"),
    commit: row.commit,
  };
}
function gitPreferenceDescriptor(value: unknown, reader: GitWorkReader): GitPreferenceDescriptor {
  const row = object(
    value,
    ["schema", "applicability", "revision", "inputId", "nodeIds", "nodes", "relations", "sources"],
    "preference descriptor",
  );
  if (row.schema !== "common.git-work-standing-preference.v1") invalid("preference schema");
  const inputId = text(row.inputId, "preference Input");
  const inputPath = gitRecordPath(reader.selection.projectId, reader.selection.workId, {
    kind: "input",
    id: inputId,
  });
  const nodes = preferenceSource(row.nodes),
    relations = preferenceSource(row.relations);
  if (
    [nodes, relations].some((ref) => ref.kind !== "snapshot" || ref.path !== inputPath) ||
    nodes.sha256 !== relations.sha256
  )
    invalid("preference projection Input");
  const nodeIds = strings(row.nodeIds, "preference node IDs");
  const sources = array(row.sources, "preference sources").map((value) => {
    const source = object(value, ["id", "source"], "preference source binding");
    return { id: text(source.id, "preference source ID"), source: preferenceSource(source.source) };
  });
  if (!nodeIds.length || nodeIds.length > 12 || !sources.length || sources.length > 12)
    invalid("preference projection budget");
  identities(nodeIds, (value) => value);
  identities(sources, (value) => value.id);
  return {
    schema: row.schema,
    applicability: text(row.applicability, "preference applicability"),
    revision: text(row.revision, "preference revision"),
    inputId,
    nodeIds,
    nodes: nodes as SnapshotSource,
    relations: relations as SnapshotSource,
    sources,
  };
}

function withinPreference(
  source: ResolvedSource,
  reader: GitWorkReader,
  descriptor: GitMemoryDescriptor,
): void {
  verifiedOriginalBytes(source);
  const preference = descriptor.preference!;
  const inputPath = gitRecordPath(reader.selection.projectId, reader.selection.workId, {
    kind: "input",
    id: preference.inputId,
  });
  const decisionPath = gitRecordPath(
    reader.selection.projectId,
    reader.selection.workId,
    descriptor.claims[0]!.record,
  );
  const primary =
    source.repositoryId === reader.selection.repositoryId &&
    source.commit === reader.commit &&
    (source.path === decisionPath ||
      (source.path === inputPath && source.sha256 === preference.nodes.sha256));
  const declared = preference.sources.some(
    ({ source: ref }) =>
      source.path === ref.path &&
      source.sha256 === ref.sha256 &&
      source.repositoryId ===
        (ref.kind === "git" ? ref.repositoryId : reader.selection.repositoryId) &&
      source.commit === (ref.kind === "git" ? ref.commit : reader.commit),
  );
  const record = assertRecord(source.document);
  if (
    (!primary && !declared) ||
    source.repositoryId !== reader.selection.repositoryId ||
    record.domainId !== reader.work.domainId ||
    record.projectId !== reader.work.projectId ||
    (record.kind !== "input" &&
      !(primary && source.path === decisionPath && record.kind === "decision"))
  )
    invalid("preference original scope");
  if (
    source.path !==
    gitRecordPath(record.projectId, record.workId, { kind: record.kind, id: record.id })
  )
    invalid("preference canonical record path");
}

async function compilePreference(
  reader: GitWorkReader,
  capture: GitWorkReader,
  descriptor: GitMemoryDescriptor,
) {
  const preference = descriptor.preference!;
  const workSource = await reader.readSource({
    kind: "snapshot",
    path: reader.workPath + "/work.json",
  });
  withinWork(workSource, reader);
  if (workSource.path !== reader.workPath + "/work.json" || !same(reader.work, workSource.document))
    invalid("Git preference Work proof binding");
  const ref = descriptor.claims[0]!.record;
  const record = await reader.readRecord(ref);
  const proof = await capture.readSource({
    kind: "snapshot",
    path: gitRecordPath(reader.selection.projectId, reader.selection.workId, ref),
  });
  if (
    !same(record, proof.document) ||
    record.kind !== "decision" ||
    record.status !== "accepted" ||
    !record.selectionContext ||
    !reader.work.governingDecisionIds.includes(record.id)
  )
    invalid("Git preference current Decision");
  await validateRecordSources(record, capture);
  const inputPath = gitRecordPath(reader.selection.projectId, reader.selection.workId, {
    kind: "input",
    id: preference.inputId,
  });
  if (record.answerSource.path !== inputPath || record.selectionContext.source.path !== inputPath)
    invalid("Git preference original answer binding");
  const nodesSource = await capture.readSource(preference.nodes);
  const relationsSource = await capture.readSource(preference.relations);
  const allNodes = array(nodesSource.value, "preference nodes").map((value) => {
    const node = object(
      value,
      ["id", "kind", "meaning", "sources", "appliesTo"],
      "preference node",
    );
    return {
      id: text(node.id, "preference node ID"),
      kind: choice(node.kind, ["preference", "direct-why", "derived-why"], "preference node class"),
      meaning: text(node.meaning, "preference node meaning", 1600),
      sources: strings(node.sources, "preference node sources"),
      ...(node.appliesTo === undefined
        ? {}
        : { appliesTo: text(node.appliesTo, "preference applicability", 256) }),
    };
  });
  identities(allNodes, (node) => node.id);
  if (allNodes.length > 32) invalid("preference node budget");
  const nodes = preference.nodeIds.map((id) => {
    const node = allNodes.find((node) => node.id === id);
    if (!node || !node.sources.length) invalid("preference selected node");
    identities(node.sources, (source) => source);
    return node;
  });
  const requiredSources = new Set(nodes.flatMap((node) => node.sources));
  if (
    !nodes.some((node) => node.kind === "preference") ||
    !nodes.some((node) => node.kind === "direct-why") ||
    requiredSources.size !== preference.sources.length ||
    preference.sources.some((source) => !requiredSources.has(source.id))
  )
    invalid("preference full selected lineage");
  const relations = array(relationsSource.value, "preference relations")
    .map((value) => {
      const edge = object(value, ["from", "type", "to", "appliesTo"], "preference relation");
      return {
        from: text(edge.from, "preference relation source"),
        type: text(edge.type, "preference relation type", 64),
        to: text(edge.to, "preference relation target"),
        ...(edge.appliesTo === undefined
          ? {}
          : { appliesTo: text(edge.appliesTo, "preference relation applicability") }),
      };
    })
    .filter(
      (edge) => preference.nodeIds.includes(edge.from) && preference.nodeIds.includes(edge.to),
    );
  const reached = new Set([nodes.find((node) => node.kind === "preference")!.id]);
  for (let count = 0; count < nodes.length; count++)
    for (const edge of relations) if (reached.has(edge.from)) reached.add(edge.to);
  if (reached.size !== nodes.length || relations.some((edge) => edge.from === edge.to))
    invalid("preference meaningful topology");
  const acyclic = (id: string, ancestors = new Set<string>()): boolean => {
    if (ancestors.has(id)) return false;
    const next = new Set([...ancestors, id]);
    return relations.filter((edge) => edge.from === id).every((edge) => acyclic(edge.to, next));
  };
  if (relations.length > 32 || nodes.some((node) => !acyclic(node.id)))
    invalid("preference cyclic topology");
  const originals = [];
  for (const source of preference.sources) {
    const resolved = await capture.readSource(source.source);
    const original = assertRecord(resolved.document);
    if (original.kind !== "input" || original.inputKind !== "actual-user-answer")
      invalid("preference actual original class");
    const originalText = text(resolved.value, "preference original text", 12_000);
    originals.push({
      id: source.id,
      source: originalAddress(resolved),
      ...(originalText === record.answer ? {} : { text: originalText }),
      originKind: original.origin.kind,
      nativeAuthorshipVerified: false,
    });
  }
  const projection = {
    schema: "common.standing-preference-projection.v1",
    topicId: descriptor.topicId,
    revision: preference.revision,
    applicability: preference.applicability,
    decision: {
      id: record.id,
      answer: record.answer,
      meaning: record.meaning,
      selectionContext: record.selectionContext,
    },
    nodes,
    relations,
    originals,
  };
  const projectionText = JSON.stringify(projection);
  if (projectionText.length > 6000) invalid("preference projection text budget");
  return { records: new Map([[recordKey(record), record]]), text: projectionText };
}
function same(value: unknown, other: unknown): boolean {
  return isDeepStrictEqual(value, other);
}
function originalAddress(source: ResolvedSource): ExternalGitSource {
  return {
    kind: "git",
    repositoryId: source.repositoryId,
    commit: source.commit,
    path: source.path,
    sha256: source.sha256,
    fragment: source.reference.fragment ?? "#",
  };
}
function portableOriginal(source: ResolvedSource): PortableSource {
  verifiedOriginalBytes(source);
  return portable({
    repositoryId: source.repositoryId,
    revision: `git:${source.commit}`,
    relativePath: source.path,
    sha256: source.sha256,
    byteLength: source.byteLength,
  });
}
function withinWork(source: ResolvedSource, reader: GitWorkReader): void {
  verifiedOriginalBytes(source);
  if (
    source.repositoryId !== reader.selection.repositoryId ||
    (source.reference.kind === "snapshot" && source.commit !== reader.commit) ||
    !source.path.startsWith(reader.workPath + "/")
  )
    invalid("Git original scope");
  const record = assertRecord(source.document);
  if (
    record.projectId !== reader.selection.projectId ||
    record.workId !== reader.selection.workId ||
    record.domainId !== reader.work.domainId
  )
    invalid("Git original record scope");
}
function reviewBinding(value: {
  descriptor: GitMemoryDescriptor;
  episodes: Episode[];
  memories: DerivedMemory[];
  authoritativeClaims: AuthoritativeClaim[];
  sourceBindings: GitSourceBinding[];
  originalsCommit: string;
}): string {
  return sha256(JSON.stringify(value));
}
async function gitReview(reader: GitWorkReader, ref: SourceReference | undefined, binding: string) {
  if (!ref)
    return {
      receipts: [] as DerivationReceipt[],
      authoritativeReceipts: [] as AuthoritativeClaimReceipt[],
      owner: null as string | null,
    };
  const resolved = await reader.readSource(ref);
  withinWork(resolved, reader);
  const input = assertRecord(resolved.document);
  const row = object(
    resolved.value,
    ["schema", "ownerId", "bindingSha256", "status", "receipts", "authoritativeReceipts"],
    "Git owner review",
  );
  if (
    input.kind !== "input" ||
    input.inputKind !== "source-owner-record" ||
    row.schema !== "common.git-work-memory-owner-review.v1" ||
    row.ownerId !== input.origin.ownerId ||
    row.bindingSha256 !== binding ||
    row.status !== "reviewed"
  )
    invalid("Git owner review binding");
  const receipts = array(row.receipts, "review receipts").map(receipt);
  const authoritativeReceipts = array(
    row.authoritativeReceipts,
    "review authoritative receipts",
  ).map(authoritativeReceipt);
  identities(receipts, (row) => row.memoryId);
  identities(authoritativeReceipts, (row) => JSON.stringify([row.sourceKey, row.key]));
  return { receipts, authoritativeReceipts, owner: text(row.ownerId, "review owner") };
}
/** Pure deterministic source projection, not LLM Reflect, owner attestation or a writer. */
export async function prepareGitWorkMemoryArtifacts(
  reader: GitWorkReader,
  value: unknown,
  ownerReview?: SourceReference,
) {
  if (reader.commit !== reader.selection.commit) invalid("Git reader selection binding");
  const descriptor = gitDescriptor(value, reader);
  if (Buffer.byteLength(JSON.stringify(descriptor)) > maxFileBytes)
    invalid("Git descriptor byte budget");
  if (!descriptor.preference && !reader.work.continuity?.purposeSources.length)
    invalid("Git purpose originals missing");
  let records = new Map<string, GitWorkRecord>();
  const resolved: ResolvedSource[] = [];
  // The wrapper captures every canonical validation read, including nested prerequisite receipts.
  const capture: GitWorkReader = {
    ...reader,
    readSource: async (ref) => {
      const result = await reader.readSource(ref);
      if (descriptor.preference) withinPreference(result, reader, descriptor);
      else withinWork(result, reader);
      resolved.push(result);
      return result;
    },
  };
  const preference = descriptor.preference
    ? await compilePreference(reader, capture, descriptor)
    : undefined;
  if (preference) records = preference.records;
  const queue: RecordReference[] = preference
    ? []
    : [
        { kind: "work", id: reader.selection.workId },
        { kind: "task", id: descriptor.taskId! },
        ...descriptor.receiptTaskIds.map((id) => ({ kind: "task" as const, id })),
        ...descriptor.claims.map((row) => row.record),
      ];
  let existingEffects: Set<string> | undefined;
  const includeExistingEffects = async (ids: string[]) => {
    if (!ids.length) return;
    existingEffects ??= new Set(
      (await reader.listRecords()).filter((row) => row.kind === "effect").map((row) => row.id),
    );
    for (const id of ids) if (existingEffects.has(id)) queue.push({ kind: "effect", id });
  };
  while (queue.length) {
    const ref = queue.shift()!,
      key = recordKey(ref);
    if (records.has(key)) continue;
    if (records.size >= maxItems) invalid("Git record budget");
    const record = await reader.readRecord(ref);
    const original = await capture.readSource({
      kind: "snapshot",
      path: gitRecordPath(reader.selection.projectId, reader.selection.workId, ref),
    });
    if (
      !same(record, original.document) ||
      (record.kind === "work" && !same(reader.work, original.document))
    )
      invalid("Git record proof binding");
    records.set(key, record);
    queue.push(...recordDependencies(record));
    await validateRecordSources(record, capture);
    if (record.kind === "task") await includeExistingEffects(record.pendingEffectIds ?? []);
    if (record.kind === "work" && record.continuity)
      for (const ref of record.continuity.effectInventorySources) {
        const inventory = effectInventory((await capture.readSource(ref)).value);
        await includeExistingEffects([
          ...inventory.completedEffectIds,
          ...inventory.uncertainEffectIds,
        ]);
      }
  }
  if (!preference) {
    const validation = await validateRecordSet(records, capture);
    if (!validation.valid) invalid("Git record/source closure");
    const selected = records.get(recordKey({ kind: "task", id: descriptor.taskId! }));
    if (selected?.kind !== "task") invalid("Git selected Task");
    for (const dependency of selected.dependsOn)
      if ((await dependencyFindings(selected, dependency, records, capture)).length)
        invalid("Git prerequisite closure");
    for (const id of descriptor.receiptTaskIds) {
      const task = records.get(recordKey({ kind: "task", id }));
      if (
        task?.kind !== "task" ||
        (task.executionClass === "native-effect" && !task.executionEvidenceId) ||
        !task.verificationEvidenceIds.length ||
        task.state.execution !== "complete" ||
        task.state.verification !== "passed"
      )
        invalid("Git receipt Task closure");
      if (!selected.dependsOn.some((row) => row.taskId === id))
        invalid("Git unrelated receipt Task");
    }
  }
  const bindings = new Map<string, GitSourceBinding>();
  const fragments = new Map<string, Set<string>>();
  let total = 0;
  for (const result of resolved) {
    const source = portableOriginal(result),
      key = sourceKey(source);
    const declaration = result.declaredNativeSource ?? result.reference;
    const fragment = JSON.stringify([key, result.reference.fragment ?? "#"]);
    let binding = bindings.get(key);
    if (!binding) {
      total += source.byteLength;
      if (total > maxOriginalBytes) invalid("aggregate original byte budget");
      binding = {
        source,
        origin: originalAddress(result),
        blobOid: result.blobOid,
        declarations: [],
      };
      bindings.set(key, binding);
      fragments.set(key, new Set());
    }
    if (!binding.declarations.some((row) => same(row, declaration)))
      binding.declarations.push(structuredClone(declaration));
    fragments.get(key)!.add(fragment);
  }
  const sourceBindings = [...bindings.values()].sort((a, b) =>
    sourceKey(a.source) < sourceKey(b.source)
      ? -1
      : sourceKey(a.source) > sourceKey(b.source)
        ? 1
        : 0,
  );
  const episode: Episode = {
    id: `git-episode:${sha256(JSON.stringify([descriptor.topicId, descriptor.generationId, sourceBindings]))}`,
    provenance: "source-linked",
    sources: sourceBindings.map((row) => ({
      source: row.source,
      fragmentIds: [...fragments.get(sourceKey(row.source))!].sort(),
    })),
  };
  const authoritativeClaims: AuthoritativeClaim[] = descriptor.claims.map((row) => {
    const record = records.get(recordKey(row.record))!;
    if (record.kind === "decision" && !record.selectionContext)
      invalid("Git Decision question original missing");
    if (
      record.kind === "decision" &&
      (record.status !== "accepted" || !reader.work.governingDecisionIds.includes(record.id))
    )
      invalid("Git claim is not a current governing Decision");
    if (
      record.kind === "spec" &&
      (record.id !== reader.work.spec.id || record.revision !== reader.work.spec.revision)
    )
      invalid("Git claim is not the bound Spec");
    const path = gitRecordPath(reader.selection.projectId, reader.selection.workId, row.record);
    const binding = sourceBindings.find(
      (binding) => binding.origin.commit === reader.commit && binding.origin.path === path,
    )!;
    return authoritativeClaim({
      key: row.key,
      value:
        record.kind === "decision" ? record.answer : record.kind === "spec" ? record.outcome : "",
      recordClass: record.kind,
      status: "adopted-current",
      source: binding.source,
    });
  });
  if (
    2 * descriptor.claims.length + 2 * sourceBindings.length + 1 > maxItems ||
    Buffer.byteLength(JSON.stringify(episode.sources.map((row) => sourceKey(row.source)))) *
      descriptor.claims.length >
      maxFileBytes
  )
    invalid("Git dataset item/lineage budget");
  const memories = authoritativeClaims.map((row) =>
    memory({
      id: `git-memory:${sha256(JSON.stringify([descriptor.topicId, row.key, descriptor.generationId]))}`,
      episodeId: episode.id,
      generationId: descriptor.generationId,
      policyRevision: descriptor.policy.policy.revision,
      text: preference?.text ?? row.value,
      contentHash: sha256(preference?.text ?? row.value),
      sourceKeys: episode.sources.map((row) => sourceKey(row.source)),
      scope: descriptor.preference
        ? { kind: "common" }
        : { kind: "repository", repositoryId: reader.selection.repositoryId },
      claim: { key: row.key, value: row.value },
    }),
  );
  const candidate = {
    descriptor,
    episodes: [episode],
    memories,
    authoritativeClaims,
    sourceBindings,
    originalsCommit: reader.commit,
  };
  const bindingSha256 = reviewBinding(candidate);
  const review = await gitReview(reader, ownerReview, bindingSha256);
  const dataset = {
    schema: "common.git-work-memory-dataset.v1" as const,
    projectId: reader.selection.projectId,
    workId: reader.selection.workId,
    repositoryId: reader.selection.repositoryId,
    ...candidate,
    receipts: review.receipts,
    authoritativeReceipts: review.authoritativeReceipts,
    ownerReview: ownerReview ? structuredClone(ownerReview) : null,
    reviewBindingSha256: bindingSha256,
  };
  if (
    Buffer.byteLength(JSON.stringify(dataset)) > maxFileBytes ||
    dataset.receipts.length +
      dataset.authoritativeReceipts.length +
      2 * descriptor.claims.length +
      2 * sourceBindings.length +
      1 >
      maxItems
  )
    invalid("Git dataset byte/item budget");
  const pin = (id: string) => ({
    kind: "snapshot" as const,
    path: gitRecordPath(reader.selection.projectId, reader.selection.workId, { kind: "input", id }),
    fragment: "#/content",
  });
  return {
    request: {
      schema: "common.git-work-memory-request.v1" as const,
      projectId: reader.selection.projectId,
      workId: reader.selection.workId,
      repositoryId: reader.selection.repositoryId,
      topicId: descriptor.topicId,
      dataset: pin(descriptor.inputIds.dataset),
      policy: pin(descriptor.inputIds.policy),
      query: descriptor.query,
      claimKey: descriptor.claimKey,
      ...(descriptor.limit === undefined ? {} : { limit: descriptor.limit }),
      ...(descriptor.maxChars === undefined ? {} : { maxChars: descriptor.maxChars }),
    },
    policy: descriptor.policy,
    dataset,
    diagnostics: {
      kind: "deterministic-source-projection" as const,
      reviewOwnerDeclared: review.owner,
      reviewBindingSha256: bindingSha256,
      requestPinsRequireOwnerPublication: true,
      nativeAuthorshipAuthenticated: false,
      nativePermissionGranted: false,
      writesPerformed: false,
    },
  };
}

/** A literal branch address, never Git revision/reflog syntax accepted by rev-parse. */
function literalOwningHead(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length <= 256 &&
    value.startsWith("refs/heads/") &&
    !/[\u0000-\u0020\u007f~^:?*\[\\]/.test(value) &&
    !value.includes("..") &&
    !value.includes("@{") &&
    !value.includes("//") &&
    value
      .split("/")
      .every(
        (part) =>
          part.length > 0 &&
          !part.startsWith(".") &&
          !part.endsWith(".") &&
          !part.endsWith(".lock"),
      )
  );
}

/** Observe a mutable owning local ref; an immutable reader alone cannot establish currentness. */
export async function readGitWorkMemoryRequest(
  reader: GitWorkReader,
  requestRef: SourceReference,
  ownerSelection: {
    owningRef: string;
    admittedCommit: string;
    policyRevision: string;
    activeGenerationId: string;
    state: "current" | "unknown" | "publishing";
    commonPreferences?: GitCommonPreferenceSelection;
  },
) {
  if (reader.commit !== reader.selection.commit) invalid("Git reader selection binding");
  object(
    ownerSelection,
    [
      "owningRef",
      "admittedCommit",
      "policyRevision",
      "activeGenerationId",
      "state",
      "commonPreferences",
    ],
    "Git owner selection",
  );
  const witness = structuredClone(ownerSelection);
  if (
    witness.state !== "current" ||
    !literalOwningHead(witness.owningRef) ||
    witness.admittedCommit !== reader.commit
  )
    invalid("Git current owner selection");
  const observeHead = async () => {
    const actual = await resolveGitWorkSelection({
      ...reader.selection,
      commit: witness.owningRef,
    });
    if (actual.commit !== witness.admittedCommit) invalid("Git owning head advanced");
    return actual;
  };
  await observeHead();
  const fresh = await openGitWork(reader.selection);
  const triple: ResolvedSource[] = [];
  const readInput = async (ref: SourceReference) => {
    object(ref, ["kind", "path", "sha256", "fragment"], "Git Input pin");
    if (ref.kind !== "snapshot" || !ref.sha256 || ref.fragment !== "#/content")
      invalid("Git full Input pin");
    digest(ref.sha256, "Git Input hash");
    const result = await fresh.readSource(ref);
    withinWork(result, fresh);
    const record = assertRecord(result.document);
    if (
      record.kind !== "input" ||
      record.inputKind !== "source-owner-record" ||
      result.path !==
        gitRecordPath(fresh.selection.projectId, fresh.selection.workId, {
          kind: "input",
          id: record.id,
        })
    )
      invalid("Git Input class/path");
    triple.push(result);
    return result;
  };
  const requestSource = await readInput(requestRef);
  const input = object(
    requestSource.value,
    [
      "schema",
      "projectId",
      "workId",
      "repositoryId",
      "topicId",
      "dataset",
      "policy",
      "query",
      "claimKey",
      "limit",
      "maxChars",
    ],
    "Git request",
  );
  if (
    input.schema !== "common.git-work-memory-request.v1" ||
    input.projectId !== fresh.selection.projectId ||
    input.workId !== fresh.selection.workId ||
    input.repositoryId !== fresh.selection.repositoryId
  )
    invalid("Git request identity");
  const dataSource = await readInput(input.dataset as SourceReference),
    policySource = await readInput(input.policy as SourceReference);
  const data = object(
    dataSource.value,
    [
      "schema",
      "projectId",
      "workId",
      "repositoryId",
      "descriptor",
      "episodes",
      "memories",
      "authoritativeClaims",
      "sourceBindings",
      "originalsCommit",
      "receipts",
      "authoritativeReceipts",
      "ownerReview",
      "reviewBindingSha256",
    ],
    "Git dataset",
  );
  if (
    data.schema !== "common.git-work-memory-dataset.v1" ||
    data.projectId !== input.projectId ||
    data.workId !== input.workId ||
    data.repositoryId !== input.repositoryId
  )
    invalid("Git dataset identity");
  const current = policy(policySource.value, fresh.selection.projectId, fresh.selection.workId);
  if (
    current.policy.revision !== witness.policyRevision ||
    current.activeGenerationId !== witness.activeGenerationId
  )
    invalid("Git policy/generation advanced");
  const originals = await openGitWork({
    ...fresh.selection,
    commit: text(data.originalsCommit, "originals commit"),
  });
  const compiled = await prepareGitWorkMemoryArtifacts(
    originals,
    data.descriptor,
    (data.ownerReview ?? undefined) as SourceReference | undefined,
  );
  const preferenceDescriptor = compiled.dataset.descriptor.preference;
  let preferenceSelection: GitCommonPreferenceSelection | undefined;
  if (preferenceDescriptor || witness.commonPreferences !== undefined) {
    const selected = object(
      witness.commonPreferences,
      ["topicId", "claimKey", "applicability", "descriptorSha256", "consumer"],
      "common preference owner selection",
    );
    const consumer = object(
      selected.consumer,
      ["repositoryId", "workId"],
      "common preference consumer",
    );
    preferenceSelection = {
      topicId: text(selected.topicId, "common preference topic"),
      claimKey: text(selected.claimKey, "common preference claim"),
      applicability: text(selected.applicability, "common preference applicability"),
      descriptorSha256: digest(selected.descriptorSha256, "common preference descriptor hash"),
      consumer: {
        repositoryId: text(consumer.repositoryId, "common preference consumer repository"),
        ...(consumer.workId === undefined
          ? {}
          : { workId: text(consumer.workId, "common preference consumer Work") }),
      },
    };
    if (
      !preferenceDescriptor ||
      preferenceSelection.topicId !== compiled.request.topicId ||
      preferenceSelection.claimKey !== compiled.request.claimKey ||
      preferenceSelection.applicability !== preferenceDescriptor.applicability ||
      preferenceSelection.descriptorSha256 !== sha256(JSON.stringify(compiled.dataset.descriptor))
    )
      invalid("common preference owner binding");
  }
  if (
    !same(compiled.dataset, data) ||
    !same(compiled.policy, current) ||
    input.topicId !== compiled.request.topicId ||
    input.query !== compiled.request.query ||
    input.claimKey !== compiled.request.claimKey ||
    input.limit !== compiled.request.limit ||
    input.maxChars !== compiled.request.maxChars
  )
    invalid("Git compiled source/review binding");
  for (const [index, id] of [
    compiled.dataset.descriptor.inputIds.request,
    compiled.dataset.descriptor.inputIds.dataset,
    compiled.dataset.descriptor.inputIds.policy,
  ].entries())
    if (
      triple[index]!.path !==
      gitRecordPath(fresh.selection.projectId, fresh.selection.workId, { kind: "input", id })
    )
      invalid("Git selected Input identities");
  const observeOriginals = async (currentReader: GitWorkReader) => {
    const observations: SourceObservation[] = [];
    if (
      preferenceDescriptor &&
      !currentReader.work.governingDecisionIds.includes(
        compiled.dataset.descriptor.claims[0]!.record.id,
      )
    )
      invalid("common preference authority is no longer current");
    for (const binding of compiled.dataset.sourceBindings) {
      const historical = await originals.readSource(binding.origin);
      const present = await currentReader.readSource({
        kind: "snapshot",
        path: binding.origin.path,
        fragment: binding.origin.fragment,
      });
      if (
        historical.blobOid !== binding.blobOid ||
        historical.sha256 !== present.sha256 ||
        historical.blobOid !== present.blobOid
      )
        invalid("Git original is no longer current");
      // Every native/receipt declaration remains part of the source binding, not native authentication.
      for (const declaration of binding.declarations)
        verifiedOriginalBytes(await originals.readSource(declaration));
      observations.push({
        repositoryId: binding.source.repositoryId,
        revision: binding.source.revision,
        relativePath: binding.source.relativePath,
        status: current.policy.excludedSourceHashes.includes(binding.source.sha256)
          ? "denied"
          : "available",
        bytes: verifiedOriginalBytes(historical),
        canonicalPathVerified: true,
        access: "original",
      } as SourceObservation);
    }
    if (compiled.dataset.ownerReview) {
      const oldReview = await originals.readSource(compiled.dataset.ownerReview);
      const now = await currentReader.readSource({
        kind: "snapshot",
        path: oldReview.path,
        fragment: oldReview.reference.fragment,
      });
      if (oldReview.sha256 !== now.sha256 || oldReview.blobOid !== now.blobOid)
        invalid("Git owner review advanced");
    }
    return observations;
  };
  const observations = await observeOriginals(fresh),
    grants = new Set([fresh.selection.repositoryId]);
  const readableMemoryIds = new Set(
    compiled.dataset.memories
      .filter((memory) => {
        const episode = compiled.dataset.episodes.find((row) => row.id === memory.episodeId)!;
        return (
          qualifySourceClosure(
            episode.sources.map((row) => row.source),
            observations,
            grants,
          ).status === "verified"
        );
      })
      .map((row) => row.id),
  );
  const context = {
    repositoryId: fresh.selection.repositoryId,
    explicitlySharedRepositoryIds: new Set<string>(),
    commonEnabled: preferenceSelection !== undefined,
    applicability: preferenceSelection?.applicability ?? fresh.selection.workId,
    currentPolicyRevision: current.policy.revision,
    policyHead: "current" as const,
    activeGenerationId: current.activeGenerationId,
    readableMemoryIds,
  };
  // Claims depend on the selected Episode too: an excluded or inaccessible original
  // cannot be dropped while retaining an authoritative projection of that Episode.
  const fullLineageEligible = compiled.dataset.memories.every(
    (memory) =>
      qualifyMemory(
        memory,
        compiled.dataset.episodes.find((row) => row.id === memory.episodeId),
        current.policy,
        context,
        compiled.dataset.receipts,
      ).status === "eligible",
  );
  const result = lexicalRecall(
    {
      ...compiled.dataset,
      authoritativeClaims: fullLineageEligible ? compiled.dataset.authoritativeClaims : [],
      policy: current.policy,
      context,
      observations,
      originalRepositoryGrants: grants,
      claimKey: text(input.claimKey, "claim key"),
    },
    {
      query: text(input.query, "query", 500),
      // Source metadata comes only from the actual pinned Input after full compile/data equality above.
      hitProjection: {
        kind: "git-dataset-memory-reference.v1",
        datasetInput: {
          repositoryId: dataSource.repositoryId,
          commit: dataSource.commit,
          path: dataSource.path,
          sha256: dataSource.sha256,
          blobOid: dataSource.blobOid,
          byteLength: dataSource.byteLength,
        },
      },
      ...(compiled.request.limit === undefined ? {} : { limit: compiled.request.limit }),
      ...(compiled.request.maxChars === undefined ? {} : { maxChars: compiled.request.maxChars }),
    },
  );
  await observeHead();
  const after = await openGitWork(await observeHead());
  for (const before of triple) {
    const now = await after.readSource(before.reference);
    if (
      now.sha256 !== before.sha256 ||
      now.blobOid !== before.blobOid ||
      !same(now.value, before.value)
    )
      invalid("Git selected triple advanced");
  }
  await observeOriginals(after);
  await observeHead();
  const output = {
    ...result,
    gitWork: {
      topicId: input.topicId,
      originalsCommit: originals.commit,
      admittedCommit: fresh.commit,
      owningRef: witness.owningRef,
      observedHeadBefore: fresh.commit,
      observedHeadAfter: after.commit,
      triple: triple.map(originalAddress),
      policyRevision: current.policy.revision,
      activeGenerationId: current.activeGenerationId,
      reviewOwnerDeclared: compiled.diagnostics.reviewOwnerDeclared,
      ...(preferenceSelection
        ? {
            commonPreferences: {
              topicId: preferenceSelection.topicId,
              claimKey: preferenceSelection.claimKey,
              applicability: preferenceSelection.applicability,
              revision: preferenceDescriptor!.revision,
              consumer: preferenceSelection.consumer,
              consumerWorkChanged: false,
            },
          }
        : {}),
      sourceBindingSha256: compiled.dataset.reviewBindingSha256,
      currentness: "optimistic-local-ref-and-byte-observation" as const,
      limits:
        "No remote head, native permission, native authorship or receipt authenticity attested.",
    },
  };
  output.outputChars = JSON.stringify(output).length;
  output.outputChars = JSON.stringify(output).length;
  if (output.outputChars > (compiled.request.maxChars ?? 8000))
    invalid("Git Recall output character budget");
  return output;
}
