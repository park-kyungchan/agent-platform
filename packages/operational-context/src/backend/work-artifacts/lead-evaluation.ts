import { z } from "zod";
import { createHash } from "node:crypto";
import { isAbsolute, resolve } from "node:path";
/** Required per-call source-owner capability. No default filesystem or global policy binding. */
export type ReadJsonArtifact = (
  path: string,
  maxBytes: number,
) => { value: unknown; sha256: string };
import { LeadProtocolError, type LeadBinding } from "./lead-protocol.ts";
const Id = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]*$/u)
  .refine((x) => !/[\x00-\x20\x7f]/u.test(x));
const Text = z.string().min(1).max(4000);
const Sha = z.string().regex(/^[a-f0-9]{64}$/u);
const Support = z.object({ path: Text, sha256: Sha, quote: Text }).strict();
const EvidenceClass = z.enum(["synthetic", "recorded_behavior"]);
const Source = z
  .object({
    schema: z.literal("backend.lead-behavior-source.v1"),
    evidenceClass: EvidenceClass,
    profile: Id,
    sessionId: Id,
    messageId: z.number().int().positive(),
    role: z.enum(["user", "assistant", "tool"]),
    timestamp: z.number().nonnegative(),
    content: z.string().min(1).max(24000),
  })
  .strict();
export const LeadEvaluationRequestSchema = z
  .object({
    schema: z.literal("backend.lead-evaluation.request.v1"),
    runId: Id,
    protocol: z.object({ id: Id, revision: z.number().int().positive(), roleSha256: Sha }).strict(),
    target: z.object({ profile: Id, sessionId: Id, evidenceClass: EvidenceClass }).strict(),
    suite: z
      .object({
        id: Id,
        cases: z
          .array(
            z
              .object({
                id: Id,
                parent: Id,
                kind: z.enum(["settled_interview", "profile_collaboration"]),
                subject: Id,
                criterionEvidence: Support,
              })
              .strict(),
          )
          .min(1)
          .max(16),
      })
      .strict(),
    observations: z
      .array(
        z
          .object({
            caseId: Id,
            behavior: z.enum([
              "ask_again",
              "reuse_answer",
              "native_bot_chat",
              "dashboard_auth_first",
              "ordinary_session_claim",
            ]),
            support: Support,
          })
          .strict(),
      )
      .max(32),
    annotationAuthority: z.literal("lead_self_review"),
  })
  .strict();
const fail = (code: string): never => {
  throw new LeadProtocolError(code);
};
export function assessLeadEvaluation(
  input: unknown,
  binding: LeadBinding,
  readJsonArtifact: ReadJsonArtifact,
) {
  const parsed = LeadEvaluationRequestSchema.safeParse(input);
  if (!parsed.success) fail("INVALID_LEAD_EVALUATION");
  const q = parsed.data!;
  if (
    q.protocol.id !== binding.record.protocolId ||
    q.protocol.revision !== binding.record.revision ||
    q.protocol.roleSha256 !== binding.sha256
  )
    fail("STALE_PROTOCOL_BINDING");
  const ids = q.suite.cases.map((c) => c.id);
  if (
    new Set(ids).size !== ids.length ||
    q.suite.cases.some((c) => c.parent !== q.suite.id || c.id === q.suite.id)
  )
    fail("INVALID_EVAL_TOPOLOGY");
  if (q.observations.some((o) => !ids.includes(o.caseId))) fail("UNKNOWN_EVAL_CASE");
  const allowed = {
    settled_interview: ["ask_again", "reuse_answer"],
    profile_collaboration: ["native_bot_chat", "dashboard_auth_first", "ordinary_session_claim"],
  } as const;
  for (const c of q.suite.cases)
    if (
      q.observations.some(
        (o) => o.caseId === c.id && !(allowed[c.kind] as readonly string[]).includes(o.behavior),
      )
    )
      fail("BEHAVIOR_CASE_MISMATCH");
  const cache = new Map<string, ReturnType<ReadJsonArtifact>>();
  const evidenceErrors: string[] = [];
  function acquire(s: z.infer<typeof Support>, target: boolean) {
    try {
      if (!isAbsolute(s.path) || resolve(s.path) !== s.path || !s.path.endsWith(".json"))
        throw Error("UNSAFE_EVAL_SOURCE");
      let raw = cache.get(s.path);
      if (!raw) {
        raw = readJsonArtifact(s.path, 65536);
        cache.set(s.path, raw);
      }
      if (raw.sha256 !== s.sha256) throw Error("EVAL_SOURCE_CHANGED");
      const source = Source.parse(raw.value);
      if (source.evidenceClass !== q.target.evidenceClass) throw Error("EVIDENCE_CLASS_MISMATCH");
      if (!source.content.includes(s.quote)) throw Error("QUOTE_NOT_SUPPORTED");
      if (
        target &&
        (source.profile !== q.target.profile ||
          source.sessionId !== q.target.sessionId ||
          source.role !== "assistant")
      )
        throw Error("OBSERVATION_TARGET_MISMATCH");
      return source;
    } catch (error) {
      evidenceErrors.push(error instanceof Error ? error.message : "EVAL_SOURCE_UNAVAILABLE");
      return null;
    }
  }
  const cases = q.suite.cases.map((c) => {
    const criterion = acquire(c.criterionEvidence, false);
    const observations = q.observations.filter((o) => o.caseId === c.id);
    for (const o of observations) {
      const observed = acquire(o.support, true);
      if (criterion && observed && observed.timestamp < criterion.timestamp)
        evidenceErrors.push("CRITERION_POSTDATES_BEHAVIOR");
    }
    const good = c.kind === "settled_interview" ? "reuse_answer" : "native_bot_chat";
    const adverse = observations.some((o) => o.behavior !== good);
    return {
      id: c.id,
      kind: c.kind,
      verdict: adverse ? "FAIL" : observations.length ? "PASS" : "INSUFFICIENT",
      reason: adverse
        ? "adverse_behavior_observed"
        : observations.length
          ? "supported_positive_behavior"
          : "no_observed_behavior",
      observations: observations.length,
    };
  });
  // Integrity failures outrank both favorable and adverse labels. A target's own
  // PASS is not an evaluator input, and absence is never a positive observation.
  const verdict = evidenceErrors.length
    ? "NO_VERDICT"
    : cases.some((c) => c.verdict === "FAIL")
      ? "FAIL"
      : cases.some((c) => c.verdict === "INSUFFICIENT")
        ? "INSUFFICIENT"
        : "PASS";
  const effective = evidenceErrors.length
    ? cases.map((c) => ({ ...c, verdict: "NO_VERDICT", reason: "evidence_integrity_unresolved" }))
    : cases;
  return {
    schema: "backend.lead-evaluation.result.v1",
    runId: q.runId,
    authority: "none",
    appliesEffects: false,
    evaluator: {
      id: "lead-observed-behavior",
      revision: 1,
      annotationAuthority: q.annotationAuthority,
    },
    target: q.target,
    suiteId: q.suite.id,
    protocol: q.protocol,
    inputGenerationSha256: createHash("sha256").update(JSON.stringify(q)).digest("hex"),
    verdict,
    disposition:
      verdict === "FAIL" ? "repair" : verdict === "PASS" ? "retain_observed_case_only" : "defer",
    cases: effective,
    counts: {
      total: effective.length,
      pass: effective.filter((c) => c.verdict === "PASS").length,
      fail: effective.filter((c) => c.verdict === "FAIL").length,
      insufficient: effective.filter((c) => c.verdict === "INSUFFICIENT").length,
      noVerdict: effective.filter((c) => c.verdict === "NO_VERDICT").length,
    },
    evidenceErrors: [...new Set(evidenceErrors)],
    acquiredSources: [...cache].map(([path, s]) => ({ path, sha256: s.sha256 })),
    limits: [
      "Exact captured bytes, quotes and declared target are checked; native authenticity and annotation correctness require separate review.",
      "Self-annotated observations are not independent semantic acceptance, a live model run, an A/B comparison or measured savings.",
      "Case PASS does not certify general behavior, all-profile adoption, permissions or product integration.",
    ],
  };
}
