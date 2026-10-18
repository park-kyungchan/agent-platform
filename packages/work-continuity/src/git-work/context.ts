import { assertCommit, assertGitPath, gitRecordPath, gitWorkPath } from "./source.ts";
import { assertLogicalIdentity } from "../logical-identity.ts";
import { validateTaskOperationalState } from "../operational-state.ts";
import {
  dependencyFindings,
  effectInventory,
  loadRecords,
  outcome,
  recordDependencies,
  recordKey,
  validateRecordSet,
  validateRecordSources,
} from "./records.ts";
import type {
  ContextRecordProjection,
  CompactGitWorkContext,
  MetadataPackedGitWorkContext,
  GitWorkCompactOutput,
  CompactGitSourceAddress,
  CompactSourcePointer,
  CompactPurpose,
  LogicalIdentity,
  TaskOperationalState,
  DecodedCompactGitWorkContext,
  ContextRequest,
  GitWorkContext,
  GitWorkContinuityBinding,
  GitWorkReader,
  GitWorkRecord,
  JsonValue,
  NormativeContextSource,
  RecordReference,
  ResolvedSource,
  SourceReference,
  StartRequest,
  ValidationResult,
} from "./types.ts";

async function buildGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
): Promise<{ context: GitWorkContext; requiredRecords: number }> {
  request = structuredClone(request);
  const maxChars = request.maxChars ?? 24000,
    maxRecords = request.maxRecords ?? 256;
  if (
    !Number.isSafeInteger(maxChars) ||
    maxChars < 1024 ||
    maxChars > 24000 ||
    !Number.isSafeInteger(maxRecords) ||
    maxRecords < 1 ||
    maxRecords > 2048 ||
    !["current", "advanced", "unknown", "unavailable"].includes(request.latestRelevant)
  )
    throw Error("INVALID_CONTEXT_BUDGET_OR_CURRENTNESS");
  const records = new Map<string, GitWorkRecord>(),
    holdReasons: string[] = [],
    sources: ResolvedSource[] = [];
  const normativeSources = new Map<string, ResolvedSource>();
  let existingEffects: Set<string> | undefined;
  // Inventory only canonical tree addresses; unrelated payloads remain unread.
  const queueExistingEffects = async (ids: string[]) => {
    if (!ids.length) return;
    existingEffects ??= new Set(
      (await reader.listRecords())
        .filter((reference) => reference.kind === "effect")
        .map((reference) => reference.id),
    );
    for (const id of ids) if (existingEffects.has(id)) queue.push({ kind: "effect", id });
  };
  const queue: RecordReference[] = [{ kind: "work", id: reader.selection.workId }];
  if (request.taskId) queue.push({ kind: "task", id: request.taskId });
  if (request.decisionId) queue.push({ kind: "decision", id: request.decisionId });
  const seen = new Set<string>();
  let omittedRecords = 0;
  while (queue.length) {
    const reference = queue.shift()!,
      key = recordKey(reference);
    if (seen.has(key)) continue;
    seen.add(key);
    if (records.size >= maxRecords) {
      omittedRecords++;
      holdReasons.push("REQUIRED_CONTEXT_RECORD_LIMIT");
      continue;
    }
    try {
      const record = await reader.readRecord(reference);
      records.set(key, record);
      queue.push(...recordDependencies(record));
      const resolved = await validateRecordSources(record, reader);
      for (const source of resolved) {
        sources.push(source);
      }
      if (["work", "spec", "plan", "task"].includes(record.kind)) {
        const source = await reader.readSource({
          kind: "snapshot",
          path: gitRecordPath(reader.selection.projectId, reader.selection.workId, reference),
        });
        normativeSources.set(key, source);
        sources.push(source);
      }
      if (record.kind === "task") await queueExistingEffects(record.pendingEffectIds ?? []);
      if (record.kind === "work" && record.continuity)
        for (const reference of record.continuity.effectInventorySources) {
          const inventory = effectInventory((await reader.readSource(reference)).value);
          await queueExistingEffects([
            ...inventory.completedEffectIds,
            ...inventory.uncertainEffectIds,
          ]);
        }
    } catch (error) {
      holdReasons.push(
        `${error instanceof Error ? error.message : "REQUIRED_SOURCE_UNAVAILABLE"}:${key}`,
      );
    }
  }
  holdReasons.push(...(await validateRecordSet(records, reader)).findings);
  // Effect inventories require the producer's complete proof graph for validation.
  // That reverse provenance edge does not make an unrelated producer's historical
  // action contract normative for the selected Work/Task. Explicit Task dependency
  // edges still carry their own complete, possibly historical, Spec/Plan bindings.
  const normativeRecords = new Set<string>();
  const normativeQueue: RecordReference[] = [{ kind: "work", id: reader.selection.workId }];
  if (request.taskId) normativeQueue.push({ kind: "task", id: request.taskId });
  if (request.decisionId) normativeQueue.push({ kind: "decision", id: request.decisionId });
  while (normativeQueue.length) {
    const reference = normativeQueue.shift()!,
      key = recordKey(reference);
    if (normativeRecords.has(key)) continue;
    normativeRecords.add(key);
    const record = records.get(key);
    if (record)
      normativeQueue.push(
        ...recordDependencies(record).filter(
          (dependency) => record.kind !== "effect" || dependency.kind !== "task",
        ),
      );
  }
  if (request.taskId) {
    const selected = records.get(recordKey({ kind: "task", id: request.taskId }));
    if (selected?.kind === "task") {
      if (reader.work.continuity && selected.nextAction === undefined)
        holdReasons.push(`SELECTED_TASK_NEXT_ACTION_REQUIRED:${selected.id}`);
      for (const dependency of selected.dependsOn)
        holdReasons.push(...(await dependencyFindings(selected, dependency, records, reader)));
    }
  }
  if (request.latestRelevant !== "current")
    holdReasons.push(`REQUIRED_LATEST_CONTEXT_${request.latestRelevant.toUpperCase()}`);
  const fences = [...records.values()].filter((record) => record.kind === "effect-fence");
  const effects = [...records.values()].filter((record) => record.kind === "effect");
  const completed = new Set([
      ...fences.flatMap((record) => record.completedEffectIds),
      ...effects.filter((record) => record.status === "completed").map((record) => record.id),
    ]),
    uncertain = new Set([
      ...fences.flatMap((record) => record.uncertainEffectIds),
      ...effects.filter((record) => record.status === "uncertain").map((record) => record.id),
    ]);
  const relevantIds = [
    ...new Set(
      [...records.values()].flatMap((record) =>
        record.kind === "task"
          ? [record.id, ...record.effectIds, ...(record.pendingEffectIds ?? [])]
          : [],
      ),
    ),
  ];
  // Blob identity alone cannot represent distinct original owner declarations.
  const sourceKey = (reference: SourceReference): string => {
    const original = reference.kind === "native" ? reference.receipt : reference;
    const repositoryId =
      original.kind === "git" ? original.repositoryId : reader.selection.repositoryId;
    const commit = original.kind === "git" ? original.commit : reader.commit;
    return JSON.stringify([
      reference.kind,
      ...(reference.kind === "native" ? [reference.ownerId, reference.locator] : []),
      original.kind,
      repositoryId,
      commit,
      original.path,
      original.fragment ?? null,
      original.sha256 ?? null,
    ]);
  };
  const sourceByDeclaration = new Map(
    sources.map((source) => [sourceKey(source.declaredNativeSource ?? source.reference), source]),
  );
  const uniqueSources = [...sourceByDeclaration.values()];
  const sourceIds = new Map(
    [...sourceByDeclaration.keys()].map((key, index) => [key, `s${index + 1}`]),
  );
  const sourceId = (reference: SourceReference) => sourceIds.get(sourceKey(reference)) ?? null;
  const json = (value: unknown) => JSON.parse(JSON.stringify(value)) as JsonValue;
  const projected: ContextRecordProjection[] = [];
  for (const [key, record] of records) {
    if (record.kind === "effect-fence") continue;
    const base = { kind: record.kind, id: record.id };
    if (["work", "spec", "plan", "task"].includes(record.kind) && !normativeRecords.has(key)) {
      const source = normativeSources.get(key);
      projected.push({
        ...base,
        contextRole: "provenance",
        source: source ? sourceId(source.reference) : null,
        ...(record.kind === "spec" || record.kind === "plan" ? { revision: record.revision } : {}),
        ...(record.kind === "task" ? { logical: json(record.logical) } : {}),
      });
      continue;
    }
    switch (record.kind) {
      case "work":
        projected.push({
          ...base,
          purpose: json(record.purpose),
          domainId: record.domainId,
          issueId: record.issueId,
          spec: json(record.spec),
          plan: json(record.plan),
        });
        break;
      case "decision":
        projected.push({
          ...base,
          status: record.status,
          meaning: record.meaning,
          answerSource: sourceId(record.answerSource),
          ...(record.selectionContext
            ? { selectionContextSource: sourceId(record.selectionContext.source) }
            : {}),
        });
        break;
      case "task":
        projected.push({
          ...base,
          title: record.title,
          ...(record.nextAction === undefined ? {} : { nextAction: record.nextAction }),
          executionClass: record.executionClass,
          logical: json(record.logical),
          state: json(record.state),
          dependsOn: json(record.dependsOn),
          effectIds: record.effectIds,
          ...(record.artifact?.kind === "task"
            ? { acceptanceCriteria: record.artifact.payload.acceptanceCriteria }
            : {}),
          ...(record.pendingEffectIds?.length ? { pendingEffectIds: record.pendingEffectIds } : {}),
        });
        break;
      case "spec":
        projected.push({
          ...base,
          revision: record.revision,
          outcome: record.outcome,
          criteria: record.criteria,
        });
        break;
      case "plan":
        projected.push({
          ...base,
          revision: record.revision,
          objective: record.objective,
          specId: record.specId,
          specRevision: record.specRevision,
          ...(record.clauses ? { clauses: record.clauses } : {}),
        });
        break;
      case "evidence":
        projected.push({
          ...base,
          evidenceKind: record.evidenceKind,
          source: sourceId(record.source),
        });
        break;
      case "input":
        projected.push({ ...base, inputKind: record.inputKind, origin: json(record.origin) });
        break;
      case "effect":
      case "relation":
        projected.push(json(record) as ContextRecordProjection);
        break;
    }
  }
  const contracts: NormativeContextSource[] = [];
  for (const [key, source] of normativeSources) {
    if (!normativeRecords.has(key)) continue;
    const record = records.get(key)!;
    const base = { id: record.id, source: sourceId(source.reference)! };
    if (record.kind === "work")
      contracts.push({
        ...base,
        kind: "work",
        fields: {
          purpose: "#/purpose",
          spec: "#/spec",
          plan: "#/plan",
        },
        coverage: "complete",
      });
    if (record.kind === "spec")
      contracts.push({
        ...base,
        kind: "spec",
        revision: record.revision,
        fields: { outcome: "#/outcome", criteria: "#/criteria" },
        coverage: "complete",
      });
    if (record.kind === "plan")
      contracts.push({
        ...base,
        kind: "plan",
        revision: record.revision,
        fields: {
          objective: "#/objective",
          specId: "#/specId",
          specRevision: "#/specRevision",
          ...(record.clauses ? { clauses: "#/clauses" } : {}),
        },
        coverage: record.clauses ? "complete" : "legacy-objective-only",
      });
    if (record.kind === "task")
      contracts.push({
        ...base,
        kind: "task",
        fields: record.nextAction === undefined ? {} : { nextAction: "#/nextAction" },
        coverage: record.nextAction === undefined ? "unspecified" : "complete",
      });
  }
  const binding = reader.work.continuity;
  const purposeSources = (binding?.purposeSources ?? []).map(sourceId);
  const effectInventorySources = (binding?.effectInventorySources ?? []).map(sourceId);
  const result: GitWorkContext = {
    status: holdReasons.length ? "held" : "source-qualified",
    repositoryId: reader.selection.repositoryId,
    commit: reader.commit,
    projectId: reader.selection.projectId,
    workId: reader.selection.workId,
    records: projected,
    sources: uniqueSources.map((source, index) => ({
      id: `s${index + 1}`,
      path: source.path.startsWith(`${reader.workPath}/`)
        ? source.path.slice(reader.workPath.length + 1)
        : source.path,
      pathBase: source.path.startsWith(`${reader.workPath}/`) ? "work" : "repository",
      blobOid: source.blobOid,
      ...(source.reference.fragment ? { fragment: source.reference.fragment } : {}),
      ...(source.repositoryId !== reader.selection.repositoryId || source.commit !== reader.commit
        ? { repositoryId: source.repositoryId, commit: source.commit }
        : {}),
      ...(source.declaredNativeSource
        ? {
            declaredNativeSource: source.declaredNativeSource,
            nativeAuthorshipVerified: false as const,
          }
        : {}),
    })),
    projection: "verified-semantic-context",
    sourcePathBase: reader.workPath,
    holdReasons: [...new Set(holdReasons)],
    omittedRecords,
    latestRelevant: request.latestRelevant,
    normative: { schema: "common.git-work-context.normative.v1", contracts },
    continuityBinding: {
      schema: "common.git-work-context.continuity.v1",
      coverage: !binding
        ? "legacy-unbound"
        : [...purposeSources, ...effectInventorySources].includes(null)
          ? "unavailable"
          : "bound",
      purposeSources: purposeSources.filter((id): id is string => id !== null),
      effectInventorySources: effectInventorySources.filter((id): id is string => id !== null),
    },
    historicalEffects: {
      completedCount: completed.size,
      uncertainCount: uncertain.size,
      relevantCompletedIds: relevantIds.filter((id) => completed.has(id)),
      relevantUncertainIds: relevantIds.filter((id) => uncertain.has(id)),
      projectedFenceRecords: fences.length,
    },
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
  return { context: result, requiredRecords: records.size };
}

function outputLimit(context: GitWorkContext, requiredRecords: number): GitWorkContext {
  return {
    ...context,
    status: "held",
    records: [],
    sources: [],
    historicalEffects: undefined,
    normative: { schema: "common.git-work-context.normative.v1", contracts: [] },
    continuityBinding: {
      schema: "common.git-work-context.continuity.v1",
      coverage: "unavailable",
      purposeSources: [],
      effectInventorySources: [],
    },
    holdReasons: ["REQUIRED_CONTEXT_OUTPUT_LIMIT"],
    omittedRecords: requiredRecords + context.omittedRecords,
  };
}

/** The diagnostic raw representation retains its existing 24k output budget. */
export async function readGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
): Promise<GitWorkContext> {
  request = structuredClone(request);
  const { context, requiredRecords } = await buildGitWorkContext(reader, request);
  return JSON.stringify(context).length > (request.maxChars ?? 24000)
    ? outputLimit(context, requiredRecords)
    : context;
}

/** Compose the one canonical validation pipeline and codec before output sizing.
 * The unbounded intermediate projection is private, never a caller-supplied
 * qualified context. Native owners still enforce their wrapper-inclusive caps.
 */
export function readCompactGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
  selection: { taskId?: string; decisionIds?: string[] },
): Promise<CompactGitWorkContext>;
export function readCompactGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
  selection: { taskId?: string; decisionIds?: string[] },
  encoding: "metadata.v1",
): Promise<GitWorkCompactOutput>;
export async function readCompactGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
  selection: { taskId?: string; decisionIds?: string[] },
  encoding?: "metadata.v1",
): Promise<GitWorkCompactOutput> {
  if (encoding !== undefined && encoding !== "metadata.v1") throw Error("INVALID_COMPACT_ENCODING");
  request = structuredClone(request);
  selection = structuredClone(selection);
  if (selection.taskId !== request.taskId) throw Error("COMPACT_REQUEST_SELECTION_MISMATCH");
  const { context, requiredRecords } = await buildGitWorkContext(reader, request);
  const ordinary = encodeCompactGitWorkContext(context, selection, false);
  const compact =
    encoding && ordinary.status === "source-qualified" ? packCompactMetadata(ordinary) : ordinary;
  const maxChars = request.maxChars ?? 24000;
  if (JSON.stringify(compact).length <= maxChars) return compact;
  const held = compactGitWorkContext(outputLimit(context, requiredRecords), selection);
  // The existing HOLD wire includes required metadata too. A valid request
  // budget may be too small even for that refusal; never return oversized data
  // or invent a weaker compact representation to make it fit.
  if (JSON.stringify(held).length > maxChars)
    throw new CompactContextError("COMPACT_CONTEXT_OUTPUT_BUDGET_TOO_SMALL");
  return held;
}

const compactObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const compactText = (value: unknown): value is string =>
  typeof value === "string" &&
  value.trim().length > 0 &&
  value.length <= 12_000 &&
  !value.includes("\0");
class CompactContextError extends Error {}
function compactAssert(condition: unknown, code: string): asserts condition {
  if (!condition) throw new CompactContextError(code);
}
// The raw reader retains forensic detail; native compact output carries owned codes only.
const compactSourceHoldCodes = new Set([
  "REQUIRED_CONTEXT_OUTPUT_LIMIT",
  "REQUIRED_CONTEXT_RECORD_LIMIT",
  "REQUIRED_LATEST_CONTEXT_ADVANCED",
  "REQUIRED_LATEST_CONTEXT_UNKNOWN",
  "REQUIRED_LATEST_CONTEXT_UNAVAILABLE",
  "SELECTED_TASK_NEXT_ACTION_REQUIRED",
  "COMPLETED_EFFECT_REPLAY",
  "UNCERTAIN_EFFECT_REPLAY",
  "EFFECT_REPLAY_OR_UNCERTAIN",
  "EFFECT_STATUS_CONFLICT",
  "CONSUMED_COMPLETED_EFFECT_NOT_REACHABLE",
  "CONSUMED_UNCERTAIN_EFFECT_NOT_REACHABLE",
  "SOURCE_CONTENT_MISMATCH",
]);
const compactSourceHold = (value: unknown): string => {
  const code = typeof value === "string" ? value.split(":", 1)[0]! : "";
  return compactSourceHoldCodes.has(code) ? code : "SOURCE_CONTEXT_UNAVAILABLE";
};
const compactFieldsMatch = (actual: unknown, expected: Record<string, string>) =>
  compactObject(actual) &&
  Object.keys(actual).length === Object.keys(expected).length &&
  Object.entries(expected).every(([key, value]) => actual[key] === value);
const compactLayout = {
  strings:
    "Zero-based string indices; preserve array order and empty arrays. fields are exact source JSON pointers.",
  sources:
    "[pathBase,path,blobOid,repositoryId?,commit?]; null/omitted repo+commit=context. work paths=sourcePathBase/path.",
  pointer:
    "[sources index,JSON pointer,native declaration?]; Git proof never authenticates native provenance.",
  normative:
    "work:[kind,id,source,fields,coverage,[objective,successCriteria,inScope,outOfScope],{spec:[id,revision],plan:[id,revision]},{domainId,issueId}]; spec:[kind,id,revision,source,fields,coverage,[outcome,criteria]]; plan:[kind,id,revision,source,fields,coverage,[objective,clauses?],[specId,specRevision]]. Only normative text values index strings; IDs/revisions are literal.",
  task: "logical:[domainId,projectId,workId,issueId,specId,specRevision,planId,planRevision,taskId]; state:[execution,verification,localAudit,sharedAudit,formalCompletion]. nextAction.value indexes strings.",
  decisions:
    "[id,status,answer pointer,question pointer?]; all reached originals retained, without raw text or Input.origin.",
};

/** Lossless source-only projection for native owners to wrap and budget themselves.
 * No reads, native effects, raw original text, fallback prose, or budget expansion occur here. */
export function compactGitWorkContext(
  context: GitWorkContext,
  selection: { taskId?: string; decisionIds?: string[] },
): CompactGitWorkContext {
  return encodeCompactGitWorkContext(context, selection, true);
}

// Only the composed reader above can pass its privately constructed projection
// without the standalone codec's serialized-input limit. Both use this one codec.
function encodeCompactGitWorkContext(
  context: GitWorkContext,
  selection: { taskId?: string; decisionIds?: string[] },
  boundedExternalInput: boolean,
): CompactGitWorkContext {
  const result: CompactGitWorkContext = {
    schema: "common.git-work-context.compact.v1",
    sourcePathBase: "",
    layout: { ...compactLayout },
    status: "held",
    repositoryId: "",
    commit: "",
    projectId: "",
    workId: "",
    latestRelevant: "unknown",
    strings: [],
    sources: [],
    normative: [],
    decisions: [],
    continuityBinding: {
      schema: "common.git-work-context.continuity.v1",
      coverage: "unavailable",
      purposeSources: [],
      effectInventorySources: [],
    },
    historicalEffects: undefined,
    holdReasons: [],
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
  try {
    compactAssert(compactObject(context), "INVALID_COMPACT_CONTEXT");
    result.holdReasons = Array.isArray(context.holdReasons)
      ? context.holdReasons.map(compactSourceHold)
      : [];
    const { repositoryId, commit, projectId, workId, sourcePathBase, latestRelevant } = context;
    compactAssert(compactText(repositoryId), "COMPACT_REPOSITORY_REQUIRED");
    assertCommit(commit);
    const workPath = gitWorkPath(projectId, workId);
    compactAssert(sourcePathBase === workPath, "COMPACT_SOURCE_BASE_MISMATCH");
    compactAssert(
      ["current", "advanced", "unknown", "unavailable"].includes(latestRelevant),
      "COMPACT_CURRENTNESS_UNAVAILABLE",
    );
    Object.assign(result, {
      repositoryId,
      commit,
      projectId,
      workId,
      sourcePathBase,
      latestRelevant,
    });
    result.historicalEffects = copyCompactHistoricalEffects(context.historicalEffects);
    compactAssert(context.status === "source-qualified", "SOURCE_CONTEXT_HELD");
    compactAssert(
      result.holdReasons.length === 0 &&
        context.omittedRecords === 0 &&
        context.authority === "none" &&
        context.nativePermissionGranted === false &&
        context.semanticAcceptanceEstablished === false &&
        context.projection === "verified-semantic-context",
      "INVALID_COMPACT_SOURCE_QUALIFICATION",
    );
    compactAssert(context.latestRelevant === "current", "COMPACT_CURRENTNESS_UNAVAILABLE");
    compactAssert(
      !boundedExternalInput || JSON.stringify(context).length <= 24_000,
      "REQUIRED_COMPACT_INPUT_LIMIT",
    );
    compactAssert(
      Array.isArray(context.records) &&
        context.records.length <= 2048 &&
        Array.isArray(context.sources) &&
        context.sources.length <= 8192 &&
        context.normative?.schema === "common.git-work-context.normative.v1" &&
        Array.isArray(context.normative.contracts),
      "INVALID_COMPACT_CONTEXT",
    );
    compactAssert(
      compactObject(selection) && (selection.taskId === undefined || compactText(selection.taskId)),
      "INVALID_COMPACT_SELECTION",
    );
    const strings = new Map<string, number>();
    const stringIndex = (value: unknown): number => {
      compactAssert(compactText(value), "INVALID_COMPACT_NORMATIVE_TEXT");
      let index = strings.get(value);
      if (index === undefined) {
        index = result.strings.length;
        strings.set(value, index);
        result.strings.push(value);
      }
      return index;
    };
    const texts = compactTexts;
    const indices = (value: unknown): number[] => texts(value).map(stringIndex);
    const records = new Map<string, ContextRecordProjection>();
    for (const record of context.records) {
      if (record.kind === "spec" || record.kind === "plan")
        compactAssert(
          Number.isSafeInteger(record.revision) && Number(record.revision) > 0,
          "INVALID_COMPACT_CONTRACT_REVISION",
        );
      const key = recordKey({
        kind: record.kind,
        id: record.id,
        ...(record.kind === "spec" || record.kind === "plan"
          ? { revision: Number(record.revision) }
          : {}),
      });
      compactAssert(!records.has(key), "DUPLICATE_COMPACT_RECORD");
      records.set(key, record);
    }
    const declarations = new Map(context.sources.map((source) => [source.id, source]));
    compactAssert(declarations.size === context.sources.length, "DUPLICATE_COMPACT_SOURCE_ID");
    const pooled = new Map<string, number>();
    const pointer = (id: unknown): CompactSourcePointer => {
      compactAssert(compactText(id), "COMPACT_SOURCE_ID_REQUIRED");
      const source = declarations.get(id);
      compactAssert(source, "COMPACT_SOURCE_UNAVAILABLE");
      compactAssert(
        source.pathBase === "work" || source.pathBase === "repository",
        "COMPACT_SOURCE_PATH_BASE_UNAVAILABLE",
      );
      const path = source.pathBase === "work" ? `${workPath}/${source.path}` : source.path;
      assertGitPath(path);
      const repositoryId = source.repositoryId ?? context.repositoryId;
      const commit = source.commit ?? context.commit;
      compactAssert(compactText(repositoryId), "COMPACT_SOURCE_REPOSITORY_REQUIRED");
      assertCommit(commit);
      compactAssert(
        /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(source.blobOid),
        "INVALID_COMPACT_BLOB_ID",
      );
      const fragment = source.fragment ?? "#";
      compactAssert(
        typeof fragment === "string" &&
          (fragment === "#" || fragment.startsWith("#/")) &&
          !/~(?![01])/.test(fragment),
        "INVALID_COMPACT_SOURCE_FRAGMENT",
      );
      const key = JSON.stringify([repositoryId, commit, path, source.blobOid]);
      let index = pooled.get(key);
      if (index === undefined) {
        index = result.sources.length;
        pooled.set(key, index);
        const address: CompactGitSourceAddress = [source.pathBase, source.path, source.blobOid];
        if (repositoryId !== context.repositoryId || commit !== context.commit)
          address.push(
            repositoryId === context.repositoryId ? null : repositoryId,
            commit === context.commit ? null : commit,
          );
        result.sources.push(address);
      }
      const native = source.declaredNativeSource;
      if (native) {
        compactAssert(
          native.kind === "native" &&
            compactText(native.ownerId) &&
            compactText(native.locator) &&
            native.receipt &&
            ["snapshot", "git"].includes(native.receipt.kind),
          "INVALID_COMPACT_NATIVE_DECLARATION",
        );
        const receipt = native.receipt;
        compactAssert(
          receipt.path === path &&
            (receipt.fragment ?? "#") === fragment &&
            (receipt.kind === "git" ? receipt.repositoryId : context.repositoryId) ===
              repositoryId &&
            (receipt.kind === "git" ? receipt.commit : context.commit) === commit &&
            (receipt.sha256 === undefined || /^[a-f0-9]{64}$/.test(receipt.sha256)),
          "COMPACT_NATIVE_RECEIPT_ADDRESS_MISMATCH",
        );
        return [
          index,
          fragment,
          {
            ownerId: native.ownerId,
            locator: native.locator,
            receiptKind: receipt.kind,
            ...(receipt.sha256 ? { sha256: receipt.sha256 } : {}),
          },
        ];
      }
      return [index, fragment];
    };
    const contractSources = new Map<string, NormativeContextSource>();
    for (const contract of context.normative.contracts) {
      compactAssert(
        ["work", "spec", "plan", "task"].includes(contract.kind),
        "INVALID_COMPACT_NORMATIVE_KIND",
      );
      const key = recordKey({
        kind: contract.kind,
        id: contract.id,
        ...(contract.kind === "spec" || contract.kind === "plan"
          ? { revision: contract.revision }
          : {}),
      });
      compactAssert(
        !contractSources.has(key) && records.has(key),
        "INVALID_COMPACT_NORMATIVE_IDENTITY",
      );
      contractSources.set(key, contract);
    }
    const contractSource = (record: ContextRecordProjection) => {
      const reference = {
        kind: record.kind,
        id: record.id,
        ...(record.kind === "spec" || record.kind === "plan"
          ? { revision: Number(record.revision) }
          : {}),
      };
      const contract = contractSources.get(recordKey(reference));
      compactAssert(contract, "COMPACT_NORMATIVE_ADDRESS_REQUIRED");
      const source = pointer(contract.source);
      const address = result.sources[source[0]]!;
      compactAssert(
        source[1] === "#" &&
          !source[2] &&
          (address[3] ?? context.repositoryId) === context.repositoryId &&
          (address[4] ?? context.commit) === context.commit &&
          (address[0] === "work" ? `${workPath}/${address[1]}` : address[1]) ===
            gitRecordPath(context.projectId, context.workId, reference),
        "COMPACT_NORMATIVE_SOURCE_BINDING_MISMATCH",
      );
      return { contract, source };
    };
    const contractRef = (value: unknown, kind: "spec" | "plan"): [string, number] => {
      compactAssert(
        compactObject(value) &&
          compactText(value.id) &&
          Number.isSafeInteger(value.revision) &&
          Number(value.revision) > 0,
        "INVALID_COMPACT_CONTRACT_BINDING",
      );
      const revision = Number(value.revision);
      const key = recordKey({ kind, id: value.id, revision });
      compactAssert(
        records.has(key) && contractSources.has(key),
        "COMPACT_BOUND_CONTRACT_UNAVAILABLE",
      );
      return [value.id, revision];
    };
    let works = 0;
    for (const record of context.records) {
      if (!["work", "spec", "plan"].includes(record.kind)) continue;
      if (record.contextRole === "provenance") {
        compactAssert(
          !contractSources.has(
            recordKey({ kind: record.kind, id: record.id, revision: Number(record.revision) }),
          ),
          "COMPACT_PROVENANCE_CONTRACT_CONFLICT",
        );
        continue;
      }
      const { contract, source } = contractSource(record);
      const sourceIndex = source[0];
      if (record.kind === "work") {
        works++;
        compactAssert(
          record.id === context.workId &&
            contract.coverage === "complete" &&
            compactFieldsMatch(contract.fields, {
              purpose: "#/purpose",
              spec: "#/spec",
              plan: "#/plan",
            }) &&
            compactObject(record.purpose) &&
            Object.keys(record.purpose).length === 4 &&
            compactText(record.domainId) &&
            compactText(record.issueId),
          "INVALID_COMPACT_WORK_PURPOSE",
        );
        const purpose: CompactPurpose = [
          stringIndex(record.purpose.objective),
          indices(record.purpose.successCriteria),
          indices(record.purpose.inScope),
          indices(record.purpose.outOfScope),
        ];
        compactAssert(purpose[1].length > 0, "COMPACT_WORK_SUCCESS_CRITERIA_REQUIRED");
        result.normative.push([
          "work",
          record.id,
          sourceIndex,
          {
            purpose: "#/purpose",
            spec: "#/spec",
            plan: "#/plan",
            domainId: "#/domainId",
            issueId: "#/issueId",
          },
          "complete",
          purpose,
          { spec: contractRef(record.spec, "spec"), plan: contractRef(record.plan, "plan") },
          { domainId: record.domainId, issueId: record.issueId },
        ]);
      }
      if (record.kind === "spec") {
        compactAssert(
          contract.coverage === "complete" &&
            compactFieldsMatch(contract.fields, { outcome: "#/outcome", criteria: "#/criteria" }),
          "INVALID_COMPACT_SPEC_COVERAGE",
        );
        const criteria = indices(record.criteria);
        compactAssert(criteria.length > 0, "COMPACT_SPEC_CRITERIA_REQUIRED");
        result.normative.push([
          "spec",
          record.id,
          Number(record.revision),
          sourceIndex,
          { outcome: "#/outcome", criteria: "#/criteria" },
          "complete",
          [stringIndex(record.outcome), criteria],
        ]);
      }
      if (record.kind === "plan") {
        const clauses = record.clauses === undefined ? undefined : indices(record.clauses);
        compactAssert(
          (clauses === undefined
            ? contract.coverage === "legacy-objective-only"
            : contract.coverage === "complete" && clauses.length > 0) &&
            compactFieldsMatch(contract.fields, {
              objective: "#/objective",
              specId: "#/specId",
              specRevision: "#/specRevision",
              ...(clauses === undefined ? {} : { clauses: "#/clauses" }),
            }),
          "INVALID_COMPACT_PLAN_COVERAGE",
        );
        result.normative.push([
          "plan",
          record.id,
          Number(record.revision),
          sourceIndex,
          {
            objective: "#/objective",
            specId: "#/specId",
            specRevision: "#/specRevision",
            ...(clauses === undefined ? {} : { clauses: "#/clauses" }),
          },
          clauses === undefined ? "legacy-objective-only" : "complete",
          clauses === undefined
            ? [stringIndex(record.objective)]
            : [stringIndex(record.objective), clauses],
          contractRef({ id: record.specId, revision: record.specRevision }, "spec"),
        ]);
      }
    }
    compactAssert(works === 1, "COMPACT_WORK_REQUIRED");
    if (selection.taskId !== undefined) {
      const record = records.get(recordKey({ kind: "task", id: selection.taskId }));
      compactAssert(record?.kind === "task", "COMPACT_SELECTED_TASK_UNAVAILABLE");
      const { contract, source } = contractSource(record);
      const authored = record.nextAction !== undefined;
      compactAssert(
        contract.kind === "task" &&
          compactFieldsMatch(contract.fields, authored ? { nextAction: "#/nextAction" } : {}) &&
          contract.coverage === (authored ? "complete" : "unspecified"),
        "INVALID_COMPACT_NEXT_ACTION_COVERAGE",
      );
      const logical = record.logical as unknown as LogicalIdentity;
      const state = record.state as unknown as TaskOperationalState;
      assertLogicalIdentity(logical);
      validateTaskOperationalState(state);
      compactAssert(
        logical.taskId === record.id &&
          logical.workId === context.workId &&
          logical.projectId === context.projectId &&
          ["source-only", "native-effect"].includes(String(record.executionClass)),
        "INVALID_COMPACT_TASK_IDENTITY",
      );
      contractRef({ id: logical.specId, revision: logical.specRevision }, "spec");
      contractRef({ id: logical.planId, revision: logical.planRevision }, "plan");
      const work = records.get(recordKey({ kind: "work", id: context.workId }));
      const plan = records.get(
        recordKey({ kind: "plan", id: logical.planId, revision: logical.planRevision }),
      );
      compactAssert(
        work?.domainId === logical.domainId &&
          work.issueId === logical.issueId &&
          plan?.specId === logical.specId &&
          plan.specRevision === logical.specRevision,
        "COMPACT_TASK_SCOPE_OR_PLAN_SPEC_MISMATCH",
      );
      result.task = {
        id: record.id,
        source,
        logical: [
          logical.domainId,
          logical.projectId,
          logical.workId,
          logical.issueId,
          logical.specId,
          logical.specRevision,
          logical.planId,
          logical.planRevision,
          logical.taskId,
        ],
        state: [
          state.execution,
          state.verification,
          state.localAudit,
          state.sharedAudit,
          state.formalCompletion,
        ],
        executionClass: record.executionClass as "source-only" | "native-effect",
        effectIds: [...texts(record.effectIds)],
        pendingEffectIds: [...texts(record.pendingEffectIds ?? [])],
        nextActionCoverage: authored ? "authored" : "unspecified",
        ...(authored
          ? {
              nextAction: {
                value: stringIndex(record.nextAction),
                source: [source[0], "#/nextAction"] as CompactSourcePointer,
              },
            }
          : {}),
      };
    }
    const allDecisionIds = context.records
      .filter((record) => record.kind === "decision")
      .map((record) => record.id);
    const requestedDecisionIds =
      selection.decisionIds === undefined ? allDecisionIds : texts(selection.decisionIds);
    compactAssert(
      requestedDecisionIds.length <= 1024 &&
        new Set(requestedDecisionIds).size === requestedDecisionIds.length,
      "INVALID_COMPACT_DECISION_SELECTION",
    );
    compactAssert(
      requestedDecisionIds.every((id) => allDecisionIds.includes(id)),
      "COMPACT_SELECTED_DECISION_UNAVAILABLE",
    );
    // Selection prioritizes originals; it never hides other reached governing/historical originals.
    const decisionIds = [
      ...requestedDecisionIds,
      ...allDecisionIds.filter((id) => !requestedDecisionIds.includes(id)),
    ];
    for (const id of decisionIds) {
      const record = records.get(recordKey({ kind: "decision", id }));
      compactAssert(
        record?.kind === "decision" && ["accepted", "superseded"].includes(String(record.status)),
        "COMPACT_SELECTED_DECISION_UNAVAILABLE",
      );
      const answerSource = pointer(record.answerSource);
      compactAssert(answerSource[1].startsWith("#/"), "COMPACT_ORIGINAL_ANSWER_FRAGMENT_REQUIRED");
      const selectionContextSource =
        record.selectionContextSource === undefined
          ? undefined
          : pointer(record.selectionContextSource);
      compactAssert(
        selectionContextSource === undefined || selectionContextSource[1].startsWith("#/"),
        "COMPACT_ORIGINAL_QUESTION_FRAGMENT_REQUIRED",
      );
      result.decisions.push(
        selectionContextSource === undefined
          ? [id, record.status as "accepted" | "superseded", answerSource]
          : [id, record.status as "accepted" | "superseded", answerSource, selectionContextSource],
      );
    }
    const binding = context.continuityBinding;
    compactAssert(
      binding?.schema === "common.git-work-context.continuity.v1" &&
        ["bound", "legacy-unbound"].includes(binding.coverage),
      "COMPACT_CONTINUITY_UNAVAILABLE",
    );
    const purposeSources = texts(binding.purposeSources).map(pointer);
    const effectInventorySources = texts(binding.effectInventorySources).map(pointer);
    compactAssert(
      binding.coverage === "bound"
        ? purposeSources.length > 0 && effectInventorySources.length > 0
        : purposeSources.length === 0 && effectInventorySources.length === 0,
      "INVALID_COMPACT_CONTINUITY_BINDING",
    );
    compactAssert(
      binding.coverage !== "bound" || !result.task || result.task.nextActionCoverage === "authored",
      "COMPACT_SELECTED_TASK_NEXT_ACTION_REQUIRED",
    );
    result.continuityBinding = {
      schema: binding.schema,
      coverage: binding.coverage,
      purposeSources,
      effectInventorySources,
    };
    compactAssert(result.historicalEffects, "INVALID_COMPACT_HISTORICAL_EFFECTS");
    result.status = "source-qualified";
    validateAndDecodeCompactContext(result);
    return result;
  } catch (error) {
    result.status = "held";
    result.holdReasons = [
      ...new Set([
        ...result.holdReasons,
        error instanceof CompactContextError ? error.message : "COMPACT_CONTEXT_UNAVAILABLE",
      ]),
    ];
    result.strings = [];
    result.sources = [];
    result.normative = [];
    result.decisions = [];
    delete result.task;
    result.continuityBinding = {
      schema: "common.git-work-context.continuity.v1",
      coverage: "unavailable",
      purposeSources: [],
      effectInventorySources: [],
    };
    return result;
  }
}

/** Reject unknown/missing fields rather than silently dropping required compact semantics. */
function compactShape(value: unknown, required: string[], optional: string[] = []): void {
  compactAssert(
    compactObject(value) &&
      required.every((key) => Object.hasOwn(value, key)) &&
      Object.keys(value).every((key) => required.includes(key) || optional.includes(key)),
    "INVALID_COMPACT_SCHEMA",
  );
}
function compactTuple(value: unknown, minimum: number, maximum = minimum): void {
  compactAssert(
    Array.isArray(value) && value.length >= minimum && value.length <= maximum,
    "INVALID_COMPACT_TUPLE",
  );
}
function compactTexts(value: unknown): string[] {
  compactAssert(
    Array.isArray(value) &&
      value.length <= 10_000 &&
      value.every(compactText) &&
      new Set(value).size === value.length,
    "INVALID_COMPACT_TEXT_ARRAY",
  );
  return value;
}

/** Validate and copy only owned historical fields, including on a held native view. */
function copyCompactHistoricalEffects(value: unknown): GitWorkContext["historicalEffects"] {
  if (value === undefined) return undefined;
  const effects = value as NonNullable<GitWorkContext["historicalEffects"]>;
  compactShape(effects, [
    "completedCount",
    "uncertainCount",
    "relevantCompletedIds",
    "relevantUncertainIds",
    "projectedFenceRecords",
  ]);
  compactAssert(
    effects &&
      [effects.completedCount, effects.uncertainCount, effects.projectedFenceRecords].every(
        (count) => Number.isSafeInteger(count) && count >= 0,
      ),
    "INVALID_COMPACT_HISTORICAL_EFFECTS",
  );
  const completed = compactTexts(effects.relevantCompletedIds),
    uncertain = compactTexts(effects.relevantUncertainIds);
  compactAssert(
    completed.length <= effects.completedCount &&
      uncertain.length <= effects.uncertainCount &&
      !completed.some((id) => uncertain.includes(id)),
    "INVALID_COMPACT_RELEVANT_EFFECTS",
  );
  return {
    completedCount: effects.completedCount,
    uncertainCount: effects.uncertainCount,
    relevantCompletedIds: [...completed],
    relevantUncertainIds: [...uncertain],
    projectedFenceRecords: effects.projectedFenceRecords,
  };
}

/** One finite structural boundary shared by construction and public decoding.
 * It checks the carried contracts, not Git content or native producer authenticity. */
function validateAndDecodeCompactContext(
  context: CompactGitWorkContext,
): DecodedCompactGitWorkContext {
  compactShape(
    context,
    [
      "schema",
      "sourcePathBase",
      "layout",
      "status",
      "repositoryId",
      "commit",
      "projectId",
      "workId",
      "latestRelevant",
      "strings",
      "sources",
      "normative",
      "decisions",
      "continuityBinding",
      "historicalEffects",
      "holdReasons",
      "authority",
      "nativePermissionGranted",
      "semanticAcceptanceEstablished",
    ],
    ["task"],
  );
  compactAssert(
    context.schema === "common.git-work-context.compact.v1" &&
      context.status === "source-qualified" &&
      context.latestRelevant === "current" &&
      context.authority === "none" &&
      context.nativePermissionGranted === false &&
      context.semanticAcceptanceEstablished === false &&
      Array.isArray(context.holdReasons) &&
      context.holdReasons.length === 0 &&
      JSON.stringify(context.layout) === JSON.stringify(compactLayout),
    "COMPACT_CONTEXT_NOT_QUALIFIED",
  );
  compactAssert(compactText(context.repositoryId), "COMPACT_REPOSITORY_REQUIRED");
  assertCommit(context.commit);
  const workPath = gitWorkPath(context.projectId, context.workId);
  compactAssert(context.sourcePathBase === workPath, "COMPACT_SOURCE_BASE_MISMATCH");
  compactTexts(context.strings);
  const string = (index: number) => {
    compactAssert(
      Number.isSafeInteger(index) && index >= 0 && compactText(context.strings[index]),
      "INVALID_COMPACT_STRING_INDEX",
    );
    return context.strings[index]!;
  };
  const strings = (indices: number[], nonempty = false): string[] => {
    compactAssert(
      Array.isArray(indices) &&
        indices.length <= 10_000 &&
        (!nonempty || indices.length > 0) &&
        new Set(indices).size === indices.length,
      "INVALID_COMPACT_NORMATIVE_ARRAY",
    );
    return indices.map(string);
  };
  compactTuple(context.sources, 1, 8192);
  const sourceKeys = new Set<string>();
  const sources = context.sources.map((row) => {
    compactTuple(row, 3, 5);
    const [pathBase, path, blobOid, repositoryId, commit] = row;
    compactAssert(
      (pathBase === "work" || pathBase === "repository") &&
        compactText(path) &&
        (repositoryId == null || compactText(repositoryId)) &&
        (commit == null || typeof commit === "string") &&
        typeof blobOid === "string" &&
        /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(blobOid),
      "INVALID_COMPACT_SOURCE_ADDRESS",
    );
    assertGitPath(path);
    const fullPath = pathBase === "work" ? `${workPath}/${path}` : path;
    assertGitPath(fullPath);
    const exact = {
      repositoryId: repositoryId ?? context.repositoryId,
      commit: commit ?? context.commit,
      path: fullPath,
      blobOid,
    };
    assertCommit(exact.commit);
    const key = JSON.stringify(exact);
    compactAssert(!sourceKeys.has(key), "DUPLICATE_COMPACT_SOURCE_ADDRESS");
    sourceKeys.add(key);
    return exact;
  });
  const source = (index: number) => {
    compactAssert(
      Number.isSafeInteger(index) && index >= 0 && sources[index],
      "INVALID_COMPACT_SOURCE_INDEX",
    );
    return sources[index]!;
  };
  const pointer = (value: CompactSourcePointer): CompactSourcePointer => {
    compactTuple(value, 2, 3);
    const address = source(value[0]);
    compactAssert(
      typeof value[1] === "string" &&
        value[1].length <= 12_000 &&
        (value[1] === "#" || value[1].startsWith("#/")) &&
        !/~(?![01])/.test(value[1]),
      "INVALID_COMPACT_SOURCE_FRAGMENT",
    );
    if (value.length === 3) {
      const native = value[2];
      compactShape(native, ["ownerId", "locator", "receiptKind"], ["sha256"]);
      compactAssert(
        native &&
          compactText(native.ownerId) &&
          compactText(native.locator) &&
          ["snapshot", "git"].includes(native.receiptKind) &&
          (native.sha256 === undefined ||
            (typeof native.sha256 === "string" && /^[a-f0-9]{64}$/.test(native.sha256))) &&
          (native.receiptKind !== "snapshot" ||
            (address.repositoryId === context.repositoryId && address.commit === context.commit)),
        "INVALID_COMPACT_NATIVE_DECLARATION",
      );
    }
    return structuredClone(value);
  };
  const canonicalSource = (index: number, reference: RecordReference) => {
    const address = source(index);
    compactAssert(
      address.repositoryId === context.repositoryId &&
        address.commit === context.commit &&
        address.path === gitRecordPath(context.projectId, context.workId, reference),
      "COMPACT_NORMATIVE_SOURCE_BINDING_MISMATCH",
    );
  };
  compactTuple(context.normative, 1, 2048);
  const records = new Map<string, ContextRecordProjection>();
  let work: ContextRecordProjection | undefined;
  const normative = context.normative.map((row): ContextRecordProjection => {
    compactTuple(row, 7, 8);
    const kind = row[0];
    compactAssert(
      ["work", "spec", "plan"].includes(kind) && compactText(row[1]),
      "INVALID_COMPACT_NORMATIVE_IDENTITY",
    );
    const reference: RecordReference = {
      kind,
      id: row[1],
      ...(kind === "work" ? {} : { revision: row[2] }),
    };
    const key = recordKey(reference);
    compactAssert(!records.has(key), "DUPLICATE_COMPACT_RECORD");
    canonicalSource(kind === "work" ? row[2] : row[3], reference);
    let record: ContextRecordProjection;
    if (row[0] === "work") {
      compactTuple(row, 8);
      compactAssert(
        !work &&
          row[1] === context.workId &&
          row[4] === "complete" &&
          compactFieldsMatch(row[3], {
            purpose: "#/purpose",
            spec: "#/spec",
            plan: "#/plan",
            domainId: "#/domainId",
            issueId: "#/issueId",
          }),
        "INVALID_COMPACT_WORK_CONTRACT",
      );
      compactTuple(row[5], 4);
      compactShape(row[6], ["spec", "plan"]);
      compactShape(row[7], ["domainId", "issueId"]);
      compactAssert(
        compactText(row[7].domainId) && compactText(row[7].issueId),
        "INVALID_COMPACT_WORK_SCOPE",
      );
      const [objective, successCriteria, inScope, outOfScope] = row[5];
      const spec = row[6].spec,
        plan = row[6].plan;
      compactTuple(spec, 2);
      compactTuple(plan, 2);
      recordKey({ kind: "spec", id: spec[0], revision: spec[1] });
      recordKey({ kind: "plan", id: plan[0], revision: plan[1] });
      record = {
        kind: "work",
        id: row[1],
        domainId: row[7].domainId,
        issueId: row[7].issueId,
        spec: { id: spec[0], revision: spec[1] },
        plan: { id: plan[0], revision: plan[1] },
        purpose: {
          objective: string(objective),
          successCriteria: strings(successCriteria, true),
          inScope: strings(inScope),
          outOfScope: strings(outOfScope),
        },
      };
      work = record;
    } else if (row[0] === "spec") {
      compactTuple(row, 7);
      compactTuple(row[6], 2);
      compactAssert(
        row[5] === "complete" &&
          compactFieldsMatch(row[4], { outcome: "#/outcome", criteria: "#/criteria" }),
        "INVALID_COMPACT_SPEC_COVERAGE",
      );
      record = {
        kind: "spec",
        id: row[1],
        revision: row[2],
        outcome: string(row[6][0]),
        criteria: strings(row[6][1], true),
      };
    } else {
      compactTuple(row, 8);
      compactTuple(row[6], 1, 2);
      compactTuple(row[7], 2);
      const clauses = row[6].length === 1 ? undefined : strings(row[6][1]!, true);
      compactAssert(
        (clauses === undefined ? row[5] === "legacy-objective-only" : row[5] === "complete") &&
          compactFieldsMatch(row[4], {
            objective: "#/objective",
            specId: "#/specId",
            specRevision: "#/specRevision",
            ...(clauses === undefined ? {} : { clauses: "#/clauses" }),
          }),
        "INVALID_COMPACT_PLAN_COVERAGE",
      );
      recordKey({ kind: "spec", id: row[7][0], revision: row[7][1] });
      record = {
        kind: "plan",
        id: row[1],
        revision: row[2],
        objective: string(row[6][0]),
        specId: row[7][0],
        specRevision: row[7][1],
        ...(clauses === undefined ? {} : { clauses }),
      };
    }
    records.set(key, record);
    return record;
  });
  compactAssert(work, "COMPACT_WORK_REQUIRED");
  const contractRef = (kind: "spec" | "plan", id: unknown, revision: unknown) => {
    compactAssert(
      compactText(id) && Number.isSafeInteger(revision) && Number(revision) > 0,
      "INVALID_COMPACT_CONTRACT_BINDING",
    );
    const record = records.get(recordKey({ kind, id, revision: Number(revision) }));
    compactAssert(record, "COMPACT_BOUND_CONTRACT_UNAVAILABLE");
    return record;
  };
  compactAssert(
    compactObject(work.spec) && compactObject(work.plan),
    "INVALID_COMPACT_CONTRACT_BINDING",
  );
  const activeSpec = contractRef("spec", work.spec.id, work.spec.revision);
  const activePlan = contractRef("plan", work.plan.id, work.plan.revision);
  compactAssert(
    activePlan.specId === activeSpec.id && activePlan.specRevision === activeSpec.revision,
    "COMPACT_ACTIVE_PLAN_SPEC_MISMATCH",
  );
  for (const record of normative)
    if (record.kind === "plan") contractRef("spec", record.specId, record.specRevision);

  compactTuple(context.decisions, 0, 1024);
  const decisionIds = new Set<string>();
  const decisions = context.decisions.map((row) => {
    compactTuple(row, 3, 4);
    const [id, status, answerSource, selectionContextSource] = row;
    compactAssert(
      compactText(id) && !decisionIds.has(id) && ["accepted", "superseded"].includes(status),
      "INVALID_COMPACT_DECISION",
    );
    decisionIds.add(id);
    const answer = pointer(answerSource);
    const question = row.length === 4 ? pointer(selectionContextSource!) : undefined;
    compactAssert(
      answer[1].startsWith("#/") && (!question || question[1].startsWith("#/")),
      "COMPACT_ORIGINAL_FRAGMENT_REQUIRED",
    );
    return {
      id,
      status,
      answerSource: answer,
      ...(question ? { selectionContextSource: question } : {}),
    };
  });
  const binding = context.continuityBinding;
  compactShape(binding, ["schema", "coverage", "purposeSources", "effectInventorySources"]);
  compactAssert(
    binding.schema === "common.git-work-context.continuity.v1" &&
      ["bound", "legacy-unbound"].includes(binding.coverage),
    "COMPACT_CONTINUITY_UNAVAILABLE",
  );
  const bound = binding.coverage === "bound";
  compactTuple(binding.purposeSources, bound ? 1 : 0, bound ? 1024 : 0);
  compactTuple(binding.effectInventorySources, bound ? 1 : 0, bound ? 1024 : 0);
  for (const value of binding.purposeSources)
    compactAssert(pointer(value)[1].startsWith("#/"), "INVALID_COMPACT_PURPOSE_POINTER");
  for (const value of binding.effectInventorySources) pointer(value);
  const effects = copyCompactHistoricalEffects(context.historicalEffects);
  compactAssert(effects, "INVALID_COMPACT_HISTORICAL_EFFECTS");
  const completed = effects.relevantCompletedIds,
    uncertain = effects.relevantUncertainIds;
  let task: DecodedCompactGitWorkContext["task"];
  if (Object.hasOwn(context, "task")) {
    const compact = context.task;
    compactShape(
      compact,
      [
        "id",
        "source",
        "logical",
        "state",
        "executionClass",
        "effectIds",
        "pendingEffectIds",
        "nextActionCoverage",
      ],
      ["nextAction"],
    );
    compactAssert(compact, "COMPACT_SELECTED_TASK_UNAVAILABLE");
    compactTuple(compact.logical, 9);
    compactTuple(compact.state, 5);
    const [
      domainId,
      projectId,
      workId,
      issueId,
      specId,
      specRevision,
      planId,
      planRevision,
      taskId,
    ] = compact.logical;
    const [execution, verification, localAudit, sharedAudit, formalCompletion] = compact.state;
    const logical = {
      domainId,
      projectId,
      workId,
      issueId,
      specId,
      specRevision,
      planId,
      planRevision,
      taskId,
    };
    const state = { execution, verification, localAudit, sharedAudit, formalCompletion };
    assertLogicalIdentity(logical);
    validateTaskOperationalState(state);
    compactAssert(
      compact.id === taskId &&
        domainId === work.domainId &&
        issueId === work.issueId &&
        projectId === context.projectId &&
        workId === context.workId &&
        ["source-only", "native-effect"].includes(compact.executionClass),
      "INVALID_COMPACT_TASK_IDENTITY",
    );
    contractRef("spec", specId, specRevision);
    const plan = contractRef("plan", planId, planRevision);
    compactAssert(
      plan.specId === specId && plan.specRevision === specRevision,
      "COMPACT_TASK_PLAN_SPEC_MISMATCH",
    );
    const taskSource = pointer(compact.source);
    compactAssert(taskSource.length === 2 && taskSource[1] === "#", "INVALID_COMPACT_TASK_SOURCE");
    canonicalSource(taskSource[0], { kind: "task", id: taskId });
    const effectIds = compactTexts(compact.effectIds),
      pendingEffectIds = compactTexts(compact.pendingEffectIds);
    compactAssert(
      compact.executionClass !== "source-only" || (!effectIds.length && !pendingEffectIds.length),
      "SOURCE_ONLY_TASK_CANNOT_CLAIM_NATIVE_EFFECT",
    );
    compactAssert(
      ["authored", "unspecified"].includes(compact.nextActionCoverage) &&
        (compact.nextActionCoverage === "authored"
          ? Object.hasOwn(compact, "nextAction")
          : !Object.hasOwn(compact, "nextAction")) &&
        (!bound || compact.nextActionCoverage === "authored"),
      "INVALID_COMPACT_NEXT_ACTION_COVERAGE",
    );
    let nextAction: NonNullable<DecodedCompactGitWorkContext["task"]>["nextAction"];
    if (compact.nextActionCoverage === "authored") {
      compactShape(compact.nextAction, ["value", "source"]);
      compactAssert(compact.nextAction, "COMPACT_SELECTED_TASK_NEXT_ACTION_REQUIRED");
      const nextSource = pointer(compact.nextAction.source);
      compactAssert(
        nextSource.length === 2 &&
          nextSource[0] === taskSource[0] &&
          nextSource[1] === "#/nextAction",
        "INVALID_COMPACT_NEXT_ACTION_SOURCE",
      );
      nextAction = { value: string(compact.nextAction.value), source: nextSource };
    }
    if (["planned", "running"].includes(execution))
      compactAssert(
        !completed.includes(taskId) &&
          !uncertain.includes(taskId) &&
          !pendingEffectIds.some((id) => completed.includes(id) || uncertain.includes(id)),
        "COMPACT_CONSUMED_EFFECT_REPLAY",
      );
    task = {
      id: compact.id,
      source: taskSource,
      logical,
      state,
      executionClass: compact.executionClass,
      effectIds: [...effectIds],
      pendingEffectIds: [...pendingEffectIds],
      nextActionCoverage: compact.nextActionCoverage,
      ...(nextAction ? { nextAction } : {}),
    };
  }
  return { normative, sources, decisions, ...(task ? { task } : {}) };
}

const packedFragments = [
  "#",
  "#/content/text",
  "#/content/question",
  "#/content",
  "#/content/purpose",
  "#/content/effectCustody",
  "#/nextAction",
];
const packedStates = [
  "planned",
  "pending",
  "intent-durable",
  "complete",
  "passed",
  "result-durable",
  "delivery-confirmed",
  "audit-synchronization-pending",
  "uncertainty-durable",
  "running",
];
// Header metadata stays literal so owners can retain an immutable source pointer
// even when a later native wrapper has to hold delivery.
const metadataKeys = [
  "sources",
  "normative",
  "decisions",
  "continuityBinding",
  "historicalEffects",
  "task",
];
const metadataMap = (value: JsonValue, change: (text: string) => string): JsonValue =>
  typeof value === "string"
    ? change(value)
    : Array.isArray(value)
      ? value.map((item) => metadataMap(item, change))
      : value && typeof value === "object"
        ? Object.fromEntries(
            Object.entries(value).map(([key, item]) => [key, metadataMap(item, change)]),
          )
        : value;

/** One opt-in metadata representation; normative text bytes are never rewritten. */
function packCompactMetadata(context: CompactGitWorkContext): MetadataPackedGitWorkContext {
  const pointer = (p: CompactSourcePointer): JsonValue[] => {
    const index = packedFragments.indexOf(p[1]);
    return [p[0], index < 0 ? p[1] : index, ...(p[2] ? [p[2]] : [])];
  };
  const contract = (pair: [string, number], kind: string): number => {
    const index = context.normative.findIndex(
      (row) => row[0] === kind && row[1] === pair[0] && row[2] === pair[1],
    );
    compactAssert(index >= 0, "COMPACT_BOUND_CONTRACT_UNAVAILABLE");
    return index;
  };
  const normative = context.normative.map((original): JsonValue[] => {
    const row = structuredClone(original) as JsonValue[];
    if (original[0] === "work") {
      row[1] = null;
      row[3] = 0;
      row[4] = 0;
      row[6] = [contract(original[6].spec, "spec"), contract(original[6].plan, "plan")];
    } else {
      row[4] = original[0] === "spec" ? 1 : original[5] === "complete" ? 2 : 3;
      row[5] = original[5] === "complete" ? 0 : 1;
      if (original[0] === "plan") row[7] = contract(original[7], "spec");
    }
    return row;
  });
  const task = context.task;
  const effects = context.historicalEffects;
  compactAssert(effects, "INVALID_COMPACT_HISTORICAL_EFFECTS");
  const packed: MetadataPackedGitWorkContext = {
    schema: context.schema,
    encoding: {
      version: "metadata.v1",
      prefixes: [],
      decoder: "@palantirkc/work-continuity/git-work#decodeCompactGitWorkContext",
    },
    metadata: [
      context.repositoryId,
      context.commit,
      context.projectId,
      context.workId,
      context.latestRelevant === "current" ? 0 : context.latestRelevant,
    ],
    status: context.status,
    strings: [...context.strings],
    sources: context.sources.map(([base, path, oid, ...external]) => [
      base === "work" ? 0 : 1,
      path,
      Buffer.from(oid, "hex").toString("base64url"),
      ...external.map((value) => value ?? null),
    ]),
    normative,
    decisions: context.decisions.map(([id, status, answer, question]) => [
      id,
      status === "accepted" ? 0 : 1,
      pointer(answer),
      ...(question ? [pointer(question)] : []),
    ]),
    continuityBinding: [
      context.continuityBinding.coverage === "bound" ? 0 : 1,
      context.continuityBinding.purposeSources.map(pointer),
      context.continuityBinding.effectInventorySources.map(pointer),
    ],
    historicalEffects: [
      effects.completedCount,
      effects.uncertainCount,
      effects.relevantCompletedIds,
      effects.relevantUncertainIds,
      effects.projectedFenceRecords,
    ],
    ...(task
      ? {
          task: [
            task.id,
            pointer(task.source),
            [
              contract([task.logical[4], task.logical[5]], "spec"),
              contract([task.logical[6], task.logical[7]], "plan"),
            ],
            task.state.map((state) => {
              const index = packedStates.indexOf(state);
              return index < 0 ? state : index;
            }),
            task.executionClass === "source-only" ? 0 : 1,
            task.effectIds,
            task.pendingEffectIds,
            task.nextActionCoverage === "authored" ? 0 : 1,
            ...(task.nextAction ? [[task.nextAction.value, pointer(task.nextAction.source)]] : []),
          ],
        }
      : {}),
    holdReasons: [],
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
  const texts: string[] = [];
  for (const key of metadataKeys)
    if (Object.hasOwn(packed, key))
      metadataMap((packed as unknown as Record<string, JsonValue>)[key]!, (text) => {
        texts.push(text);
        return text;
      });
  const candidates = new Set<string>();
  // Bound optimization effort independently of source size. Sampling only picks
  // optional prefixes; every metadata value is still emitted and decoded exactly.
  const sample = texts.slice(0, 512);
  for (const text of sample)
    for (let i = 6; i <= Math.min(text.length, 160); i++)
      if (candidates.size < 2048 && /[-/:._@]$/.test(text.slice(0, i)))
        candidates.add(text.slice(0, i));
  const prefixes: string[] = [];
  for (let i = 0; i < 4; i++) {
    let best = "",
      saving = 12;
    for (const prefix of candidates) {
      const count = sample.filter(
        (text) => !prefixes.some((chosen) => text.startsWith(chosen)) && text.startsWith(prefix),
      ).length;
      const benefit = count * (prefix.length - 3) - JSON.stringify(prefix).length - 1;
      if (benefit > saving) {
        best = prefix;
        saving = benefit;
      }
    }
    if (!best) break;
    prefixes.push(best);
  }
  packed.encoding.prefixes = prefixes;
  for (const key of metadataKeys)
    if (Object.hasOwn(packed, key))
      (packed as unknown as Record<string, JsonValue>)[key] = metadataMap(
        (packed as unknown as Record<string, JsonValue>)[key]!,
        (text) => {
          if (text.startsWith("~")) return "~" + text;
          const index = prefixes.findIndex((prefix) => text.startsWith(prefix));
          return index < 0 ? text : "~" + index + ":" + text.slice(prefixes[index]!.length);
        },
      );
  // Representation must preserve exactly the already-validated compact meaning.
  if (JSON.stringify(packed).length <= 24000)
    compactAssert(
      JSON.stringify(unpackCompactMetadata(packed)) === JSON.stringify(context),
      "COMPACT_METADATA_ROUNDTRIP_MISMATCH",
    );
  return packed;
}

function unpackCompactMetadata(value: MetadataPackedGitWorkContext): CompactGitWorkContext {
  compactAssert(JSON.stringify(value).length <= 24000, "REQUIRED_COMPACT_INPUT_LIMIT");
  compactShape(
    value,
    [
      "schema",
      "encoding",
      "metadata",
      "status",
      "strings",
      "sources",
      "normative",
      "decisions",
      "continuityBinding",
      "historicalEffects",
      "holdReasons",
      "authority",
      "nativePermissionGranted",
      "semanticAcceptanceEstablished",
    ],
    ["task"],
  );
  compactShape(value.encoding, ["version", "prefixes", "decoder"]);
  compactAssert(
    value.encoding.version === "metadata.v1" &&
      value.encoding.decoder === "@palantirkc/work-continuity/git-work#decodeCompactGitWorkContext",
    "INVALID_COMPACT_ENCODING",
  );
  const prefixes = value.encoding.prefixes;
  compactAssert(
    Array.isArray(prefixes) &&
      prefixes.length <= 4 &&
      new Set(prefixes).size === prefixes.length &&
      prefixes.every((p) => typeof p === "string" && p.length >= 6 && p.length <= 160),
    "INVALID_COMPACT_PREFIXES",
  );
  const p = structuredClone(value) as unknown as Record<string, JsonValue>;
  for (const key of metadataKeys)
    if (Object.hasOwn(p, key))
      p[key] = metadataMap(p[key]!, (text) => {
        if (text.startsWith("~~")) return text.slice(1);
        if (!text.startsWith("~")) return text;
        const match = /^~([0-3]):([\s\S]*)$/.exec(text);
        compactAssert(
          match && Number(match[1]) < prefixes.length,
          "INVALID_COMPACT_PREFIX_REFERENCE",
        );
        return prefixes[Number(match[1])]! + match[2]!;
      });
  const tuple = (v: JsonValue | undefined, min: number, max = min): JsonValue[] => {
    compactTuple(v, min, max);
    return v as JsonValue[];
  };
  const text = (v: JsonValue | undefined): string => {
    compactAssert(compactText(v), "INVALID_COMPACT_METADATA_TEXT");
    return v;
  };
  const rows = (v: JsonValue | undefined): JsonValue[][] => {
    compactAssert(Array.isArray(v), "INVALID_COMPACT_METADATA_ROWS");
    return v.map((r) => tuple(r, 0, 8192));
  };
  const enumeration = (v: JsonValue | undefined, choices: string[]): string => {
    if (typeof v === "string") return v;
    compactAssert(
      Number.isSafeInteger(v) && Number(v) >= 0 && Number(v) < choices.length,
      "INVALID_COMPACT_METADATA_ENUM",
    );
    return choices[Number(v)]!;
  };
  const meta = tuple(p.metadata, 5),
    repositoryId = text(meta[0]),
    commit = text(meta[1]),
    projectId = text(meta[2]),
    workId = text(meta[3]);
  const pointer = (v: JsonValue): CompactSourcePointer => {
    const row = tuple(v, 2, 3);
    return [row[0], enumeration(row[1], packedFragments), ...row.slice(2)] as CompactSourcePointer;
  };
  const normative = rows(p.normative).map((r) => [...r]);
  const pair = (index: JsonValue | undefined, kind: string): [string, number] => {
    compactAssert(
      Number.isSafeInteger(index) && Number(index) >= 0,
      "INVALID_COMPACT_CONTRACT_INDEX",
    );
    const r = normative[Number(index)];
    compactAssert(
      r && r[0] === kind && typeof r[1] === "string" && Number.isSafeInteger(r[2]),
      "INVALID_COMPACT_CONTRACT_INDEX",
    );
    return [r[1], Number(r[2])];
  };
  for (const r of normative) {
    if (r[0] === "work") {
      compactTuple(r, 8);
      compactAssert(r[1] === null && r[3] === 0 && r[4] === 0, "INVALID_COMPACT_PACKED_WORK");
      r[1] = workId;
      r[3] = {
        purpose: "#/purpose",
        spec: "#/spec",
        plan: "#/plan",
        domainId: "#/domainId",
        issueId: "#/issueId",
      };
      r[4] = "complete";
      const refs = tuple(r[6], 2);
      r[6] = { spec: pair(refs[0], "spec"), plan: pair(refs[1], "plan") };
    } else if (r[0] === "spec") {
      compactTuple(r, 7);
      compactAssert(r[4] === 1 && r[5] === 0, "INVALID_COMPACT_PACKED_SPEC");
      r[4] = { outcome: "#/outcome", criteria: "#/criteria" };
      r[5] = "complete";
    } else {
      compactTuple(r, 8);
      compactAssert(
        r[0] === "plan" && (r[4] === 2 || r[4] === 3) && r[5] === (r[4] === 2 ? 0 : 1),
        "INVALID_COMPACT_PACKED_PLAN",
      );
      const clauses = r[4] === 2;
      r[4] = {
        objective: "#/objective",
        specId: "#/specId",
        specRevision: "#/specRevision",
        ...(clauses ? { clauses: "#/clauses" } : {}),
      };
      r[5] = clauses ? "complete" : "legacy-objective-only";
      r[7] = pair(r[7], "spec");
    }
  }
  const work = normative.find((r) => r[0] === "work");
  compactAssert(work && compactObject(work[7]), "COMPACT_WORK_REQUIRED");
  const sources = rows(p.sources).map((r) => {
    compactTuple(r, 3, 5);
    const encoded = text(r[2]);
    compactAssert(/^[A-Za-z0-9_-]+$/.test(encoded), "INVALID_COMPACT_PACKED_BLOB");
    const bytes = Buffer.from(encoded, "base64url");
    compactAssert(
      [20, 32].includes(bytes.length) && bytes.toString("base64url") === encoded,
      "INVALID_COMPACT_PACKED_BLOB",
    );
    compactAssert(r[0] === 0 || r[0] === 1, "INVALID_COMPACT_PACKED_SOURCE_BASE");
    return [
      r[0] === 0 ? "work" : "repository",
      r[1],
      bytes.toString("hex"),
      ...r.slice(3),
    ] as CompactGitSourceAddress;
  });
  const binding = tuple(p.continuityBinding, 3),
    effects = tuple(p.historicalEffects, 5);
  const task = p.task === undefined ? undefined : tuple(p.task, 8, 9);
  const taskContracts = task ? tuple(task[2], 2) : undefined;
  const spec = taskContracts ? pair(taskContracts[0], "spec") : undefined,
    plan = taskContracts ? pair(taskContracts[1], "plan") : undefined;
  const next = task?.[8] === undefined ? undefined : tuple(task[8], 2);
  return {
    schema: value.schema,
    sourcePathBase: gitWorkPath(projectId, workId),
    layout: { ...compactLayout },
    status: value.status,
    repositoryId,
    commit,
    projectId,
    workId,
    latestRelevant: enumeration(meta[4], ["current"]) as ContextRequest["latestRelevant"],
    strings: value.strings,
    sources,
    normative: normative as CompactGitWorkContext["normative"],
    decisions: rows(p.decisions).map((r) => {
      compactTuple(r, 3, 4);
      return [
        r[0],
        enumeration(r[1], ["accepted", "superseded"]),
        pointer(r[2]!),
        ...(r[3] === undefined ? [] : [pointer(r[3])]),
      ] as CompactGitWorkContext["decisions"][number];
    }),
    continuityBinding: {
      schema: "common.git-work-context.continuity.v1",
      coverage: enumeration(binding[0], [
        "bound",
        "legacy-unbound",
      ]) as GitWorkContinuityBinding["coverage"],
      purposeSources: (binding[1] as JsonValue[]).map(pointer),
      effectInventorySources: (binding[2] as JsonValue[]).map(pointer),
    },
    historicalEffects: {
      completedCount: effects[0],
      uncertainCount: effects[1],
      relevantCompletedIds: effects[2],
      relevantUncertainIds: effects[3],
      projectedFenceRecords: effects[4],
    } as CompactGitWorkContext["historicalEffects"],
    holdReasons: value.holdReasons,
    authority: value.authority,
    nativePermissionGranted: value.nativePermissionGranted,
    semanticAcceptanceEstablished: value.semanticAcceptanceEstablished,
    ...(task && spec && plan
      ? {
          task: {
            id: task[0],
            source: pointer(task[1]!),
            logical: [
              work[7].domainId,
              projectId,
              workId,
              work[7].issueId,
              ...spec,
              ...plan,
              task[0],
            ],
            state: (task[3] as JsonValue[]).map((v) => enumeration(v, packedStates)),
            executionClass: enumeration(task[4], ["source-only", "native-effect"]),
            effectIds: task[5],
            pendingEffectIds: task[6],
            nextActionCoverage: enumeration(task[7], ["authored", "unspecified"]),
            ...(next ? { nextAction: { value: next[0], source: pointer(next[1]!) } } : {}),
          } as CompactGitWorkContext["task"],
        }
      : {}),
  };
}

/** Decode only a structurally complete, non-authorizing qualified compact view. */
export function decodeCompactGitWorkContext(
  context: GitWorkCompactOutput,
): DecodedCompactGitWorkContext {
  try {
    return validateAndDecodeCompactContext(
      "encoding" in context ? unpackCompactMetadata(context) : context,
    );
  } catch (error) {
    throw error instanceof CompactContextError
      ? error
      : new CompactContextError("COMPACT_CONTEXT_UNAVAILABLE");
  }
}

/** Validates an owner observation. It never fetches, resets, merges or authenticates that owner. */
export function qualifyGitWorkStart(request: StartRequest): ValidationResult {
  const findings: string[] = [];
  try {
    assertCommit(request.selectedCommit);
    if (typeof request.repositoryId !== "string" || !request.repositoryId.trim())
      throw Error("START_REPOSITORY_REQUIRED");
    if (request.mode === "new") {
      const observation = request.remoteMain;
      if (
        !observation ||
        observation.repositoryId !== request.repositoryId ||
        observation.commit !== request.selectedCommit ||
        observation.synchronized !== true ||
        !observation.receipt ||
        observation.checkedForStartId !== request.startId ||
        !request.startId
      )
        throw Error("FRESH_MAIN_OBSERVATION_REQUIRED");
      assertCommit(observation.commit);
      const at = Date.parse(observation.observedAtUtc),
        now = Date.parse(request.nowUtc);
      if (
        !Number.isFinite(at) ||
        !Number.isFinite(now) ||
        at > now ||
        !Number.isSafeInteger(request.maxObservationAgeMs) ||
        request.maxObservationAgeMs < 0 ||
        now - at > request.maxObservationAgeMs
      )
        throw Error("STALE_MAIN_OBSERVATION");
    } else if (request.mode === "continue") {
      if (
        !request.previous ||
        !request.current ||
        !["domainId", "projectId", "workId"].every((key) => {
          const prior = request.previous[key as keyof typeof request.previous];
          return (
            typeof prior === "string" &&
            prior.length > 0 &&
            prior === request.current[key as keyof typeof request.current]
          );
        }) ||
        typeof request.branch !== "string" ||
        !request.branch.trim() ||
        /[\u0000-\u001f]/.test(request.branch)
      )
        throw Error("CONTINUING_WORK_IDENTITY_REQUIRED");
    } else throw Error("INVALID_WORK_START_MODE");
  } catch (error) {
    findings.push(error instanceof Error ? error.message : "INVALID_WORK_START");
  }
  return outcome(findings);
}

/** Finite snapshot loading is exposed for CLI/reporting without another writer or store. */
export { loadRecords };
