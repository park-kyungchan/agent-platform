import { Database } from "bun:sqlite";
import { isDeepStrictEqual } from "node:util";
import { sha256 } from "../context/source";
import { qualifyRecall } from "./qualification";

export type LexicalRecallInput = Parameters<typeof qualifyRecall>[0];
export interface LexicalRecallOptions {
  query: string;
  limit?: number;
  maxChars?: number;
  hitProjection?: undefined;
}
export const lexicalRecallLimits = {
  maxCorpusItems: 1000,
  maxQueryChars: 500,
  maxReturnedHits: 20,
  maxOutputChars: 16000,
  minOutputChars: 1024,
  maxCorpusTextChars: 2_097_152,
} as const;
export interface LexicalHit {
  memoryId: string;
  episodeId: string;
  text: string;
  claim: { key: string; value: string };
  sourceKeys: string[];
  status: "unconfirmed-source-attributed-inference";
}
/** Address metadata only. The fixed Git wrapper verifies actual bytes; this engine performs no reads/authentication. */
export interface GitDatasetInputPin {
  repositoryId: string;
  commit: string;
  path: string;
  sha256: string;
  blobOid: string;
  byteLength: number;
}
export interface GitDatasetHitProjection {
  kind: "git-dataset-memory-reference.v1";
  datasetInput: GitDatasetInputPin;
}
export interface GitDatasetLexicalRecallOptions extends Omit<
  LexicalRecallOptions,
  "hitProjection"
> {
  hitProjection: GitDatasetHitProjection;
}
export interface GitDatasetLexicalHit extends Omit<LexicalHit, "sourceKeys"> {
  provenance: GitDatasetHitProjection & {
    fragment: string;
    sourceKeysFragment: string;
    sourceKeysCount: number;
    sourceKeysSha256: string;
  };
}
type Authoritative = ReturnType<typeof qualifyRecall>["authoritative"][number];
export interface LexicalRecallResult {
  schema: "workspace-architecture.lexical-recall-result.v1";
  claimKey: string;
  hits: LexicalHit[];
  authoritative: Authoritative[];
  selectedValue: string | null;
  selectionClass: "current-authoritative-record" | "unresolved";
  conflict: boolean;
  authoritativeConflict: boolean;
  conflictScope: "eligible-explicit-claim-key";
  counts: {
    corpus: number;
    eligible: number;
    withheld: number;
    matched: number;
    authoritative: number;
    withheldAuthoritative: number;
  };
  omitted: {
    hitLimit: number;
    hitChars: number;
    authoritative: number;
    authoritativeSelection: boolean;
  };
  index: { kind: "sqlite-fts5"; lifetime: "one-in-memory-query"; disposed: true };
  disclosure: string[];
  nativePermissionGranted: false;
  productionRelevanceQualified: false;
  outputChars: number;
}

export interface GitDatasetLexicalRecallResult extends Omit<
  LexicalRecallResult,
  "schema" | "hits"
> {
  schema: "workspace-architecture.lexical-recall-git-dataset-reference-result.v1";
  hits: GitDatasetLexicalHit[];
}
function projectionOption(
  value: unknown,
  repositoryId: string,
): GitDatasetHitProjection | undefined {
  if (value === undefined) return undefined;
  const exact = (value: unknown, keys: string[]): value is Record<string, unknown> =>
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    Object.keys(value).sort().join("|") === [...keys].sort().join("|");
  if (
    !exact(value, ["kind", "datasetInput"]) ||
    value.kind !== "git-dataset-memory-reference.v1" ||
    !exact(value.datasetInput, [
      "repositoryId",
      "commit",
      "path",
      "sha256",
      "blobOid",
      "byteLength",
    ])
  )
    throw Error("Recall Git dataset projection metadata is invalid.");
  const pin = value.datasetInput;
  if (
    typeof pin.repositoryId !== "string" ||
    !pin.repositoryId.trim() ||
    pin.repositoryId.length > 256 ||
    pin.repositoryId !== repositoryId ||
    typeof pin.commit !== "string" ||
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(pin.commit) ||
    typeof pin.path !== "string" ||
    pin.path.length > 4096 ||
    pin.path.includes("\\") ||
    pin.path.includes(":") ||
    /[\u0000-\u001f\u007f]/.test(pin.path) ||
    pin.path.split("/").some((part) => !part || part === "." || part === "..") ||
    !/^work\/[^/]+\/[^/]+\/inputs\/[^/]+\.json$/.test(pin.path) ||
    typeof pin.sha256 !== "string" ||
    !/^[a-f0-9]{64}$/.test(pin.sha256) ||
    typeof pin.blobOid !== "string" ||
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(pin.blobOid) ||
    pin.blobOid.length !== pin.commit.length ||
    !Number.isSafeInteger(pin.byteLength) ||
    Number(pin.byteLength) < 1 ||
    Number(pin.byteLength) > 16_777_216
  )
    throw Error("Recall Git dataset projection metadata is invalid.");
  return structuredClone(value) as unknown as GitDatasetHitProjection;
}

function assertUniqueIds(rows: readonly { id: string }[], kind: string): void {
  const ids = rows.map((row) => row.id);
  if (ids.some((id) => !id) || new Set(ids).size !== ids.length)
    throw Error(`Recall ${kind} identities are absent or ambiguous.`);
}
/** Match literal unicode tokens; no FTS operators, prefix queries or SQL are accepted. */
function literalMatch(query: string): string | null {
  const terms = [...new Set(query.match(/[\p{L}\p{N}_]+/gu) ?? [])];
  return terms.length ? terms.map((term) => `"${term.replaceAll('"', '""')}"`).join(" AND ") : null;
}

/** Query-scoped derived index only. Existing source/policy owners supply all input. */
export function lexicalRecall(
  input: LexicalRecallInput,
  options: GitDatasetLexicalRecallOptions,
): GitDatasetLexicalRecallResult;
export function lexicalRecall(
  input: LexicalRecallInput,
  options: LexicalRecallOptions,
): LexicalRecallResult;
export function lexicalRecall(
  input: LexicalRecallInput,
  options: LexicalRecallOptions | GitDatasetLexicalRecallOptions,
): LexicalRecallResult | GitDatasetLexicalRecallResult {
  const limit = options.limit ?? 10,
    maxChars = options.maxChars ?? 8000;
  if (typeof options.query !== "string" || options.query.length > lexicalRecallLimits.maxQueryChars)
    throw Error("Recall query must be a string of at most 500 characters.");
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > lexicalRecallLimits.maxReturnedHits)
    throw Error("Recall limit must be an integer between 1 and 20.");
  if (
    !Number.isSafeInteger(maxChars) ||
    maxChars < lexicalRecallLimits.minOutputChars ||
    maxChars > lexicalRecallLimits.maxOutputChars
  )
    throw Error("Recall maxChars must be an integer between 1024 and 16000.");
  if (!input.claimKey?.trim()) throw Error("Recall requires an explicit claimKey.");
  if (
    [input.memories, input.episodes, input.authoritativeClaims].some(
      (rows) => rows.length > lexicalRecallLimits.maxCorpusItems,
    )
  )
    throw Error("Recall corpus exceeds 1000 items.");
  if (
    input.memories.reduce((sum, row) => sum + row.text.length, 0) >
    lexicalRecallLimits.maxCorpusTextChars
  )
    throw Error("Recall corpus text exceeds the local 2 MiB character bound.");
  assertUniqueIds(input.memories, "memory");
  assertUniqueIds(input.episodes, "Episode");

  const projection = projectionOption(options.hitProjection, input.context.repositoryId);
  const originalMemories = projection ? structuredClone(input.memories) : undefined;
  // Qualification happens before any derived text enters SQLite or any hit is projected.
  const qualified = qualifyRecall(input);
  const expression = literalMatch(options.query);
  let rows: Array<{ ordinal: number }> = [],
    matched = 0;
  const db = new Database(":memory:");
  try {
    db.run("CREATE VIRTUAL TABLE recall USING fts5(ordinal UNINDEXED, text, tokenize='unicode61')");
    const insert = db.query("INSERT INTO recall(ordinal, text) VALUES (?, ?)");
    qualified.eligible.forEach((memory, ordinal) => insert.run(ordinal, memory.text));
    if (expression) {
      matched = db
        .query<{ count: number }, [string]>(
          "SELECT count(*) AS count FROM recall WHERE recall MATCH ?",
        )
        .get(expression)!.count;
      rows = db
        .query<{ ordinal: number }, [string, number]>(
          "SELECT ordinal FROM recall WHERE recall MATCH ? ORDER BY bm25(recall), rowid LIMIT ?",
        )
        .all(expression, limit);
    }
  } finally {
    db.close();
  }

  const result: Omit<LexicalRecallResult, "schema" | "hits"> & {
    schema: LexicalRecallResult["schema"] | GitDatasetLexicalRecallResult["schema"];
    hits: Array<LexicalHit | GitDatasetLexicalHit>;
  } = {
    schema: projection
      ? "workspace-architecture.lexical-recall-git-dataset-reference-result.v1"
      : "workspace-architecture.lexical-recall-result.v1",
    claimKey: input.claimKey,
    hits: [],
    authoritative: [],
    selectedValue: null,
    selectionClass: "unresolved",
    conflict: qualified.conflict,
    authoritativeConflict: qualified.authoritativeConflict,
    conflictScope: "eligible-explicit-claim-key",
    counts: {
      corpus: input.memories.length,
      eligible: qualified.eligible.length,
      withheld: qualified.withheld.length,
      matched,
      authoritative: qualified.authoritative.length,
      withheldAuthoritative: qualified.withheldAuthoritative.length,
    },
    omitted: {
      hitLimit: matched - rows.length,
      hitChars: rows.length,
      authoritative: qualified.authoritative.length,
      authoritativeSelection: qualified.selectedValue !== null,
    },
    index: { kind: "sqlite-fts5", lifetime: "one-in-memory-query", disposed: true },
    disclosure: [
      "Exact-token lexical Recall over current qualified input; semantic/morphological relevance and remote policy/receipt authenticity remain unqualified.",
      "Derived hits remain unconfirmed. Applicable current source-owned records take precedence; omitted or unavailable records do not become accepted inferences.",
    ],
    nativePermissionGranted: false,
    productionRelevanceQualified: false,
    outputChars: 0,
  };
  if (projection)
    result.disclosure.push(
      "Projected provenance addresses the complete immutable dataset row and full source-key list; references grant no source access, receipt authenticity or native permission.",
    );
  const measured = () => {
    result.outputChars = JSON.stringify(result).length;
    result.outputChars = JSON.stringify(result).length;
    return result.outputChars;
  };
  const updateSelection = () => {
    const visible =
      qualified.selectedValue !== null &&
      result.authoritative.some((row) => row.value === qualified.selectedValue);
    result.selectedValue = visible ? qualified.selectedValue : null;
    result.selectionClass = visible ? "current-authoritative-record" : "unresolved";
    result.omitted.authoritativeSelection = qualified.selectedValue !== null && !visible;
  };
  if (measured() > maxChars)
    throw Error(
      "Recall metadata exceeds maxChars; use a shorter claimKey or larger output budget.",
    );
  // Whole authoritative records come first. Omission never promotes a derived hit.
  for (const authoritative of qualified.authoritative.slice(
    0,
    lexicalRecallLimits.maxReturnedHits,
  )) {
    result.authoritative.push(structuredClone(authoritative));
    result.omitted.authoritative--;
    updateSelection();
    if (measured() > maxChars) {
      result.authoritative.pop();
      result.omitted.authoritative++;
      updateSelection();
    }
  }
  for (const row of rows) {
    const memory = qualified.eligible[row.ordinal]!;
    const core = {
      memoryId: memory.id,
      episodeId: memory.episodeId,
      text: memory.text,
      claim: structuredClone(memory.claim),
    };
    let hit: LexicalHit | GitDatasetLexicalHit;
    if (projection) {
      if (!isDeepStrictEqual(input.memories, originalMemories))
        throw Error("Recall Git dataset memory order/content changed during projection.");
      const indices = originalMemories!.flatMap((row, index) =>
        row.id === memory.id ? [index] : [],
      );
      const { claimClass, ...qualifiedMemory } = memory;
      if (
        indices.length !== 1 ||
        !isDeepStrictEqual(originalMemories![indices[0]!], qualifiedMemory)
      )
        throw Error("Recall Git dataset memory row reference is absent, ambiguous or mismatched.");
      const fragment = `#/content/memories/${indices[0]!}`;
      hit = {
        ...core,
        status: claimClass,
        provenance: {
          ...structuredClone(projection),
          fragment,
          sourceKeysFragment: fragment + "/sourceKeys",
          sourceKeysCount: memory.sourceKeys.length,
          sourceKeysSha256: sha256(JSON.stringify(memory.sourceKeys)),
        },
      };
    } else
      hit = {
        ...core,
        sourceKeys: [...memory.sourceKeys],
        status: "unconfirmed-source-attributed-inference",
      };
    result.hits.push(hit);
    result.omitted.hitChars--;
    if (measured() > maxChars) {
      result.hits.pop();
      result.omitted.hitChars++;
    }
  }
  if (measured() > maxChars)
    throw Error("Recall output could not preserve the configured whole-row budget.");
  return result as LexicalRecallResult | GitDatasetLexicalRecallResult;
}
