/** Pure governance contracts. Native Hermes retains dispatch, stores, retry and session ownership. */
export interface ModelSelection {
  model: "gpt-6.1-sol-900k" | "gpt-6-luna-900k";
  provider: "openai-codex";
  effort: "high" | "xhigh";
}
export interface LunaExecutionScope {
  destinationIds: string[];
  inputsComplete: boolean;
  effect: string;
  completion: string;
  substantiveJudgment: boolean;
  conflict: boolean;
  evidenceInterpretation: boolean;
  design: boolean;
  code: boolean;
  deletion: boolean;
  uncertainEffects: boolean;
}
export interface ActorBinding {
  actorId: string;
  profileId: string;
  sessionId: string;
  nativeRoot: string;
  role: "main" | "lead" | "worker";
  userControlled: boolean;
  model: string;
  provider: string;
  effort: string;
  lunaScope?: LunaExecutionScope;
}
export interface ProfileBinding {
  profileId: string;
  nativeRoot: string;
  model: string;
  provider: string;
  effort: string;
  delegateTaskEnabled: false;
  taskBackend: "native-kanban";
}
export interface GovernancePolicy {
  workId: string;
  mainSessionId: string;
  mainActorId: string;
  ownerProjectId: "hermes-harness-engineering";
}
/** Native field names are intentionally preserved for exact owner readback. */
export interface NativeTaskRequirements {
  task_id: string;
  work_id: string;
  actor_id: string;
  assignee: string;
  model_override: ModelSelection["model"];
  provider_override: ModelSelection["provider"];
  reasoning_effort: ModelSelection["effort"];
  max_runtime_seconds: number;
  /** Native consecutive failure threshold: 1 blocks on the first failure. */
  max_retries: number;
  idempotency_key: string;
  completion_contract: string;
  dispatch_backend: "native-kanban";
}
export interface NativeSessionReceipt {
  session_id: string;
  profile_id: string;
  native_root: string;
  actor_id: string;
  source_ref: string;
}
export interface NativeRunReceipt {
  run_id: string;
  task_id: string;
  session_id: string;
  assignee: string;
  native_root: string;
  /** Actual process/runtime values, never inferred from a requested profile. */
  effective_model: string;
  effective_provider: string;
  effective_effort: string;
  max_runtime_seconds: number;
  max_retries: number;
  idempotency_key: string;
  completion_contract: string;
  source_ref: string;
}
export type EffectiveExecutionVerification =
  | { status: "unknown"; reasons: string[] }
  | { status: "verified"; runId: string; sessionId: string; sourceRefs: string[] };

export function record(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(`${name}: object required`);
  return value as Record<string, unknown>;
}
export function exact(value: Record<string, unknown>, keys: readonly string[], name: string): void {
  const extras = Object.keys(value).filter((key) => !keys.includes(key));
  if (extras.length) throw new Error(`${name}: unrecognized fields: ${extras.join(", ")}`);
}
export function text(value: unknown, name: string): asserts value is string {
  if (typeof value !== "string" || !value.trim() || value.trim() !== value)
    throw new Error(`${name}: explicit nonblank string required`);
}
export function count(value: unknown, name: string, positive = false): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) < (positive ? 1 : 0))
    throw new Error(`${name}: ${positive ? "positive" : "nonnegative"} safe integer required`);
}
export function same(left: unknown, right: unknown, name: string): void {
  if (left !== right) throw new Error(`${name}: binding mismatch`);
}
export function utc(value: unknown, name: string): number {
  text(value, name);
  const time = Date.parse(value);
  if (!Number.isFinite(time) || new Date(time).toISOString() !== value)
    throw new Error(`${name}: canonical UTC timestamp required`);
  return time;
}
