import type { Graph, GitSourcePin, Contract, Task } from "./generated/v3.ts";
import { assertRecord, dependencyFindings, loadRecords } from "./records.ts";
import { canonicalV3, digestV3 } from "./v3-codec.ts";
import {
  legacyIssueContract,
  legacySpecContract,
  legacyPlanContract,
  legacyTaskBinding,
} from "./legacy-model.ts";
import {
  refreshGitWorkV3Qualifications,
  readExactV3Source,
  type ExactV3SourceReader,
} from "./external-artifacts.ts";

type LegacySourceProof = { fingerprint: string; edges: Graph["dependencies"]; holds: string[] };
const qualified = new WeakMap<object, LegacySourceProof>();
const graphBindings = new WeakMap<object, Set<object>>();
const supportedEdges = (task: import("./types.ts").TaskRecord): Graph["dependencies"] =>
  task.dependsOn
    .filter((d) => ["execution-and-verification", "acceptance"].includes(d.milestone))
    .map((d) => ({
      producer: { projectId: task.projectId, workId: task.workId, taskId: d.taskId },
      consumer: { projectId: task.projectId, workId: task.workId, taskId: task.id },
      required: d.milestone === "acceptance" ? "accepted" : "verified",
    }));
const sorted = <T>(rows: T[]): T[] =>
  [...rows].sort((a, b) => canonicalV3(a).localeCompare(canonicalV3(b)));
const same = (a: unknown, b: unknown): boolean => canonicalV3(a) === canonicalV3(b);
/** Revalidate the original typed legacy records; do not trust a compatibility flag. */
export async function stageLegacyGraphBindings(
  graph: Graph,
  reader: ExactV3SourceReader,
): Promise<Graph> {
  const completed = new Map<object, LegacySourceProof>();
  const snapshots = new Map<
    string,
    Promise<{
      reader: import("./types.ts").GitWorkReader;
      loaded: Awaited<ReturnType<typeof loadRecords>>;
    }>
  >();
  for (const task of graph.tasks.filter((t) => t.legacyBinding !== undefined)) {
    const binding = task.legacyBinding!;
    const source = await readExactV3Source(reader, binding.source);
    const original = assertRecord(source.document);
    if (
      original.kind !== "task" ||
      original.schema !== binding.protocol ||
      original.id !== task.taskId ||
      original.projectId !== task.projectId ||
      original.workId !== task.workId ||
      Object.keys(task.contractOverrides).length
    )
      throw Error("V3_LEGACY_TASK_SOURCE_IDENTITY_MISMATCH");
    const work = graph.works.find(
      (w) => w.projectId === task.projectId && w.workId === task.workId,
    );
    if (!work?.legacySource) throw Error("V3_LEGACY_WORK_SOURCE_REQUIRED");
    const workSource = await readExactV3Source(reader, work.legacySource);
    const oldWork = assertRecord(workSource.document);
    if (
      oldWork.kind !== "work" ||
      oldWork.projectId !== task.projectId ||
      oldWork.workId !== task.workId ||
      original.logical.issueId !== oldWork.issueId
    )
      throw Error("V3_LEGACY_WORK_IDENTITY_MISMATCH");
    const refs = binding.contracts;
    if (
      refs.issue.id !== oldWork.issueId ||
      refs.spec.id !== original.logical.specId ||
      refs.spec.revision !== original.logical.specRevision ||
      refs.plan.id !== original.logical.planId ||
      refs.plan.revision !== original.logical.planRevision
    )
      throw Error("V3_LEGACY_TASK_CONTRACT_PINS_CHANGED");
    const issue = graph.contracts.find((c) =>
      same(
        { id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest },
        refs.issue,
      ),
    );
    const spec = graph.contracts.find((c) =>
      same(
        { id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest },
        refs.spec,
      ),
    );
    const plan = graph.contracts.find((c) =>
      same(
        { id: c.id, revision: c.revision, owner: c.owner, kind: c.kind, digest: c.digest },
        refs.plan,
      ),
    );
    if (!issue || !spec || !plan || !same(issue, legacyIssueContract(oldWork, work.legacySource)))
      throw Error("V3_LEGACY_ISSUE_PROVENANCE_MISMATCH");
    const pinOf = (c: Contract): GitSourcePin =>
      (c.body as Record<string, unknown>).legacySource as GitSourcePin;
    const specSource = await readExactV3Source(reader, pinOf(spec));
    const oldSpec = assertRecord(specSource.document);
    const planSource = await readExactV3Source(reader, pinOf(plan));
    const oldPlan = assertRecord(planSource.document);
    if (
      oldSpec.kind !== "spec" ||
      oldPlan.kind !== "plan" ||
      oldSpec.projectId !== task.projectId ||
      oldSpec.workId !== task.workId ||
      oldPlan.projectId !== task.projectId ||
      oldPlan.workId !== task.workId ||
      oldPlan.specId !== oldSpec.id ||
      oldPlan.specRevision !== oldSpec.revision ||
      !same(spec, legacySpecContract(oldSpec, pinOf(spec))) ||
      !same(plan, legacyPlanContract(oldPlan, pinOf(plan), refs.spec))
    )
      throw Error("V3_LEGACY_SPEC_PLAN_SOURCE_MISMATCH");
    if (!same(binding, legacyTaskBinding(original, binding.source, refs)))
      throw Error("V3_LEGACY_UNSUPPORTED_SEMANTICS_DROPPED");
    if (
      spec.kind !== "spec" ||
      !same(
        [...task.acceptanceCriteria].sort(),
        spec.body.acceptanceCriteria.map((c) => c.id).sort(),
      )
    )
      throw Error("V3_LEGACY_TASK_CRITERIA_FALSELY_NARROWED");
    const holds: string[] = [];
    if (original.dependsOn.length) {
      if (!reader.openLegacyWork) throw Error("V3_LEGACY_DEPENDENCY_SOURCE_READER_REQUIRED");
      const key = canonicalV3([
        binding.source.repositoryId,
        binding.source.commit,
        task.projectId,
        task.workId,
      ]);
      if (!snapshots.has(key))
        snapshots.set(
          key,
          (async () => {
            const legacy = await reader.openLegacyWork!(binding.source, {
              projectId: task.projectId,
              workId: task.workId,
              taskId: task.taskId,
            });
            return { reader: legacy, loaded: await loadRecords(legacy) };
          })(),
        );
      const snapshot = await snapshots.get(key)!;
      if (snapshot.loaded.findings.length)
        throw Error("V3_LEGACY_DEPENDENCY_SNAPSHOT_RECORDS_INVALID");
      for (const dependency of original.dependsOn) {
        const findings = await dependencyFindings(
          original,
          dependency,
          snapshot.loaded.records,
          snapshot.reader,
        );
        for (const finding of findings)
          holds.push(
            "LEGACY_" +
              finding.split(":")[0] +
              ":" +
              digestV3([task.projectId, task.workId, task.taskId, dependency]).slice(0, 16),
          );
      }
    }
    completed.set(binding, {
      fingerprint: canonicalV3(binding),
      edges: supportedEdges(original),
      holds: [...new Set(holds)].sort(),
    });
  }
  for (const [binding, proof] of completed) qualified.set(binding, proof);
  graphBindings.set(graph, new Set(completed.keys()));
  return graph;
}
/** Internal refresh support. Revocation never evaluates graph accessors. */
export function revokeLegacyGraphProofs(graph: object): void {
  for (const binding of graphBindings.get(graph) ?? []) qualified.delete(binding);
  graphBindings.delete(graph);
  try {
    const tasks = Object.getOwnPropertyDescriptor(graph, "tasks")?.value;
    if (Array.isArray(tasks))
      for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(tasks))) {
        const task = descriptor.value;
        if (/^[0-9]+$/.test(key) && task && typeof task === "object") {
          const binding = Object.getOwnPropertyDescriptor(task, "legacyBinding")?.value;
          if (binding && typeof binding === "object") qualified.delete(binding);
        }
      }
  } catch {
    /* Full validation still rejects malformed source data after revocation. */
  }
}
/** Prepare all writes before committing any staged proof to caller-owned data. */
export function prepareLegacyProofPublication(target: Graph, staged: Graph): () => void {
  const pairs: [object, LegacySourceProof][] = [];
  for (let index = 0; index < target.tasks.length; index++) {
    const binding = target.tasks[index]!.legacyBinding;
    if (!binding) continue;
    const stagedBinding = staged.tasks[index]?.legacyBinding;
    const proof = stagedBinding && qualified.get(stagedBinding);
    if (!proof || !same(binding, stagedBinding) || proof.fingerprint !== canonicalV3(binding))
      throw Error("V3_COMPLETE_LEGACY_REFRESH_REQUIRED");
    pairs.push([binding, proof]);
  }
  return () => {
    for (const [binding, proof] of pairs) qualified.set(binding, proof);
    graphBindings.set(target, new Set(pairs.map(([binding]) => binding)));
  };
}
/** Public legacy qualification shares the complete fail-closed refresh boundary. */
export async function qualifyLegacyGraphBindings(
  graph: Graph,
  reader: ExactV3SourceReader,
): Promise<Graph> {
  await refreshGitWorkV3Qualifications(graph, reader);
  return graph;
}

export function assertQualifiedLegacyTask(task: Task, graph?: Graph): void {
  if (!task.legacyBinding) return;
  const proof = qualified.get(task.legacyBinding);
  if (!proof || proof.fingerprint !== canonicalV3(task.legacyBinding))
    throw Error("V3_LEGACY_ACTUAL_SOURCE_QUALIFICATION_REQUIRED");
  if (
    graph &&
    !same(
      sorted(
        graph.dependencies.filter((d) =>
          same(d.consumer, { projectId: task.projectId, workId: task.workId, taskId: task.taskId }),
        ),
      ),
      sorted(proof.edges),
    )
  )
    throw Error("V3_LEGACY_DEPENDENCY_SOURCE_CLOSURE_CHANGED");
}
export function legacyDependencyHoldReasons(graph: Graph, task: Task): string[] {
  assertQualifiedLegacyTask(task, graph);
  return task.legacyBinding ? [...qualified.get(task.legacyBinding)!.holds] : [];
}
