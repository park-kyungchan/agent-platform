import { describe, expect, test } from "bun:test";
import {
  JEV_MODEL,
  prepareSemanticDecision,
  normalizeSemanticReply,
  consumeReadonlySelection,
  persistReadConsumptionReceipt,
  prepareWorkflowDecision,
  normalizeWorkflowReply,
  qualifyItemSupplementState,
  DIRECT_POLICY,
  prepareDirectDecision,
  normalizeDirectReply,
  consumeDirectAction,
  qualifyDirectAdapter,
  projectDirectRequest,
  semanticDigest,
  type NativeBinding,
  type WholeLens,
  type SemanticEvent,
  type ReadonlyCandidate,
  type PreparedDecision,
  type CurrentReadBinding,
  type ReadObservation,
  type WorkflowInput,
  type PreparedWorkflowDecision,
  type CurrentWorkflowBinding,
  type ItemSupplementState,
  type CurrentItemOwner,
  type PreparedDirectDecision,
  type DirectActionBinding,
  type CurrentActionBinding,
  type NativeActionObservation,
  type SemanticAdapterCapability,
  type Json,
} from "./index.ts";

interface FixtureAnswer {
  type: string;
  noul?: number;
  choice?: string;
  confidence?: number | null;
  probabilities?: Record<string, number>;
  legend?: Record<string, Json>;
  score?: number;
  box?: Record<string, number | string>;
}
interface FixtureReply extends Record<string, unknown> {
  answers: Record<string, FixtureAnswer>;
}

// Fictional independent consumer fixtures. No native owner or provider is invoked.
const nativeBinding = (): NativeBinding => ({
  runtime: "native-codex",
  sessionId: "independent-session",
  agentId: null,
  agentType: null,
  parentSessionId: null,
  rootSessionId: null,
  lineageSessionIds: [],
  profileId: null,
  nativeTaskId: null,
  nativeRunId: null,
  logical: null,
  admissionRef: "owner:fictional-admission",
});
const wholeLens = (): WholeLens => ({
  currentAuthority: { text: "Verify this fictional source result.", sourceRefs: ["user:scope"] },
  wholeObjective:
    "Preserve native owners and unknown outcomes while consuming bounded source decisions.",
  taskGoal: "Independently verify the admitted W02 source boundary.",
  acceptedWhy: [
    {
      text: "Preserve exact source and original effect outcomes.",
      attribution: "current-user",
      sourceRefs: ["user:why"],
    },
  ],
  acceptedDecisions: [],
  owners: [{ role: "native", id: "owner:fictional-native" }],
  dependencies: [{ id: "common", revision: "independent-source", currentness: "current" }],
  completedEffectRefs: ["effect:completed"],
  uncertainEffectRefs: ["effect:unknown"],
  remainingAcceptance: ["native:activation"],
});
const semanticEvent = (): SemanticEvent => ({
  eventId: "independent-event",
  kind: "worker_start",
  binding: nativeBinding(),
  sourceRevision: "independent-source",
  materialRevision: "material-1",
  material: true,
  privacy: "admitted",
  externalEvidence: false,
  facts: { result: "fictional current source" },
});
const readCandidate = (): ReadonlyCandidate => ({
  candidateId: "read-offered",
  kind: "read",
  effectClass: "read_only",
  description: "Read one exact admitted fictional source.",
  handlerKey: "fixed.read",
  targetId: "fictional:source",
  parameters: { section: "contract" },
  preconditionDigest: "precondition-1",
  authorizationRef: "owner:read-permission",
  sourceRefs: ["source:fictional"],
});
const prepareRead = (): PreparedDecision => {
  const p = prepareSemanticDecision({
    requestId: "independent-read",
    event: semanticEvent(),
    lens: wholeLens(),
    candidates: [readCandidate()],
  });
  if (p.status !== "prepared") throw Error("independent read fixture not prepared");
  return p;
};
const nativeReply = (p: PreparedDecision): FixtureReply => ({
  schema:
    p.request.schema === "jev.semantic-request.v2"
      ? "jev.semantic-result.v2"
      : "jev.semantic-result.v1",
  ...(p.request.schema === "jev.semantic-request.v2"
    ? { consumptionPolicy: p.request.consumptionPolicy, requestDigest: p.requestDigest }
    : {}),
  requestId: p.request.requestId,
  purpose: "semantic_bundle",
  stateDigest: p.request.stateDigest,
  requestedModel: JEV_MODEL,
  returnedModel: "jev-1.14.0",
  status: "ok",
  sourcePostconditionVerified: true,
  executionAuthorityGranted: false,
  automaticWrite: false,
  answers: Object.fromEntries(
    Object.entries(p.request.questions).map(([id, q]) => [
      id,
      q.type === "noul"
        ? { type: "noul", noul: 0.11 }
        : {
            type: "choice",
            choice: Object.keys(q.criteria)[0],
            confidence: 0.8,
            probabilities: Object.fromEntries(
              Object.keys(q.criteria).map((key, i) => [key, i === 0 ? 1 : 0]),
            ),
          },
    ]),
  ),
});
const currentRead = (p: PreparedDecision): CurrentReadBinding => ({
  authorized: true,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  candidateDigest: p.candidateDigests["read-offered"]!,
  authorizationRef: "owner:read-permission",
  evidenceRefs: ["owner:current-read"],
});
const readObservation = (): ReadObservation => ({
  status: "observed",
  observationId: "read:original-outcome",
  evidenceRefs: ["read:evidence"],
  postconditionSatisfied: true,
});

describe("W02 independent F02 actual imported read consumer", () => {
  for (const [name, value] of [
    ["null", null],
    ["undefined", undefined],
    ["primitive", 7],
    ["empty", {}],
    ["missing evidence", { authorized: true }],
    ["invalid evidence", { evidenceRefs: null }],
  ] as const) {
    test(`post-read ${name} preserves actual dispatch and original observation`, async () => {
      const p = prepareRead();
      let handlers = 0,
        observations = 0;
      const receipt = await consumeReadonlySelection({
        prepared: p,
        decision: normalizeSemanticReply(p, nativeReply(p)),
        handlers: new Map([
          [
            "fixed.read",
            async () => {
              handlers++;
              return readObservation();
            },
          ],
        ]),
        observeCurrent: async () =>
          (++observations === 1 ? currentRead(p) : value) as CurrentReadBinding,
      });
      expect(handlers).toBe(1);
      expect(observations).toBe(2);
      expect(receipt.status).toBe("read-unknown");
      expect(receipt.dispatched).toBe(true);
      expect(receipt.observation?.observationId).toBe("read:original-outcome");
      expect(receipt.retryAllowed).toBe(false);
      expect(receipt.executionAuthorityGranted).toBe(false);
    });
  }
  test("post-read observer throwing keeps one dispatched read; pre-read revoked permission keeps zero", async () => {
    const p = prepareRead();
    let reads = 0,
      observations = 0;
    const handlers = new Map([
      [
        "fixed.read",
        async () => {
          reads++;
          return readObservation();
        },
      ],
    ]);
    const decision = normalizeSemanticReply(p, nativeReply(p));
    const after = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers,
      observeCurrent: async () => {
        if (++observations > 1) throw Error("fictional observer unavailable");
        return currentRead(p);
      },
    });
    expect(after.status).toBe("read-unknown");
    expect(after.dispatched).toBe(true);
    expect(after.observation).toEqual(readObservation());
    expect(after.retryAllowed).toBe(false);
    const before = await consumeReadonlySelection({
      prepared: p,
      decision,
      handlers,
      observeCurrent: async () => ({ ...currentRead(p), authorized: false }),
    });
    expect(before.status).toBe("held");
    expect(before.dispatched).toBe(false);
    expect(reads).toBe(1);
  });
  test("handler-result serialization failure retains safe original outcome metadata", async () => {
    const p = prepareRead();
    const malformed = { ...readObservation(), loop: null as unknown };
    malformed.loop = malformed;
    let reads = 0;
    const result = await consumeReadonlySelection({
      prepared: p,
      decision: normalizeSemanticReply(p, nativeReply(p)),
      handlers: new Map([
        [
          "fixed.read",
          async () => {
            reads++;
            return malformed;
          },
        ],
      ]),
      observeCurrent: async () => currentRead(p),
    });
    expect(reads).toBe(1);
    expect(result.status).toBe("read-unknown");
    expect(result.dispatched).toBe(true);
    expect(result.retryAllowed).toBe(false);
    expect(result.failure?.boundary).toBe("receipt-serialization");
    expect(result.failure?.originalStatus).toBe("observed");
    expect(result.failure?.originalObservationId).toBe("read:original-outcome");
    expect(() => JSON.stringify(result)).not.toThrow();
  });
  test("reused mutable owner binding cannot hide a post-read source/authorization change", async () => {
    const p = prepareRead(),
      shared = currentRead(p);
    let reads = 0;
    const receipt = await consumeReadonlySelection({
      prepared: p,
      decision: normalizeSemanticReply(p, nativeReply(p)),
      handlers: new Map([
        [
          "fixed.read",
          async () => {
            reads++;
            shared.sourceRevision = "changed-during-read";
            shared.authorizationRef = "owner:revoked-successor";
            return readObservation();
          },
        ],
      ]),
      observeCurrent: async () => shared,
    });
    expect(reads).toBe(1);
    expect(receipt.status).toBe("read-unknown");
    expect(receipt.dispatched).toBe(true);
    expect(receipt.retryAllowed).toBe(false);
    expect(receipt.observation?.observationId).toBe("read:original-outcome");
  });
  test("existing-owner write failure and serialization failure remain distinct, bounded and never retried", async () => {
    const p = prepareRead();
    const original = await consumeReadonlySelection({
      prepared: p,
      decision: normalizeSemanticReply(p, nativeReply(p)),
      handlers: new Map([["fixed.read", async () => readObservation()]]),
      observeCurrent: async () => currentRead(p),
    });
    let writes = 0;
    const failed = await persistReadConsumptionReceipt(original, async () => {
      writes++;
      throw Error("fictional ambiguous write");
    });
    expect(writes).toBe(1);
    expect(failed.status).toBe("storage-unknown");
    expect(failed.receipt.dispatched).toBe(true);
    expect(failed.retryAllowed).toBe(false);
    expect(failed.receipt.failure).toMatchObject({
      boundary: "owner-persistence",
      originalStatus: "verified-read",
      originalObservationId: "read:original-outcome",
    });
    const circular = { ...original, loop: null as unknown };
    circular.loop = circular;
    const serialized = await persistReadConsumptionReceipt(circular, async () => {
      writes++;
    });
    expect(writes).toBe(1);
    expect(serialized.receipt.failure?.boundary).toBe("receipt-serialization");
    expect(serialized.receipt.dispatched).toBe(true);
    expect(serialized.receipt.retryAllowed).toBe(false);
    expect(serialized.receipt.failure?.originalObservationId).toBe("read:original-outcome");
    expect(() => JSON.stringify(serialized)).not.toThrow();
  });
});

const delegationInput = (): Extract<WorkflowInput, { kind: "delegation" }> => ({
  kind: "delegation",
  taskPacket: {
    id: "independent-child",
    goal: "Verify fictional source only.",
    packetDigest: "a".repeat(64),
    requiredPostconditionIds: ["source-reviewed"],
  },
  rolePolicy: {
    schema: "jev.fixed-role-policy.v1",
    role: "ordinary-child",
    allowedTuples: [{ model: "gpt-6.1-sol", effort: "high" }],
    authorizationRef: "user:fixed-child",
    sourceRevision: "independent-source",
    capabilityEvidenceRefs: ["native:capability"],
  },
  profiles: [
    {
      profileId: "ordinary-high",
      model: "gpt-6.1-sol",
      effort: "high",
      availability: "available",
      applicability: "applicable",
      authorizationRef: "owner:profile",
      sourceRefs: ["native:catalog"],
    },
  ],
});
const prepareDelegation = (workflow = delegationInput()): PreparedWorkflowDecision => {
  const p = prepareWorkflowDecision({
    requestId: "independent-delegation",
    event: semanticEvent(),
    lens: wholeLens(),
    workflow,
  });
  if (p.status !== "prepared") throw Error("independent delegation fixture not prepared");
  return p;
};
const currentWorkflow = (p: PreparedWorkflowDecision): CurrentWorkflowBinding => ({
  authorized: true,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  workflowDigest: p.workflowDigest,
  evidenceRefs: ["owner:current-profile"],
  ...(p.workflow.kind === "delegation" && p.workflow.rolePolicy
    ? { rolePolicyDigest: semanticDigest(p.workflow.rolePolicy) }
    : {}),
});
describe("W02 independent fixed role and singleton admission", () => {
  test("explicit direct-v2 delegation with omitted role policy holds before premium profile admission", () => {
    const w = delegationInput();
    delete w.rolePolicy;
    w.profiles[0] = {
      ...w.profiles[0]!,
      model: "gpt-6-astra",
      effort: "xhigh",
      profileId: "off-policy-premium",
    };
    const result = prepareWorkflowDecision({
      requestId: "independent-v2-missing-policy",
      workflow: w,
      lens: wholeLens(),
      event: semanticEvent(),
      consumptionPolicy: "jev.direct-selected-answer.v2",
    });
    expect(result.status).toBe("held");
    expect("request" in result).toBe(false);
  });
  test("qualified singleton drops only profile selection and retains independently useful questions", () => {
    const p = prepareDelegation();
    expect(p.selectionQuestionId).toBeNull();
    expect(Object.values(p.questionMeanings).sort()).toEqual(
      ["task_scope_satisfaction", "delegation_coverage", "context_relevance"].sort(),
    );
    expect(Object.values(p.request.questions).every((q) => q.type === "noul")).toBe(true);
    const a = normalizeWorkflowReply(p, nativeReply(p), currentWorkflow(p));
    expect(a.selectedProfile?.profileId).toBe("ordinary-high");
    expect(a.deterministic).toBe(true);
    expect(a.executionAuthorityGranted).toBe(false);
    expect(a.taskCompletionGranted).toBe(false);
  });
  for (const state of ["zero", "off-policy", "unavailable", "unknown capability"] as const) {
    test(`${state} has no qualified tuple and holds before semantic preparation`, () => {
      const w = delegationInput();
      if (state === "zero") w.profiles = [];
      if (state === "off-policy")
        w.profiles[0] = { ...w.profiles[0]!, model: "gpt-6-astra", effort: "xhigh" };
      if (state === "unavailable") w.profiles[0]!.availability = "unavailable";
      if (state === "unknown capability") w.profiles[0]!.applicability = "unknown";
      // An invalid event would throw if semantic question preparation ran first.
      const result = prepareWorkflowDecision({
        requestId: "independent-empty",
        workflow: w,
        lens: wholeLens(),
        event: { ...semanticEvent(), eventId: "" },
      });
      expect(result.status).toBe("held");
      expect("request" in result).toBe(false);
    });
  }
  test("fresh policy/source/authorization are rechecked; independent result does not carry old permission", () => {
    const p = prepareDelegation(),
      raw = nativeReply(p);
    for (const change of [
      { rolePolicyDigest: "b".repeat(64) },
      { authorized: false },
      { sourceRevision: "foreign-source" },
      { workflowDigest: "c".repeat(64) },
    ]) {
      const a = normalizeWorkflowReply(p, raw, { ...currentWorkflow(p), ...change });
      expect(a.status).toBe("held");
      expect(a.selectedProfile).toBeNull();
      expect(a.executionAuthorityGranted).toBe(false);
    }
    const w = delegationInput();
    w.rolePolicy!.sourceRevision = "stale-source";
    expect(
      prepareWorkflowDecision({
        requestId: "independent-stale",
        workflow: w,
        lens: wholeLens(),
        event: semanticEvent(),
      }).status,
    ).toBe("held");
  });
  test("ordinary-role policy itself cannot admit a premium tuple", () => {
    const w = delegationInput();
    w.rolePolicy!.allowedTuples = [{ model: "gpt-6-astra", effort: "xhigh" }];
    expect(() => prepareDelegation(w)).toThrow();
  });
  test("explicit future multi-candidate route consumes allowed selected tie at low confidence under v2", () => {
    const w = delegationInput();
    w.rolePolicy!.role = "explicit-routing";
    w.rolePolicy!.allowedTuples.push({ model: "gpt-6.1-sol", effort: "ultra" });
    w.profiles.push({ ...w.profiles[0]!, profileId: "explicit-ultra", effort: "ultra" });
    const p = prepareDelegation(w),
      raw = nativeReply(p),
      key = p.selectionQuestionId!;
    expect(p.request.schema).toBe("jev.semantic-request.v2");
    expect(key).not.toBeNull();
    raw.answers[key] = {
      type: "choice",
      choice: "explicit-ultra",
      confidence: 0.01,
      probabilities: { "ordinary-high": 0.5, "explicit-ultra": 0.5, __abstain: 0 },
    };
    const advice = normalizeWorkflowReply(p, raw, currentWorkflow(p));
    expect(advice.status).toBe("advice");
    expect(advice.selectedProfile?.profileId).toBe("explicit-ultra");
    expect(advice.deterministic).toBe(false);
    expect(advice.executionAuthorityGranted).toBe(false);
  });
  test("historical v1 role-policy absence retains its separate protocol and confidence policy", () => {
    const w = delegationInput();
    delete w.rolePolicy;
    const p = prepareDelegation(w),
      raw = nativeReply(p);
    expect(p.request.schema).toBe("jev.semantic-request.v1");
    expect(p.selectionQuestionId).not.toBeNull();
    raw.answers[p.selectionQuestionId!]!.confidence = 0.01;
    const advice = normalizeWorkflowReply(p, raw, currentWorkflow(p));
    expect(advice.status).toBe("abstained");
    expect(advice.selectedProfile).toBeNull();
    expect(() =>
      normalizeSemanticReply(p, {
        ...raw,
        schema: "jev.semantic-result.v2",
        consumptionPolicy: "jev.direct-selected-answer.v2",
      }),
    ).toThrow();
  });
});

const ownerPin = { path: "fictional/owner-checkpoint.json", sha256: "d".repeat(64) };
const itemState = (): ItemSupplementState => ({
  schema: "jev.item-supplement-state.v1",
  item: {
    domainId: "native-harness",
    projectId: "fictional-project",
    workId: "fictional-work",
    itemId: "stable-item",
  },
  revision: 1,
  previousDigest: null,
  claim: { claimId: "claim-1", revision: 1, actorId: "actor-a", contextRevision: "context-a" },
  supplements: 0,
  terminal: null,
  effectFences: [],
  ownerReceipt: ownerPin,
  continuation: "update_plan",
});
const itemOwner = (s: ItemSupplementState): CurrentItemOwner => ({
  authorized: true,
  itemDigest: semanticDigest(s.item),
  previousDigest: s.previousDigest,
  stateDigest: semanticDigest(s),
  ownerReceipt: s.ownerReceipt,
  evidenceRefs: ["owner:checkpoint-current"],
});
const successor = (
  s: ItemSupplementState,
  change: Partial<ItemSupplementState> = {},
): ItemSupplementState => ({
  ...structuredClone(s),
  revision: s.revision + 1,
  previousDigest: semanticDigest(s),
  ...change,
});
const qualify = (
  previous: ItemSupplementState | null,
  observed: ItemSupplementState,
  operation: Parameters<typeof qualifyItemSupplementState>[0]["operation"],
) => qualifyItemSupplementState({ previous, observed, operation, current: itemOwner(observed) });

describe("W02 independent stable item supplementation and terminal persistence", () => {
  test("two supplements survive claim/actor/context handoff and exact restore; a third never qualifies", () => {
    const a = itemState(),
      b = successor(a, { supplements: 1 }),
      c = successor(b, { supplements: 2 });
    expect(qualify(null, a, "initialize").status).toBe("verified-owner-state");
    expect(qualify(a, b, "supplement").nextSupplementAllowed).toBe(true);
    expect(qualify(b, c, "supplement").nextSupplementAllowed).toBe(false);
    const handoff = successor(c, {
      claim: { claimId: "claim-2", revision: 2, actorId: "actor-b", contextRevision: "context-b" },
    });
    expect(qualify(c, handoff, "handoff").status).toBe("verified-owner-state");
    const restore = qualify(handoff, structuredClone(handoff), "restore");
    expect(restore.state?.supplements).toBe(2);
    expect(restore.nextSupplementAllowed).toBe(false);
    expect(restore.permitsReplay).toBe(false);
    expect(qualify(handoff, successor(handoff, { supplements: 3 as 2 }), "supplement").status).toBe(
      "held",
    );
    expect(qualify(handoff, successor(handoff, { supplements: 0 }), "handoff").status).toBe("held");
  });
  for (const outcome of ["supported", "negative", "insufficient", "rejected"] as const) {
    test(`${outcome} terminal result survives handoff/restore without reinterpreting completion`, () => {
      const a = itemState(),
        terminal = successor(a, { terminal: { outcome, resultRevision: 1, result: ownerPin } });
      expect(qualify(a, terminal, "terminal").status).toBe("verified-owner-state");
      const moved = successor(terminal, {
        claim: { ...terminal.claim, revision: 2, actorId: "actor-b", contextRevision: "context-b" },
      });
      const h = qualify(terminal, moved, "handoff"),
        r = qualify(moved, moved, "restore");
      expect(h.state?.terminal).toEqual(terminal.terminal);
      expect(r.state?.terminal).toEqual(terminal.terminal);
      expect(r.nextSupplementAllowed).toBe(false);
      expect(r.executionAuthorityGranted).toBe(false);
      expect(qualify(moved, successor(moved, { terminal: null }), "handoff").status).toBe("held");
      expect(qualify(moved, successor(moved, { supplements: 1 }), "supplement").status).toBe(
        "held",
      );
    });
  }
  test("same-transition unknown API outcome plus semantic terminal holds; separate unknown checkpoint remains unknown", () => {
    const a = itemState(),
      fence = {
        effectId: "api:dispatched-unknown",
        outcome: "unknown" as const,
        receipt: ownerPin,
      };
    const invalid = successor(a, {
      effectFences: [fence],
      terminal: { outcome: "negative", resultRevision: 1, result: ownerPin },
    });
    const rejected = qualify(a, invalid, "terminal");
    expect(rejected.status).toBe("held");
    expect(rejected.state).toBeNull();
    expect(rejected.nextSupplementAllowed).toBe(false);
    const unknown = successor(a, { effectFences: [fence] });
    const kept = qualify(a, unknown, "handoff");
    expect(kept.state?.effectFences).toEqual([fence]);
    expect(kept.state?.terminal).toBeNull();
    expect(kept.nextSupplementAllowed).toBe(false);
    expect(
      qualify(unknown, successor(unknown, { terminal: invalid.terminal }), "terminal").status,
    ).toBe("held");
    expect(qualify(unknown, successor(unknown, { effectFences: [] }), "handoff").status).toBe(
      "held",
    );
  });
  test("completed receipt fences, exact owner, stable identity and revision cannot be dropped or rebound", () => {
    const a = itemState();
    a.effectFences = [{ effectId: "effect:completed", outcome: "completed", receipt: ownerPin }];
    expect(qualify(a, successor(a, { effectFences: [] }), "handoff").status).toBe("held");
    expect(
      qualify(a, successor(a, { item: { ...a.item, itemId: "foreign-item" } }), "handoff").status,
    ).toBe("held");
    expect(qualify(a, successor(a, { previousDigest: "e".repeat(64) }), "handoff").status).toBe(
      "held",
    );
    expect(
      qualify(a, successor(a, { claim: { ...a.claim, actorId: "actor-b" } }), "handoff").status,
    ).toBe("held");
    const next = successor(a);
    expect(
      qualifyItemSupplementState({
        previous: a,
        observed: next,
        operation: "handoff",
        current: { ...itemOwner(next), authorized: false },
      }).status,
    ).toBe("held");
    expect(qualify(a, { ...a, supplements: 1 }, "restore").status).toBe("held");
  });
});

// A deterministic valid 280x280 still PNG. Image decoding belongs to the owner;
// these tests exercise its explicit source attestation and exact base64-byte binding.
const pngBase64 =
  "iVBORw0KGgoAAAANSUhEUgAAARgAAAEYCAIAAAAI7H7bAAAA+0lEQVR4nO3BAQ0AAADCoPdPbQ43oAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD4NmAUAATtYWQQAAAAASUVORK5CYII=";
const imageRef = "image-9e72-independent";
const imageHash = "ab18c850cba3fb81f4ec465e173cbc2ba324e927f060647f1de1d4441298fc38";
const imageOwnerEvidence = () => ({
  source: { path: "fictional/decoded.png", sha256: imageHash },
  sha256: imageHash,
  width: 280,
  height: 280,
  byteLength: 308,
  processedByteLength: 308,
  format: "png" as const,
  still: true as const,
  decoded: true as const,
  evidenceRefs: ["owner:image-decoded"],
});
const directAction = (): DirectActionBinding => ({
  actionId: "inspect-region",
  handlerKey: "fixed.inspect",
  effectClass: "native_permitted",
  authorizationRef: "owner:action-permission",
  permissionDigest: "1".repeat(64),
  preconditionDigest: "2".repeat(64),
  sourceRefs: ["source:fixed-action"],
  when: { questionId: "route", choices: ["inspect"] },
  arguments: {
    route: { kind: "answer", questionId: "route" },
    rank: { kind: "answer", questionId: "rank" },
    area: { kind: "answer", questionId: "region" },
    present: { kind: "answer", questionId: "present" },
    target: { kind: "literal", value: "fictional-target" },
  },
});
const directInput = (): Parameters<typeof prepareDirectDecision>[0] => ({
  requestId: "independent-direct",
  event: { ...semanticEvent(), facts: { photo: imageRef } },
  lens: wholeLens(),
  opportunity: {
    role: "child",
    unit: "image-region",
    stableItemId: "independent-item",
    revision: "1",
    nativeHookId: "native:unrun-hook",
    consumerId: "consumer:fictional",
  },
  questions: {
    // Box intentionally precedes presence: consumer ordering must be explicit.
    region: { type: "box", instructions: "Locate the target in facts.photo." },
    route: {
      type: "choice",
      instructions: "Select a permitted route.",
      criteria: { inspect: { meaning: "Inspect the region." }, skip: null },
    },
    rank: {
      type: "score",
      instructions: "Rate the amount of evidence.",
      criteria: ["absent", { meaning: "partial" }, "complete"],
    },
    present: {
      type: "noul",
      instructions: "Is the target present in facts.photo?",
      criteria: { true: "Present", false: "Absent" },
    },
  },
  actions: [directAction()],
  assets: { [imageRef]: { type: "image", format: "png", data_base64: pngBase64 } },
  imageEvidence: { [imageRef]: imageOwnerEvidence() },
  imageOptIn: true,
  boxes: { region: { assetRef: imageRef, presenceQuestionId: "present", minimumPresence: 0.6 } },
});
const prepareDirect = (input = directInput()): PreparedDirectDecision => {
  const p = prepareDirectDecision(input);
  if (p.status !== "prepared-direct") throw Error("independent direct fixture not prepared");
  return p;
};
const directReply = (p: PreparedDirectDecision): FixtureReply => ({
  schema: "jev.semantic-result.v2",
  consumptionPolicy: DIRECT_POLICY,
  status: "ok",
  requestId: p.request.requestId,
  requestDigest: p.requestDigest,
  stateDigest: p.request.stateDigest,
  purpose: "semantic_bundle",
  requestedModel: JEV_MODEL,
  returnedModel: "jev-1.14.0",
  sourcePostconditionVerified: true,
  executionAuthorityGranted: false,
  automaticWrite: false,
  answers: Object.fromEntries(
    Object.entries(p.request.questions).map(([id, q]) => {
      if (q.type === "box")
        return [
          id,
          { type: "box", box: { top: 0.1, bottom: 0.9, left: 0.2, right: 0.8 }, confidence: null },
        ];
      if (q.type === "choice")
        return [
          id,
          {
            type: "choice",
            choice: "inspect",
            confidence: 0.01,
            probabilities: { inspect: 0.5, skip: 0.5 },
          },
        ];
      if (q.type === "score")
        return [
          id,
          {
            type: "score",
            score: 1.25,
            confidence: 0.01,
            legend: Object.fromEntries(q.criteria.map((v, index) => [String(index), v])),
            probabilities: { "0": 0, "1": 0.75, "2": 0.25 },
          },
        ];
      return [id, { type: "noul", noul: id === "present" ? 0.9 : 0.12 }];
    }),
  ),
});
const newAdapter = (): SemanticAdapterCapability => ({
  requestSchemas: ["jev.semantic-request.v2"],
  questionTypes: ["noul", "choice", "score", "box"],
  assetModes: ["base64", "multipart"],
  experimentalImages: true,
  source: ownerPin,
});
const currentAction = (
  p: PreparedDirectDecision,
  effectFence: CurrentActionBinding["effectFence"] = "clear",
): CurrentActionBinding => ({
  authorized: true,
  requestDigest: p.requestDigest,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  actionDigest: semanticDigest(p.actions[0]!),
  authorizationRef: p.actions[0]!.authorizationRef,
  permissionDigest: p.actions[0]!.permissionDigest,
  preconditionDigest: p.actions[0]!.preconditionDigest,
  occurrenceId: "owner:occurrence-original",
  effectFence,
  evidenceRefs: ["owner:current-action"],
});
const actionObservation = (): NativeActionObservation => ({
  status: "observed",
  observationId: "action:original-result",
  resultRevision: 1,
  evidenceRefs: ["owner:action-result"],
  postconditionSatisfied: true,
  result: { inspected: true },
});

describe("W02 independent direct typed source boundary and native adapter hold", () => {
  test("official Choice/Noul/Score/Box shapes survive low confidence and tied selection; model pin and opt-in project exactly", () => {
    const p = prepareDirect(),
      selected = normalizeDirectReply(p, directReply(p));
    expect(selected.answers.route).toMatchObject({
      type: "choice",
      choice: "inspect",
      confidence: 0.01,
    });
    expect(selected.answers.rank).toMatchObject({ type: "score", score: 1.25, confidence: 0.01 });
    expect(selected.answers.region).toMatchObject({ type: "box", confidence: null });
    expect(selected.ignoredQuestionIds).toEqual([]);
    expect(selected.executionAuthorityGranted).toBe(false);
    const projected = projectDirectRequest(p, newAdapter());
    expect(projected.body.model).toBe("jev-1.14.0");
    expect(projected.headers).toEqual({ "TypeSafe-Danger-Zone-Images": "true" });
    expect(projected.body.assets?.[imageRef]).toEqual(p.request.assets?.[imageRef]);
    expect(projected.body.questions.rank?.type).toBe("score");
    const old = {
      ...newAdapter(),
      requestSchemas: ["jev.semantic-request.v1"],
      questionTypes: ["choice", "noul"],
      assetModes: [],
    };
    expect(qualifyDirectAdapter(p, old).status).toBe("held");
    expect(() => projectDirectRequest(p, old)).toThrow();
  });
  const malformed: [string, (r: FixtureReply) => void][] = [
    [
      "unknown choice",
      (r) => {
        r.answers.route.choice = "not-offered";
      },
    ],
    [
      "unknown answer",
      (r) => {
        r.answers.foreign = { type: "noul", noul: 1 };
      },
    ],
    [
      "missing answer",
      (r) => {
        delete r.answers.rank;
      },
    ],
    [
      "wrong model",
      (r) => {
        r.returnedModel = "jev-latest";
      },
    ],
    [
      "wrong request digest",
      (r) => {
        r.requestDigest = "f".repeat(64);
      },
    ],
    [
      "wrong schema",
      (r) => {
        r.schema = "jev.semantic-result.v1";
      },
    ],
    [
      "unknown consumption policy",
      (r) => {
        r.consumptionPolicy = "future";
      },
    ],
    [
      "API error",
      (r) => {
        r.status = "error";
      },
    ],
    [
      "Noul outside range",
      (r) => {
        r.answers.present.noul = 1.1;
      },
    ],
    [
      "nonfinite probability",
      (r) => {
        r.answers.route.probabilities!.inspect = NaN;
      },
    ],
    [
      "unknown distribution key",
      (r) => {
        r.answers.route.probabilities!.foreign = 0;
      },
    ],
    [
      "wrong Score legend",
      (r) => {
        r.answers.rank.legend!["1"] = "different meaning";
      },
    ],
    [
      "wrong Score expected value",
      (r) => {
        r.answers.rank.score = 2;
      },
    ],
    [
      "Score outside rubric",
      (r) => {
        r.answers.rank.score = -1;
      },
    ],
    [
      "reversed Box",
      (r) => {
        r.answers.region.box!.left = 0.9;
      },
    ],
    [
      "Box coordinate outside image",
      (r) => {
        r.answers.region.box!.top = -0.1;
      },
    ],
    [
      "unknown Box field",
      (r) => {
        r.answers.region.box!.extra = 0;
      },
    ],
  ];
  for (const [name, change] of malformed)
    test(`${name} is rejected by actual imported typed normalizer`, () => {
      const p = prepareDirect(),
        raw = directReply(p);
      change(raw);
      expect(() => normalizeDirectReply(p, raw)).toThrow();
    });
  test("absent target gates Box before geometry and abstains before native action", async () => {
    const p = prepareDirect(),
      raw = directReply(p);
    raw.answers.present.noul = 0.1;
    raw.answers.region = {
      type: "box",
      box: { invalid: "absent-target output" },
      confidence: null,
    };
    const selected = normalizeDirectReply(p, raw);
    let reads = 0,
      observations = 0;
    expect(selected.ignoredQuestionIds).toEqual(["region"]);
    expect(selected.answers.region).toBeUndefined();
    const receipt = await consumeDirectAction({
      prepared: p,
      selection: selected,
      actionId: "inspect-region",
      adapter: newAdapter(),
      handlers: new Map([
        [
          "fixed.inspect",
          async () => {
            reads++;
            return actionObservation();
          },
        ],
      ]),
      observeCurrent: async () => {
        observations++;
        return currentAction(p);
      },
    });
    expect(receipt.status).toBe("abstained");
    expect(receipt.dispatched).toBe(false);
    expect(reads).toBe(0);
    expect(observations).toBe(0);
  });
  test("one allowed typed native-owner action binds exact arguments and one existing-owner receipt", async () => {
    const p = prepareDirect(),
      selection = normalizeDirectReply(p, directReply(p));
    let calls = 0,
      observations = 0,
      persisted = "";
    const receipt = await consumeDirectAction({
      prepared: p,
      selection,
      actionId: "inspect-region",
      adapter: newAdapter(),
      handlers: new Map([
        [
          "fixed.inspect",
          async (action, args) => {
            calls++;
            expect(action.handlerKey).toBe("fixed.inspect");
            expect(args).toEqual({
              route: "inspect",
              rank: 1.25,
              area: { top: 0.1, bottom: 0.9, left: 0.2, right: 0.8 },
              present: 0.9,
              target: "fictional-target",
            });
            return actionObservation();
          },
        ],
      ]),
      observeCurrent: async () => currentAction(p, ++observations === 1 ? "clear" : "completed"),
      persist: async (body) => {
        expect(persisted).toBe("");
        persisted = body;
      },
    });
    expect(calls).toBe(1);
    expect(observations).toBe(2);
    expect(receipt.status).toBe("verified-action");
    expect(JSON.parse(persisted)).toEqual(receipt);
    expect(receipt.retryAllowed).toBe(false);
    expect(receipt.executionAuthorityGranted).toBe(false);
    expect(receipt.taskCompletionGranted).toBe(false);
  });
  for (const change of [
    { authorized: false },
    { permissionDigest: "f".repeat(64) },
    { sourceRevision: "old-source" },
    { effectFence: "completed" as const },
    { effectFence: "unknown" as const },
  ]) {
    test(`stale or fenced action ${JSON.stringify(change)} never dispatches`, async () => {
      const p = prepareDirect();
      let calls = 0;
      const receipt = await consumeDirectAction({
        prepared: p,
        selection: normalizeDirectReply(p, directReply(p)),
        actionId: "inspect-region",
        adapter: newAdapter(),
        handlers: new Map([
          [
            "fixed.inspect",
            async () => {
              calls++;
              return actionObservation();
            },
          ],
        ]),
        observeCurrent: async () => ({ ...currentAction(p), ...change }),
      });
      expect(calls).toBe(0);
      expect(receipt.status).toBe("held");
      expect(receipt.retryAllowed).toBe(false);
    });
  }
  test("reused mutable action-owner observation cannot change occurrence during its one dispatch", async () => {
    const p = prepareDirect(),
      shared = currentAction(p);
    let calls = 0;
    const receipt = await consumeDirectAction({
      prepared: p,
      selection: normalizeDirectReply(p, directReply(p)),
      actionId: "inspect-region",
      adapter: newAdapter(),
      handlers: new Map([
        [
          "fixed.inspect",
          async () => {
            calls++;
            shared.occurrenceId = "owner:foreign-occurrence";
            shared.effectFence = "completed";
            return actionObservation();
          },
        ],
      ]),
      observeCurrent: async () => shared,
    });
    expect(calls).toBe(1);
    expect(receipt.status).toBe("action-unknown");
    expect(receipt.dispatched).toBe(true);
    expect(receipt.occurrenceId).toBe("owner:occurrence-original");
    expect(receipt.retryAllowed).toBe(false);
  });
  test("post-action null currentness and ambiguous owner persistence retain dispatched result without retry", async () => {
    const p = prepareDirect();
    let calls = 0,
      observations = 0,
      writes = 0;
    const receipt = await consumeDirectAction({
      prepared: p,
      selection: normalizeDirectReply(p, directReply(p)),
      actionId: "inspect-region",
      adapter: newAdapter(),
      handlers: new Map([
        [
          "fixed.inspect",
          async () => {
            calls++;
            return actionObservation();
          },
        ],
      ]),
      observeCurrent: async () =>
        (++observations === 1 ? currentAction(p) : null) as CurrentActionBinding,
      persist: async () => {
        writes++;
        throw Error("fictional ambiguous write");
      },
    });
    expect(calls).toBe(1);
    expect(writes).toBe(1);
    expect(receipt.status).toBe("action-unknown");
    expect(receipt.observation?.observationId).toBe("action:original-result");
    expect(receipt.failure?.boundary).toBe("owner-persistence");
    expect(receipt.retryAllowed).toBe(false);
  });
  for (const [name, mutate] of [
    [
      "no opt-in",
      (i: ReturnType<typeof directInput>) => {
        i.imageOptIn = false;
      },
    ],
    [
      "too narrow",
      (i: ReturnType<typeof directInput>) => {
        i.imageEvidence![imageRef]!.width = 279;
      },
    ],
    [
      "too tall",
      (i: ReturnType<typeof directInput>) => {
        i.imageEvidence![imageRef]!.height = 1569;
      },
    ],
    [
      "not decoded",
      (i: ReturnType<typeof directInput>) => {
        i.imageEvidence![imageRef]!.decoded = false as true;
      },
    ],
    [
      "base64 byte mismatch",
      (i: ReturnType<typeof directInput>) => {
        i.assets![imageRef] = { type: "image", format: "png", data_base64: "AAAA" };
      },
    ],
    [
      "missing presence binding",
      (i: ReturnType<typeof directInput>) => {
        i.boxes = {};
      },
    ],
    [
      "unknown action answer",
      (i: ReturnType<typeof directInput>) => {
        i.actions[0]!.arguments.missing = { kind: "answer", questionId: "foreign" };
      },
    ],
  ] as const)
    test(`image/action request ${name} rejects during public preparation`, () => {
      const i = directInput();
      mutate(i);
      expect(() => prepareDirect(i)).toThrow();
    });
  test("expanded image budget includes references added through projected scope", () => {
    const i = directInput();
    i.event = semanticEvent();
    i.lens.taskGoal = `${imageRef} ${imageRef} ${imageRef}`;
    i.questions = { relevance: { type: "noul", instructions: "Is the content relevant?" } };
    i.actions = [];
    i.boxes = {};
    i.imageEvidence![imageRef]!.processedByteLength = 8 * 1024 * 1024;
    // Three references in requiredContext plus the same three in projected scope:
    // 48MiB before any text, exceeding the official 32MiB expanded-request limit.
    expect(() => prepareDirect(i)).toThrow("DIRECT_IMAGE_REQUEST_BOUND");
    i.imageEvidence![imageRef]!.processedByteLength = 1024 * 1024;
    expect(prepareDirect(i).status).toBe("prepared-direct");
  });
  test("expanded image budget counts actual string replacements inside directBoundary metadata", () => {
    const i = directInput();
    i.actions[0]!.arguments.metadata = { kind: "literal", value: `${imageRef} ${imageRef}` };
    i.imageEvidence![imageRef]!.processedByteLength = 10 * 1024 * 1024;
    // One actual photo, one Box binding and two metadata literals exceed 32MiB.
    expect(() => prepareDirect(i)).toThrow("DIRECT_IMAGE_REQUEST_BOUND");
    i.imageEvidence![imageRef]!.processedByteLength = 1024 * 1024;
    expect(prepareDirect(i).status).toBe("prepared-direct");
  });
  test("metadata-only asset references do not satisfy actual image representation", () => {
    const i = directInput();
    i.event = semanticEvent();
    expect(() => prepareDirect(i)).toThrow("DIRECT_IMAGE_NOT_REPRESENTED");
  });
  test("question-type metadata cannot substitute for an image reference in actual content", () => {
    const i = directInput();
    i.event = semanticEvent();
    i.questions = {
      independentCheck: {
        type: "noul",
        instructions: "Does the admitted evidence support the task?",
      },
    };
    i.actions = [];
    i.boxes = {};
    i.assets = { noul: i.assets![imageRef]! };
    i.imageEvidence = { noul: imageOwnerEvidence() };
    // Official replacement only visits state, instructions and criteria. The
    // question type is protocol metadata, so this asset is actually unreferenced.
    expect(() => prepareDirect(i)).toThrow("DIRECT_IMAGE_NOT_REPRESENTED");
  });
});
