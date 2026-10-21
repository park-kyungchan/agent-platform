import type { Graph, GitSourcePin, Contract, Task } from "./generated/v3.ts";
import { assertRecord } from "./records.ts";
import { canonicalV3 } from "./v3-codec.ts";
import { legacyIssueContract, legacySpecContract, legacyPlanContract, legacyTaskBinding } from "./legacy-model.ts";
import { readExactV3Source, type ExactV3SourceReader } from "./external-artifacts.ts";

const qualified = new WeakMap<object, string>();
const same = (a: unknown, b: unknown): boolean => canonicalV3(a) === canonicalV3(b);
/** Revalidate the original typed legacy records; do not trust a compatibility flag. */
export async function qualifyLegacyGraphBindings(graph: Graph, reader: ExactV3SourceReader): Promise<Graph> {
  for (const task of graph.tasks.filter((t) => t.legacyBinding !== undefined)) {
    const binding = task.legacyBinding!;
    const source = await readExactV3Source(reader, binding.source);
    const original = assertRecord(source.document);
    if (original.kind !== "task" || original.schema !== binding.protocol || original.id !== task.taskId || original.projectId !== task.projectId || original.workId !== task.workId || Object.keys(task.contractOverrides).length) throw Error("V3_LEGACY_TASK_SOURCE_IDENTITY_MISMATCH");
    const work = graph.works.find((w) => w.projectId === task.projectId && w.workId === task.workId);
    if (!work?.legacySource) throw Error("V3_LEGACY_WORK_SOURCE_REQUIRED");
    const workSource = await readExactV3Source(reader, work.legacySource); const oldWork = assertRecord(workSource.document);
    if (oldWork.kind !== "work" || oldWork.projectId !== task.projectId || oldWork.workId !== task.workId || original.logical.issueId !== oldWork.issueId) throw Error("V3_LEGACY_WORK_IDENTITY_MISMATCH");
    const refs = binding.contracts;
    if (refs.issue.id !== oldWork.issueId || refs.spec.id !== original.logical.specId || refs.spec.revision !== original.logical.specRevision || refs.plan.id !== original.logical.planId || refs.plan.revision !== original.logical.planRevision) throw Error("V3_LEGACY_TASK_CONTRACT_PINS_CHANGED");
    const issue = graph.contracts.find((c) => same({ id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest }, refs.issue));
    const spec = graph.contracts.find((c) => same({ id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest }, refs.spec));
    const plan = graph.contracts.find((c) => same({ id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest }, refs.plan));
    if (!issue || !spec || !plan || !same(issue, legacyIssueContract(oldWork, work.legacySource))) throw Error("V3_LEGACY_ISSUE_PROVENANCE_MISMATCH");
    const pinOf = (c: Contract): GitSourcePin => (c.body as Record<string, unknown>).legacySource as GitSourcePin;
    const specSource = await readExactV3Source(reader, pinOf(spec)); const oldSpec = assertRecord(specSource.document);
    const planSource = await readExactV3Source(reader, pinOf(plan)); const oldPlan = assertRecord(planSource.document);
    if (oldSpec.kind !== "spec" || oldPlan.kind !== "plan" || oldSpec.projectId !== task.projectId || oldSpec.workId !== task.workId || oldPlan.projectId !== task.projectId || oldPlan.workId !== task.workId || oldPlan.specId !== oldSpec.id || oldPlan.specRevision !== oldSpec.revision || !same(spec, legacySpecContract(oldSpec, pinOf(spec))) || !same(plan, legacyPlanContract(oldPlan, pinOf(plan), refs.spec))) throw Error("V3_LEGACY_SPEC_PLAN_SOURCE_MISMATCH");
    if (!same(binding, legacyTaskBinding(original, binding.source, refs))) throw Error("V3_LEGACY_UNSUPPORTED_SEMANTICS_DROPPED");
    if (spec.kind !== "spec" || !same([...task.acceptanceCriteria].sort(), spec.body.acceptanceCriteria.map((c) => c.id).sort())) throw Error("V3_LEGACY_TASK_CRITERIA_FALSELY_NARROWED");
    qualified.set(binding, canonicalV3(binding));
  }
  return graph;
}
export function assertQualifiedLegacyTask(task: Task): void {
  if (task.legacyBinding && qualified.get(task.legacyBinding) !== canonicalV3(task.legacyBinding)) throw Error("V3_LEGACY_ACTUAL_SOURCE_QUALIFICATION_REQUIRED");
}
