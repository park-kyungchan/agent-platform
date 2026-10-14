import { assertCommit } from "./source.ts";
import {
  dependencyFindings,
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
      for (const source of await validateRecordSources(record, reader)) {
        sources.push(source);
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
    if (selected?.kind === "task")
      for (const dependency of selected.dependsOn)
        holdReasons.push(...(await dependencyFindings(selected, dependency, records, reader)));
  }
  if (request.latestRelevant !== "current")
    holdReasons.push(`REQUIRED_LATEST_CONTEXT_${request.latestRelevant.toUpperCase()}`);
  const fences = [...records.values()].filter((record) => record.kind === "effect-fence");
  const completed = new Set(fences.flatMap((record) => record.completedEffectIds)),
    uncertain = new Set(fences.flatMap((record) => record.uncertainEffectIds));
  const selected = request.taskId
    ? records.get(recordKey({ kind: "task", id: request.taskId }))
    : undefined;
  const relevantIds =
    selected?.kind === "task"
      ? [selected.id, ...selected.effectIds, ...(selected.pendingEffectIds ?? [])]
      : [];
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
          executionClass: record.executionClass,
          logical: json(record.logical),
          state: json(record.state),
          dependsOn: json(record.dependsOn),
          ...(record.artifact?.kind === "task"
            ? { acceptanceCriteria: record.artifact.payload.acceptanceCriteria }
            : {}),
          ...(record.pendingEffectIds?.length ? { pendingEffectIds: record.pendingEffectIds } : {}),
        });
        break;
      case "spec":
        projected.push({ ...base, revision: record.revision, outcome: record.outcome });
        break;
      case "plan":
        projected.push({ ...base, revision: record.revision, objective: record.objective });
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
