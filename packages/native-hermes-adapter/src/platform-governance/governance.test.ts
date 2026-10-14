import { describe, expect, test } from "bun:test";
import {
  CONTROLLED_MODEL_POLICY,
  createNativeEvaluationSuite,
  exposeCustodyGaps,
  expectedNativeWorkerArgv,
  planBoundaryCustody,
  projectBotCorrelation,
  reconcileBotObservation,
  reconcileNativeModelRequest,
  reserveNativeModelRequest,
  validateActorBinding,
  validateNativeEvaluationSuite,
  validateNativeTask,
  verifyBoundaryCustodyReadback,
  verifyNativeExecution,
  type ActorBinding,
  type BotCorrelationObservation,
  type BotCorrelationState,
  type CustodyBoundary,
  type GovernancePolicy,
  type LunaExecutionScope,
  type NativeCheckpointSnapshot,
  type NativeRequestObservation,
  type NativeRunReceipt,
  type NativeSessionReceipt,
  type NativeTaskRequirements,
  type ProfileBinding,
  type RequestAdmission,
} from "./index.ts";

const now = "2026-10-11T00:00:00.000Z";
const later = "2026-10-11T00:00:01.000Z";
const policy: GovernancePolicy = {
  workId: "work-new",
  mainSessionId: "main-session",
  mainActorId: "main-actor",
  ownerProjectId: "hermes-harness-engineering",
};
const profile: ProfileBinding = {
  profileId: "engineering",
  nativeRoot: "/native/engineering",
  ...CONTROLLED_MODEL_POLICY.primary,
  delegateTaskEnabled: false,
  taskBackend: "native-kanban",
};
const actor: ActorBinding = {
  actorId: "lead",
  profileId: profile.profileId,
  sessionId: "session-1",
  nativeRoot: profile.nativeRoot,
  role: "lead",
  userControlled: false,
  ...CONTROLLED_MODEL_POLICY.primary,
};
const task: NativeTaskRequirements = {
  task_id: "task-1",
  work_id: policy.workId,
  actor_id: actor.actorId,
  assignee: profile.profileId,
  model_override: actor.model as "gpt-6.1-sol-900k",
  provider_override: "openai-codex",
  reasoning_effort: "high",
  max_runtime_seconds: 600,
  max_retries: 1,
  idempotency_key: "task-key",
  completion_contract: "local-only",
  dispatch_backend: "native-kanban",
};
const session: NativeSessionReceipt = {
  session_id: actor.sessionId,
  profile_id: actor.profileId,
  actor_id: actor.actorId,
  native_root: actor.nativeRoot,
  source_ref: "source:session",
};
const run: NativeRunReceipt = {
  run_id: "run-1",
  task_id: task.task_id,
  session_id: actor.sessionId,
  assignee: actor.profileId,
  native_root: actor.nativeRoot,
  effective_model: actor.model,
  effective_provider: actor.provider,
  effective_effort: actor.effort,
  max_runtime_seconds: task.max_runtime_seconds,
  max_retries: task.max_retries,
  idempotency_key: task.idempotency_key,
  completion_contract: task.completion_contract,
  source_ref: "source:run",
};
const lunaScope: LunaExecutionScope = {
  destinationIds: ["destination"],
  inputsComplete: true,
  effect: "Copy supplied bytes to resolved destination",
  completion: "Independent byte digest matches supplied digest",
  substantiveJudgment: false,
  conflict: false,
  evidenceInterpretation: false,
  design: false,
  code: false,
  deletion: false,
  uncertainEffects: false,
};

describe("native actor and Task binding policy", () => {
  test("Main is exempt only with exact user-controlled identity", () => {
    const mainProfile = {
      ...profile,
      model: "user-selected-model",
      provider: "user-provider",
      effort: "user-selected-effort",
    };
    const main: ActorBinding = {
      ...actor,
      actorId: policy.mainActorId,
      sessionId: policy.mainSessionId,
      role: "main",
      userControlled: true,
      model: mainProfile.model,
      provider: mainProfile.provider,
      effort: mainProfile.effort,
    };
    expect(() => validateActorBinding(main, policy, mainProfile)).not.toThrow();
    expect(() =>
      validateActorBinding({ ...main, actorId: "forged-main" }, policy, mainProfile),
    ).toThrow("binding mismatch");
    expect(() =>
      validateActorBinding({ ...actor, sessionId: policy.mainSessionId }, policy, profile),
    ).toThrow("alias");
    expect(() =>
      validateNativeTask({ ...task, actor_id: main.actorId }, main, policy, mainProfile),
    ).toThrow("user-controlled");
  });
  test("controlled Lead and worker require exact explicit model/provider/effort", () => {
    expect(() => validateActorBinding(actor, policy, profile)).not.toThrow();
    for (const patch of [{ effort: "xhigh" }, { model: "gpt-6.1-sol" }, { provider: "fallback" }]) {
      expect(() =>
        validateActorBinding({ ...actor, ...patch }, policy, { ...profile, ...patch }),
      ).toThrow();
    }
    expect(() =>
      validateActorBinding(actor, policy, {
        ...profile,
        delegateTaskEnabled: true,
      } as unknown as ProfileBinding),
    ).toThrow();
  });
  test("Luna is eligible only for bounded worker execution", () => {
    const p = { ...profile, ...CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker };
    const a: ActorBinding = {
      ...actor,
      ...CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker,
      role: "worker",
      lunaScope,
    };
    expect(() => validateActorBinding(a, policy, p)).not.toThrow();
    expect(() => validateActorBinding({ ...a, role: "lead" }, policy, p)).toThrow();
    expect(() =>
      validateActorBinding(
        { ...a, lunaScope: { ...lunaScope, destinationIds: ["a", "b"] } },
        policy,
        p,
      ),
    ).toThrow();
    for (const key of [
      "substantiveJudgment",
      "conflict",
      "evidenceInterpretation",
      "design",
      "code",
      "deletion",
      "uncertainEffects",
    ] as const) {
      expect(() =>
        validateActorBinding({ ...a, lunaScope: { ...lunaScope, [key]: true } }, policy, p),
      ).toThrow(key);
    }
    expect(() =>
      validateActorBinding({ ...a, lunaScope: { ...lunaScope, inputsComplete: false } }, policy, p),
    ).toThrow();
  });
  test("no native Task field can fall back to inheritance or request_overrides", () => {
    expect(expectedNativeWorkerArgv(task, actor, policy, profile)).toEqual([
      "-m",
      "gpt-6.1-sol-900k",
      "--provider",
      "openai-codex",
      "--reasoning",
      "high",
    ]);
    for (const key of [
      "model_override",
      "provider_override",
      "reasoning_effort",
      "assignee",
      "max_runtime_seconds",
      "max_retries",
      "idempotency_key",
      "completion_contract",
    ]) {
      const missing = { ...task } as Record<string, unknown>;
      delete missing[key];
      expect(() => validateNativeTask(missing, actor, policy, profile)).toThrow();
    }
    expect(() =>
      validateNativeTask(
        { ...task, request_overrides: { reasoning_effort: "low" } },
        actor,
        policy,
        profile,
      ),
    ).toThrow("unrecognized");
    expect(() => validateNativeTask({ ...task, max_retries: 0 }, actor, policy, profile)).toThrow();
  });
  test("actual run receipt is mandatory and every binding is checked", () => {
    expect(verifyNativeExecution(task, actor, policy, profile, null, null).status).toBe("unknown");
    expect(verifyNativeExecution(task, actor, policy, profile, session, run).status).toBe(
      "verified",
    );
    for (const patch of [
      { effective_model: "fallback-model" },
      { effective_effort: "xhigh" },
      { assignee: "other" },
      { session_id: "other" },
      { task_id: "other" },
      { native_root: "other" },
      { max_retries: 4 },
      { completion_contract: "other" },
    ]) {
      expect(() =>
        verifyNativeExecution(task, actor, policy, profile, session, { ...run, ...patch }),
      ).toThrow("mismatch");
    }
    expect(() =>
      verifyNativeExecution(
        task,
        actor,
        policy,
        profile,
        { ...session, parent_session_id: policy.mainSessionId },
        run,
      ),
    ).toThrow("unrecognized");
  });
});

const botState: BotCorrelationState = {
  request: {
    workId: policy.workId,
    correlationId: "dm-correlation",
    senderActorId: actor.actorId,
    senderSessionId: actor.sessionId,
    recipientActorId: "worker",
    recipientProfileId: "worker-profile",
    nativeMessageId: "dm-1",
    sourceRef: "source:bot-request",
  },
  observations: [],
};
function botObservation(kind: string, extra = {}): BotCorrelationObservation {
  return {
    observationId: kind + "-receipt",
    correlationId: botState.request.correlationId,
    nativeMessageId: botState.request.nativeMessageId,
    atUtc: now,
    sourceRef: "source:bot-observation",
    kind,
    ...extra,
  } as BotCorrelationObservation;
}
describe("asynchronous native Bot DM correlation", () => {
  test("settled delivery never establishes a Task, Evaluation or AcceptanceDecision", () => {
    let state = reconcileBotObservation(botState, botObservation("queued"));
    state = reconcileBotObservation(
      state,
      botObservation("claimed", { recipientSessionId: "recipient-1" }),
    );
    state = reconcileBotObservation(
      state,
      botObservation("settled", { recipientSessionId: "recipient-1" }),
    );
    expect(projectBotCorrelation(state)).toMatchObject({
      delivery: "settled",
      recipientSessionId: "recipient-1",
      taskBindings: [],
      evaluationRefs: [],
      acceptanceDecisionRefs: [],
    });
    expect(reconcileBotObservation(state, state.observations[0]!)).toEqual(state);
  });
  test("conflicting native recipient claims remain ambiguous after settle", () => {
    let state = reconcileBotObservation(
      botState,
      botObservation("claimed", { recipientSessionId: "recipient-1" }),
    );
    state = reconcileBotObservation(
      state,
      botObservation("settled", { recipientSessionId: "recipient-2" }),
    );
    expect(projectBotCorrelation(state).delivery).toBe("ambiguous");
  });
  test("Task, Evaluation and AcceptanceDecision need separate matching observations", () => {
    const binding = { taskId: "task-2", runId: "run-2" };
    expect(() =>
      reconcileBotObservation(
        botState,
        botObservation("acceptance", {
          ...binding,
          evaluationRef: "eval",
          acceptanceDecisionRef: "accept",
        }),
      ),
    ).toThrow("Task/Run");
    let state = reconcileBotObservation(
      botState,
      botObservation("task", { ...binding, taskStatus: "completed" }),
    );
    expect(projectBotCorrelation(state).acceptanceDecisionRefs).toEqual([]);
    expect(() =>
      reconcileBotObservation(
        state,
        botObservation("acceptance", {
          ...binding,
          evaluationRef: "eval",
          acceptanceDecisionRef: "accept",
        }),
      ),
    ).toThrow("separately");
    state = reconcileBotObservation(
      state,
      botObservation("evaluation", { ...binding, evaluationRef: "eval" }),
    );
    state = reconcileBotObservation(
      state,
      botObservation("acceptance", {
        ...binding,
        evaluationRef: "eval",
        acceptanceDecisionRef: "accept",
      }),
    );
    expect(projectBotCorrelation(state)).toMatchObject({
      delivery: "unknown",
      evaluationRefs: ["eval"],
      acceptanceDecisionRefs: ["accept"],
    });
  });
  test("rejects fake lineage, wrong DM, history regression and conflicting idempotency", () => {
    expect(() =>
      reconcileBotObservation(
        {
          ...botState,
          request: { ...botState.request, parent_session_id: "invented" },
        } as BotCorrelationState,
        botObservation("queued"),
      ),
    ).toThrow("unrecognized");
    expect(() =>
      reconcileBotObservation(botState, { ...botObservation("queued"), nativeMessageId: "other" }),
    ).toThrow("mismatch");
    const claimed = reconcileBotObservation(
      botState,
      botObservation("claimed", { recipientSessionId: "recipient-1" }),
    );
    expect(() => reconcileBotObservation(claimed, botObservation("queued"))).toThrow("regress");
    expect(() =>
      reconcileBotObservation(claimed, { ...claimed.observations[0]!, sourceRef: "different" }),
    ).toThrow("conflicting");
  });
});

const boundary: CustodyBoundary = {
  boundaryId: "boundary-1",
  kind: "delegation",
  atUtc: now,
  sourceRefs: ["source:decision"],
  completedEffectRefs: ["effect:done"],
  pendingWorkRefs: ["work:pending"],
  decisionRefs: ["decision:current"],
  nativeTaskRunRefs: ["task:1/run:1"],
};
const checkpoint: NativeCheckpointSnapshot = {
  ownerId: "native-checkpoint-owner",
  workId: policy.workId,
  checkpointId: "checkpoint-1",
  revision: 0,
  digest: "opaque-owner-digest",
  boundaries: [],
  missedHookBoundaryIds: [],
};
describe("boundary custody with native owner CAS", () => {
  test("plans bounded CAS, verifies independent readback and is idempotent", () => {
    const plan = planBoundaryCustody(checkpoint, boundary);
    expect(plan.status).toBe("cas-required");
    if (plan.status !== "cas-required") throw new Error("fixture");
    expect(plan.request).toMatchObject({
      expectedRevision: 0,
      expectedDigest: checkpoint.digest,
      ownerId: checkpoint.ownerId,
    });
    const after = {
      ...checkpoint,
      revision: 1,
      digest: "new-owner-digest",
      boundaries: [boundary],
    };
    expect(() => verifyBoundaryCustodyReadback(checkpoint, plan.request, after)).not.toThrow();
    expect(planBoundaryCustody(after, boundary).status).toBe("already-recorded");
    expect(() => planBoundaryCustody(after, { ...boundary, pendingWorkRefs: [] })).toThrow(
      "conflicting",
    );
    expect(checkpoint.boundaries).toEqual([]);
  });
  test("third-state changes cannot rewrite previously preserved effects", () => {
    const before = { ...checkpoint, revision: 1, boundaries: [boundary] };
    const nextBoundary = { ...boundary, boundaryId: "boundary-2", kind: "handoff" as const };
    const plan = planBoundaryCustody(before, nextBoundary);
    if (plan.status !== "cas-required") throw new Error("fixture");
    const bad = {
      ...before,
      revision: 2,
      digest: "changed",
      boundaries: [{ ...boundary, completedEffectRefs: [] }, nextBoundary],
    };
    expect(() => verifyBoundaryCustodyReadback(before, plan.request, bad)).toThrow("rewrote");
    expect(() =>
      verifyBoundaryCustodyReadback(
        before,
        { ...plan.request, expectedDigest: "stale" },
        { ...bad, boundaries: [boundary, nextBoundary] },
      ),
    ).toThrow("predecessor");
  });
  test("hook failures are exposed and reconciled without pretending hooks block execution", () => {
    expect(exposeCustodyGaps(checkpoint, ["boundary-1"], ["boundary-1"])).toEqual({
      missedBoundaries: ["boundary-1"],
      failOpenHookFailures: ["boundary-1"],
      reconciledFailedHookBoundaries: [],
      custodyComplete: false,
    });
    const after = { ...checkpoint, boundaries: [boundary], missedHookBoundaryIds: ["boundary-1"] };
    expect(exposeCustodyGaps(after, ["boundary-1"], [])).toMatchObject({
      missedBoundaries: [],
      failOpenHookFailures: ["boundary-1"],
      reconciledFailedHookBoundaries: ["boundary-1"],
      custodyComplete: true,
    });
  });
});

function admission(id = "request-1", patch: Partial<RequestAdmission> = {}): RequestAdmission {
  return {
    admissionId: id,
    taskId: task.task_id,
    profileId: profile.profileId,
    kind: "primary",
    reservedInputTokens: 1000,
    reservedOutputTokens: 1000,
    selection: { ...CONTROLLED_MODEL_POLICY.primary },
    expectedWireModel: "gpt-6.1-sol",
    deterministicUtilityConsidered: true,
    requiresModelReason: "Interpret bounded engineering inputs",
    ...patch,
  };
}
function observation(patch: Partial<NativeRequestObservation> = {}): NativeRequestObservation {
  return {
    admissionId: "request-1",
    atUtc: later,
    nativeRequestId: "provider-request-1",
    nativeRunId: run.run_id,
    taskId: task.task_id,
    profileId: profile.profileId,
    status: "completed",
    usageCompleteness: "complete",
    usage: {
      inputTokens: 100,
      outputTokens: 50,
      cacheReadTokens: 20,
      inputAccounting: "includes-cache",
    },
    effectiveWireModel: "gpt-6.1-sol",
    effectiveProvider: "openai-codex",
    effectiveEffort: "high",
    sourceRef: "source:native-request",
    ...patch,
  };
}
function suite() {
  return createNativeEvaluationSuite(
    "suite-new-48",
    policy.workId,
    "native-owner",
    ["suite-old-closed"],
    now,
  );
}
describe("independent native model request budget", () => {
  test("old closed suite cannot reopen, caps cannot be patched and deterministic utilities come first", () => {
    expect(() =>
      createNativeEvaluationSuite(
        "suite-old-closed",
        policy.workId,
        "native-owner",
        ["suite-old-closed"],
        now,
      ),
    ).toThrow("reopened");
    expect(() =>
      reserveNativeModelRequest(
        suite(),
        admission("r", { deterministicUtilityConsidered: false as true }),
        now,
      ),
    ).toThrow();
    expect(() =>
      validateNativeEvaluationSuite(
        { ...suite(), limits: { maxRuns: 999 } } as ReturnType<typeof suite>,
        now,
      ),
    ).toThrow("unrecognized");
  });
  test("child, retry and auxiliary requests all consume separate slots and aggregate native usage without cache double-counting", () => {
    let s = reserveNativeModelRequest(suite(), admission(), now);
    for (const kind of ["child", "retry", "auxiliary"] as const)
      s = reserveNativeModelRequest(
        s,
        admission(kind, { kind, parentAdmissionId: "request-1" }),
        now,
      );
    s = reconcileNativeModelRequest(s, observation(), later);
    const result = validateNativeEvaluationSuite(s, later);
    expect(result.admittedRuns).toBe(4);
    expect(result.aggregateTokens).toBe(150);
    expect(result.reservedTokens).toBe(6000);
    expect(() =>
      reserveNativeModelRequest(s, admission("unbound-child", { kind: "child" }), later),
    ).toThrow("lineage");
  });
  test("partial usage preserves emitted counters and blocks until complete reconciliation", () => {
    let s = reserveNativeModelRequest(suite(), admission(), now);
    s = reconcileNativeModelRequest(
      s,
      observation({ status: "running", usageCompleteness: "partial" }),
      later,
    );
    expect(validateNativeEvaluationSuite(s, later)).toMatchObject({
      aggregateTokens: 150,
      unknownUsageRuns: 1,
    });
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).toThrow("unknown-usage");
    s = reconcileNativeModelRequest(
      s,
      observation({
        usage: { inputTokens: 200, outputTokens: 100, inputAccounting: "includes-cache" },
      }),
      later,
    );
    expect(validateNativeEvaluationSuite(s, later)).toMatchObject({
      aggregateTokens: 300,
      unknownUsageRuns: 0,
    });
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).not.toThrow();
  });
  test("unknown effective runtime and wrong routing preserve usage and stop additional dispatch", () => {
    let s = reserveNativeModelRequest(suite(), admission(), now);
    s = reconcileNativeModelRequest(
      s,
      observation({ effectiveWireModel: "fallback-model", effectiveEffort: null }),
      later,
    );
    expect(validateNativeEvaluationSuite(s, later).aggregateTokens).toBe(150);
    expect(s.policyViolations).toContain("request-1: effectiveWireModel mismatch");
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).toThrow(
      "policy violation",
    );
  });
  test("run, token and time limits stop admission while late actual overrun is retained", () => {
    let s = suite();
    for (let i = 0; i < 48; i++) s = reserveNativeModelRequest(s, admission("request-" + i), now);
    expect(() => reserveNativeModelRequest(s, admission("too-many"), now)).toThrow("run-cap");
    let overrun = reserveNativeModelRequest(suite(), admission(), now);
    overrun = reconcileNativeModelRequest(
      overrun,
      observation({
        usage: { inputTokens: 1_500_000, outputTokens: 600_000, inputAccounting: "includes-cache" },
      }),
      later,
    );
    expect(validateNativeEvaluationSuite(overrun, later)).toMatchObject({
      aggregateTokens: 2_100_000,
      overrun: { tokens: true },
    });
    expect(() => reserveNativeModelRequest(overrun, admission("next"), later)).toThrow("token-cap");
    expect(() =>
      reserveNativeModelRequest(
        reserveNativeModelRequest(suite(), admission(), now),
        admission("next"),
        "2026-10-11T08:00:00.000Z",
      ),
    ).toThrow("deadline");
  });
  test("one native provider request cannot consume two admissions", () => {
    let s = reserveNativeModelRequest(suite(), admission(), now);
    s = reserveNativeModelRequest(s, admission("request-2"), now);
    s = reconcileNativeModelRequest(s, observation(), later);
    expect(() =>
      reconcileNativeModelRequest(s, observation({ admissionId: "request-2" }), later),
    ).toThrow("multiple");
  });
  test("Luna accounting entrypoint also enforces execution scope", () => {
    const luna = admission("luna", {
      selection: { ...CONTROLLED_MODEL_POLICY.narrowlyEligibleWorker },
      expectedWireModel: "gpt-6-luna",
    });
    expect(() => reserveNativeModelRequest(suite(), luna, now)).toThrow();
    expect(() => reserveNativeModelRequest(suite(), { ...luna, lunaScope }, now)).not.toThrow();
    expect(() =>
      reserveNativeModelRequest(
        suite(),
        { ...luna, lunaScope: { ...lunaScope, evidenceInterpretation: true } },
        now,
      ),
    ).toThrow();
  });
  test("unknown effective receipt can be resolved without treating incomplete telemetry as zero", () => {
    let s = reserveNativeModelRequest(suite(), admission(), now);
    s = reconcileNativeModelRequest(s, observation({ effectiveEffort: null }), later);
    expect(validateNativeEvaluationSuite(s, later).admissionBlockedReasons).toContain(
      "unknown-usage",
    );
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).toThrow("unknown-usage");
    s = reconcileNativeModelRequest(s, observation(), later);
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).not.toThrow();
    expect(reconcileNativeModelRequest(s, observation(), later)).toEqual(s);
  });
  test("late partial telemetry remains blocking after a terminal execution status", () => {
    let s = reconcileNativeModelRequest(
      reserveNativeModelRequest(suite(), admission(), now),
      observation(),
      later,
    );
    s = reconcileNativeModelRequest(s, observation({ usageCompleteness: "partial" }), later);
    expect(validateNativeEvaluationSuite(s, later).unknownUsageRuns).toBe(1);
    expect(() => reserveNativeModelRequest(s, admission("next"), later)).toThrow("unknown-usage");
  });
  test("native accounting cannot lose its request provenance history", () => {
    const s = reconcileNativeModelRequest(
      reserveNativeModelRequest(suite(), admission(), now),
      observation(),
      later,
    );
    expect(() => validateNativeEvaluationSuite({ ...s, requestObservations: [] }, later)).toThrow(
      "histories",
    );
    expect(() =>
      validateNativeEvaluationSuite(
        {
          ...s,
          requestObservations: [
            {
              ...s.requestObservations[0]!,
              usage: { inputTokens: 999, outputTokens: 50, inputAccounting: "includes-cache" },
            },
          ],
        },
        later,
      ),
    ).toThrow("usage differs");
  });
  test("persisted native route and task drift cannot erase a dispatch-blocking violation", () => {
    const s = reconcileNativeModelRequest(
      reserveNativeModelRequest(suite(), admission(), now),
      observation(),
      later,
    );
    for (const patch of [
      { effectiveWireModel: "unadmitted-model" },
      { taskId: "different-task" },
      { profileId: "different-profile" },
      { effectiveEffort: "xhigh" },
    ]) {
      const changed = {
        ...s,
        policyViolations: [],
        requestObservations: [{ ...s.requestObservations[0]!, ...patch }],
      };
      expect(() => validateNativeEvaluationSuite(changed, later)).toThrow("policy violation");
      expect(() => reserveNativeModelRequest(changed, admission("next"), later)).toThrow(
        "policy violation",
      );
    }
  });
});
