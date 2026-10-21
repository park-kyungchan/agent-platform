import { describe, expect, test } from "bun:test";
import {
  DIRECT_POLICY,
  prepareSemanticDecision,
  prepareWorkflowDecision,
  prepareDirectDecision,
  normalizeSemanticReply,
  normalizeWorkflowReply,
  normalizeDirectReply,
  semanticDigest,
  type SemanticEvent,
  type WholeLens,
  type PreparedDecision,
  type PreparedWorkflowDecision,
  type PreparedDirectDecision,
} from "./index.ts";

// Actual common producers; every owner/identity/value here is fictional.
const event: SemanticEvent = {
  eventId: "w05-synthetic-event",
  kind: "worker_start",
  sourceRevision: "w05-synthetic-source",
  materialRevision: "1",
  material: true,
  privacy: "admitted",
  externalEvidence: false,
  facts: { report: "Synthetic source needs review." },
  binding: {
    runtime: "native-codex",
    sessionId: "w05-synthetic-session",
    agentId: null,
    agentType: null,
    parentSessionId: null,
    rootSessionId: null,
    lineageSessionIds: [],
    profileId: null,
    nativeTaskId: null,
    nativeRunId: null,
    logical: null,
    admissionRef: "synthetic:admission",
  },
};
const lens: WholeLens = {
  currentAuthority: { text: "Verify fictional source only.", sourceRefs: ["synthetic:scope"] },
  wholeObjective: "Preserve original owners and effect fences.",
  taskGoal: "Verify common producer to native parser and normalized consumer.",
  acceptedWhy: [
    { text: "Bind exact questions.", attribution: "current-user", sourceRefs: ["synthetic:why"] },
  ],
  acceptedDecisions: [],
  owners: [{ role: "native", id: "synthetic:owner" }],
  dependencies: [{ id: "source", revision: event.sourceRevision, currentness: "current" }],
  completedEffectRefs: ["synthetic:completed"],
  uncertainEffectRefs: ["synthetic:unknown"],
  remainingAcceptance: ["native:activation"],
};
function semantic(
  version: 1 | 2 = 2,
  purpose: "worker_supervision" | "engineering_coverage" = "worker_supervision",
) {
  const p = prepareSemanticDecision({
    requestId: `w05-semantic-v${version}`,
    event,
    lens,
    candidates: [],
    purposes: [purpose],
    ...(version === 2 ? { consumptionPolicy: DIRECT_POLICY } : {}),
  });
  if (p.status !== "prepared") throw Error("Expected actual semantic producer");
  return p;
}
function workflow(): PreparedWorkflowDecision {
  const p = prepareWorkflowDecision({
    requestId: "w05-workflow-v2",
    event,
    lens,
    workflow: {
      kind: "delegation",
      taskPacket: {
        id: "synthetic:packet",
        goal: "Review synthetic source.",
        packetDigest: "a".repeat(64),
        requiredPostconditionIds: ["synthetic:reviewed"],
      },
      rolePolicy: {
        schema: "jev.fixed-role-policy.v1",
        role: "ordinary-child",
        allowedTuples: [{ model: "gpt-6.1-sol", effort: "high" }],
        authorizationRef: "synthetic:fixed-role",
        sourceRevision: event.sourceRevision,
        capabilityEvidenceRefs: ["synthetic:capability"],
      },
      profiles: [
        {
          profileId: "synthetic-sol-high",
          model: "gpt-6.1-sol",
          effort: "high",
          availability: "available",
          applicability: "applicable",
          authorizationRef: "synthetic:profile",
          sourceRefs: ["synthetic:catalog"],
        },
      ],
    },
  });
  if (p.status !== "prepared") throw Error("Expected actual fixed-role workflow producer");
  return p;
}
function direct(): PreparedDirectDecision {
  const p = prepareDirectDecision({
    requestId: "w05-direct-v2",
    event,
    lens,
    opportunity: {
      role: "child",
      unit: "item",
      stableItemId: "synthetic:item",
      revision: "1",
      nativeHookId: "synthetic:unrun-hook",
      consumerId: "synthetic:consumer",
    },
    questions: {
      route: {
        type: "choice",
        instructions: "Which source route fits this report?",
        criteria: { review: "Review exact source.", hold: "Hold for source." },
      },
    },
    actions: [],
  });
  if (p.status !== "prepared-direct") throw Error("Expected actual direct producer");
  return p;
}
const native = (requests: unknown[], mode: "parse" | "roundtrip") => {
  const path = new URL(
    "../../../native-hermes-adapter/native/jev/test_w05_common_integration.py",
    import.meta.url,
  ).pathname;
  const run = Bun.spawnSync(["python3", "-B", path, `--${mode}`], {
    stdin: Buffer.from(JSON.stringify(requests)),
    stdout: "pipe",
    stderr: "pipe",
  });
  if (run.exitCode !== 0) throw Error(run.stderr.toString());
  return JSON.parse(run.stdout.toString()) as Record<string, unknown>[];
};
const currentWorkflow = (p: PreparedWorkflowDecision) => ({
  authorized: true,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  workflowDigest: p.workflowDigest,
  rolePolicyDigest: semanticDigest(p.workflow.kind === "delegation" ? p.workflow.rolePolicy : null),
  evidenceRefs: ["synthetic:current"],
});
function reply(p: PreparedDecision): Record<string, unknown> {
  return {
    schema: "jev.semantic-result.v2",
    consumptionPolicy: DIRECT_POLICY,
    requestId: p.request.requestId,
    requestDigest: p.requestDigest,
    stateDigest: p.request.stateDigest,
    purpose: "semantic_bundle",
    status: "ok",
    requestedModel: p.request.model,
    returnedModel: "jev-1.14.0",
    sourcePostconditionVerified: true,
    executionAuthorityGranted: false,
    automaticWrite: false,
    answers: Object.fromEntries(
      Object.keys(p.request.questions).map((k) => [k, { type: "noul", noul: 0.4 }]),
    ),
  };
}

describe("W05 actual common/native source repair", () => {
  test("unchanged semantic/workflow/direct/v1 requests cross actual parser, pinned SDK and one temporary ledger", () => {
    const s = semantic(),
      w = workflow(),
      d = direct(),
      v1 = semantic(1);
    const rows = native([s.request, w.request, d.request, v1.request], "roundtrip");
    const [sr, wr, dr, legacy] = rows;
    expect(normalizeSemanticReply(s, sr).executionAuthorityGranted).toBe(false);
    const advice = normalizeWorkflowReply(w, wr, currentWorkflow(w));
    expect(advice.selectedProfile).toMatchObject({ model: "gpt-6.1-sol", effort: "high" });
    expect(w.selectionQuestionId).toBeNull();
    expect(Object.values(w.questionMeanings).sort()).toEqual(
      ["task_scope_satisfaction", "delegation_coverage", "context_relevance"].sort(),
    );
    expect(normalizeDirectReply(d, dr).answers.route).toMatchObject({
      choice: "review",
      confidence: 0.01,
    });
    expect(normalizeSemanticReply(v1, legacy).status).toBe("decision");
    expect(s.request.imageOptIn).toBe(false);
    expect(s.request.state.semanticBoundary).toBeDefined();
    expect(d.request.state.semanticBoundary).toBeUndefined();
    expect(d.request.state.directBoundary).toBeDefined();
    for (let i = 0; i < rows.length; i++) {
      expect(rows[i]).toMatchObject({
        transportInvocations: 1,
        connectionLeases: 1,
        executionAuthorityGranted: false,
      });
    }
  });
  test("missing, conflicting, malformed, image and Box semantic boundaries refuse before transport", () => {
    const original = semantic().request;
    const changes = [
      (r: typeof original) => {
        delete r.state.semanticBoundary;
      },
      (r: typeof original) => {
        r.state.directBoundary = direct().request.state.directBoundary!;
      },
      (r: typeof original) => {
        r.state.semanticBoundary = {
          schema: "jev.semantic-boundary.v2",
          mode: "advice-only",
          executionAuthorityGranted: 0,
          automaticWrite: false,
          workerCompletionVerified: false,
        };
      },
      (r: typeof original) => {
        r.state.semanticBoundary = { ...(r.state.semanticBoundary as object), extra: false };
      },
      (r: typeof original) => {
        Object.assign(r, { imageOptIn: true });
      },
      (r: typeof original) => {
        Object.assign(r, { assets: { image: { type: "image", part: "image" } } });
      },
      (r: typeof original) => {
        Object.assign(r.questions.q0!, { type: "box" });
      },
    ];
    const requests = changes.map((change) => {
      const r = structuredClone(original);
      change(r);
      r.stateDigest = semanticDigest(r.state);
      return r;
    });
    for (const row of native(requests, "parse")) expect(row.status).toBe("held");
  });
  test("semantic and workflow require the exact full v2 request digest, including question identity", () => {
    const p = semantic(),
      other = semantic(2, "engineering_coverage"),
      w = workflow();
    expect(p.request.stateDigest).toBe(other.request.stateDigest);
    expect(Object.keys(p.request.questions)).toEqual(Object.keys(other.request.questions));
    expect(p.requestDigest).not.toBe(other.requestDigest);
    expect(normalizeSemanticReply(p, reply(p)).status).toBe("decision");
    for (const digest of [undefined, "f".repeat(64), other.requestDigest]) {
      expect(() => normalizeSemanticReply(p, { ...reply(p), requestDigest: digest })).toThrow(
        "SEMANTIC_RESPONSE_BINDING_OR_SOURCE_HELD",
      );
    }
    expect(() => normalizeSemanticReply(other, reply(p))).toThrow(
      "SEMANTIC_RESPONSE_BINDING_OR_SOURCE_HELD",
    );
    expect(normalizeWorkflowReply(w, reply(w), currentWorkflow(w)).status).toBe("advice");
    for (const digest of [undefined, "f".repeat(64), p.requestDigest]) {
      expect(() =>
        normalizeWorkflowReply(w, { ...reply(w), requestDigest: digest }, currentWorkflow(w)),
      ).toThrow("SEMANTIC_RESPONSE_BINDING_OR_SOURCE_HELD");
    }
  });
});
