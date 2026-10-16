import { assertCommit, gitRecordPath } from "./source.ts";
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
        fields: { purpose: "#/purpose" },
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
        fields: { objective: "#/objective", ...(record.clauses ? { clauses: "#/clauses" } : {}) },
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
