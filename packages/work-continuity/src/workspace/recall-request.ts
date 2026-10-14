import { constants } from "node:fs";
import { open, realpath } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";
import { sha256 } from "../context/source";
import { lexicalRecall, type LexicalRecallResult } from "./lexical-recall";
import {
  qualifySourceClosure,
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
