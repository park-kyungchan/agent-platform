import { z } from "zod";
import { LeadProtocolRecordSchema } from "./lead-protocol.ts";

const Id = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/);
const Text = z.string().min(1).max(2400);
const Path = z.string().min(1).max(1024);
const Refs = z.array(Path).max(40);
const Decision = z
  .object({
    type: z.literal("decision"),
    id: Id,
    topic: Id,
    status: z.enum(["confirmed", "proposed"]),
    rule: Text,
    sourceRefs: Refs,
  })
  .strict();
const Constraint = z
  .object({
    type: z.literal("constraint"),
    id: Id,
    category: z.enum([
      "ownership",
      "effect",
      "context",
      "retention",
      "format",
      "evaluation",
      "language",
    ]),
    rule: Text,
    sourceRefs: Refs,
  })
  .strict();
const Coverage = z
  .object({
    type: z.literal("coverage"),
    id: Id,
    area: Text,
    roots: z.array(Path),
    consumer: Text,
    state: z.enum(["open", "accepted", "deferred"]),
    disposition: z.enum(["pending", "migrate", "retain", "retire", "defer"]),
    observation: Text,
    sourceRefs: Refs,
  })
  .strict();
const Task = z
  .object({
    type: z.literal("task"),
    id: Id,
    priority: z.number().int().min(0).max(3),
    status: z.enum(["completed", "proposed", "conditional"]),
    dependsOn: z.array(Id),
    actor: z.enum([
      "lead",
      "research",
      "implementation",
      "review",
      "domain_owner",
      "external_operator",
    ]),
    outcome: Text,
    candidateTargets: z.array(Path),
    gates: z.array(Text).min(1),
    executionAuthority: z.literal("none"),
  })
  .strict();
const Finding = z
  .object({
    type: z.literal("finding"),
    id: Id,
    classification: z.enum(["observation", "source_fact", "inference", "proposal", "correction"]),
    statement: Text,
    sourceRefs: Refs.min(1),
    limits: z.array(Text),
  })
  .strict();
const Outcome = z
  .object({
    type: z.literal("outcome"),
    id: Id,
    status: z.enum([
      "research_design_complete",
      "typed_conversion_self_validated",
      "unaccepted_candidate",
      "pending",
    ]),
    independentAcceptance: z.boolean(),
    acceptedScope: z.array(Text),
    notEstablished: z.array(Text).min(1),
    evidenceRefs: Refs.min(1),
  })
  .strict();
// A source-bound declaration, not an authority grant or a substitute for the native review consumer.
const Acceptance = z
  .object({
    type: z.literal("acceptance"),
    id: Id,
    decision: z.enum(["accept", "reject", "defer"]),
    scope: Text,
    candidateSha256: z.string().regex(/^[a-f0-9]{64}$/),
    reviewEvidence: z.object({ path: Path, sha256: z.string().regex(/^[a-f0-9]{64}$/) }).strict(),
    reviewerSessionId: Id,
    leadSessionId: Id,
    limits: z.array(Text).min(1),
  })
  .strict()
  .refine((x) => x.reviewerSessionId !== x.leadSessionId, "SELF_REVIEW_NOT_INDEPENDENT");
const Link = z
  .object({
    type: z.literal("link"),
    id: Id,
    relation: z.enum([
      "plan",
      "result",
      "checkpoint",
      "canonical_knowledge",
      "consumer_state",
      "native_owner",
      "history",
      "continuation",
      "recovery",
      "schema",
      "query",
      "validation",
    ]),
    target: Path,
    selectors: z.array(z.string()),
  })
  .strict();
const Action = z
  .object({
    type: z.literal("next_action"),
    id: Id,
    sequence: z.number().int().min(1),
    intent: Text,
    argv: z.array(z.string()),
    requiresFreshAdmission: z.boolean(),
    targetRefs: Refs,
  })
  .strict();
const Budget = z
  .object({
    type: z.literal("budget"),
    id: Id,
    category: z.enum([
      "research_assignments",
      "reviewer_assignments",
      "cron_occurrences",
      "bot_turns",
      "goal_occurrences",
    ]),
    approved: z.number().int().min(0),
    consumed: z.number().int().min(0),
    renewableByPointer: z.literal(false),
  })
  .strict()
  .refine((x) => x.consumed <= x.approved, "BUDGET_OVERSPENT");
const Run = z
  .object({
    type: z.literal("native_run_observation"),
    id: Id,
    sessionId: z.string().regex(/^\d{8}_\d{6}_[a-f0-9]+$/),
    role: z.literal("research"),
    model: z.literal("gpt-6-sol"),
    provider: z.literal("openai-codex"),
    reasoning: z.literal("high"),
    ended: z.literal(true),
    mainApiCalls: z.number().int().nonnegative(),
    hermesToolCalls: z.number().int().nonnegative(),
    evidenceRefs: Refs.min(1),
    establishes: z.literal("recorded_execution_not_independent_acceptance"),
  })
  .strict();
const Evaluation = z
  .object({
    type: z.literal("evaluation"),
    id: Id,
    evaluator: z.enum([
      "bun-tests",
      "typescript",
      "typed-query",
      "native-context-loader",
      "artifact-integrity",
      "scope-coverage",
    ]),
    targetRefs: Refs.min(1),
    verdict: z.enum(["pass", "fail"]),
    caseCount: z.number().int().nonnegative(),
    failedCases: z.number().int().nonnegative(),
    exitCode: z.number().int(),
    evidenceRefs: Refs.min(1),
    limitations: z.array(Text),
  })
  .strict()
  .refine(
    (e) =>
      e.failedCases <= e.caseCount &&
      (e.verdict !== "pass" || (e.failedCases === 0 && e.exitCode === 0)),
    "INCONSISTENT_EVALUATION",
  );
export const RecordSchema = z.discriminatedUnion("type", [
  Decision,
  Constraint,
  Coverage,
  Task,
  Finding,
  Outcome,
  Link,
  Action,
  Budget,
  Run,
  Evaluation,
  Acceptance,
  LeadProtocolRecordSchema,
]);
export type WorkRecord = z.infer<typeof RecordSchema>;
export const LegacyArtifactSchema = z
  .object({
    schema: z.enum(["backend.work-artifact.v1", "backend.work-artifact.v2"]),
    kind: z.enum([
      "program_plan",
      "current_work",
      "work_result",
      "kickoff",
      "native_context_binding",
    ]),
    id: Id,
    profile: z.literal("default"),
    projectId: z.literal("p_566ac0ba"),
    authority: z.literal("none"),
    observedAt: z.string().datetime(),
    sourceRefs: Refs.min(1),
    records: z.array(RecordSchema).min(1),
  })
  .strict();
// The old producer identity is immutable. New owner identity needs a new schema
// version, not a caller-selected alias for default evidence.
export const NamedArtifactSchema = LegacyArtifactSchema.extend({
  schema: z.literal("backend.work-artifact.v3"),
  profile: z.literal("ontology-control-plane"),
  projectId: z.literal("p_366c7ae4"),
}).strict();
// Native rename gets a new producer generation. Old source records remain literal
// and readable; this schema is not proof that a native rename occurred.
export const PlatformArtifactSchema = NamedArtifactSchema.extend({
  schema: z.literal("backend.work-artifact.v4"),
  profile: z.literal("ontology-platform"),
}).strict();
export const ArtifactSchema = z.discriminatedUnion("schema", [
  LegacyArtifactSchema,
  NamedArtifactSchema,
  PlatformArtifactSchema,
]);
export type Artifact = z.infer<typeof ArtifactSchema>;
export class ArtifactError extends Error {
  constructor(
    readonly code: string,
    detail = "",
  ) {
    super(`${code}${detail ? `: ${detail}` : ""}`);
  }
}
export function parseArtifact(input: unknown): Artifact {
  const parsed = ArtifactSchema.safeParse(input);
  if (!parsed.success)
    throw new ArtifactError(
      "INVALID_ARTIFACT",
      parsed.error.issues.map((i) => `${i.path.join(".")}:${i.code}`).join(";"),
    );
  const value = parsed.data;
  const ids = value.records.map((r) => r.id);
  if (new Set(ids).size !== ids.length) throw new ArtifactError("DUPLICATE_RECORD_ID");
  const tasks = value.records.filter((r): r is z.infer<typeof Task> => r.type === "task");
  const byId = new Map(tasks.map((t) => [t.id, t]));
  const visiting = new Set<string>();
  const done = new Set<string>();
  const visit = (id: string): void => {
    if (visiting.has(id)) throw new ArtifactError("TDG_CYCLE");
    if (done.has(id)) return;
    const task = byId.get(id);
    if (!task) throw new ArtifactError("UNKNOWN_TDG_DEPENDENCY", id);
    visiting.add(id);
    task.dependsOn.forEach(visit);
    visiting.delete(id);
    done.add(id);
  };
  tasks.forEach((t) => visit(t.id));
  if (
    value.kind === "program_plan" &&
    (!tasks.length || !value.records.some((r) => r.type === "coverage"))
  )
    throw new ArtifactError("INCOMPLETE_PLAN");
  if (value.kind === "work_result" && !value.records.some((r) => r.type === "outcome"))
    throw new ArtifactError("MISSING_OUTCOME");
  const accepted = value.records.filter(
    (r): r is z.infer<typeof Acceptance> => r.type === "acceptance" && r.decision === "accept",
  );
  const outcomes = value.records.filter((r) => r.type === "outcome");
  if (
    value.schema === "backend.work-artifact.v1" &&
    (value.records.some((r) => r.type === "acceptance") ||
      outcomes.some((r) => r.independentAcceptance))
  )
    throw new ArtifactError("ACCEPTANCE_REQUIRES_V2");
  if (
    outcomes.some((r) => r.independentAcceptance) &&
    (accepted.length !== 1 || outcomes.some((r) => !r.acceptedScope.length))
  )
    throw new ArtifactError("MISSING_ACCEPTANCE");
  if (
    accepted.length &&
    (value.kind !== "work_result" ||
      !outcomes.length ||
      outcomes.some((r) => !r.independentAcceptance))
  )
    throw new ArtifactError("INCONSISTENT_ACCEPTANCE");
  if (
    accepted.length &&
    outcomes.some(
      (r) =>
        r.status === "pending" ||
        r.status === "unaccepted_candidate" ||
        r.acceptedScope.length !== 1 ||
        r.acceptedScope[0] !== accepted[0]!.scope,
    )
  )
    throw new ArtifactError("INCONSISTENT_ACCEPTANCE");
  // Legacy v1 acceptedScope described self-validated delivery, not independent acceptance.
  // Preserve its meaning while keeping later-version declarations unambiguous.
  if (
    value.schema !== "backend.work-artifact.v1" &&
    outcomes.some((r) => !r.independentAcceptance && r.acceptedScope.length)
  )
    throw new ArtifactError("INCONSISTENT_ACCEPTANCE");
  return value;
}
export function projectArtifact(input: unknown, type?: WorkRecord["type"], id?: string) {
  const value = parseArtifact(input);
  const records = value.records.filter((r) => (!type || r.type === type) && (!id || r.id === id));
  if (id && records.length !== 1) throw new ArtifactError("UNKNOWN_RECORD", id);
  const common = {
    sourceArtifactId: value.id,
    kind: value.kind,
    authority: "none" as const,
    selectionComplete: true,
    records,
  };
  return value.schema === "backend.work-artifact.v4"
    ? {
        schema: "backend.work-artifact.projection.v3" as const,
        ...common,
        sourceIdentity: {
          schema: value.schema,
          profile: value.profile,
          projectId: value.projectId,
        },
      }
    : value.schema === "backend.work-artifact.v3"
      ? {
          schema: "backend.work-artifact.projection.v2" as const,
          ...common,
          sourceIdentity: {
            schema: value.schema,
            profile: value.profile,
            projectId: value.projectId,
          },
        }
      : { schema: "backend.work-artifact.projection.v1" as const, ...common };
}
export const RedirectSchema = z
  .object({
    schema: z.literal("backend.artifact-redirect.v1"),
    authority: z.literal("none"),
    canonicalPath: Path,
    archivedPath: Path,
    archivedSha256: z.string().regex(/^[a-f0-9]{64}$/),
  })
  .strict();
export const NativeAdapterSchema = z
  .object({
    schema: z.literal("backend.native-context-adapter.v1"),
    authority: z.literal("none"),
    canonicalPath: Path,
    canonicalSha256: z.string().regex(/^[a-f0-9]{64}$/),
    requiredReadBeforeWork: z.array(z.string()).min(1),
    currentWorkPath: Path,
    freshNativeOwnershipRequiredBeforeEffects: z.literal(true),
  })
  .strict();
export const MigrationSchema = z
  .object({
    schema: z.literal("backend.typed-artifact-migration.v1"),
    authority: z.literal("artifact_conversion_only"),
    sourceMessageId: z.literal(139244),
    observedAt: z.string().datetime(),
    status: z.enum(["prepared", "applied"]),
    entries: z
      .array(
        z
          .object({
            path: Path,
            canonicalPath: Path,
            archivedPath: Path,
            archivedSha256: z.string().regex(/^[a-f0-9]{64}$/),
            successorSha256: z.string().regex(/^[a-f0-9]{64}$/),
            effect: z.enum(["typed_redirect", "native_typed_adapter"]),
          })
          .strict(),
      )
      .min(1),
    recovery: z.literal(
      "Restore only a still-owned successor matching successorSha256; original bytes must match archivedSha256. Recheck native peer scope. Never reset native state or another writer.",
    ),
  })
  .strict();
export type Migration = z.infer<typeof MigrationSchema>;
export type ArtifactRedirect = z.infer<typeof RedirectSchema>;
export const jsonSchema = () => ({
  ...z.toJSONSchema(ArtifactSchema, { target: "draft-2020-12" }),
  $comment:
    "Shape schema generated from contract.ts. parseArtifact additionally enforces unique IDs, complete plan/result kinds, TDG reference closure/cycles and budget cross-field limits. Neither establishes semantic truth or authority.",
});
