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
  CompactGitSourceAddress,
  CompactSourcePointer,
  CompactPurpose,
  LogicalIdentity,
  TaskOperationalState,
  DecodedCompactGitWorkContext,
  ContextRequest,
  GitWorkContext,
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

export async function readGitWorkContext(
  reader: GitWorkReader,
  request: ContextRequest,
): Promise<GitWorkContext> {
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
  for (const record of records.values()) {
    if (record.kind === "effect-fence") continue;
    const base = { kind: record.kind, id: record.id };
    switch (record.kind) {
      case "work":
        projected.push({
          ...base,
          purpose: json(record.purpose),
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
    const record = records.get(key)!;
    const base = { id: record.id, source: sourceId(source.reference)! };
    if (record.kind === "work")
      contracts.push({
        ...base,
        kind: "work",
        fields: { purpose: "#/purpose", spec: "#/spec", plan: "#/plan" },
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
  if (JSON.stringify(result).length > maxChars) {
    return {
      ...result,
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
      omittedRecords: records.size + omittedRecords,
    };
  }
  return result;
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
const compactLayout = {
  strings:
    "Zero-based string indices; preserve array order and empty arrays. fields are exact source JSON pointers.",
  sources:
    "[pathBase,path,blobOid,repositoryId?,commit?]; null/omitted repo+commit=context. work paths=sourcePathBase/path.",
  pointer:
    "[sources index,JSON pointer,native declaration?]; Git proof never authenticates native provenance.",
  normative:
    "work:[kind,id,source,fields,coverage,[objective,successCriteria,inScope,outOfScope],{spec:[id,revision],plan:[id,revision]}]; spec:[kind,id,revision,source,fields,coverage,[outcome,criteria]]; plan:[kind,id,revision,source,fields,coverage,[objective,clauses?],[specId,specRevision]]. Only normative text values index strings; IDs/revisions are literal.",
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
  const result: CompactGitWorkContext = {
    schema: "common.git-work-context.compact.v1",
    sourcePathBase: context.sourcePathBase,
    layout: { ...compactLayout },
    status: "held",
    repositoryId: context.repositoryId,
    commit: context.commit,
    projectId: context.projectId,
    workId: context.workId,
    latestRelevant: context.latestRelevant ?? "unknown",
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
    historicalEffects: structuredClone(context.historicalEffects),
    holdReasons: Array.isArray(context.holdReasons)
      ? context.holdReasons.map(compactSourceHold)
      : [],
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
  try {
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
    assertCommit(context.commit);
    compactAssert(compactText(context.repositoryId), "COMPACT_REPOSITORY_REQUIRED");
    const workPath = gitWorkPath(context.projectId, context.workId);
    compactAssert(context.sourcePathBase === workPath, "COMPACT_SOURCE_BASE_MISMATCH");
    compactAssert(JSON.stringify(context).length <= 24_000, "REQUIRED_COMPACT_INPUT_LIMIT");
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
    const texts = (value: unknown): string[] => {
      compactAssert(
        Array.isArray(value) && value.length <= 10_000 && value.every(compactText),
        "INVALID_COMPACT_TEXT_ARRAY",
      );
      return value;
    };
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
    const fieldsMatch = (actual: unknown, expected: Record<string, string>) =>
      compactObject(actual) &&
      Object.keys(actual).length === Object.keys(expected).length &&
      Object.entries(expected).every(([key, value]) => actual[key] === value);
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
      compactAssert(
        records.has(recordKey({ kind, id: value.id, revision })),
        "COMPACT_BOUND_CONTRACT_UNAVAILABLE",
      );
      return [value.id, revision];
    };
    let works = 0;
    for (const record of context.records) {
      if (!["work", "spec", "plan"].includes(record.kind)) continue;
      const { contract, source } = contractSource(record);
      const sourceIndex = source[0];
      if (record.kind === "work") {
        works++;
        compactAssert(
          record.id === context.workId &&
            contract.coverage === "complete" &&
            fieldsMatch(contract.fields, {
              purpose: "#/purpose",
              spec: "#/spec",
              plan: "#/plan",
            }) &&
            compactObject(record.purpose) &&
            Object.keys(record.purpose).length === 4,
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
          { purpose: "#/purpose", spec: "#/spec", plan: "#/plan" },
          "complete",
          purpose,
          { spec: contractRef(record.spec, "spec"), plan: contractRef(record.plan, "plan") },
        ]);
      }
      if (record.kind === "spec") {
        compactAssert(
          contract.coverage === "complete" &&
            fieldsMatch(contract.fields, { outcome: "#/outcome", criteria: "#/criteria" }),
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
            fieldsMatch(contract.fields, {
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
          fieldsMatch(contract.fields, authored ? { nextAction: "#/nextAction" } : {}) &&
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
    const effects = context.historicalEffects;
    compactAssert(
      effects &&
        [effects.completedCount, effects.uncertainCount, effects.projectedFenceRecords].every(
          (count) => Number.isSafeInteger(count) && count >= 0,
        ),
      "INVALID_COMPACT_HISTORICAL_EFFECTS",
    );
    compactAssert(
      texts(effects.relevantCompletedIds).length <= effects.completedCount &&
        texts(effects.relevantUncertainIds).length <= effects.uncertainCount,
      "INVALID_COMPACT_RELEVANT_EFFECTS",
    );
    result.status = "source-qualified";
    return result;
  } catch (error) {
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

/** Decode the published compact layout without reading sources or adding authority.
 * Native owners enforce their own envelope budget before delivering either representation. */
export function decodeCompactGitWorkContext(
  context: CompactGitWorkContext,
): DecodedCompactGitWorkContext {
  compactAssert(
    context.schema === "common.git-work-context.compact.v1" &&
      context.status === "source-qualified" &&
      context.latestRelevant === "current" &&
      context.authority === "none" &&
      context.nativePermissionGranted === false &&
      context.semanticAcceptanceEstablished === false &&
      context.holdReasons.length === 0 &&
      JSON.stringify(context.layout) === JSON.stringify(compactLayout),
    "COMPACT_CONTEXT_NOT_QUALIFIED",
  );
  compactAssert(
    context.sourcePathBase === gitWorkPath(context.projectId, context.workId),
    "COMPACT_SOURCE_BASE_MISMATCH",
  );
  const string = (index: number) => {
    compactAssert(
      Number.isSafeInteger(index) && index >= 0 && compactText(context.strings[index]),
      "INVALID_COMPACT_STRING_INDEX",
    );
    return context.strings[index]!;
  };
  const strings = (indices: number[]) => indices.map(string);
  const sources = context.sources.map(([pathBase, path, blobOid, repositoryId, commit]) => {
    compactAssert(
      pathBase === "work" || pathBase === "repository",
      "COMPACT_SOURCE_PATH_BASE_UNAVAILABLE",
    );
    const fullPath = pathBase === "work" ? `${context.sourcePathBase}/${path}` : path;
    assertGitPath(fullPath);
    assertCommit(commit ?? context.commit);
    compactAssert(/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(blobOid), "INVALID_COMPACT_BLOB_ID");
    return {
      repositoryId: repositoryId ?? context.repositoryId,
      commit: commit ?? context.commit,
      path: fullPath,
      blobOid,
    };
  });
  const source = (index: number) => {
    compactAssert(
      Number.isSafeInteger(index) && index >= 0 && sources[index],
      "INVALID_COMPACT_SOURCE_INDEX",
    );
  };
  const normative: ContextRecordProjection[] = context.normative.map((row) => {
    if (row[0] === "work") {
      source(row[2]);
      const [objective, successCriteria, inScope, outOfScope] = row[5];
      return {
        kind: "work",
        id: row[1],
        spec: { id: row[6].spec[0], revision: row[6].spec[1] },
        plan: { id: row[6].plan[0], revision: row[6].plan[1] },
        purpose: {
          objective: string(objective),
          successCriteria: strings(successCriteria),
          inScope: strings(inScope),
          outOfScope: strings(outOfScope),
        },
      };
    }
    source(row[3]);
    if (row[0] === "spec")
      return {
        kind: "spec",
        id: row[1],
        revision: row[2],
        outcome: string(row[6][0]),
        criteria: strings(row[6][1]),
      };
    compactAssert(row[0] === "plan", "INVALID_COMPACT_NORMATIVE_KIND");
    return {
      kind: "plan",
      id: row[1],
      revision: row[2],
      objective: string(row[6][0]),
      specId: row[7][0],
      specRevision: row[7][1],
      ...(row[6][1] === undefined ? {} : { clauses: strings(row[6][1]) }),
    };
  });
  const pointer = (pointer: CompactSourcePointer) => {
    source(pointer[0]);
    compactAssert(
      typeof pointer[1] === "string" &&
        (pointer[1] === "#" || pointer[1].startsWith("#/")) &&
        !/~(?![01])/.test(pointer[1]),
      "INVALID_COMPACT_SOURCE_FRAGMENT",
    );
    return structuredClone(pointer);
  };
  const decisions = context.decisions.map(([id, status, answerSource, selectionContextSource]) => ({
    id,
    status,
    answerSource: pointer(answerSource),
    ...(selectionContextSource === undefined
      ? {}
      : { selectionContextSource: pointer(selectionContextSource) }),
  }));
  let task: DecodedCompactGitWorkContext["task"];
  if (context.task) {
    const compact = context.task;
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
    task = {
      ...structuredClone(compact),
      source: pointer(compact.source),
      logical,
      state,
      ...(compact.nextAction === undefined
        ? { nextAction: undefined }
        : {
            nextAction: {
              value: string(compact.nextAction.value),
              source: pointer(compact.nextAction.source),
            },
          }),
    };
  }
  return { normative, sources, decisions, ...(task ? { task } : {}) };
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
