import { describe, test, expect } from "bun:test";
import {
  JEV_MODEL,
  ABSTAIN_ID,
  semanticEvents,
  semanticPurposes,
  semanticDigest,
  prepareSemanticDecision,
  prepareChildSemanticPacket,
  normalizeSemanticReply,
  consumeReadonlySelection,
  type NativeBinding,
  type WholeLens,
  type SemanticEvent,
  type ReadonlyCandidate,
  type PreparedDecision,
  type CurrentReadBinding,
} from "./index.ts";

const binding = (): NativeBinding => ({
  runtime: "native-codex",
  sessionId: "native-root",
  agentId: null,
  agentType: null,
  parentSessionId: null,
  rootSessionId: "native-root",
  lineageSessionIds: ["native-root"],
  profileId: null,
  nativeTaskId: null,
  nativeRunId: null,
  logical: null,
  admissionRef: "native-owner:admission:root",
});
const lens = (): WholeLens => ({
  currentAuthority: {
    text: "User authorizes bounded readonly evidence selection.",
    sourceRefs: ["user-input:current"],
  },
  wholeObjective: "Useful native integration preserving accepted meaning and original owners.",
  taskGoal: "Read current source and verify the bounded child result.",
  acceptedWhy: [
    {
      text: "Keep original reasons and ownership to prevent drift after handoff.",
      attribution: "current-user",
      sourceRefs: ["user-input:why"],
    },
  ],
  acceptedDecisions: [
    { id: "decision:source-owner", revision: "v2", sourceRefs: ["input:owner-v2"] },
  ],
  owners: [
    { role: "native", id: "codex" },
    { role: "domain", id: "hermes-default" },
  ],
  dependencies: [{ id: "common-source", revision: "source-v1", currentness: "current" }],
  completedEffectRefs: ["effect:consumed-receive"],
  uncertainEffectRefs: ["effect:unknown-promotion"],
  remainingAcceptance: ["parent:native-use", "parent:whole-delivery"],
});
const event = (): SemanticEvent => ({
  eventId: "event:1",
  kind: "tool_result",
  binding: binding(),
  sourceRevision: "source-v1",
  materialRevision: "result-v1",
  material: true,
  privacy: "admitted",
  externalEvidence: false,
  facts: {
    checkPassed: false,
    error: "Source candidate is complete but native use is unverified.",
  },
});
const candidate = (): ReadonlyCandidate => ({
  candidateId: "read-source",
  kind: "read",
  effectClass: "read_only",
  description: "Read the exact current native source/adoption receipt.",
  handlerKey: "native.read-source",
  targetId: "source:current",
  parameters: { section: "native-use" },
  preconditionDigest: "source-v1-target-v1",
  authorizationRef: "user-input:current",
  sourceRefs: ["source:current"],
});
function prepare(
  overrides: Partial<Parameters<typeof prepareSemanticDecision>[0]> = {},
): PreparedDecision {
  const result = prepareSemanticDecision({
    requestId: "request:1",
    event: event(),
    lens: lens(),
    candidates: [candidate()],
    ...overrides,
  });
  if (result.status !== "prepared") throw Error("expected prepared fixture");
  return result;
}
function reply(p: PreparedDecision, selected = "read-source"): Record<string, unknown> {
  return {
    schema: "jev.semantic-result.v1",
    requestId: p.request.requestId,
    purpose: "semantic_bundle",
    stateDigest: p.request.stateDigest,
    requestedModel: JEV_MODEL,
    returnedModel: JEV_MODEL,
    status: "ok",
    sourcePostconditionVerified: true,
    executionAuthorityGranted: false,
    automaticWrite: false,
    answers: Object.fromEntries(
      Object.entries(p.request.questions).map(([key, q]) => [
        key,
        q.type === "noul"
          ? { type: "noul", noul: 0.12 }
          : {
              type: "choice",
              choice: selected,
              confidence: 0.9,
              probabilities: Object.fromEntries(
                Object.keys(q.criteria).map((id) => [id, id === selected ? 1 : 0]),
              ),
            },
      ]),
    ),
  };
}
const current = (p: PreparedDecision): CurrentReadBinding => ({
  authorized: true,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  candidateDigest: p.candidateDigests["read-source"]!,
  authorizationRef: "user-input:current",
  evidenceRefs: ["native:fresh-precondition"],
});

describe("native scope and inherited whole context", () => {
  test("root, child and nested child retain nullable actual native IDs and all governing content", () => {
    const root = binding(),
      child = {
        ...root,
        sessionId: "native-child",
        parentSessionId: root.sessionId,
        lineageSessionIds: [root.sessionId, "native-child"],
        admissionRef: "native:child",
      };
    const first = prepareChildSemanticPacket({
      parentBinding: root,
      childBinding: child,
      parentLens: lens(),
      childTaskGoal: "Deliver source-only patch.",
    });
    const grand = {
      ...child,
      sessionId: "native-grandchild",
      parentSessionId: child.sessionId,
      lineageSessionIds: [...child.lineageSessionIds, "native-grandchild"],
      admissionRef: "native:grandchild",
    };
    const packet = prepareChildSemanticPacket({
      parentBinding: child,
      childBinding: grand,
      parentLens: first.lens,
      childTaskGoal: "Read a bounded source slice.",
    });
    expect(packet.lens.wholeObjective).toBe(lens().wholeObjective);
    expect(packet.lens.acceptedWhy).toEqual(lens().acceptedWhy);
    expect(packet.lens.owners).toEqual(lens().owners);
    expect(packet.lens.uncertainEffectRefs).toEqual(lens().uncertainEffectRefs);
    expect(packet.lens.remainingAcceptance).toEqual(lens().remainingAcceptance);
    expect(packet.parentTaskGoal).toBe("Deliver source-only patch.");
    expect(packet.binding.agentId).toBeNull();
    expect(packet.binding.logical).toBeNull();
    expect(packet.lens.taskGoal).toBe("Read a bounded source slice.");
  });
  test("foreign or invented lineage cannot inherit a packet", () => {
    const child = {
      ...binding(),
      sessionId: "foreign",
      parentSessionId: "another-root",
      lineageSessionIds: ["native-root", "foreign"],
    };
    expect(() =>
      prepareChildSemanticPacket({
        parentBinding: binding(),
        childBinding: child,
        parentLens: lens(),
        childTaskGoal: "Task",
      }),
    ).toThrow();
    const broken = {
      ...event(),
      binding: { ...binding(), agentId: undefined },
    } as unknown as SemanticEvent;
    expect(() => prepare({ event: broken })).toThrow("EXPLICIT_NATIVE_REFERENCE_REQUIRED");
  });
  test("required original WHY cannot be dropped to meet a size cap", () => {
    const broken = lens();
    broken.acceptedWhy = [];
    expect(() => prepare({ lens: broken })).toThrow("MANDATORY_WHOLE_LENS_CONTEXT_REQUIRED");
    const large = lens();
    large.acceptedWhy = Array.from({ length: 32 }, () => ({
      text: "x".repeat(4000),
      attribution: "current-user" as const,
      sourceRefs: ["original:why"],
    }));
    expect(() => prepare({ lens: large })).toThrow("MANDATORY_CONTEXT_OUTPUT_HELD");
  });
});

describe("material semantic eligibility and general-purpose bundles", () => {
  test("every material event has a semantic path and no fixture-specific purposes", () => {
    for (const kind of semanticEvents.filter((kind) => kind !== "carrier")) {
      const p = prepare({ event: { ...event(), kind } });
      expect(Object.keys(p.request.questions).length).toBeGreaterThanOrEqual(3);
      expect(semanticDigest(p.request.state.requiredContext)).toBe(semanticDigest(lens()));
      expect(p.request.questions[p.selectionQuestionId!]!.type).toBe("choice");
    }
    const p = prepare({ purposes: semanticPurposes });
    expect(Object.keys(p.request.questions)).toHaveLength(15);
    expect(p.request.scope.why).toContain("original reasons");
  });
  test("privacy, carrier and unchanged meaning resolve in code, not inference", () => {
    expect(
      prepareSemanticDecision({
        requestId: "r",
        event: { ...event(), privacy: "private" },
        lens: lens(),
        candidates: [],
      }),
    ).toMatchObject({ status: "skipped", reason: "private" });
    expect(
      prepareSemanticDecision({
        requestId: "r",
        event: { ...event(), kind: "carrier" },
        lens: lens(),
        candidates: [],
      }),
    ).toMatchObject({ status: "skipped", reason: "carrier" });
    expect(
      prepareSemanticDecision({
        requestId: "r",
        event: { ...event(), material: false },
        lens: lens(),
        candidates: [],
      }),
    ).toMatchObject({ status: "skipped", reason: "not-material" });
    const p = prepare();
    expect(
      prepareSemanticDecision({
        requestId: "new-request",
        event: { ...event(), eventId: "new-event" },
        lens: lens(),
        candidates: [candidate()],
        previousConsumedReuseKey: p.reuseKey,
      }),
    ).toMatchObject({ status: "skipped", reason: "unchanged" });
  });
  test("current decisions, candidate parameters, source and mandatory dependencies invalidate reuse", () => {
    const p = prepare();
    const changed = lens();
    changed.acceptedDecisions[0]!.revision = "v3";
    expect(prepare({ lens: changed }).reuseKey).not.toBe(p.reuseKey);
    expect(prepare({ event: { ...event(), sourceRevision: "source-v2" } }).reuseKey).not.toBe(
      p.reuseKey,
    );
    expect(
      prepare({ candidates: [{ ...candidate(), parameters: { section: "different" } }] }).reuseKey,
    ).not.toBe(p.reuseKey);
    const deps = lens();
    deps.dependencies[0]!.currentness = "unknown";
    expect(prepare({ lens: deps }).reuseKey).not.toBe(p.reuseKey);
  });
  test("external screening is included independently and no generated shell candidate is accepted", () => {
    const p = prepare({ event: { ...event(), externalEvidence: true } });
    expect(Object.values(p.questionMeanings)).toContain("external_screening");
    expect(() =>
      prepare({
        candidates: [{ ...candidate(), effectClass: "write" } as unknown as ReadonlyCandidate],
      }),
    ).toThrow("FIXED_READ_ONLY_CANDIDATE_REQUIRED");
    expect(() => prepare({ candidates: [{ ...candidate(), handlerKey: null }] })).toThrow(
      "NATIVE_READ_HANDLER_AUTHORIZATION_REQUIRED",
    );
    expect(() => prepare({ event: { ...event(), facts: { score: Number.NaN } } })).toThrow(
      "SEMANTIC_JSON_REQUIRED",
    );
  });
});

describe("exact answer and observed read gate", () => {
  test("Noul advice does not grant child, parent or Goal completion", () => {
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    expect(decision.executionAuthorityGranted).toBe(false);
    expect(decision.judgments.task_scope_satisfaction).toBe(0.12);
    expect("completed" in decision).toBe(false);
  });
  test("mismatched model, state, postcondition, coverage, candidate and numeric values refuse", () => {
    const p = prepare();
    for (const patch of [
      { returnedModel: "another" },
      { stateDigest: "stale" },
      { sourcePostconditionVerified: false },
      { automaticWrite: true },
      { executionAuthorityGranted: true },
      { answers: {} },
    ])
      expect(() => normalizeSemanticReply(p, { ...reply(p), ...patch })).toThrow();
    expect(() => normalizeSemanticReply(p, reply(p, "unoffered"))).toThrow();
    const r = reply(p);
    (r.answers as Record<string, unknown>).q0 = { type: "noul", noul: 1.1 };
    expect(() => normalizeSemanticReply(p, r)).toThrow("SEMANTIC_NOUL_RANGE");
  });
  test("an empty or inadequate candidate set can explicitly abstain without executing anything", async () => {
    const p = prepare({ candidates: [] }),
      decision = normalizeSemanticReply(p, reply(p, ABSTAIN_ID));
    let observations = 0;
    const out = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map(),
      observeCurrent: async () => {
        observations++;
        return current(p);
      },
    });
    expect(out).toMatchObject({ status: "abstained", dispatched: false, retryAllowed: false });
    expect(observations).toBe(0);
  });
  test("fresh fixed native read returns exact observed postcondition, not completion", async () => {
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    let count = 0;
    const out = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map([
        [
          "native.read-source",
          async (c: ReadonlyCandidate) => {
            count++;
            expect(c.targetId).toBe("source:current");
            return {
              status: "observed" as const,
              observationId: "native:observation:1",
              evidenceRefs: ["native:exact-result"],
              postconditionSatisfied: true,
            };
          },
        ],
      ]),
      observeCurrent: async () => current(p),
    });
    expect(count).toBe(1);
    expect(out).toMatchObject({
      status: "verified-read",
      dispatched: true,
      executionAuthorityGranted: false,
      taskCompletionGranted: false,
      retryAllowed: false,
    });
  });
  test("stale resources, unauthorized reads and missing handlers block before dispatch", async () => {
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    let count = 0;
    const handlers = new Map([
      [
        "native.read-source",
        async () => {
          count++;
          throw Error("must not execute");
        },
      ],
    ]);
    for (const patch of [
      { authorized: false },
      { sourceRevision: "other" },
      { bindingDigest: "foreign" },
      { candidateDigest: "changed" },
      { authorizationRef: "other" },
    ]) {
      const out = await consumeReadonlySelection({
        prepared: p,
        decision,
        handlers,
        observeCurrent: async () => ({ ...current(p), ...patch }),
      });
      expect(out).toMatchObject({ status: "held", dispatched: false });
    }
    expect(count).toBe(0);
    expect(
      await consumeReadonlySelection({
        prepared: p,
        decision,
        handlers: new Map(),
        observeCurrent: async () => current(p),
      }),
    ).toMatchObject({ status: "held", dispatched: false });
  });
  test("mutated parameters are detected; handler errors and missing postconditions are never retried", async () => {
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    const out = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map([
        [
          "native.read-source",
          async () => {
            throw Error("timeout");
          },
        ],
      ]),
      observeCurrent: async () => current(p),
    });
    expect(out).toMatchObject({ status: "read-unknown", dispatched: true, retryAllowed: false });
    p.candidates[0]!.parameters.section = "mutated";
    expect(
      await consumeReadonlySelection({
        prepared: p,
        decision,
        handlers: new Map(),
        observeCurrent: async () => current(p),
      }),
    ).toMatchObject({ status: "held", reason: "candidate-drift" });
  });
  test("post-read source drift keeps the observation but refuses verified success", async () => {
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    let count = 0;
    const out = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map([
        [
          "native.read-source",
          async () => ({
            status: "observed" as const,
            observationId: "observation:read",
            evidenceRefs: ["receipt:read"],
            postconditionSatisfied: true,
          }),
        ],
      ]),
      observeCurrent: async () => ({
        ...current(p),
        sourceRevision: ++count === 1 ? p.sourceRevision : "changed",
      }),
    });
    expect(out).toMatchObject({
      status: "read-unknown",
      reason: "post-read-currentness-changed",
      dispatched: true,
    });
    expect(out.observation?.observationId).toBe("observation:read");
  });
  test("canonical state hashing ignores object key order and accepts actual fractional facts", () => {
    expect(semanticDigest({ b: 0.17, a: [1, null] })).toBe(
      semanticDigest({ a: [1, null], b: 0.17 }),
    );
    expect(() => semanticDigest({ missing: undefined })).toThrow();
  });
});

test("no-candidate event keeps semantic questions but does not submit an invalid one-option Choice", () => {
  const p = prepare({ candidates: [] });
  expect(p.selectionQuestionId).toBeNull();
  expect(Object.values(p.request.questions).every((q) => q.type === "noul")).toBe(true);
  const decision = normalizeSemanticReply(p, reply(p));
  expect(decision.selectedCandidateId).toBe(ABSTAIN_ID);
  expect(decision.nativeChoiceConfidence).toBeNull();
  expect(decision.nativeChoiceProbabilities).toBeNull();
  expect(decision.selectionReason).toBe("no-candidates");
});
test("near-uniform native Choice probabilities survive normalization without a threshold or reply alias", () => {
  const p = prepare(),
    r = reply(p);
  const probabilities = { [ABSTAIN_ID]: 0.4999, "read-source": 0.5001 };
  (r.answers as Record<string, unknown>)[p.selectionQuestionId!] = {
    type: "choice",
    choice: "read-source",
    confidence: 0.0002,
    probabilities,
  };
  const decision = normalizeSemanticReply(p, r);
  expect(decision.selectedCandidateId).toBe("read-source");
  expect(decision.selectionReason).toBe("model-selection");
  expect(decision.nativeChoiceConfidence).toBe(0.0002);
  expect(decision.nativeChoiceProbabilities).toEqual(probabilities);
  expect(decision.nativeChoiceProbabilities).not.toBe(probabilities);
  expect(JSON.parse(JSON.stringify(decision)).nativeChoiceProbabilities).toEqual(probabilities);
  probabilities["read-source"] = 0;
  expect(decision.nativeChoiceProbabilities?.["read-source"]).toBe(0.5001);
  expect(decision.judgments.task_scope_satisfaction).toBe(0.12);
  expect(decision.executionAuthorityGranted).toBe(false);
});
test("explicit model abstention retains its native distribution and distinct provenance", async () => {
  const p = prepare(),
    decision = normalizeSemanticReply(p, reply(p, ABSTAIN_ID));
  expect(decision.nativeChoiceProbabilities).toEqual({ "read-source": 0, [ABSTAIN_ID]: 1 });
  expect(decision.selectionReason).toBe("model-abstained");
  let observations = 0;
  expect(
    await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map(),
      observeCurrent: async () => {
        observations++;
        return current(p);
      },
    }),
  ).toMatchObject({ status: "abstained", dispatched: false, retryAllowed: false });
  expect(observations).toBe(0);
});
test("exact tied distributions abstain before any readonly execution without inventing a threshold", async () => {
  const p = prepare(),
    r = reply(p);
  (r.answers as Record<string, unknown>)[p.selectionQuestionId!] = {
    type: "choice",
    choice: "read-source",
    confidence: 0,
    probabilities: { "read-source": 0.5, [ABSTAIN_ID]: 0.5 },
  };
  const decision = normalizeSemanticReply(p, r);
  expect(decision.selectedCandidateId).toBe(ABSTAIN_ID);
  expect(decision.nativeChoiceProbabilities).toEqual({ "read-source": 0.5, [ABSTAIN_ID]: 0.5 });
  expect(decision.nativeChoiceConfidence).toBe(0);
  expect(decision.selectionReason).toBe("tied-distribution");
  expect(
    await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map(),
      observeCurrent: async () => current(p),
    }),
  ).toMatchObject({ status: "abstained", dispatched: false });
});
test("a precondition callback cannot change target parameters before the fixed handler", async () => {
  const p = prepare(),
    decision = normalizeSemanticReply(p, reply(p));
  let dispatches = 0;
  const out = await consumeReadonlySelection({
    prepared: p,
    decision,
    handlers: new Map([
      [
        "native.read-source",
        async () => {
          dispatches++;
          throw Error("must not execute");
        },
      ],
    ]),
    observeCurrent: async (c) => {
      c.parameters.section = "wrong-target";
      return current(p);
    },
  });
  expect(out).toMatchObject({ status: "held", reason: "candidate-changed-during-precondition" });
  expect(dispatches).toBe(0);
});
test("prepared context or questions cannot change between request, reply and consumption", async () => {
  const p = prepare(),
    r = reply(p),
    decision = normalizeSemanticReply(p, r);
  p.request.scope.taskGoal = "Different task";
  expect(() => normalizeSemanticReply(p, r)).toThrow("PREPARED_SEMANTIC_REQUEST_DRIFT");
  expect(
    await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map(),
      observeCurrent: async () => current(p),
    }),
  ).toMatchObject({ status: "held", reason: "decision-binding-mismatch" });
});
test("a handler exit or claimed success without observed postcondition cannot verify the read", async () => {
  const p = prepare(),
    decision = normalizeSemanticReply(p, reply(p));
  const out = await consumeReadonlySelection({
    prepared: p,
    decision,
    handlers: new Map([
      [
        "native.read-source",
        async () => ({
          status: "observed" as const,
          observationId: "native:read",
          evidenceRefs: [],
          postconditionSatisfied: false,
        }),
      ],
    ]),
    observeCurrent: async () => current(p),
  });
  expect(out).toMatchObject({
    status: "read-failed",
    dispatched: true,
    taskCompletionGranted: false,
  });
});

test("native exact1.14 documented model alias preserves provenance without changing requested model", () => {
  const p = prepare();
  const decision = normalizeSemanticReply(p, { ...reply(p), returnedModel: "jev-1.14.0" });
  expect(decision.selectedCandidateId).toBe("read-source");
  expect(() => normalizeSemanticReply(p, { ...reply(p), returnedModel: "jev-1.13.0" })).toThrow();
  expect(() => normalizeSemanticReply(p, { ...reply(p), requestedModel: "jev-1.14.0" })).toThrow();
});
test("postcondition callback mutation retains the actual read but cannot return verified-read", async () => {
  const p = prepare(),
    decision = normalizeSemanticReply(p, reply(p));
  let observations = 0;
  const out = await consumeReadonlySelection({
    prepared: p,
    decision,
    handlers: new Map([
      [
        "native.read-source",
        async () => ({
          status: "observed" as const,
          observationId: "read:observed",
          evidenceRefs: ["read:receipt"],
          postconditionSatisfied: true,
        }),
      ],
    ]),
    observeCurrent: async (c) => {
      observations++;
      if (observations === 2) c.parameters.section = "postcheck-mutated";
      return current(p);
    },
  });
  expect(out).toMatchObject({
    status: "read-unknown",
    dispatched: true,
    reason: "candidate-changed-during-postcondition",
  });
  expect(out.observation?.observationId).toBe("read:observed");
  expect(out.retryAllowed).toBe(false);
});

test("actual task and whole objective retain12000-character native scope without4096 clipping", () => {
  const actual = lens();
  actual.taskGoal = "g".repeat(6000);
  actual.wholeObjective = "w".repeat(6000);
  const p = prepare({ lens: actual });
  expect(p.request.scope.taskGoal).toBe(actual.taskGoal);
  expect(p.request.scope.wholeObjective).toBe(actual.wholeObjective);
  const root = binding(),
    child = {
      ...root,
      sessionId: "native-child",
      parentSessionId: root.sessionId,
      lineageSessionIds: [root.sessionId, "native-child"],
      admissionRef: "native:child",
    };
  const packet = prepareChildSemanticPacket({
    parentBinding: root,
    childBinding: child,
    parentLens: lens(),
    childTaskGoal: "t".repeat(12000),
  });
  expect(packet.lens.taskGoal).toHaveLength(12000);
  expect(() => prepare({ lens: { ...lens(), taskGoal: "t".repeat(12001) } })).toThrow(
    "MANDATORY_WHOLE_LENS_CONTEXT_REQUIRED",
  );
});
test("native model alias remains a typed string and cannot be supplied through coercion", () => {
  const p = prepare();
  expect(() => normalizeSemanticReply(p, { ...reply(p), returnedModel: ["jev-1.14.0"] })).toThrow();
});

test("native event with unexposed root retains null/empty lineage and any actually observed parent", () => {
  const partial: NativeBinding = {
    ...binding(),
    rootSessionId: null,
    lineageSessionIds: [],
    parentSessionId: "actual-observed-parent",
  };
  const p = prepare({ event: { ...event(), binding: partial } });
  expect(p.request.state.binding).toMatchObject({
    rootSessionId: null,
    lineageSessionIds: [],
    parentSessionId: "actual-observed-parent",
  });
  expect(p.request.scope.sessionId).toBe("native-root");
  expect(() =>
    prepare({
      event: { ...event(), binding: { ...partial, lineageSessionIds: ["invented-root"] } },
    }),
  ).toThrow("INVALID_NATIVE_SEMANTIC_BINDING");
});
test("partial native ancestry never qualifies as a fully verified inherited child packet", () => {
  const parent: NativeBinding = { ...binding(), rootSessionId: null, lineageSessionIds: [] };
  const child: NativeBinding = {
    ...parent,
    sessionId: "actual-observed-child",
    parentSessionId: parent.sessionId,
    admissionRef: "actual:child-admission",
  };
  expect(() =>
    prepareChildSemanticPacket({
      parentBinding: parent,
      childBinding: child,
      parentLens: lens(),
      childTaskGoal: "Child task",
    }),
  ).toThrow("ACTUAL_NATIVE_CHILD_BINDING_REQUIRED");
  expect(() =>
    prepare({ event: { ...event(), binding: { ...binding(), lineageSessionIds: [] } } }),
  ).toThrow("INVALID_NATIVE_SEMANTIC_BINDING");
});

describe("W02 F02 post-dispatch failure boundary", () => {
  for (const after of [null, undefined, {}, { authorized: true }, { evidenceRefs: 7 }])
    test(`missing or malformed post-read owner state preserves a single dispatch: ${JSON.stringify(after)}`, async () => {
      const p = prepare(),
        decision = normalizeSemanticReply(p, reply(p));
      let reads = 0,
        observations = 0;
      const original = {
        status: "observed" as const,
        observationId: "read:completed",
        evidenceRefs: ["read:source"],
        postconditionSatisfied: true,
      };
      const result = await consumeReadonlySelection({
        prepared: p,
        decision,
        handlers: new Map([
          [
            "native.read-source",
            async () => {
              reads++;
              return original;
            },
          ],
        ]),
        observeCurrent: async () =>
          (++observations === 1 ? current(p) : after) as CurrentReadBinding,
      });
      expect(result.status).toBe("read-unknown");
      expect(result.dispatched).toBe(true);
      expect(result.retryAllowed).toBe(false);
      expect(result.observation).toEqual(original);
      expect(reads).toBe(1);
      expect(() => JSON.stringify(result)).not.toThrow();
    });
  test("candidate digest failure after execution and receipt serialization failure are typed unknown", async () => {
    for (const failure of ["digest", "serialization"] as const) {
      const p = prepare(),
        decision = normalizeSemanticReply(p, reply(p));
      const observation = {
        status: "observed" as const,
        observationId: "read:done",
        evidenceRefs: ["source:done"],
        postconditionSatisfied: true,
      };
      const result = await consumeReadonlySelection({
        prepared: p,
        decision,
        handlers: new Map([
          [
            "native.read-source",
            async () => {
              if (failure === "digest")
                (p.candidates[0]!.parameters as Record<string, unknown>).invalid = BigInt(1);
              else Object.assign(observation, { invalid: BigInt(1) });
              return observation;
            },
          ],
        ]),
        observeCurrent: async () => current(p),
      });
      expect(result.status).toBe("read-unknown");
      expect(result.dispatched).toBe(true);
      expect(result.retryAllowed).toBe(false);
      expect(result.failure?.boundary).toBe(
        failure === "digest" ? "post-read" : "receipt-serialization",
      );
      expect(() => JSON.stringify(result)).not.toThrow();
    }
  });
  test("existing owner persistence failure preserves original outcome without a second write", async () => {
    const { persistReadConsumptionReceipt } = await import("./index.ts");
    const p = prepare(),
      decision = normalizeSemanticReply(p, reply(p));
    const original = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers: new Map([
        [
          "native.read-source",
          async () => ({
            status: "observed",
            observationId: "read:done",
            evidenceRefs: ["source:done"],
            postconditionSatisfied: true,
          }),
        ],
      ]),
      observeCurrent: async () => current(p),
    });
    let writes = 0;
    const result = await persistReadConsumptionReceipt(original, async () => {
      writes++;
      throw Error("owner write ambiguous");
    });
    expect(result.status).toBe("storage-unknown");
    expect(result.receipt.dispatched).toBe(true);
    expect(result.receipt.failure?.originalStatus).toBe("verified-read");
    expect(result.receipt.failure?.originalObservationId).toBe("read:done");
    expect(writes).toBe(1);
    expect(result.retryAllowed).toBe(false);
    expect(() => JSON.stringify(result)).not.toThrow();
    Object.assign(original, { invalid: BigInt(1) });
    const serialization = await persistReadConsumptionReceipt(original, async () => {
      writes++;
    });
    expect(serialization.receipt.failure?.boundary).toBe("receipt-serialization");
    expect(serialization.receipt.dispatched).toBe(true);
    expect(writes).toBe(1);
  });
});

test("direct-v2 consumes an allowed tied selection while explicit legacy v1 still abstains", async () => {
  const p = prepare({ consumptionPolicy: "jev.direct-selected-answer.v2" });
  const raw = reply(p);
  raw.schema = "jev.semantic-result.v2";
  raw.consumptionPolicy = "jev.direct-selected-answer.v2";
  (raw.answers as Record<string, unknown>)[p.selectionQuestionId!] = {
    type: "choice",
    choice: "read-source",
    confidence: 0,
    probabilities: { "read-source": 0.5, [ABSTAIN_ID]: 0.5 },
  };
  const selected = normalizeSemanticReply(p, raw);
  expect(selected.selectedCandidateId).toBe("read-source");
  expect(selected.nativeChoiceConfidence).toBe(0);
  expect(
    (
      await consumeReadonlySelection({
        prepared: p,
        decision: selected,
        handlers: new Map([
          [
            "native.read-source",
            async () => ({
              status: "observed",
              observationId: "source:direct-read",
              evidenceRefs: ["source:checked"],
              postconditionSatisfied: true,
            }),
          ],
        ]),
        observeCurrent: async () => current(p),
      })
    ).status,
  ).toBe("verified-read");
  expect(() => normalizeSemanticReply(p, { ...raw, schema: "jev.semantic-result.v1" })).toThrow();
  const legacy = prepare();
  expect(() => normalizeSemanticReply(legacy, raw)).toThrow();
});

test("reused mutable native current binding cannot rewrite the original pre-read authorization or source", async () => {
  const p = prepare(),
    reused = current(p);
  let observes = 0;
  const result = await consumeReadonlySelection({
    prepared: p,
    decision: normalizeSemanticReply(p, reply(p)),
    handlers: new Map([
      [
        "native.read-source",
        async () => ({
          status: "observed",
          observationId: "read:original",
          evidenceRefs: ["source:one"],
          postconditionSatisfied: true,
        }),
      ],
    ]),
    observeCurrent: async () => {
      if (++observes > 1) {
        reused.sourceRevision = "changed";
        reused.authorizationRef = "changed-permission";
      }
      return reused;
    },
  });
  expect(result.status).toBe("read-unknown");
  expect(result.dispatched).toBe(true);
  expect(result.retryAllowed).toBe(false);
});
