import { assertAbsolutePath, insideRoot } from './source';
import type { ContinuityCheckpoint, SourcePin, WorkContextRequest } from './types';

function object(value: unknown, label: string): asserts value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw Error(`${label} must be an object`);
}
function keys(value: Record<string, unknown>, allowed: string[], label: string) {
  if (Object.keys(value).some((key) => !allowed.includes(key))) throw Error(`${label} has unsupported fields`);
}
function text(value: unknown, label: string, max = 12_000): asserts value is string {
  if (typeof value !== 'string' || !value.trim() || value.length > max) throw Error(`${label} must be nonempty bounded text`);
}
function iso(value: unknown, label: string) {
  text(value, label, 40);
  if (!/^\d{4}-\d\d-\d\dT/.test(value) || !Number.isFinite(Date.parse(value)) || !value.endsWith('Z')) throw Error(`${label} must be a UTC timestamp`);
}
function array(value: unknown, label: string, max = 256): asserts value is unknown[] {
  if (!Array.isArray(value) || value.length > max) throw Error(`${label} must be a bounded array`);
}
function strings(value: unknown, label: string): asserts value is string[] {
  array(value, label); value.forEach((item) => text(item, label));
}
function oneOf(value: unknown, values: readonly string[], label: string) { if (!values.includes(String(value))) throw Error(`${label} is invalid`); }
function pin(value: unknown, root: string, label: string): asserts value is SourcePin {
  object(value, label); text(value.path, `${label}.path`); assertAbsolutePath(value.path);
  if (!insideRoot(root, value.path)) throw Error(`${label} is outside owning root`);
  if (typeof value.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.sha256)) throw Error(`${label}.sha256 must be a lowercase SHA-256`);
  if (value.role !== undefined) oneOf(value.role, ['purpose', 'user-answer', 'decision', 'evidence', 'context', 'code'], `${label}.role`);
  if (value.recordKind !== undefined) oneOf(value.recordKind, ['project', 'task', 'proposal', 'decision', 'evidence', 'verification', 'run'], `${label}.recordKind`);
}
function uniqueIds(rows: unknown[], label: string) {
  const ids = new Set<string>();
  for (const row of rows) {
    object(row, label); text(row.id, `${label}.id`, 256);
    if (ids.has(row.id)) throw Error(`${label} IDs must be unique`); ids.add(row.id);
  }
  return ids;
}
export function validateCheckpoint(value: unknown): ContinuityCheckpoint {
  object(value, 'Checkpoint');
  keys(value, ['schema', 'projectId', 'workId', 'root', 'owner', 'version', 'updatedAt', 'purpose', 'userDecisions', 'completed', 'pending', 'nextAction', 'nativeThreads', 'evidenceRefs', 'sourcePins', 'uncertainEffects', 'uninterpretedInputs', 'authority', 'nativePermissionGranted'], 'Checkpoint');
  if (value.schema !== 'codex.continuity.checkpoint.v1' || value.authority !== 'none' || value.nativePermissionGranted !== false) throw Error('Invalid checkpoint schema or authority');
  text(value.projectId, 'projectId', 256); text(value.workId, 'workId', 256); text(value.root, 'root'); assertAbsolutePath(value.root);
  if (!Number.isSafeInteger(value.version) || Number(value.version) < 1) throw Error('Checkpoint version must be a positive safe integer');
  iso(value.updatedAt, 'updatedAt'); object(value.owner, 'owner'); keys(value.owner, ['agentId', 'threadId'], 'owner'); text(value.owner.agentId, 'owner.agentId', 256); text(value.owner.threadId, 'owner.threadId', 256);
  object(value.purpose, 'purpose'); keys(value.purpose, ['objective', 'successCriteria', 'inScope', 'outOfScope'], 'purpose'); text(value.purpose.objective, 'purpose.objective');
  for (const name of ['successCriteria', 'inScope', 'outOfScope']) strings(value.purpose[name], `purpose.${name}`);
  if ((value.purpose.successCriteria as unknown[]).length === 0) throw Error('At least one success criterion required');
  // Long-lived Work retains immutable evidence. Keep explicit cardinality limits
  // alongside the store's 2 MiB byte limit without exhausting it after 256 pins.
  array(value.sourcePins, 'sourcePins', 1024);
  const sources = new Map<string, SourcePin>();
  for (const entry of value.sourcePins) {
    object(entry, 'sourcePin'); keys(entry, ['path', 'sha256', 'role', 'recordKind'], 'sourcePin'); pin(entry, value.root, 'sourcePin');
    if (sources.has(entry.path)) throw Error('One source pin per exact path required'); sources.set(entry.path, entry);
  }
  const admitted = (entry: unknown, label: string) => {
    pin(entry, value.root as string, label);
    const source = sources.get(entry.path);
    if (!source || source.sha256 !== entry.sha256 || (entry.recordKind !== undefined && entry.recordKind !== source.recordKind)) throw Error(`${label} must reference an admitted source pin`);
  };
  array(value.userDecisions, 'userDecisions'); const decisionIds = uniqueIds(value.userDecisions, 'userDecisions');
  for (const row of value.userDecisions) {
    object(row, 'decision'); keys(row, ['id', 'question', 'answer', 'status', 'source', 'supersedes'], 'decision'); text(row.answer, 'decision.answer');
    if (row.question !== undefined) text(row.question, 'decision.question'); oneOf(row.status, ['accepted', 'superseded'], 'decision.status');
    object(row.source, 'decision.source'); keys(row.source, ['path', 'sha256', 'role', 'recordKind', 'kind', 'locator'], 'decision.source');
    if (row.source.kind !== 'actual-user-answer' || sources.get(String(row.source.path))?.role !== 'user-answer') throw Error('Decision must reference an actual user answer, not generated history');
    text(row.source.locator, 'decision.source.locator', 1024); admitted(row.source, 'decision.source');
    if (row.supersedes !== undefined && (typeof row.supersedes !== 'string' || !decisionIds.has(row.supersedes) || row.supersedes === row.id)) throw Error('Decision supersedes must reference a different retained decision');
  }
  array(value.evidenceRefs, 'evidenceRefs', 1024);
  for (const row of value.evidenceRefs) {
    object(row, 'evidenceRef'); keys(row, ['path', 'sha256', 'role', 'recordKind', 'kind', 'recordId'], 'evidenceRef'); oneOf(row.kind, ['observation', 'verification', 'acceptance', 'source'], 'evidenceRef.kind'); admitted(row, 'evidenceRef');
    if (row.recordId !== undefined) text(row.recordId, 'evidenceRef.recordId', 512);
    if (row.kind === 'verification' && row.recordKind !== 'verification') throw Error('Verification refs require a validated Verification record');
    if (row.kind === 'acceptance' && row.recordKind !== 'decision') throw Error('Acceptance refs require a separate Decision record');
  }
  const evidencePaths = new Set(value.evidenceRefs.map((row) => (row as SourcePin).path));
  array(value.completed, 'completed'); const completedIds = uniqueIds(value.completed, 'completed');
  for (const row of value.completed) {
    object(row, 'completed'); keys(row, ['id', 'action', 'completedAt', 'verificationRefs', 'replay'], 'completed'); text(row.action, 'completed.action'); iso(row.completedAt, 'completed.completedAt');
    strings(row.verificationRefs, 'completed.verificationRefs'); if (row.replay !== 'do-not-repeat' || row.verificationRefs.length === 0 || row.verificationRefs.some((ref) => !evidencePaths.has(String(ref)))) throw Error('Completed effect requires pinned evidence and do-not-repeat');
  }
  array(value.pending, 'pending'); const pendingIds = uniqueIds(value.pending, 'pending');
  for (const row of value.pending) {
    object(row, 'pending'); keys(row, ['id', 'action', 'dependsOn', 'status'], 'pending'); text(row.action, 'pending.action'); strings(row.dependsOn, 'pending.dependsOn'); oneOf(row.status, ['ready', 'blocked', 'unknown'], 'pending.status');
    if (completedIds.has(String(row.id))) throw Error('A completed effect cannot be queued for replay');
    if (row.dependsOn.some((id) => id === row.id || (!pendingIds.has(String(id)) && !completedIds.has(String(id))))) throw Error('Pending dependency must reference a retained task/effect');
  }
  const dependencies = new Map(value.pending.map((row) => { const task = row as unknown as { id: string; dependsOn: string[] }; return [task.id, task.dependsOn] as const; }));
  const visiting = new Set<string>(), visited = new Set<string>();
  const visit = (id: string) => {
    if (visiting.has(id)) throw Error('Pending dependency cycle');
    if (visited.has(id)) return;
    visiting.add(id); for (const dependency of dependencies.get(id) ?? []) if (dependencies.has(dependency)) visit(dependency);
    visiting.delete(id); visited.add(id);
  };
  for (const id of dependencies.keys()) visit(id);
  if (value.nextAction !== null) {
    object(value.nextAction, 'nextAction'); keys(value.nextAction, ['taskId', 'action'], 'nextAction'); text(value.nextAction.action, 'nextAction.action');
    if (typeof value.nextAction.taskId !== 'string' || !pendingIds.has(value.nextAction.taskId)) throw Error('Next action must refer to pending work');
  }
  array(value.nativeThreads, 'nativeThreads'); const threads = new Set<string>();
  for (const row of value.nativeThreads) {
    object(row, 'nativeThread'); keys(row, ['threadId', 'relation', 'taskId', 'status'], 'nativeThread'); text(row.threadId, 'nativeThread.threadId', 256);
    if (threads.has(row.threadId)) throw Error('Native thread links must be unique'); threads.add(row.threadId);
    oneOf(row.relation, ['owner', 'child', 'predecessor'], 'nativeThread.relation'); oneOf(row.status, ['active', 'complete', 'interrupted', 'unknown'], 'nativeThread.status');
    if (row.taskId !== undefined && (typeof row.taskId !== 'string' || (!pendingIds.has(row.taskId) && !completedIds.has(row.taskId)))) throw Error('Thread task link must refer to retained work');
  }
  array(value.uncertainEffects, 'uncertainEffects'); uniqueIds(value.uncertainEffects, 'uncertainEffects');
  for (const row of value.uncertainEffects) {
    object(row, 'uncertainEffect'); keys(row, ['id', 'action', 'reason', 'requiredObservation'], 'uncertainEffect');
    for (const field of ['action', 'reason', 'requiredObservation']) text(row[field], `uncertainEffect.${field}`);
    if (completedIds.has(String(row.id)) || pendingIds.has(String(row.id))) throw Error('Uncertain effects must remain distinct from completed or queued actions');
  }
  array(value.uninterpretedInputs, 'uninterpretedInputs');
  for (const row of value.uninterpretedInputs) {
    object(row, 'uninterpretedInput'); keys(row, ['path', 'sha256', 'role', 'recordKind', 'kind', 'locator', 'observedAt'], 'uninterpretedInput');
    if (row.kind !== 'pending-user-input') throw Error('Uninterpreted input must retain pending-user-input classification'); admitted(row, 'uninterpretedInput'); text(row.locator, 'uninterpretedInput.locator', 1024); iso(row.observedAt, 'uninterpretedInput.observedAt');
  }
  return value as unknown as ContinuityCheckpoint;
}
export function validateWorkContextRequest(value: unknown): WorkContextRequest {
  object(value, 'Work request'); keys(value, ['schema', 'projectId', 'workId', 'root', 'cwd', 'checkpointPath', 'checkpointSha256', 'maxChars'], 'Work request');
  if (value.schema !== 'codex.work-context.request.v1') throw Error('Explicit native Codex Work schema required');
  for (const field of ['projectId', 'workId', 'root', 'cwd', 'checkpointPath']) text(value[field], field);
  for (const field of ['root', 'cwd', 'checkpointPath']) assertAbsolutePath(value[field] as string);
  if (!insideRoot(value.root as string, value.cwd as string) || !insideRoot(value.root as string, value.checkpointPath as string)) throw Error('Foreign cwd or checkpoint outside owning root');
  if (value.checkpointSha256 !== undefined && (typeof value.checkpointSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.checkpointSha256))) throw Error('Checkpoint hash must be SHA-256');
  if (value.maxChars !== undefined && (!Number.isSafeInteger(value.maxChars) || Number(value.maxChars) < 1024 || Number(value.maxChars) > 24_000)) throw Error('Context maxChars must be between 1024 and 24000');
  return value as unknown as WorkContextRequest;
}
