import { createHash } from 'node:crypto';
import { exact, record, same, text, utc } from './contracts.ts';

export interface BotCorrelationRequest {
  workId: string;
  correlationId: string;
  senderActorId: string;
  senderSessionId: string;
  recipientActorId: string;
  recipientProfileId: string;
  nativeMessageId: string;
  sourceRef: string;
}
export type BotCorrelationObservation = {
  observationId: string;
  correlationId: string;
  nativeMessageId: string;
  atUtc: string;
  sourceRef: string;
} & (
  | { kind: 'queued' }
  | { kind: 'claimed'; recipientSessionId: string }
  | { kind: 'settled'; recipientSessionId: string }
  | { kind: 'ambiguous'; reason: string }
  | { kind: 'task'; taskId: string; runId: string; taskStatus: string }
  | { kind: 'evaluation'; taskId: string; runId: string; evaluationRef: string }
  | { kind: 'acceptance'; taskId: string; runId: string; evaluationRef: string; acceptanceDecisionRef: string }
);
export interface BotCorrelationState {
  request: BotCorrelationRequest;
  observations: BotCorrelationObservation[];
}
export interface BotCorrelationProjection {
  delivery: 'unknown' | 'queued' | 'claimed' | 'settled' | 'ambiguous';
  recipientSessionId: string | null;
  taskBindings: { taskId: string; runId: string; taskStatus: string }[];
  evaluationRefs: string[];
  acceptanceDecisionRefs: string[];
  ambiguities: string[];
}
/** Stable comparison for idempotency; never use property order as semantic identity. */
export function stableJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return '[' + value.map(stableJson).join(',') + ']';
  return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + stableJson((value as Record<string, unknown>)[key])).join(',') + '}';
}
export function contentDigest(value: unknown): string { return createHash('sha256').update(stableJson(value)).digest('hex'); }
export function validateBotRequest(value: unknown): asserts value is BotCorrelationRequest {
  const r = record(value, 'botRequest');
  const keys = ['workId', 'correlationId', 'senderActorId', 'senderSessionId', 'recipientActorId', 'recipientProfileId', 'nativeMessageId', 'sourceRef'];
  exact(r, keys, 'botRequest'); for (const key of keys) text(r[key], key);
  if (r.senderActorId === r.recipientActorId) throw new Error('Bot DM correlation requires distinct sender and recipient');
}
function validateObservation(value: unknown, request: BotCorrelationRequest): asserts value is BotCorrelationObservation {
  const o = record(value, 'botObservation');
  const base = ['observationId', 'correlationId', 'nativeMessageId', 'atUtc', 'sourceRef', 'kind'];
  const extras: Record<string, string[]> = { queued: [], claimed: ['recipientSessionId'], settled: ['recipientSessionId'], ambiguous: ['reason'], task: ['taskId', 'runId', 'taskStatus'], evaluation: ['taskId', 'runId', 'evaluationRef'], acceptance: ['taskId', 'runId', 'evaluationRef', 'acceptanceDecisionRef'] };
  text(o.kind, 'kind'); const fields = extras[o.kind]; if (!fields) throw new Error('Unknown Bot observation kind');
  exact(o, [...base, ...fields], 'botObservation'); for (const key of [...base, ...fields]) text(o[key], key);
  utc(o.atUtc, 'atUtc'); same(o.correlationId, request.correlationId, 'correlationId'); same(o.nativeMessageId, request.nativeMessageId, 'nativeMessageId');
}
export function projectBotCorrelation(state: BotCorrelationState): BotCorrelationProjection {
  validateBotRequest(state.request);
  if (!Array.isArray(state.observations)) throw new Error('Bot observation history required');
  const projection: BotCorrelationProjection = { delivery: 'unknown', recipientSessionId: null, taskBindings: [], evaluationRefs: [], acceptanceDecisionRefs: [], ambiguities: [] };
  const seen = new Set<string>(), evaluations = new Set<string>();
  let previousTime = -Infinity, rank = 0;
  for (const o of state.observations) {
    validateObservation(o, state.request);
    if (seen.has(o.observationId)) throw new Error('Duplicate Bot observation identity'); seen.add(o.observationId);
    const time = utc(o.atUtc, 'atUtc'); if (time < previousTime) throw new Error('Bot history must be monotonic'); previousTime = time;
    if (o.kind === 'ambiguous') projection.ambiguities.push(o.reason);
    else if (o.kind === 'queued' || o.kind === 'claimed' || o.kind === 'settled') {
      const nextRank = { queued: 1, claimed: 2, settled: 3 }[o.kind];
      if (nextRank < rank) throw new Error('Delivery cannot regress'); rank = nextRank;
      if (o.kind !== 'queued') {
        if (projection.recipientSessionId !== null && projection.recipientSessionId !== o.recipientSessionId) projection.ambiguities.push('Multiple recipient sessions claimed the same DM');
        else projection.recipientSessionId = o.recipientSessionId;
      }
      projection.delivery = o.kind;
    } else if (o.kind === 'task') {
      projection.taskBindings.push({ taskId: o.taskId, runId: o.runId, taskStatus: o.taskStatus });
    } else {
      if (!projection.taskBindings.some(t => t.taskId === o.taskId && t.runId === o.runId)) throw new Error('Evaluation/acceptance requires observed Task/Run binding');
      const evaluationKey = stableJson([o.taskId, o.runId, o.evaluationRef]);
      if (o.kind === 'evaluation') { evaluations.add(evaluationKey); projection.evaluationRefs.push(o.evaluationRef); }
      else {
        if (!evaluations.has(evaluationKey)) throw new Error('AcceptanceDecision requires separately observed Evaluation');
        projection.acceptanceDecisionRefs.push(o.acceptanceDecisionRef);
      }
    }
  }
  if (projection.ambiguities.length) projection.delivery = 'ambiguous';
  return projection;
}
/** Native observations only. Settled DM delivery never infers Task completion or acceptance. */
export function reconcileBotObservation(state: BotCorrelationState, observation: BotCorrelationObservation): BotCorrelationState {
  projectBotCorrelation(state); validateObservation(observation, state.request);
  const previous = state.observations.find(o => o.observationId === observation.observationId);
  if (previous) {
    if (stableJson(previous) !== stableJson(observation)) throw new Error('Bot idempotency identity reused with conflicting content');
    return structuredClone(state);
  }
  const next = structuredClone({ ...state, observations: [...state.observations, observation] });
  projectBotCorrelation(next); return next;
}
