import { describe, expect, test } from "bun:test";
import {
  JEV_MODEL,
  type SemanticEvent,
  type WholeLens,
  type NativeBinding,
  type ChildAssignment,
} from "./contract.ts";
import { canonicalJson, semanticDigest, qualifyChildAssignmentAuthority } from "./policy.ts";
import {
  prepareWorkflowDecision,
  normalizeWorkflowReply,
  recordNativeDelegationReceipt,
  recordChildArtifactDisposition,
  type WorkflowInput,
  type WorkflowEvidence,
  type PreparedWorkflowDecision,
  type CurrentWorkflowBinding,
  type NativeDelegationObservation,
} from "./workflows.ts";

const childAssignment = (): ChildAssignment => ({
  schema: "jev.child-assignment.v1",
  assignmentId: "child-assignment",
  revision: 1,
  status: "admitted",
  actorId: "child-actor",
  logical: {
    domainId: "native-harness",
    projectId: "project",
    workId: "work",
    issueId: "issue",
    specId: "spec",
    specRevision: 1,
    planId: "plan",
    planRevision: 1,
    taskId: "child-task",
  },
  sourceRevision: "source-v1",
  sourceClosure: { path: "closure.json", sha256: "a".repeat(64) },
  packetDigest: "b".repeat(64),
  parentAuthority: {
    assignmentId: "parent-assignment",
    revision: 1,
    status: "active",
    ownerId: "lead",
    domainId: "native-harness",
    projectId: "project",
    workId: "work",
    authorityReference: { path: "authority.json", sha256: "a".repeat(64) },
  },
  route: { kind: "native-child" },
});
const assignmentReference = (a = childAssignment()) => ({
  assignmentId: a.assignmentId,
  revision: a.revision,
  assignmentDigest: semanticDigest(a),
});
const childAuthority = (a = childAssignment()) => ({
  parentAuthority: structuredClone(a.parentAuthority),
  childAssignment: assignmentReference(a),
  childStatus: a.status,
  carryForward: null,
});
const binding = (): NativeBinding => ({
  runtime: "native-codex",
  sessionId: "root",
  agentId: null,
  agentType: null,
  parentSessionId: null,
  rootSessionId: "root",
  lineageSessionIds: ["root"],
  profileId: null,
  nativeTaskId: null,
  nativeRunId: null,
  logical: null,
  admissionRef: "owner:root",
});
const lens = (): WholeLens => ({
  currentAuthority: { text: "Improve scoped native Jev consumers.", sourceRefs: ["user:current"] },
  wholeObjective: "Governed Ontology Data/Logic/Action/Security and useful native work.",
  taskGoal: "Review a useful bounded child result.",
  acceptedWhy: [
    {
      text: "Preserve purpose and original decisions across sessions.",
      attribution: "current-user",
      sourceRefs: ["user:why"],
    },
  ],
  acceptedDecisions: [{ id: "decision:owners", revision: "1", sourceRefs: ["user:owners"] }],
  owners: [
    { role: "native", id: "codex" },
    { role: "domain", id: "hermes-default" },
  ],
  dependencies: [{ id: "common-source", revision: "source-v1", currentness: "current" }],
  completedEffectRefs: ["effect:old-routing-consumed"],
  uncertainEffectRefs: ["effect:unknown"],
  remainingAcceptance: ["whole:operator-delivery"],
});
const event = (): SemanticEvent => ({
  eventId: "event:1",
  kind: "worker_result",
  binding: binding(),
  sourceRevision: "source-v1",
  materialRevision: "result-v1",
  material: true,
  privacy: "admitted",
  externalEvidence: false,
  facts: {
    observed: "Actual parent-owned source/check result; child initial plaintext unavailable.",
  },
});
const evidence = (id = "source"): WorkflowEvidence => ({
  id,
  sourceRef: `source:${id}`,
  sha256: "a".repeat(64),
  revision: "1",
  currentness: "current",
  admissionRef: "owner:source",
  content: "Exact useful source and independently observed result.",
});
const delegation = (): Extract<WorkflowInput, { kind: "delegation" }> => ({
  kind: "delegation",
  taskPacket: {
    id: "task:source-review",
    goal: "Review exact owned source.",
    packetDigest: "b".repeat(64),
    requiredPostconditionIds: ["exact-source-reviewed"],
    expectedChild: childAssignment(),
  },
  profiles: [
    {
      profileId: "sol-low",
      model: "gpt-6.1-sol",
      effort: "low",
      availability: "available",
      applicability: "applicable",
      authorizationRef: "user:child-profiles",
      sourceRefs: ["native:profile-catalog"],
    },
    {
      profileId: "sol-ultra",
      model: "gpt-6.1-sol",
      effort: "ultra",
      availability: "available",
      applicability: "applicable",
      authorizationRef: "user:child-profiles",
      sourceRefs: ["native:profile-catalog"],
    },
  ],
});
const review = (): Extract<WorkflowInput, { kind: "worker_review" }> => ({
  kind: "worker_review",
  resultId: "result:1",
  resultDigest: "c".repeat(64),
  evidence: [evidence()],
  criteria: [
    {
      id: "source-check",
      description: "Exact source checks passed and evidence is current.",
      evidenceIds: ["source"],
    },
  ],
});
const prepare = (workflow: WorkflowInput): PreparedWorkflowDecision => {
  const result = prepareWorkflowDecision({
    requestId: "request:1",
    event: event(),
    lens: lens(),
    workflow,
  });
  if (result.status !== "prepared") throw Error(`expected prepared, got ${result.status}`);
  return result;
};
const current = (p: PreparedWorkflowDecision): CurrentWorkflowBinding => ({
  authorized: true,
  childAuthority: childAuthority(
    p.workflow.kind === "delegation" ? p.workflow.taskPacket.expectedChild : undefined,
  ),
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  workflowDigest: p.workflowDigest,
  evidenceRefs: ["native:fresh-owner-source-task-check"],
});
const reply = (
  p: PreparedWorkflowDecision,
  signals: Record<string, number> = {},
  selected = "sol-low",
  confidence = 0.95,
): Record<string, unknown> => ({
  schema:
    p.request.schema === "jev.semantic-request.v2"
      ? "jev.semantic-result.v2"
      : "jev.semantic-result.v1",
  ...(p.request.consumptionPolicy
    ? { consumptionPolicy: p.request.consumptionPolicy, requestDigest: p.requestDigest }
    : {}),
  status: "ok",
  requestId: p.request.requestId,
  stateDigest: p.request.stateDigest,
  purpose: "semantic_bundle",
  requestedModel: JEV_MODEL,
  returnedModel: JEV_MODEL,
  sourcePostconditionVerified: true,
  executionAuthorityGranted: false,
  automaticWrite: false,
  answers: Object.fromEntries(
    Object.entries(p.request.questions).map(([key, q]) => [
      key,
      q.type === "noul"
        ? {
            type: "noul",
            noul:
              signals[p.questionMeanings[key]!] ??
              (p.questionMeanings[key]!.endsWith(":conflict") ? 0.02 : 0.98),
          }
        : {
            type: "choice",
            choice: selected,
            confidence,
            probabilities: Object.fromEntries(
              Object.keys(q.criteria).map((id) => [id, id === selected ? 1 : 0]),
            ),
          },
    ]),
  ),
});
const observation = (): NativeDelegationObservation => ({
  observationId: "native:dispatch-observation",
  assignment: assignmentReference(),
  taskPacketDigest: delegation().taskPacket.packetDigest,
  requestedProfile: { profileId: "sol-low", model: "gpt-6.1-sol", effort: "low" },
  resolvedProfile: { profileId: "sol-low", model: "gpt-6.1-sol", effort: "low" },
  childBinding: {
    ...binding(),
    sessionId: "child",
    parentSessionId: "root",
    lineageSessionIds: ["root", "child"],
    admissionRef: "owner:child",
    logical: childAssignment().logical,
  },
  status: "observed",
  postconditions: [
    { id: "exact-source-reviewed", satisfied: true, evidenceRefs: ["result:checked-source"] },
  ],
  evidenceRefs: ["native:requested-profile", "native:resolved-profile"],
});

describe("ongoing parent-owned complete-profile recommendation", () => {
  test("every actual task prepares its own complete finite catalog with whole purpose/WHY and nullable native IDs", () => {
    const p = prepare(delegation());
    expect(canonicalJson(p.request.state.requiredContext)).toBe(canonicalJson(lens()));
    expect(canonicalJson(p.request.state.binding)).toBe(canonicalJson(binding()));
    expect(
      Object.keys(
        p.request.questions[p.selectionQuestionId!]!.type === "choice"
          ? (p.request.questions[p.selectionQuestionId!] as { criteria: object }).criteria
          : {},
      ),
    ).toEqual(["sol-low", "sol-ultra", "__abstain"]);
    const changed = delegation();
    changed.taskPacket.packetDigest = "d".repeat(64);
    changed.taskPacket.expectedChild!.packetDigest = changed.taskPacket.packetDigest;
    expect(prepare(changed).reuseKey).not.toBe(p.reuseKey);
    const advice = normalizeWorkflowReply(p, reply(p), current(p));
    expect(advice.selectedProfile?.effort).toBe("low");
    expect(advice.executionAuthorityGranted).toBe(false);
    expect(advice.taskCompletionGranted).toBe(false);
  });
  test("unavailable profiles never enter Choice and low-confidence or abstention has no effect", () => {
    const workflow = delegation();
    workflow.profiles[0]!.availability = "unknown";
    const p = prepare(workflow);
    expect(JSON.stringify(p.request.questions[p.selectionQuestionId!]!)).not.toContain(
      '"sol-low":',
    );
    expect(normalizeWorkflowReply(p, reply(p, {}, "sol-ultra", 0.2), current(p)).status).toBe(
      "abstained",
    );
    expect(
      normalizeWorkflowReply(p, reply(p, {}, "__abstain"), current(p)).selectedProfile,
    ).toBeNull();
  });
  test("private/carrier/unchanged events suppress paid preparation without a profile replay", () => {
    const input = { requestId: "r", event: event(), lens: lens(), workflow: delegation() };
    expect(
      prepareWorkflowDecision({ ...input, event: { ...event(), privacy: "private" } }).status,
    ).toBe("skipped");
    expect(
      prepareWorkflowDecision({ ...input, event: { ...event(), kind: "carrier" } }).status,
    ).toBe("skipped");
    const p = prepareWorkflowDecision(input);
    if (p.status !== "prepared") throw Error("fixture");
    expect(prepareWorkflowDecision({ ...input, previousConsumedReuseKey: p.reuseKey })).toEqual({
      status: "skipped",
      reason: "unchanged",
      reuseKey: p.reuseKey,
    });
  });
  test("available but inapplicable or unknown applicability profiles never enter recommendations or receipts", () => {
    for (const applicability of ["inapplicable", "unknown"] as const) {
      const workflow = delegation();
      workflow.profiles[0]!.applicability = applicability;
      const p = prepare(workflow);
      const question = p.request.questions[p.selectionQuestionId!]!;
      if (question.type !== "choice") throw Error("fixture");
      expect(Object.keys(question.criteria)).not.toContain("sol-low");
      const advice = normalizeWorkflowReply(p, reply(p, {}, "sol-ultra"), current(p));
      const forged = { ...advice, selectedProfile: workflow.profiles[0]! };
      expect(
        recordNativeDelegationReceipt({
          prepared: p,
          advice: forged,
          current: current(p),
          observation: observation(),
        }).status,
      ).toBe("held");
      workflow.profiles[1]!.applicability = applicability;
      expect(
        prepareWorkflowDecision({ requestId: "held", event: event(), lens: lens(), workflow })
          .status,
      ).toBe("held");
    }
  });
  test("fresh source/task/owner drift or changed prepared candidate cannot yield advice", () => {
    const p = prepare(delegation());
    expect(
      normalizeWorkflowReply(p, reply(p), { ...current(p), sourceRevision: "source-v2" }).status,
    ).toBe("held");
    const raw = reply(p);
    if (p.workflow.kind !== "delegation") throw Error("fixture");
    p.workflow.profiles[0]!.effort = "ultra";
    expect(normalizeWorkflowReply(p, raw, current(p)).status).toBe("held");
  });
});

describe("native requested/resolved/postcondition receipt", () => {
  test("actual observation verifies without inventing Task/Run IDs, dispatch or completion", () => {
    const p = prepare(delegation()),
      a = normalizeWorkflowReply(p, reply(p), current(p));
    const receipt = recordNativeDelegationReceipt({
      prepared: p,
      advice: a,
      current: current(p),
      observation: observation(),
    });
    expect(receipt.status).toBe("verified-native-delegation");
    expect(receipt.observation.childBinding?.nativeTaskId).toBeNull();
    expect(receipt.observation.childBinding?.nativeRunId).toBeNull();
    expect(receipt.taskCompletionGranted).toBe(false);
    expect(receipt.retryAllowed).toBe(false);
  });
  test("native fallback, wrong packet, foreign child, missing required postcondition and unknown outcomes are distinct", () => {
    const p = prepare(delegation()),
      a = normalizeWorkflowReply(p, reply(p), current(p));
    const run = (o: NativeDelegationObservation) =>
      recordNativeDelegationReceipt({
        prepared: p,
        advice: a,
        current: current(p),
        observation: o,
      });
    expect(
      run({
        ...observation(),
        resolvedProfile: { profileId: "sol-ultra", model: "gpt-6.1-sol", effort: "ultra" },
      }).status,
    ).toBe("held");
    expect(run({ ...observation(), taskPacketDigest: "d".repeat(64) }).status).toBe("held");
    expect(
      run({
        ...observation(),
        childBinding: {
          ...observation().childBinding!,
          sessionId: "foreign",
          rootSessionId: "foreign",
          parentSessionId: null,
          lineageSessionIds: ["foreign"],
        },
      }).status,
    ).toBe("held");
    expect(
      run({
        ...observation(),
        postconditions: [{ id: "unrelated", satisfied: true, evidenceRefs: ["source:unrelated"] }],
      }).status,
    ).toBe("native-postcondition-failed");
    expect(
      run({ ...observation(), status: "unknown", childBinding: null, resolvedProfile: null })
        .status,
    ).toBe("native-outcome-unknown");
  });
  test("postcondition IDs are compared structurally, including delimiter characters", () => {
    const workflow = delegation();
    workflow.taskPacket.requiredPostconditionIds = ["a|b", "c"];
    const p = prepare(workflow),
      advice = normalizeWorkflowReply(p, reply(p), current(p));
    const receipt = recordNativeDelegationReceipt({
      prepared: p,
      advice,
      current: current(p),
      observation: {
        ...observation(),
        postconditions: ["a", "b|c"].map((id) => ({
          id,
          satisfied: true,
          evidenceRefs: ["native:proof"],
        })),
      },
    });
    expect(receipt.status).toBe("native-postcondition-failed");
  });
  test("internally valid child ancestry must extend the exact parent lineage", () => {
    const parent: NativeBinding = {
      ...binding(),
      sessionId: "parent",
      parentSessionId: "A",
      lineageSessionIds: ["root", "A", "parent"],
    };
    const p = prepareWorkflowDecision({
      requestId: "nested:1",
      event: { ...event(), binding: parent },
      lens: lens(),
      workflow: delegation(),
    });
    if (p.status !== "prepared") throw Error("fixture");
    const advice = normalizeWorkflowReply(p, reply(p), current(p));
    const run = (ancestor: string) =>
      recordNativeDelegationReceipt({
        prepared: p,
        advice,
        current: current(p),
        observation: {
          ...observation(),
          childBinding: {
            ...observation().childBinding!,
            parentSessionId: "parent",
            lineageSessionIds: ["root", ancestor, "parent", "child"],
          },
        },
      });
    expect(run("A").status).toBe("verified-native-delegation");
    expect(run("B").status).toBe("held");
    expect(run("B").reason).toBe("native-child-parent-lineage-mismatch");
  });
  test("unexposed native profile stays null; a non-null contradictory native profile cannot verify", () => {
    const p = prepare(delegation()),
      advice = normalizeWorkflowReply(p, reply(p), current(p));
    const run = (profileId: string | null) =>
      recordNativeDelegationReceipt({
        prepared: p,
        advice,
        current: current(p),
        observation: {
          ...observation(),
          childBinding: { ...observation().childBinding!, profileId },
        },
      });
    expect(run(null).status).toBe("verified-native-delegation");
    expect(run(null).observation.childBinding!.profileId).toBeNull();
    expect(run("sol-low").status).toBe("verified-native-delegation");
    expect(run("sol-ultra").status).toBe("held");
    expect(run("sol-ultra").reason).toBe("native-child-profile-mismatch");
  });
});

describe("criterion-bound result review and extraction cascade", () => {
  test("aggregate satisfaction cannot hide a failing exact criterion", () => {
    const p = prepare(review());
    const advice = normalizeWorkflowReply(
      p,
      reply(p, { task_scope_satisfaction: 1, "criterion:source-check:support": 0.3 }),
      current(p),
    );
    expect(advice.disposition).toBe("escalate-to-native-owner");
    expect(advice.criterionSignals["source-check"]!.support).toBe(0.3);
    expect(advice.taskCompletionGranted).toBe(false);
  });
  test("only evidence bound to required criteria gates support; unused stale evidence does not hold independent work", () => {
    const w = review();
    w.evidence.push({ ...evidence("unused"), currentness: "stale" });
    const p = prepare(w);
    expect(normalizeWorkflowReply(p, reply(p), current(p)).disposition).toBe("review-supported");
    w.evidence[0]!.currentness = "unknown";
    const changed = prepare(w);
    expect(normalizeWorkflowReply(changed, reply(changed), current(changed)).disposition).toBe(
      "escalate-to-native-owner",
    );
  });
  test("shared-state independent per-field evidence checks gate extraction escalation", () => {
    const w: WorkflowInput = {
      ...review(),
      kind: "extraction",
      fields: [{ id: "source-count", value: 168, sourceEvidenceIds: ["source"] }],
    };
    const p = prepare(w);
    expect(Object.values(p.questionMeanings)).toContain("field:source-count:support");
    expect(
      normalizeWorkflowReply(p, reply(p, { "field:source-count:support": 0.4 }), current(p))
        .disposition,
    ).toBe("escalate-to-native-owner");
    expect(normalizeWorkflowReply(p, reply(p), current(p)).disposition).toBe("verify-extraction");
    expect(normalizeWorkflowReply(p, reply(p), current(p)).memoryAdmissionGranted).toBe(false);
    expect(new Set(Object.values(p.questionMeanings)).size).toBe(
      Object.keys(p.request.questions).length,
    );
  });
  test("an extracted field cannot hide an unsupported criterion with a similar ID", () => {
    const w: WorkflowInput = {
      ...review(),
      kind: "extraction",
      criteria: [
        {
          id: "field:count",
          description: "Required independent source condition.",
          evidenceIds: ["source"],
        },
      ],
      fields: [{ id: "count", value: 168, sourceEvidenceIds: ["source"] }],
    };
    const p = prepare(w);
    const advice = normalizeWorkflowReply(
      p,
      reply(p, { "criterion:field:count:support": 0.1, "field:count:support": 0.99 }),
      current(p),
    );
    expect(advice.criterionSignals["field:count"]!.support).toBe(0.1);
    expect(advice.fieldSignals.count!.support).toBe(0.99);
    expect(advice.disposition).toBe("escalate-to-native-owner");
  });
  test("foreign evidence IDs, duplicate criterion IDs and incomplete response coverage are refused", () => {
    const w = review();
    w.criteria[0]!.evidenceIds = ["absent"];
    expect(() => prepare(w)).toThrow("WORKFLOW_CRITERION_EVIDENCE_REQUIRED");
    const duplicate = review();
    duplicate.criteria.push({ ...duplicate.criteria[0]! });
    expect(() => prepare(duplicate)).toThrow("WORKFLOW_UNIQUE_IDS_REQUIRED");
    const p = prepare(review()),
      raw = reply(p);
    delete (raw.answers as Record<string, unknown>).q0;
    expect(() => normalizeWorkflowReply(p, raw, current(p))).toThrow("SEMANTIC_ANSWER_COVERAGE");
  });
  test("adversarial criterion and extraction field IDs are refused before a negative judgment could be omitted", () => {
    for (const id of ["__proto__", "constructor", "prototype"]) {
      const criterion = review();
      criterion.criteria[0]!.id = id;
      expect(() => prepare(criterion)).toThrow("WORKFLOW_UNIQUE_IDS_REQUIRED");
      const extraction: WorkflowInput = {
        ...review(),
        kind: "extraction",
        fields: [{ id, value: "unverified", sourceEvidenceIds: ["source"] }],
      };
      expect(() => prepare(extraction)).toThrow("WORKFLOW_UNIQUE_IDS_REQUIRED");
    }
    const p = prepare(review());
    const negative = normalizeWorkflowReply(
      p,
      reply(p, { "criterion:source-check:support": 0.1 }),
      current(p),
    );
    expect(negative.disposition).toBe("escalate-to-native-owner");
    expect(Object.getPrototypeOf(negative.criterionSignals)).toBeNull();
    expect(Object.getPrototypeOf(negative.fieldSignals)).toBeNull();
    expect(Object.getPrototypeOf(negative.rawJudgments)).toBeNull();
    expect(Object.values(negative.criterionSignals)).toEqual([{ support: 0.1, conflict: 0.02 }]);
  });
});

describe("deterministic local-first retrieval and source-owned continuity", () => {
  test("unique exact current local match returns without transport, while ambiguity produces source-bound screening", () => {
    const w: Extract<WorkflowInput, { kind: "retrieval" }> = {
      kind: "retrieval",
      query: "exact source identifier",
      candidates: [{ ...evidence(), localMatch: "exact", knownConflict: false }],
    };
    const result = prepareWorkflowDecision({
      requestId: "local:1",
      event: event(),
      lens: lens(),
      workflow: w,
    });
    expect(result.status).toBe("local");
    if (result.status === "local") {
      expect(result.advice.selectedEvidenceIds).toEqual(["source"]);
      expect(result.advice.deterministic).toBe(true);
    }
    w.candidates[0]!.localMatch = "ambiguous";
    w.candidates.push({ ...evidence("other"), localMatch: "ambiguous", knownConflict: false });
    const p = prepare(w);
    const advice = normalizeWorkflowReply(
      p,
      reply(p, { "evidence:other:conflict": 0.9 }),
      current(p),
    );
    expect(advice.selectedEvidenceIds).toEqual(["source"]);
    expect(canonicalJson(p.request.state.workflow)).toBe(canonicalJson(w));
  });
  test("known conflict or stale source never becomes a deterministic shortcut or accepted evidence", () => {
    const p = prepare({
      kind: "retrieval",
      query: "source",
      candidates: [{ ...evidence(), localMatch: "exact", knownConflict: true }],
    });
    expect(normalizeWorkflowReply(p, reply(p), current(p)).selectedEvidenceIds).toEqual([]);
    const stale = prepare({
      kind: "retrieval",
      query: "source",
      candidates: [
        { ...evidence(), currentness: "stale", localMatch: "exact", knownConflict: false },
      ],
    });
    expect(normalizeWorkflowReply(stale, reply(stale), current(stale)).status).toBe("abstained");
  });
  test("reserved candidate/profile IDs cannot enter evidence signal maps or finite profile criteria", () => {
    expect(() =>
      prepare({
        kind: "retrieval",
        query: "source",
        candidates: [{ ...evidence("__proto__"), localMatch: "ambiguous", knownConflict: false }],
      }),
    ).toThrow("WORKFLOW_UNIQUE_IDS_REQUIRED");
    const workflow = delegation();
    workflow.profiles[0]!.profileId = "__proto__";
    expect(() => prepare(workflow)).toThrow("WORKFLOW_UNIQUE_IDS_REQUIRED");
  });
  test("supported continuity nomination never supplies missing owner derivation/authority/Memory admission", () => {
    const p = prepare({
      kind: "continuity",
      nominations: [
        {
          id: "why",
          topicId: "odi:purpose",
          text: "Purpose is useful governed native work.",
          sourceRefs: ["user:why"],
          derivationRef: null,
          authorityRef: null,
        },
      ],
    });
    const advice = normalizeWorkflowReply(p, reply(p), current(p));
    expect(advice.nominationIds).toEqual(["why"]);
    expect(advice.disposition).toBe("nominate-for-owner-review");
    expect(advice.memoryAdmissionGranted).toBe(false);
    expect(advice.executionAuthorityGranted).toBe(false);
  });
  test("unknown required dependency and oversized mandatory packet hold rather than truncate original WHY", () => {
    const l = lens();
    l.dependencies[0]!.currentness = "unknown";
    expect(
      prepareWorkflowDecision({ requestId: "r", event: event(), lens: l, workflow: review() }),
    ).toEqual({ status: "held", reason: "workflow-dependency-currentness-unavailable" });
    const w = review();
    w.evidence = Array.from({ length: 6 }, (_, i) => ({
      ...evidence(`source-${i}`),
      content: "x".repeat(11000),
    }));
    w.criteria[0]!.evidenceIds = ["source-0"];
    expect(() => prepare(w)).toThrow("WORKFLOW_MANDATORY_CONTEXT_BOUND");
  });
});

describe("exact child and parent authority revisions", () => {
  const run = (o = observation(), modify?: (c: CurrentWorkflowBinding) => void) => {
    const p = prepare(delegation()),
      c = current(p);
    modify?.(c);
    return recordNativeDelegationReceipt({
      prepared: p,
      advice: normalizeWorkflowReply(p, reply(p), current(p)),
      current: c,
      observation: o,
    });
  };
  test.each(["domainId", "projectId", "workId", "issueId", "specId", "planId", "taskId"] as const)(
    "foreign child %s cannot verify",
    (key) => {
      const o = observation();
      o.childBinding!.logical![key] = "foreign";
      expect(run(o).status).toBe("held");
    },
  );
  test.each(["specRevision", "planRevision"] as const)("stale child %s cannot verify", (key) => {
    const o = observation();
    o.childBinding!.logical![key]++;
    expect(run(o).status).toBe("held");
  });
  test("null child logical, foreign assignment and stale assignment digest hold", () => {
    const o = observation();
    o.childBinding!.logical = null;
    expect(run(o).status).toBe("held");
    expect(
      run({ ...observation(), assignment: { ...assignmentReference(), revision: 2 } }).status,
    ).toBe("held");
    expect(
      run({
        ...observation(),
        assignment: { ...assignmentReference(), assignmentDigest: "0".repeat(64) },
      }).status,
    ).toBe("held");
  });
  test("historical unbound advice cannot produce a verified receipt", () => {
    const w = delegation();
    delete w.taskPacket.expectedChild;
    const p = prepare(w),
      c = current(p);
    expect(
      recordNativeDelegationReceipt({
        prepared: p,
        advice: normalizeWorkflowReply(p, reply(p), c),
        current: c,
        observation: observation(),
      }).status,
    ).toBe("held");
  });
  test("retired/revoked child or parent and stale parent revision cannot continue", () => {
    for (const status of ["revoked", "retired"] as const) {
      expect(
        run(observation(), (c) => {
          c.childAuthority!.parentAuthority.status = status;
        }).status,
      ).toBe("held");
      expect(
        run(observation(), (c) => {
          c.childAuthority!.childStatus = status;
        }).status,
      ).toBe("held");
    }
    expect(
      run(observation(), (c) => {
        c.childAuthority!.parentAuthority.revision++;
      }).status,
    ).toBe("held");
  });
  test("explicit exact successor carries forward without result permission or replay", () => {
    const a = childAssignment();
    const c: NonNullable<CurrentWorkflowBinding["childAuthority"]> = childAuthority(a);
    c.parentAuthority = { ...a.parentAuthority, revision: 2, ownerId: "successor" };
    c.carryForward = {
      schema: "jev.child-carry-forward.v1",
      from: a.parentAuthority,
      to: c.parentAuthority,
      childAssignment: assignmentReference(a),
      successorOwnerDecision: { path: "successor.json", sha256: "c".repeat(64) },
      pendingAndUnknownEffects: [{ path: "unknown-effects.json", sha256: "d".repeat(64) }],
      permitsReplay: false,
    };
    expect(qualifyChildAssignmentAuthority(a, c).status).toBe("carried-forward");
    expect(
      run(observation(), (current) => {
        current.childAuthority = c;
      }).status,
    ).toBe("verified-native-delegation");
    c.carryForward.childAssignment.revision++;
    expect(qualifyChildAssignmentAuthority(a, c).status).toBe("held");
  });
  test("late result remains historical evidence under changed authority", () => {
    const a = childAssignment(),
      result = {
        assignment: assignmentReference(a),
        logical: a.logical,
        sourceRevision: a.sourceRevision,
        sourceClosure: a.sourceClosure,
        packetDigest: a.packetDigest,
        artifact: { path: "late-result.json", sha256: "e".repeat(64) },
      };
    const historical = recordChildArtifactDisposition({ assignment: a, result, readoption: null });
    expect(historical.status).toBe("historical-evidence-only");
    expect(historical.result).toEqual(result);
    const c = childAuthority(a);
    c.parentAuthority.status = "retired";
    expect(
      recordChildArtifactDisposition({
        assignment: a,
        result,
        readoption: { current: c, ownerDecision: a.sourceClosure },
      }).status,
    ).toBe("historical-evidence-only");
    expect(
      recordChildArtifactDisposition({
        assignment: a,
        result,
        readoption: { current: childAuthority(a), ownerDecision: a.sourceClosure },
      }).status,
    ).toBe("explicitly-readopted-artifact");
    expect(historical.continuedEffectsAuthorizedByResult).toBe(false);
    expect(historical.permitsReplay).toBe(false);
  });
  test("independent sessions use explicit observed assignment without fabricated ancestry", () => {
    const o = observation(),
      parent = event();
    o.childBinding = {
      ...binding(),
      runtime: "native-hermes",
      sessionId: "independent",
      logical: childAssignment().logical,
      rootSessionId: null,
      parentSessionId: null,
      lineageSessionIds: [],
    };
    const w = delegation(),
      a = w.taskPacket.expectedChild!;
    a.route = {
      kind: "logical-assignment",
      parentBindingDigest: semanticDigest(parent.binding),
      childBindingDigest: semanticDigest(o.childBinding),
      relationEvidenceRefs: ["native:observed-assignment"],
    };
    o.assignment = assignmentReference(a);
    const p = prepare(w),
      c = current(p),
      advice = normalizeWorkflowReply(p, reply(p), c);
    expect(
      recordNativeDelegationReceipt({ prepared: p, advice, current: c, observation: o }).status,
    ).toBe("verified-native-delegation");
    expect(o.childBinding.parentSessionId).toBeNull();
    o.childBinding.sessionId = "replacement";
    expect(
      recordNativeDelegationReceipt({ prepared: p, advice, current: c, observation: o }).status,
    ).toBe("held");
  });
});

describe("W02 fixed-role qualification before semantic preparation", () => {
  const fixed = (): Extract<WorkflowInput, { kind: "delegation" }> => {
    const workflow = delegation();
    workflow.rolePolicy = {
      schema: "jev.fixed-role-policy.v1",
      role: "ordinary-child",
      allowedTuples: [{ model: "gpt-6.1-sol", effort: "high" }],
      authorizationRef: "user:fixed-child",
      sourceRevision: "source-v1",
      capabilityEvidenceRefs: ["native:fixed-capability"],
    };
    workflow.profiles = [{ ...workflow.profiles[0]!, profileId: "sol-high", effort: "high" }];
    return workflow;
  };
  test("qualified singleton omits profile selection and keeps all independent semantic purposes", () => {
    const p = prepare(fixed());
    expect(p.selectionQuestionId).toBeNull();
    expect(Object.values(p.questionMeanings)).toEqual([
      "task_scope_satisfaction",
      "worker_supervision",
      "engineering_coverage",
    ]);
    const fresh = { ...current(p), rolePolicyDigest: semanticDigest(fixed().rolePolicy) };
    const advice = normalizeWorkflowReply(p, reply(p), fresh);
    expect(advice.status).toBe("advice");
    expect(advice.selectedProfile?.effort).toBe("high");
    expect(advice.deterministic).toBe(true);
    expect(normalizeWorkflowReply(p, reply(p), current(p)).status).toBe("held");
    const o = observation();
    o.requestedProfile = o.resolvedProfile = {
      profileId: "sol-high",
      model: "gpt-6.1-sol",
      effort: "high",
    };
    expect(
      recordNativeDelegationReceipt({ prepared: p, advice, current: fresh, observation: o }).status,
    ).toBe("verified-native-delegation");
    o.resolvedProfile = { ...o.requestedProfile, effort: "low" };
    expect(
      recordNativeDelegationReceipt({ prepared: p, advice, current: fresh, observation: o }).status,
    ).toBe("held");
  });
  test("zero tuples hold before semantic event preparation and off-policy profiles cannot enter routing", () => {
    for (const profiles of [
      [],
      delegation().profiles,
      [{ ...fixed().profiles[0]!, availability: "unknown" as const }],
    ]) {
      const workflow = { ...fixed(), profiles };
      // Even malformed semantic facts are never reached once the fixed capability set is empty.
      expect(
        prepareWorkflowDecision({
          requestId: "zero",
          event: { ...event(), facts: undefined as never },
          lens: lens(),
          workflow,
        }),
      ).toEqual({ status: "held", reason: "no-authorized-available-native-profile" });
    }
    const workflow = fixed();
    workflow.profiles.push({
      ...workflow.profiles[0]!,
      profileId: "reviewer-exception",
      model: "gpt-6-astra",
      effort: "xhigh",
    });
    expect(prepare(workflow).selectionQuestionId).toBeNull();
    workflow.rolePolicy!.allowedTuples = [{ model: "gpt-6-astra", effort: "xhigh" }];
    expect(() => prepare(workflow)).toThrow("WORKFLOW_FIXED_ROLE_POLICY_REQUIRED");
  });
  test("explicit future multi-candidate role routing retains its question", () => {
    const workflow = fixed();
    workflow.rolePolicy!.role = "explicit-routing";
    workflow.rolePolicy!.allowedTuples.push({ model: "gpt-6.1-sol", effort: "low" });
    workflow.profiles.push(delegation().profiles[0]!);
    expect(prepare(workflow).selectionQuestionId).not.toBeNull();
  });
});
test("direct-v2 delegation requires typed role policy before any semantic question preparation", () => {
  expect(
    prepareWorkflowDecision({
      requestId: "v2-no-policy",
      event: { ...event(), facts: undefined as never },
      lens: lens(),
      workflow: delegation(),
      consumptionPolicy: "jev.direct-selected-answer.v2",
    }),
  ).toEqual({ status: "held", reason: "direct-delegation-fixed-role-policy-required" });
});
test("explicit direct-v2 multi-profile policy consumes the selected allowed low-confidence tie without a second judge", () => {
  const workflow = delegation();
  workflow.rolePolicy = {
    schema: "jev.fixed-role-policy.v1",
    role: "explicit-routing",
    allowedTuples: [
      { model: "gpt-6.1-sol", effort: "low" },
      { model: "gpt-6.1-sol", effort: "ultra" },
    ],
    authorizationRef: "user:explicit-future-routing",
    sourceRevision: "source-v1",
    capabilityEvidenceRefs: ["native:qualified-capability"],
  };
  const p = prepare(workflow),
    raw = reply(p, {}, "sol-low", 0);
  (raw.answers as Record<string, unknown>)[p.selectionQuestionId!] = {
    type: "choice",
    choice: "sol-low",
    confidence: 0,
    probabilities: { "sol-low": 0.5, "sol-ultra": 0.5, __abstain: 0 },
  };
  const advice = normalizeWorkflowReply(p, raw, {
    ...current(p),
    rolePolicyDigest: semanticDigest(workflow.rolePolicy),
  });
  expect(advice.status).toBe("advice");
  expect(advice.selectedProfile?.profileId).toBe("sol-low");
  expect(advice.deterministic).toBe(false);
});
