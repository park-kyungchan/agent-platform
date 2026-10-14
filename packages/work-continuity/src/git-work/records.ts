import { validateArtifact } from "@palantirkc/ontology";
import { createHash } from "node:crypto";
import { assertLogicalIdentity } from "../logical-identity.ts";
import { validateTaskOperationalState } from "../operational-state.ts";
import type {
  DecisionRecord,
  EvidenceRecord,
  GitWorkReader,
  GitWorkRecord,
  JsonValue,
  NativeOccurrence,
  PrerequisiteReceipt,
  RecordReference,
  ResolvedSource,
  SourceReference,
  TaskDependency,
  TaskRecord,
  ValidationResult,
} from "./types.ts";

const text = (value: unknown): value is string =>
  typeof value === "string" &&
  value.trim().length > 0 &&
  value.length <= 12_000 &&
  !value.includes("\0");
function requireValue(condition: unknown, code: string): asserts condition {
  if (!condition) throw Error(code);
}
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const strings = (value: unknown): value is string[] =>
  Array.isArray(value) &&
  value.length <= 10_000 &&
  value.every(text) &&
  new Set(value).size === value.length;
const positive = (value: unknown) => Number.isSafeInteger(value) && Number(value) > 0;
const identityKeys = [
  "domainId",
  "projectId",
  "workId",
  "issueId",
  "specId",
  "specRevision",
  "planId",
  "planRevision",
  "taskId",
] as const;
const stateKeys = [
  "execution",
  "verification",
  "localAudit",
  "sharedAudit",
  "formalCompletion",
] as const;
const sameFields = (a: object, b: object, keys: readonly string[]) =>
  keys.every((key) => (a as Record<string, unknown>)[key] === (b as Record<string, unknown>)[key]);
/** A scoped source address; it does not allocate a native occurrence or change the logical Task ID. */
export function taskVerificationTarget(task: TaskRecord): string {
  return (
    task.artifact?.id ??
    `common-harness:task:${createHash("sha256")
      .update(JSON.stringify([task.domainId, task.projectId, task.workId, task.id]))
      .digest("hex")}`
  );
}
export function assertRecordReference(value: unknown): asserts value is RecordReference {
  requireValue(
    object(value) &&
      text(value.id) &&
      [
        "work",
        "spec",
        "plan",
        "input",
        "decision",
        "task",
        "relation",
        "effect",
        "effect-fence",
        "evidence",
      ].includes(String(value.kind)),
    "INVALID_RECORD_REFERENCE",
  );
  requireValue(
    value.kind === "spec" || value.kind === "plan"
      ? positive(value.revision)
      : value.revision === undefined,
    "CONTRACT_REVISION_REFERENCE_REQUIRED",
  );
}
export function recordKey(reference: RecordReference): string {
  assertRecordReference(reference);
  return `${reference.kind}:${reference.id}${reference.revision === undefined ? "" : `@${reference.revision}`}`;
}
export const outcome = (findings: readonly string[]): ValidationResult => ({
  valid: findings.length === 0,
  findings: [...new Set(findings)],
  authority: "none",
  nativePermissionGranted: false,
  semanticAcceptanceEstablished: false,
});

function sourceReference(value: unknown): void {
  requireValue(object(value), "INVALID_SOURCE_REFERENCE");
  const source = value as unknown as SourceReference;
  if (source.kind === "native") {
    requireValue(
      text(source.ownerId) &&
        text(source.locator) &&
        object(source.receipt) &&
        ["snapshot", "git"].includes(source.receipt.kind),
      "INVALID_NATIVE_SOURCE_REFERENCE",
    );
    sourceReference(source.receipt);
    return;
  }
  requireValue(
    ["snapshot", "git"].includes(source.kind) && text(source.path),
    "INVALID_SOURCE_REFERENCE",
  );
  requireValue(
    source.fragment === undefined || typeof source.fragment === "string",
    "INVALID_SOURCE_FRAGMENT",
  );
  requireValue(
    source.sha256 === undefined || /^[a-f0-9]{64}$/.test(source.sha256),
    "INVALID_SOURCE_HASH",
  );
  if (source.kind === "git")
    requireValue(
      text(source.repositoryId) && /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(source.commit),
      "INVALID_EXTERNAL_GIT_SOURCE",
    );
}
function occurrence(value: NativeOccurrence): void {
  requireValue(
    value && [value.runtimeId, value.attemptId, value.occurrenceId].every(text),
    "INVALID_NATIVE_OCCURRENCE",
  );
}
export function assertRecord(value: unknown): GitWorkRecord {
  requireValue(object(value), "INVALID_WORK_RECORD");
  const record = value as unknown as GitWorkRecord;
  requireValue(
    record.schema === "common.git-work.v1" &&
      [record.id, record.domainId, record.projectId, record.workId].every(text),
    "INVALID_RECORD_IDENTITY",
  );
  switch (record.kind) {
    case "work":
      requireValue(
        record.id === record.workId &&
          text(record.repositoryId) &&
          text(record.issueId) &&
          record.purpose &&
          text(record.purpose.objective) &&
          strings(record.purpose.successCriteria) &&
          record.purpose.successCriteria.length &&
          strings(record.purpose.inScope) &&
          strings(record.purpose.outOfScope) &&
          record.spec &&
          text(record.spec.id) &&
          positive(record.spec.revision) &&
          record.plan &&
          text(record.plan.id) &&
          positive(record.plan.revision) &&
          strings(record.governingDecisionIds) &&
          strings(record.historicalFenceIds),
        "INVALID_WORK_PURPOSE_OR_CONTRACT",
      );
      break;
    case "spec":
      requireValue(
        positive(record.revision) &&
          text(record.outcome) &&
          strings(record.criteria) &&
          record.criteria.length,
        "INVALID_SPEC",
      );
      break;
    case "plan":
      requireValue(
        positive(record.revision) &&
          text(record.specId) &&
          positive(record.specRevision) &&
          text(record.objective),
        "INVALID_PLAN",
      );
      break;
    case "input":
      requireValue(
        ["actual-user-answer", "source-owner-record"].includes(record.inputKind) &&
          record.origin &&
          text(record.origin.ownerId) &&
          text(record.origin.locator) &&
          ["native-user-message", "source-owner-capture"].includes(record.origin.kind) &&
          Object.hasOwn(record, "content"),
        "INVALID_ORIGINAL_INPUT",
      );
      break;
    case "decision":
      requireValue(
        ["accepted", "superseded"].includes(record.status) &&
          text(record.answer) &&
          text(record.meaning) &&
          strings(record.supersedes),
        "INVALID_ACTUAL_DECISION",
      );
      sourceReference(record.answerSource);
      if (record.selectionContext) {
        requireValue(text(record.selectionContext.text), "INVALID_SELECTION_CONTEXT");
        sourceReference(record.selectionContext.source);
      }
      break;
    case "task":
      assertLogicalIdentity(record.logical);
      validateTaskOperationalState(record.state);
      requireValue(
        ["source-only", "native-effect"].includes(record.executionClass) &&
          record.logical.taskId === record.id &&
          record.logical.domainId === record.domainId &&
          record.logical.projectId === record.projectId &&
          record.logical.workId === record.workId &&
          text(record.title) &&
          Array.isArray(record.dependsOn) &&
          record.dependsOn.length <= 1024 &&
          new Set(record.dependsOn.map((row) => row.taskId)).size === record.dependsOn.length &&
          record.dependsOn.every(
            (row) =>
              text(row.taskId) &&
              [
                "execution-and-verification",
                "shared-audit-delivery",
                "formal-task-completion",
                "acceptance",
              ].includes(row.milestone),
          ) &&
          strings(record.decisionIds) &&
          strings(record.effectIds) &&
          strings(record.verificationEvidenceIds),
        "INVALID_TASK_CONTRACT",
      );
      if (record.pendingEffectIds !== undefined)
        requireValue(strings(record.pendingEffectIds), "INVALID_PENDING_EFFECTS");
      if (record.executionClass === "source-only")
        requireValue(
          record.occurrence === undefined &&
            record.executionEvidenceId === undefined &&
            record.effectIds.length === 0 &&
            !record.pendingEffectIds?.length,
          "SOURCE_ONLY_TASK_CANNOT_CLAIM_NATIVE_EFFECT",
        );
      if (record.runtimeId !== undefined)
        requireValue(text(record.runtimeId), "INVALID_TASK_RUNTIME");
      if (record.occurrence) {
        occurrence(record.occurrence);
        requireValue(
          record.runtimeId === record.occurrence.runtimeId,
          "TASK_RUNTIME_OCCURRENCE_MISMATCH",
        );
      }
      break;
    case "relation":
      requireValue(
        ["contains", "purpose", "dependsOn", "supersedes", "implements", "verifiedBy"].includes(
          record.relation,
        ) &&
          record.from &&
          text(record.from.id) &&
          record.to &&
          text(record.to.id),
        "INVALID_RELATION",
      );
      assertRecordReference(record.from);
      assertRecordReference(record.to);
      break;
    case "effect":
      requireValue(
        text(record.taskId) &&
          ["completed", "uncertain"].includes(record.status) &&
          strings(record.evidenceIds) &&
          record.evidenceIds.length,
        "INVALID_EFFECT",
      );
      occurrence(record.occurrence);
      break;
    case "effect-fence":
      requireValue(
        strings(record.completedEffectIds) &&
          strings(record.uncertainEffectIds) &&
          !record.completedEffectIds.some((id) => record.uncertainEffectIds.includes(id)),
        "INVALID_HISTORICAL_EFFECT_FENCE",
      );
      sourceReference(record.source);
      break;
    case "evidence":
      requireValue(
        [
          "observation",
          "verification",
          "review",
          "native-execution",
          "receiver",
          "acceptance",
        ].includes(record.evidenceKind),
        "INVALID_EVIDENCE_CLASS",
      );
      sourceReference(record.source);
      break;
    default:
      throw Error("INVALID_RECORD_KIND");
  }
  if ("artifact" in record && record.artifact !== undefined) {
    const checked = validateArtifact(record.artifact);
    requireValue(
      checked.valid &&
        checked.value?.projectId === record.projectId &&
        checked.value.domainId === record.domainId,
      "INVALID_ONTOLOGY_ARTIFACT",
    );
    if (record.kind === "task") {
      requireValue(checked.value?.kind === "task", "TASK_ARTIFACT_KIND_MISMATCH");
      const logical =
        checked.value?.kind === "task" ? checked.value.payload.details?.logical : undefined;
      requireValue(
        object(logical) && sameFields(logical, record.logical, identityKeys),
        "TASK_ARTIFACT_LOGICAL_BINDING_MISMATCH",
      );
    }
    if (record.kind === "evidence" && record.evidenceKind === "verification")
      requireValue(checked.value?.kind === "verification", "VERIFICATION_ARTIFACT_KIND_MISMATCH");
  }
  return record;
}

export function recordDependencies(record: GitWorkRecord): RecordReference[] {
  switch (record.kind) {
    case "work":
      return [
        { kind: "spec", id: record.spec.id, revision: record.spec.revision },
        { kind: "plan", id: record.plan.id, revision: record.plan.revision },
        ...record.governingDecisionIds.map((id) => ({ kind: "decision" as const, id })),
        ...record.historicalFenceIds.map((id) => ({ kind: "effect-fence" as const, id })),
      ];
    case "plan":
      return [{ kind: "spec", id: record.specId, revision: record.specRevision }];
    case "decision":
      return record.supersedes.map((id) => ({ kind: "decision", id }));
    case "task":
      return [
        { kind: "spec", id: record.logical.specId, revision: record.logical.specRevision },
        { kind: "plan", id: record.logical.planId, revision: record.logical.planRevision },
        ...record.decisionIds.map((id) => ({ kind: "decision" as const, id })),
        ...record.dependsOn.map((row) => ({ kind: "task" as const, id: row.taskId })),
        ...record.effectIds.map((id) => ({ kind: "effect" as const, id })),
        ...record.verificationEvidenceIds.map((id) => ({ kind: "evidence" as const, id })),
        ...(record.executionEvidenceId
          ? [{ kind: "evidence" as const, id: record.executionEvidenceId }]
          : []),
        ...record.dependsOn.flatMap((row) =>
          [row.receiptEvidenceId, row.receiverEvidenceId]
            .filter((id): id is string => id !== undefined)
            .map((id) => ({ kind: "evidence" as const, id })),
        ),
      ];
    case "relation":
      return [record.from, record.to];
    case "effect":
      return [
        { kind: "task", id: record.taskId },
        ...record.evidenceIds.map((id) => ({ kind: "evidence" as const, id })),
      ];
    case "spec":
    case "input":
    case "effect-fence":
    case "evidence":
      return [];
  }
}

export async function validateRecordSources(
  record: GitWorkRecord,
  reader: GitWorkReader,
): Promise<ResolvedSource[]> {
  const sources: ResolvedSource[] = [];
  const read = async (reference: SourceReference) => {
    const result = await reader.readSource(reference);
    sources.push(result);
    return result;
  };
  if (record.kind === "decision") {
    const answer = await read(record.answerSource);
    const original = assertRecord(answer.document);
    requireValue(
      original.kind === "input" &&
        original.inputKind === "actual-user-answer" &&
        original.domainId === record.domainId &&
        original.projectId === record.projectId &&
        original.workId === record.workId,
      "ACTUAL_ANSWER_SOURCE_CLASS_OR_SCOPE_MISMATCH",
    );
    requireValue(answer.value === record.answer, "ACTUAL_ANSWER_FRAGMENT_MISMATCH");
    if (record.selectionContext) {
      const selected = await read(record.selectionContext.source);
      requireValue(
        selected.value === record.selectionContext.text,
        "SELECTION_CONTEXT_FRAGMENT_MISMATCH",
      );
    }
  } else if (record.kind === "effect-fence") {
    const source = await read(record.source);
    requireValue(
      object(source.value) &&
        strings(source.value.completedEffectIds) &&
        strings(source.value.uncertainEffectIds) &&
        record.completedEffectIds.every((id) =>
          (source.value as { completedEffectIds: string[] }).completedEffectIds.includes(id),
        ) &&
        record.uncertainEffectIds.every((id) =>
          (source.value as { uncertainEffectIds: string[] }).uncertainEffectIds.includes(id),
        ),
      "HISTORICAL_FENCE_SOURCE_MISMATCH",
    );
  } else if (record.kind === "evidence") {
    const source = await read(record.source);
    if (record.evidenceKind === "verification") {
      const checked = validateArtifact(source.value);
      requireValue(
        checked.valid && checked.value?.kind === "verification",
        "VERIFICATION_SOURCE_KIND_MISMATCH",
      );
    }
    if (record.evidenceKind === "acceptance") {
      const checked = validateArtifact(source.value);
      requireValue(
        checked.valid &&
          checked.value?.kind === "decision" &&
          checked.value.payload.details?.recordClass === "AcceptanceDecision",
        "ACCEPTANCE_SOURCE_KIND_MISMATCH",
      );
    }
  }
  return sources;
}

const sameOccurrence = (a: NativeOccurrence, b: NativeOccurrence) =>
  a.runtimeId === b.runtimeId && a.attemptId === b.attemptId && a.occurrenceId === b.occurrenceId;
const mapEvidence = (
  records: Map<string, GitWorkRecord>,
  id: string | undefined,
): EvidenceRecord | undefined => {
  const value = id ? records.get(recordKey({ kind: "evidence", id })) : undefined;
  return value?.kind === "evidence" ? value : undefined;
};
/** Checks source joins only; native owners still authenticate/persist/enforce occurrences. */
export async function dependencyFindings(
  task: TaskRecord,
  dependency: TaskDependency,
  records: Map<string, GitWorkRecord>,
  reader: GitWorkReader,
): Promise<string[]> {
  const findings: string[] = [];
  const prior = records.get(recordKey({ kind: "task", id: dependency.taskId }));
  if (prior?.kind !== "task") return [`DEPENDENCY_MISSING:${dependency.taskId}`];
  const state = prior.state;
  if (!(
    state.execution === "complete" &&
    state.verification === "passed" &&
    state.localAudit === "result-durable"
  ))
    findings.push(`DEPENDENCY_NOT_DURABLE:${prior.id}`);
  if (
    dependency.milestone !== "execution-and-verification" &&
    state.sharedAudit !== "delivery-confirmed"
  )
    findings.push(`DEPENDENCY_SHARED_DELIVERY_PENDING:${prior.id}`);
  if (
    ["formal-task-completion", "acceptance"].includes(dependency.milestone) &&
    state.formalCompletion !== "complete"
  )
    findings.push(`DEPENDENCY_FORMAL_COMPLETION_PENDING:${prior.id}`);
  if (dependency.milestone === "acceptance")
    findings.push(`ACCEPTANCE_REQUIRES_OWNER_QUALIFICATION:${prior.id}`);
  if (prior.executionClass === "source-only") {
    if (!prior.verificationEvidenceIds.length)
      return [...findings, `SOURCE_PREREQUISITE_VERIFICATION_REQUIRED:${prior.id}`];
    for (const id of prior.verificationEvidenceIds) {
      try {
        const evidence = mapEvidence(records, id);
        requireValue(
          evidence?.evidenceKind === "verification",
          "SOURCE_PREREQUISITE_VERIFICATION_KIND_MISMATCH",
        );
        const proof = validateArtifact((await reader.readSource(evidence!.source)).value);
        requireValue(
          proof.valid &&
            proof.value?.kind === "verification" &&
            proof.value.projectId === prior.projectId &&
            proof.value.domainId === prior.domainId &&
            proof.value.payload.targetRefs.includes(taskVerificationTarget(prior)) &&
            proof.value.payload.criteria.every((row) => row.status === "pass"),
          "SOURCE_PREREQUISITE_VERIFICATION_MISMATCH",
        );
      } catch (error) {
        findings.push(
          `${error instanceof Error ? error.message : "SOURCE_PREREQUISITE_UNAVAILABLE"}:${prior.id}`,
        );
      }
    }
    return findings;
  }
  const evidence = mapEvidence(records, dependency.receiptEvidenceId);
  if (!evidence || evidence.evidenceKind !== "native-execution")
    return [...findings, `PREREQUISITE_RECEIPT_REQUIRED:${prior.id}`];
  try {
    const source = await reader.readSource(evidence.source);
    const receipt = source.value as unknown as PrerequisiteReceipt;
    requireValue(
      receipt?.schema === "common.git-prerequisite-receipt.v1",
      "INVALID_PREREQUISITE_RECEIPT",
    );
    assertLogicalIdentity(receipt.logical);
    occurrence(receipt.occurrence);
    validateTaskOperationalState(receipt.state);
    requireValue(
      sameFields(receipt.logical, prior.logical, identityKeys),
      "PREREQUISITE_LOGICAL_IDENTITY_MISMATCH",
    );
    requireValue(
      prior.runtimeId === receipt.occurrence.runtimeId &&
        prior.occurrence &&
        sameOccurrence(prior.occurrence, receipt.occurrence),
      "PREREQUISITE_OCCURRENCE_MISMATCH",
    );
    requireValue(sameFields(receipt.state, prior.state, stateKeys), "PREREQUISITE_STATE_MISMATCH");
    for (const ref of [receipt.intent, receipt.result, receipt.verification]) {
      sourceReference(ref);
      await reader.readSource(ref);
    }
    const verification = validateArtifact((await reader.readSource(receipt.verification)).value);
    requireValue(
      verification.valid &&
        verification.value?.kind === "verification" &&
        verification.value.payload.criteria.every((row) => row.status === "pass") &&
        verification.value.projectId === prior.projectId &&
        verification.value.domainId === prior.domainId &&
        verification.value.payload.targetRefs.includes(taskVerificationTarget(prior)),
      "PREREQUISITE_VERIFICATION_NOT_PASSED",
    );
    requireValue(text(task.runtimeId), "CONSUMING_RUNTIME_UNKNOWN");
    if (task.runtimeId !== receipt.occurrence.runtimeId) {
      const received = mapEvidence(records, dependency.receiverEvidenceId);
      requireValue(received?.evidenceKind === "receiver", "DIFFERENT_RUNTIME_RECEIPT_NOT_RECEIVED");
      const readback = (await reader.readSource(received!.source)).value;
      requireValue(
        object(readback) &&
          readback.schema === "common.git-receiver-receipt.v1" &&
          readback.receiverRuntimeId === task.runtimeId &&
          readback.prerequisiteTaskId === prior.id &&
          object(readback.producer) &&
          sameOccurrence(readback.producer as unknown as NativeOccurrence, receipt.occurrence),
        "RECEIVER_OCCURRENCE_MISMATCH",
      );
      sourceReference(readback.payload);
      const payload = await reader.readSource(readback.payload as unknown as SourceReference);
      requireValue(
        payload.repositoryId === source.repositoryId &&
          payload.commit === source.commit &&
          payload.path === source.path &&
          payload.sha256 === source.sha256 &&
          JSON.stringify(payload.value) === JSON.stringify(receipt),
        "RECEIVER_PAYLOAD_MISMATCH",
      );
    }
  } catch (error) {
    findings.push(
      `${error instanceof Error ? error.message : "PREREQUISITE_SOURCE_UNAVAILABLE"}:${prior.id}`,
    );
  }
  return findings;
}

export async function validateRecordSet(
  records: Map<string, GitWorkRecord>,
  reader: GitWorkReader,
): Promise<ValidationResult> {
  const findings: string[] = [];
  const work = records.get(recordKey({ kind: "work", id: reader.work.workId }));
  if (work?.kind !== "work") return outcome(["WORK_RECORD_MISSING"]);
  const completed = new Set<string>(),
    uncertain = new Set<string>();
  for (const record of records.values()) {
    try {
      assertRecord(record);
      requireValue(
        record.domainId === work.domainId &&
          record.projectId === work.projectId &&
          record.workId === work.workId,
        "RECORD_SCOPE_MISMATCH",
      );
      for (const ref of recordDependencies(record))
        requireValue(records.has(recordKey(ref)), `RECORD_DEPENDENCY_MISSING:${recordKey(ref)}`);
      await validateRecordSources(record, reader);
      if (record.kind === "effect-fence") {
        record.completedEffectIds.forEach((id) => completed.add(id));
        record.uncertainEffectIds.forEach((id) => uncertain.add(id));
      }
      if (record.kind === "effect")
        (record.status === "completed" ? completed : uncertain).add(record.id);
      if (record.kind === "work") {
        const spec = records.get(
            recordKey({ kind: "spec", id: record.spec.id, revision: record.spec.revision }),
          ),
          plan = records.get(
            recordKey({ kind: "plan", id: record.plan.id, revision: record.plan.revision }),
          );
        requireValue(
          spec?.kind === "spec" &&
            spec.revision === record.spec.revision &&
            plan?.kind === "plan" &&
            plan.revision === record.plan.revision,
          "ACTIVE_SPEC_PLAN_REVISION_MISMATCH",
        );
        for (const id of record.governingDecisionIds)
          requireValue(
            records.get(recordKey({ kind: "decision", id }))?.kind === "decision" &&
              (records.get(recordKey({ kind: "decision", id })) as DecisionRecord).status ===
                "accepted",
            "GOVERNING_DECISION_NOT_CURRENT",
          );
      }
      if (record.kind === "plan")
        requireValue(
          (
            records.get(
              recordKey({ kind: "spec", id: record.specId, revision: record.specRevision }),
            ) as { revision?: number }
          )?.revision === record.specRevision,
          "PLAN_SPEC_REVISION_MISMATCH",
        );
      if (record.kind === "task") {
        requireValue(record.logical.issueId === work.issueId, "TASK_ISSUE_MISMATCH");
        requireValue(
          (
            records.get(
              recordKey({
                kind: "spec",
                id: record.logical.specId,
                revision: record.logical.specRevision,
              }),
            ) as {
              revision?: number;
            }
          )?.revision === record.logical.specRevision &&
            (
              records.get(
                recordKey({
                  kind: "plan",
                  id: record.logical.planId,
                  revision: record.logical.planRevision,
                }),
              ) as {
                revision?: number;
              }
            )?.revision === record.logical.planRevision,
          "TASK_SPEC_PLAN_REVISION_MISMATCH",
        );
        if (record.state.execution === "complete") {
          for (const dependency of record.dependsOn)
            findings.push(...(await dependencyFindings(record, dependency, records, reader)));
        }
        if (record.state.verification === "passed") {
          requireValue(
            record.verificationEvidenceIds.length > 0,
            "PASSED_TASK_VERIFICATION_REQUIRED",
          );
          for (const id of record.verificationEvidenceIds) {
            const evidence = mapEvidence(records, id);
            requireValue(
              evidence?.evidenceKind === "verification",
              "TASK_VERIFICATION_EVIDENCE_KIND_MISMATCH",
            );
            const checked = validateArtifact((await reader.readSource(evidence!.source)).value);
            requireValue(
              checked.valid &&
                checked.value?.kind === "verification" &&
                checked.value.projectId === record.projectId &&
                checked.value.domainId === record.domainId &&
                checked.value.payload.targetRefs.includes(taskVerificationTarget(record)) &&
                checked.value.payload.criteria.every((row) => row.status === "pass"),
              "TASK_VERIFICATION_TARGET_OR_STATUS_MISMATCH",
            );
          }
          if (record.executionClass === "native-effect")
            findings.push(
              ...(await dependencyFindings(
                record,
                {
                  taskId: record.id,
                  milestone: "execution-and-verification",
                  receiptEvidenceId: record.executionEvidenceId,
                },
                records,
                reader,
              )),
            );
        }
      }
      if (
        record.kind === "relation" &&
        record.relation === "dependsOn" &&
        record.from.kind === "task" &&
        record.to.kind === "task"
      ) {
        const task = records.get(recordKey(record.from));
        requireValue(
          task?.kind === "task" && task.dependsOn.some((row) => row.taskId === record.to.id),
          "TASK_RELATION_DEPENDENCY_CONTRADICTION",
        );
      }
      if (record.kind === "decision")
        for (const id of record.supersedes)
          requireValue(
            id !== record.id &&
              (records.get(recordKey({ kind: "decision", id })) as DecisionRecord)?.status ===
                "superseded",
            "INVALID_DECISION_SUPERSESSION",
          );
      if (record.kind === "effect-fence")
        requireValue(work.historicalFenceIds.includes(record.id), "HISTORICAL_FENCE_NOT_LINKED");
    } catch (error) {
      findings.push(`${error instanceof Error ? error.message : "INVALID_RECORD"}:${record.id}`);
    }
  }
  for (const id of completed) if (uncertain.has(id)) findings.push(`EFFECT_STATUS_CONFLICT:${id}`);
  for (const record of records.values())
    if (record.kind === "task" && ["planned", "running"].includes(record.state.execution)) {
      if (completed.has(record.id)) findings.push(`COMPLETED_EFFECT_REPLAY:${record.id}`);
      for (const id of record.pendingEffectIds ?? [])
        if (completed.has(id) || uncertain.has(id))
          findings.push(`EFFECT_REPLAY_OR_UNCERTAIN:${id}`);
    }
  const graph = new Map<string, string[]>();
  for (const record of records.values()) {
    if (record.kind === "task")
      graph.set(recordKey(record), [
        ...(graph.get(recordKey(record)) ?? []),
        ...record.dependsOn.map((row) => recordKey({ kind: "task", id: row.taskId })),
      ]);
    if (record.kind === "decision")
      graph.set(recordKey(record), [
        ...(graph.get(recordKey(record)) ?? []),
        ...record.supersedes.map((id) => recordKey({ kind: "decision", id })),
      ]);
    if (record.kind === "relation" && ["dependsOn", "purpose"].includes(record.relation))
      graph.set(recordKey(record.from), [
        ...(graph.get(recordKey(record.from)) ?? []),
        recordKey(record.to),
      ]);
  }
  const active = new Set<string>(),
    visited = new Set<string>();
  for (const start of graph.keys()) {
    if (visited.has(start)) continue;
    const stack: Array<{ id: string; cursor: number }> = [{ id: start, cursor: 0 }];
    active.add(start);
    while (stack.length) {
      const frame = stack[stack.length - 1]!,
        edges = graph.get(frame.id) ?? [];
      if (frame.cursor >= edges.length) {
        active.delete(frame.id);
        visited.add(frame.id);
        stack.pop();
        continue;
      }
      const next = edges[frame.cursor++]!;
      if (active.has(next)) {
        findings.push(`SEMANTIC_DEPENDENCY_CYCLE:${next}`);
        continue;
      }
      if (!visited.has(next)) {
        active.add(next);
        stack.push({ id: next, cursor: 0 });
      }
    }
  }
  return outcome(findings);
}

export async function loadRecords(
  reader: GitWorkReader,
): Promise<{ records: Map<string, GitWorkRecord>; findings: string[] }> {
  const records = new Map<string, GitWorkRecord>(),
    findings: string[] = [];
  for (const reference of await reader.listRecords()) {
    try {
      const record = await reader.readRecord(reference);
      records.set(recordKey(reference), record);
    } catch (error) {
      findings.push(
        `${error instanceof Error ? error.message : "RECORD_UNAVAILABLE"}:${recordKey(reference)}`,
      );
    }
  }
  return { records, findings };
}
export async function validateGitWorkRecords(reader: GitWorkReader): Promise<ValidationResult> {
  const loaded = await loadRecords(reader);
  return outcome([
    ...loaded.findings,
    ...(await validateRecordSet(loaded.records, reader)).findings,
  ]);
}

export async function validateGitWorkTransition(
  before: GitWorkReader,
  after: GitWorkReader,
): Promise<ValidationResult> {
  const findings: string[] = [];
  if (
    before.selection.repositoryId !== after.selection.repositoryId ||
    before.work.domainId !== after.work.domainId ||
    before.work.projectId !== after.work.projectId ||
    before.work.workId !== after.work.workId
  )
    return outcome(["WORK_TRANSITION_IDENTITY_MISMATCH"]);
  const previous = await loadRecords(before),
    next = await loadRecords(after);
  findings.push(
    ...previous.findings,
    ...next.findings,
    ...(await validateRecordSet(next.records, after)).findings,
  );
  for (const [key, record] of previous.records) {
    const successor = next.records.get(key);
    if (
      record.kind === "task" &&
      record.state.execution === "complete" &&
      (successor?.kind !== "task" || !sameFields(record.logical, successor.logical, identityKeys))
    ) {
      findings.push(`COMPLETED_TASK_CONTRACT_HISTORY_CHANGED:${record.id}`);
    }
    if (
      record.kind === "work" &&
      successor?.kind === "work" &&
      record.historicalFenceIds.some((id) => !successor.historicalFenceIds.includes(id))
    )
      findings.push("HISTORICAL_FENCE_LINK_REMOVED");
    if (
      ["input", "effect", "effect-fence", "spec", "plan"].includes(record.kind) &&
      JSON.stringify(record) !== JSON.stringify(successor)
    )
      findings.push(`IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED:${key}`);
    if (record.kind === "decision") {
      if (
        successor?.kind !== "decision" ||
        successor.answer !== record.answer ||
        JSON.stringify(successor.answerSource) !== JSON.stringify(record.answerSource) ||
        JSON.stringify(successor.selectionContext) !== JSON.stringify(record.selectionContext) ||
        successor.meaning !== record.meaning ||
        (record.status === "superseded" && successor.status !== "superseded")
      )
        findings.push(`ACTUAL_DECISION_HISTORY_CHANGED:${record.id}`);
      if (
        successor?.kind === "decision" &&
        record.status === "accepted" &&
        successor.status === "superseded" &&
        ![...next.records.values()].some(
          (row) =>
            row.kind === "decision" &&
            row.status === "accepted" &&
            row.supersedes.includes(record.id),
        )
      )
        findings.push(`SUPERSESSION_REPLACEMENT_MISSING:${record.id}`);
    }
  }
  return outcome(findings);
}
