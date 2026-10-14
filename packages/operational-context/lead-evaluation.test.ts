import { test, expect } from "bun:test";
import { createHash } from "node:crypto";
import {
  assessLeadEvaluation,
  type ReadJsonArtifact,
} from "./src/backend/work-artifacts/lead-evaluation";
import { LeadProtocolError, type LeadBinding } from "./src/backend/work-artifacts/lead-protocol";
const sha = (s: string) => createHash("sha256").update(s).digest("hex");
function fixture() {
  const value = {
      schema: "backend.lead-behavior-source.v1",
      evidenceClass: "synthetic",
      profile: "fixture",
      sessionId: "fixture",
      messageId: 1,
      role: "assistant",
      timestamp: 1,
      content: "Settled. Reused. Asked again.",
    },
    bytes = JSON.stringify(value),
    path = "/synthetic/owner/source.json",
    support = (quote: string) => ({ path, sha256: sha(bytes), quote }),
    binding = {
      record: { protocolId: "ontology-platform.e2e-lead", revision: 1 },
      sha256: "a".repeat(64),
    } as LeadBinding,
    q: any = {
      schema: "backend.lead-evaluation.request.v1",
      runId: "fixture",
      protocol: { id: "ontology-platform.e2e-lead", revision: 1, roleSha256: binding.sha256 },
      target: { profile: "fixture", sessionId: "fixture", evidenceClass: "synthetic" },
      suite: {
        id: "suite",
        cases: [
          {
            id: "case",
            parent: "suite",
            kind: "settled_interview",
            subject: "fixture",
            criterionEvidence: support("Settled."),
          },
        ],
      },
      observations: [{ caseId: "case", behavior: "reuse_answer", support: support("Reused.") }],
      annotationAuthority: "lead_self_review",
    };
  return { value, bytes, path, q, binding };
}
test("per-call owner reader receives exact65k bound and shared source is acquired once", () => {
  const f = fixture(),
    calls: any[] = [];
  const reader: ReadJsonArtifact = (path, maxBytes) => {
    calls.push({ path, maxBytes });
    return { value: f.value, sha256: sha(f.bytes) };
  };
  const r = assessLeadEvaluation(f.q, f.binding, reader);
  expect(r.verdict).toBe("PASS");
  expect(r.authority).toBe("none");
  expect(r.appliesEffects).toBe(false);
  expect(calls).toEqual([{ path: f.path, maxBytes: 65536 }]);
  expect(r.acquiredSources).toEqual([{ path: f.path, sha256: sha(f.bytes) }]);
});
test("each call receives its own owner capability without singleton source reuse", () => {
  const f = fixture();
  expect(
    assessLeadEvaluation(f.q, f.binding, () => ({ value: f.value, sha256: sha(f.bytes) })).verdict,
  ).toBe("PASS");
  const r = assessLeadEvaluation(f.q, f.binding, () => {
    throw Error("OWNER_POLICY_DENIED");
  });
  expect(r.verdict).toBe("NO_VERDICT");
  expect(r.evidenceErrors).toContain("OWNER_POLICY_DENIED");
});
test("adverse and missing observations retain distinct finite verdicts", () => {
  const f = fixture(),
    reader = () => ({ value: f.value, sha256: sha(f.bytes) });
  f.q.observations[0].behavior = "ask_again";
  f.q.observations[0].support.quote = "Asked again.";
  expect(assessLeadEvaluation(f.q, f.binding, reader).verdict).toBe("FAIL");
  f.q.observations = [];
  expect(assessLeadEvaluation(f.q, f.binding, reader).verdict).toBe("INSUFFICIENT");
});
test("integrity and quote/target/class failures outrank favorable labels", () => {
  for (const kind of ["hash", "quote", "target", "class"]) {
    const f = fixture();
    if (kind === "hash") f.q.observations[0].support.sha256 = "0".repeat(64);
    if (kind === "quote") f.q.observations[0].support.quote = "unbacked";
    if (kind === "target") f.q.target.sessionId = "foreign";
    if (kind === "class") f.q.target.evidenceClass = "recorded_behavior";
    expect(
      assessLeadEvaluation(f.q, f.binding, () => ({ value: f.value, sha256: sha(f.bytes) }))
        .verdict,
    ).toBe("NO_VERDICT");
  }
});
test("unsafe source path is denied before reader and invalidrequest preserves canonical error identity", () => {
  const f = fixture();
  let calls = 0;
  f.q.suite.cases[0].criterionEvidence.path = "../unsafe.json";
  f.q.observations = [];
  expect(
    assessLeadEvaluation(f.q, f.binding, () => {
      calls++;
      throw Error("unexpected");
    }).verdict,
  ).toBe("NO_VERDICT");
  expect(calls).toBe(0);
  expect(() =>
    assessLeadEvaluation({ passed: true }, f.binding, () => {
      throw Error("unexpected");
    }),
  ).toThrow(LeadProtocolError);
});
