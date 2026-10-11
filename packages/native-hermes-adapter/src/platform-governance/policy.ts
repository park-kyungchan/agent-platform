import { exact, record, same, text, type ActorBinding, type GovernancePolicy, type LunaExecutionScope, type ProfileBinding } from './contracts.ts';

export const CONTROLLED_MODEL_POLICY = Object.freeze({
  primary: { model: 'gpt-6.1-sol-900k', provider: 'openai-codex', effort: 'high' },
  narrowlyEligibleWorker: { model: 'gpt-6-luna-900k', provider: 'openai-codex', effort: 'xhigh' },
  taskBackend: 'native-kanban', delegateTaskEnabled: false, deterministicUtilitiesFirst: true,
} as const);

export function validateGovernancePolicy(value: unknown): asserts value is GovernancePolicy {
  const p = record(value, 'policy');
  exact(p, ['workId', 'mainSessionId', 'mainActorId', 'ownerProjectId'], 'policy');
  for (const key of ['workId', 'mainSessionId', 'mainActorId']) text(p[key], key);
  same(p.ownerProjectId, 'hermes-harness-engineering', 'ownerProjectId');
}
export function validateLunaScope(value: unknown): asserts value is LunaExecutionScope {
  const s = record(value, 'lunaScope');
  exact(s, ['destinationIds', 'inputsComplete', 'effect', 'completion', 'substantiveJudgment', 'conflict', 'evidenceInterpretation', 'design', 'code', 'deletion', 'uncertainEffects'], 'lunaScope');
  if (!Array.isArray(s.destinationIds) || s.destinationIds.length !== 1) throw new Error('Luna requires one resolved destination');
  text(s.destinationIds[0], 'destinationId');
  same(s.inputsComplete, true, 'inputsComplete'); text(s.effect, 'effect'); text(s.completion, 'completion');
  for (const key of ['substantiveJudgment', 'conflict', 'evidenceInterpretation', 'design', 'code', 'deletion', 'uncertainEffects']) same(s[key], false, key);
}
export function validateProfileBinding(value: unknown): asserts value is ProfileBinding {
  const p = record(value, 'profile');
  exact(p, ['profileId', 'nativeRoot', 'model', 'provider', 'effort', 'delegateTaskEnabled', 'taskBackend'], 'profile');
  for (const key of ['profileId', 'nativeRoot', 'model', 'provider', 'effort']) text(p[key], key);
  same(p.delegateTaskEnabled, false, 'delegateTaskEnabled'); same(p.taskBackend, 'native-kanban', 'taskBackend');
}
export function validateActorBinding(value: unknown, policy: GovernancePolicy, profile: ProfileBinding): asserts value is ActorBinding {
  validateGovernancePolicy(policy); validateProfileBinding(profile);
  const a = record(value, 'actor');
  exact(a, ['actorId', 'profileId', 'sessionId', 'nativeRoot', 'role', 'userControlled', 'model', 'provider', 'effort', 'lunaScope'], 'actor');
  for (const key of ['actorId', 'profileId', 'sessionId', 'nativeRoot', 'model', 'provider', 'effort']) text(a[key], key);
  if (!['main', 'lead', 'worker'].includes(a.role as string)) throw new Error('Unknown actor role');
  if (typeof a.userControlled !== 'boolean') throw new Error('userControlled must be explicit');
  same(a.profileId, profile.profileId, 'actor.profileId'); same(a.nativeRoot, profile.nativeRoot, 'actor.nativeRoot');
  for (const key of ['model', 'provider', 'effort'] as const) same(a[key], profile[key], `profile.${key}`);
  if (a.role === 'main') {
    same(a.actorId, policy.mainActorId, 'mainActorId'); same(a.sessionId, policy.mainSessionId, 'mainSessionId');
    same(a.userControlled, true, 'main user control');
    if (a.lunaScope !== undefined) throw new Error('Main exemption does not require a Luna scope');
    return;
  }
  if (a.actorId === policy.mainActorId || a.sessionId === policy.mainSessionId) throw new Error('Controlled actor cannot alias exempt main identity');
  same(a.userControlled, false, 'controlled actor');
  const luna = a.model === CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker.model;
  const expected = luna ? CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker : CONTROLLED_MODEL_POLICY.primary;
  for (const key of ['model', 'provider', 'effort'] as const) same(a[key], expected[key], `controlled ${key}`);
  if (luna) { same(a.role, 'worker', 'Luna role'); validateLunaScope(a.lunaScope); }
  else if (a.lunaScope !== undefined) throw new Error('Luna scope cannot silently change primary actor policy');
}
/** A deterministic utility must be considered before requesting any model call. */
export function validateModelNecessity(value: unknown): void {
  const n = record(value, 'modelNecessity');
  exact(n, ['deterministicUtilityConsidered', 'requiresModelReason'], 'modelNecessity');
  same(n.deterministicUtilityConsidered, true, 'deterministicUtilityConsidered'); text(n.requiresModelReason, 'requiresModelReason');
}
