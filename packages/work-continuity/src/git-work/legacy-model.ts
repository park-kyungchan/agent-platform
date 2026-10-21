import type { Contract, ContractRef, ContractSet, GitSourcePin, Task, LegacyTaskBinding } from "./generated/v3.ts";
import type { WorkRecord, SpecRecord, PlanRecord, TaskRecord } from "./types.ts";
import { digestV3 } from "./v3-codec.ts";

const ownerOf = (row: { projectId: string; workId: string }) => ({ kind: "work" as const, projectId: row.projectId, workId: row.workId });
function contract(kind: Contract["kind"], id: string, revision: number, owner: Contract["owner"], body: unknown, source: GitSourcePin): Contract {
  const row = { kind, id, revision, owner, body, sourceRefs: [{ owner: source.repositoryId, uri: `git:${source.repositoryId}@${source.commit}:${source.path}`, revision: source.commit, digest: source.sha256 }] };
  return { ...row, digest: digestV3(row) } as Contract;
}
export function legacyIssueContract(work: WorkRecord, source: GitSourcePin): Contract {
  return contract("issue", work.issueId, 1, ownerOf(work), { legacyProtocol: work.schema, legacySource: source, issueId: work.issueId, issueTextStatus: "unknown", legacyWorkPurpose: work.purpose }, source);
}
export function legacySpecContract(spec: SpecRecord, source: GitSourcePin): Contract {
  return contract("spec", spec.id, spec.revision, ownerOf(spec), { legacyProtocol: spec.schema, legacySource: source, outcome: spec.outcome, acceptanceCriteria: spec.criteria.map((text, index) => ({ id: `legacy-criterion-${index}`, text, legacyPointer: `#/criteria/${index}` })) }, source);
}
export function legacyPlanContract(plan: PlanRecord, source: GitSourcePin, spec: ContractRef): Contract {
  return contract("plan", plan.id, plan.revision, ownerOf(plan), { legacyProtocol: plan.schema, legacySource: source, objective: plan.objective, ...(plan.clauses === undefined ? {} : { clauses: plan.clauses }), appliesToSpec: spec }, source);
}
export function legacyTaskHolds(task: TaskRecord): string[] {
  const reasons: string[] = [];
  if (task.executionClass === "native-effect") reasons.push("LEGACY_NATIVE_EFFECT_REQUIRES_OWNER_REQUALIFICATION");
  if (task.externalPrerequisites?.length) reasons.push("LEGACY_V2_EXTERNAL_PREREQUISITES_REQUIRE_VERSIONED_SOURCE_SELECTION");
  if (task.dependsOn.some((d) => !["execution-and-verification", "acceptance"].includes(d.milestone))) reasons.push("LEGACY_DEPENDENCY_MILESTONE_REQUIRES_EXPLICIT_OWNER_ADAPTATION");
  if (task.pendingEffectIds?.length) reasons.push("LEGACY_PENDING_EFFECTS_REMAIN_HELD");
  if (task.artifact) reasons.push("LEGACY_TASK_ARTIFACT_CRITERIA_REQUIRE_EXPLICIT_OWNER_ADAPTATION");
  return reasons.sort();
}
export function legacyTaskBinding(task: TaskRecord, source: GitSourcePin, contracts: ContractSet): LegacyTaskBinding {
  return { protocol: task.schema, source, contracts, holdReasons: legacyTaskHolds(task) };
}
export function legacyTaskProjection(task: TaskRecord, binding: LegacyTaskBinding, criterionIds: string[]): Task {
  const status = task.state.verification === "passed" ? "verified" : task.state.execution === "running" ? "running" : task.state.execution === "planned" ? "planned" : "blocked";
  return { projectId: task.projectId, workId: task.workId, taskId: task.id, title: task.id, assigneeActorId: null, revision: 1, contractOverrides: {}, acceptanceCriteria: [...criterionIds], resources: [], status, legacyBinding: binding };
}
