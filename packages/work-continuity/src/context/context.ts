import type { BoundedProjection, ContextIo, ContinuityCheckpoint, SourcePin, SourceStatus, WorkContextRequest } from './types';
import { localContextIo, assertContextSnapshotStable } from './source';
import { validateCheckpoint, validateWorkContextRequest } from './validation';
import { readCheckpoint } from './store';

/** Budget complete rows instead of slicing serialized JSON or hiding truncation. */
export function boundedProjection(input: ContinuityCheckpoint, options: { maxChars?: number; sourceStatus?: SourceStatus } = {}): BoundedProjection {
  const checkpoint = validateCheckpoint(input), maxChars = options.maxChars ?? 6000;
  if (!Number.isSafeInteger(maxChars) || maxChars < 1024 || maxChars > 24_000) throw Error('Context maxChars must be between 1024 and 24000');
  const omitted: Record<string, number> = {};
  const sourceRefs: Record<string, SourcePin> = {};
  const ref = (pin: Pick<SourcePin, 'path'>) => `s${checkpoint.sourcePins.findIndex((source) => source.path === pin.path)}`;
  interface Row { value: unknown; pins?: SourcePin[] }
  const rows = (values: unknown[]): Row[] => values.map((value) => ({ value }));
  const sourceStatus = options.sourceStatus ?? { status: 'repair-required', checks: [], repair: [{ path: checkpoint.root, action: 'reobserve-source', reason: 'Sources have not been verified' }], authority: 'none' };
  const projected: Record<string, unknown> = {
    schema: 'codex.continuity.projection.v1', projectId: checkpoint.projectId, workId: checkpoint.workId, version: checkpoint.version, updatedAt: checkpoint.updatedAt,
    owner: checkpoint.owner, acceptedDecisionIds: checkpoint.userDecisions.filter((row) => row.status === 'accepted').map((row) => row.id),
    completedEffectIds: checkpoint.completed.map((row) => row.id), uncertainEffectIds: checkpoint.uncertainEffects.map((row) => row.id), uninterpretedInputCount: checkpoint.uninterpretedInputs.length,
    objective: checkpoint.purpose.objective, nextAction: sourceStatus.status === 'current' ? checkpoint.nextAction : null,
    sourceState: sourceStatus.status, authority: 'none', nativePermissionGranted: false, executionReady: false,
    continuationRule: sourceStatus.status === 'current' ? 'Use accepted decisions; do not replay completed effects. Observe uncertain effects before action.' : 'Repair indicated source references before treating checkpoint context as current. Do not replay effects.',
    sourceRefs, omitted,
  };
  const serialize = () => JSON.stringify(projected);
  // An indivisible objective/next action is never presented as a misleading fragment.
  if (serialize().length > maxChars - 100) throw Error('Context budget cannot preserve objective and next action; read the source checkpoint or request a larger budget');
  const groups: Array<[string, Row[]]> = [
    ['repair', rows(sourceStatus.repair)],
    ['uninterpretedInputs', checkpoint.uninterpretedInputs.map((input) => ({ value: { kind: input.kind, sourceRef: ref(input), locator: input.locator, observedAt: input.observedAt }, pins: [input] }))],
    ['userDecisions', [...checkpoint.userDecisions.filter((row) => row.status === 'accepted'), ...checkpoint.userDecisions.filter((row) => row.status === 'superseded')].map((decision) => {
      const { source, ...meaning } = decision;
      return { value: { ...meaning, source: { kind: source.kind, ref: ref(source), locator: source.locator } }, pins: [source] };
    })],
    ['uncertainEffects', rows(checkpoint.uncertainEffects)],
    ['completed', checkpoint.completed.map((effect) => ({ value: { ...effect, verificationRefs: effect.verificationRefs.map((path) => ref({ path })) }, pins: effect.verificationRefs.map((path) => checkpoint.sourcePins.find((source) => source.path === path)!) }))],
    ['pending', rows(checkpoint.pending)], ['successCriteria', rows(checkpoint.purpose.successCriteria)], ['nativeThreads', rows(checkpoint.nativeThreads)],
    ['sourcePins', checkpoint.sourcePins.map((source) => ({ value: ref(source), pins: [source] }))],
    ['evidenceRefs', checkpoint.evidenceRefs.map((evidence) => ({ value: { sourceRef: ref(evidence), kind: evidence.kind, ...(evidence.recordId ? { recordId: evidence.recordId } : {}) }, pins: [evidence] }))],
    ['inScope', rows(checkpoint.purpose.inScope)], ['outOfScope', rows(checkpoint.purpose.outOfScope)],
  ];
  // Allocate core continuation rows first, preserving explicit omitted counts for every section.
  for (const [name, rows] of groups) { projected[name] = []; omitted[name] = rows.length; }
  for (const [name, rows] of groups) {
    const selected = projected[name] as unknown[];
    for (const row of rows) {
      const added: string[] = [];
      for (const source of row.pins ?? []) {
        const key = ref(source);
        if (!(key in sourceRefs)) { sourceRefs[key] = checkpoint.sourcePins.find((pin) => pin.path === source.path)!; added.push(key); }
      }
      selected.push(row.value); omitted[name] -= 1;
      if (serialize().length > maxChars - 40) {
        selected.pop(); omitted[name] += 1; for (const key of added) delete sourceRefs[key];
      }
    }
  }
  const truncated = Object.values(omitted).some((count) => count > 0);
  projected.truncated = truncated;
  const text = serialize();
  if (text.length > maxChars) throw Error('Context budget cannot preserve required omission metadata');
  return { text, chars: text.length, truncated, omitted, authority: 'none', executionReady: false };
}

/** Generic owner adapter. A caller may inject an exact admitted-source reader. */
export async function readWorkContext(input: WorkContextRequest | unknown, io: ContextIo = localContextIo) {
  const request = validateWorkContextRequest(input);
  if (await io.canonical(request.root) !== request.root || await io.canonical(request.cwd) !== request.cwd) throw Error('Work root and cwd must be exact canonical paths');
  const read = await readCheckpoint(request.checkpointPath, { root: request.root, verifySources: true, io });
  if (read.checkpoint.projectId !== request.projectId || read.checkpoint.workId !== request.workId) throw Error('Checkpoint does not match selected Project/Work');
  if (request.checkpointSha256 && request.checkpointSha256 !== read.sha256) throw Error('Checkpoint differs from pinned request');
  const projection = boundedProjection(read.checkpoint, { maxChars: request.maxChars ?? 6000, sourceStatus: read.sourceStatus });
  // Detect source drift during projection; the adapter never converts input context to permission.
  await assertContextSnapshotStable(read, request.checkpointPath, request.root, io, 'context');
  return { schema: 'codex.work-context.result.v1' as const, projectId: request.projectId, workId: request.workId, checkpointPin: { path: request.checkpointPath, sha256: read.sha256 }, sourceStatus: read.sourceStatus, projection, authority: 'none' as const, executionReady: false as const, nativePermissionGranted: false as const };
}
