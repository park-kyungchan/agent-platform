import {
  count,
  exact,
  record,
  same,
  text,
  type ActorBinding,
  type EffectiveExecutionVerification,
  type GovernancePolicy,
  type NativeRunReceipt,
  type NativeSessionReceipt,
  type NativeTaskRequirements,
  type ProfileBinding,
} from "./contracts.ts";
import { validateActorBinding } from "./policy.ts";

const taskKeys = [
  "task_id",
  "work_id",
  "actor_id",
  "assignee",
  "model_override",
  "provider_override",
  "reasoning_effort",
  "max_runtime_seconds",
  "max_retries",
  "idempotency_key",
  "completion_contract",
  "dispatch_backend",
] as const;
export function validateNativeTask(
  value: unknown,
  actor: ActorBinding,
  policy: GovernancePolicy,
  profile: ProfileBinding,
): asserts value is NativeTaskRequirements {
  validateActorBinding(actor, policy, profile);
  if (actor.role === "main")
    throw new Error("Exempt Main is user-controlled and cannot be a controlled Kanban worker");
  const t = record(value, "task");
  exact(t, taskKeys, "task");
  for (const key of taskKeys.filter((key) => !["max_runtime_seconds", "max_retries"].includes(key)))
    text(t[key], key);
  same(t.work_id, policy.workId, "task.work_id");
  same(t.actor_id, actor.actorId, "task.actor_id");
  same(t.assignee, actor.profileId, "task.assignee");
  same(t.model_override, actor.model, "task.model_override");
  same(t.provider_override, actor.provider, "task.provider_override");
  same(t.reasoning_effort, actor.effort, "task.reasoning_effort");
  same(t.dispatch_backend, "native-kanban", "task.dispatch_backend");
  count(t.max_runtime_seconds, "max_runtime_seconds", true);
  count(t.max_retries, "max_retries", true);
}
/** Expected native dispatcher flags. This does not execute CLI or certify effective runtime behavior. */
export function expectedNativeWorkerArgv(
  task: NativeTaskRequirements,
  actor: ActorBinding,
  policy: GovernancePolicy,
  profile: ProfileBinding,
): string[] {
  validateNativeTask(task, actor, policy, profile);
  return [
    "-m",
    task.model_override,
    "--provider",
    task.provider_override,
    "--reasoning",
    task.reasoning_effort,
  ];
}
export function verifyNativeExecution(
  task: NativeTaskRequirements,
  actor: ActorBinding,
  policy: GovernancePolicy,
  profile: ProfileBinding,
  session: unknown,
  run: unknown,
): EffectiveExecutionVerification {
  validateNativeTask(task, actor, policy, profile);
  if (session == null || run == null)
    return {
      status: "unknown",
      reasons: [
        "Actual native Session and Run receipts are required; request acceptance proves no execution.",
      ],
    };
  const s = record(session, "session");
  exact(s, ["session_id", "profile_id", "native_root", "actor_id", "source_ref"], "session");
  for (const key of ["session_id", "profile_id", "native_root", "actor_id", "source_ref"])
    text(s[key], key);
  same(s.session_id, actor.sessionId, "session.session_id");
  same(s.profile_id, actor.profileId, "session.profile_id");
  same(s.native_root, actor.nativeRoot, "session.native_root");
  same(s.actor_id, actor.actorId, "session.actor_id");
  const r = record(run, "run");
  const keys = [
    "run_id",
    "task_id",
    "session_id",
    "assignee",
    "native_root",
    "effective_model",
    "effective_provider",
    "effective_effort",
    "max_runtime_seconds",
    "max_retries",
    "idempotency_key",
    "completion_contract",
    "source_ref",
  ];
  exact(r, keys, "run");
  for (const key of keys.filter((key) => !["max_runtime_seconds", "max_retries"].includes(key)))
    text(r[key], key);
  same(r.task_id, task.task_id, "run.task_id");
  same(r.session_id, s.session_id, "run.session_id");
  same(r.assignee, task.assignee, "run.assignee");
  same(r.native_root, actor.nativeRoot, "run.native_root");
  same(r.effective_model, task.model_override, "run.effective_model");
  same(r.effective_provider, task.provider_override, "run.effective_provider");
  same(r.effective_effort, task.reasoning_effort, "run.effective_effort");
  for (const key of [
    "max_runtime_seconds",
    "max_retries",
    "idempotency_key",
    "completion_contract",
  ] as const)
    same(r[key], task[key], `run.${key}`);
  return {
    status: "verified",
    runId: r.run_id as string,
    sessionId: s.session_id as string,
    sourceRefs: [s.source_ref as string, r.source_ref as string],
  };
}
export type { NativeRunReceipt, NativeSessionReceipt };
