import type {
  Graph,
  GitSourcePin,
  SelectedExternalArtifact,
  ConsumerContractBinding,
  ProducerContractBinding,
  Dependency,
  TaskRef,
  ContractSet,
  TaskContractSourceClosure,
} from "./generated/v3.ts";
import { qualifyLegacyGraphBindings } from "./legacy-qualification.ts";
import type { GitWorkSelection, ResolvedSource, GitWorkReader } from "./types.ts";
import { openGitSourceSnapshot, openGitWork, verifiedOriginalBytes, encodedId } from "./source.ts";
import { assertV3, canonicalV3, digestV3 } from "./v3-codec.ts";
import {
  expandGitWorkV3Context,
  resolveEffectiveContract,
  v3TaskKey,
  v3WorkKey,
} from "./effective-contract.ts";

export interface ExactV3SourceReader {
  read(pin: GitSourcePin): Promise<ResolvedSource>;
  readAt?(
    identity: Pick<GitSourcePin, "repositoryId" | "commit">,
    path: string,
  ): Promise<ResolvedSource>;
  openLegacyWork?(pin: GitSourcePin, task: TaskRef): Promise<GitWorkReader>;
}
export async function openExactV3SourceReader(
  selection: GitWorkSelection,
): Promise<ExactV3SourceReader> {
  const snapshot = await openGitSourceSnapshot(selection);
  return {
    async readAt(identity, path) {
      return snapshot.readSource({
        kind: "git",
        repositoryId: identity.repositoryId,
        commit: identity.commit,
        path,
      });
    },
    async openLegacyWork(pin, task) {
      const root =
        pin.repositoryId === selection.repositoryId
          ? selection.repositoryRoot
          : selection.repositories?.[pin.repositoryId];
      if (!root) throw Error("V3_EXPLICIT_LEGACY_REPOSITORY_REQUIRED");
      return openGitWork({
        ...selection,
        repositoryRoot: root,
        repositoryId: pin.repositoryId,
        commit: pin.commit,
        projectId: task.projectId,
        workId: task.workId,
      });
    },
    async read(pin) {
      assertV3("GitSourcePin", pin);
      return snapshot.readSource({ kind: "git", ...pin });
    },
  };
}
export async function readExactV3Source(
  reader: ExactV3SourceReader,
  pin: GitSourcePin,
): Promise<ResolvedSource> {
  assertV3("GitSourcePin", pin);
  const source = await reader.read(pin);
  verifiedOriginalBytes(source);
  if (
    source.repositoryId !== pin.repositoryId ||
    source.commit !== pin.commit ||
    source.path !== pin.path ||
    source.sha256 !== pin.sha256 ||
    source.reference.fragment !== undefined ||
    canonicalV3(source.value) !== canonicalV3(source.document)
  )
    throw Error("V3_PINNED_SOURCE_RECEIPT_MISMATCH");
  return source;
}
const qualified = new WeakMap<
  object,
  {
    selected: string;
    graphDigest: string;
    producerDigest: string;
    milestone: "verified" | "accepted";
  }
>();
export function assertCurrentArtifactGraph(graph: Graph): void {
  for (const selected of graph.externalArtifacts ?? []) {
    const proof = qualified.get(selected);
    if (
      proof &&
      (proof.graphDigest !== digestV3(graph) || proof.selected !== canonicalV3(selected))
    )
      throw Error("V3_ARTIFACT_QUALIFIED_GRAPH_CHANGED_REQUALIFICATION_REQUIRED");
  }
}
const same = (a: unknown, b: unknown): boolean => canonicalV3(a) === canonicalV3(b);
export function effectiveTaskRefs(graph: Graph, target: TaskRef): ContractSet {
  const work = graph.works.find((w) => v3WorkKey(w) === v3WorkKey(target));
  const task = graph.tasks.find((t) => v3TaskKey(t) === v3TaskKey(target));
  if (!work || !task) throw Error("V3_ARTIFACT_TASK_ENDPOINT_MISSING");
  return task.legacyBinding?.contracts ?? { ...work.contracts, ...task.contractOverrides };
}
export function consumerContractBinding(graph: Graph, target: TaskRef): ConsumerContractBinding {
  const work = graph.works.find((w) => v3WorkKey(w) === v3WorkKey(target));
  const task = graph.tasks.find((t) => v3TaskKey(t) === v3TaskKey(target));
  if (!work || !task) throw Error("V3_ARTIFACT_TASK_ENDPOINT_MISSING");
  return {
    task: { projectId: target.projectId, workId: target.workId, taskId: target.taskId },
    workRevision: work.revision,
    taskRevision: task.revision,
    workContracts: work.contracts,
    contracts: effectiveTaskRefs(graph, target),
    acceptanceCriteria: [...task.acceptanceCriteria].sort(),
  };
}
function assertProducerBinding(graph: Graph, producer: ProducerContractBinding): void {
  const core = consumerContractBinding(graph, producer.task);
  for (const key of ["task", "workRevision", "taskRevision", "workContracts", "contracts"] as const)
    if (!same(core[key], producer[key])) throw Error("V3_ARTIFACT_PRODUCER_CONTRACT_MISMATCH");
}

async function qualifyTaskContractSources(
  graph: Graph,
  task: TaskRef,
  closure: TaskContractSourceClosure,
  reader: ExactV3SourceReader,
): Promise<Graph> {
  const work = graph.works.find((w) => v3WorkKey(w) === v3WorkKey(task));
  const selectedTask = graph.tasks.find((t) => v3TaskKey(t) === v3TaskKey(task));
  if (!work || !selectedTask) throw Error("V3_ARTIFACT_SOURCE_TASK_ENDPOINT_MISSING");
  const base = `work/${encodedId(task.projectId)}/${encodedId(task.workId)}/v3`;
  if (
    closure.work.path !== base + "/work.json" ||
    closure.task.path !== base + `/tasks/${encodedId(task.taskId)}.json` ||
    closure.work.repositoryId !== closure.task.repositoryId ||
    closure.work.commit !== closure.task.commit ||
    closure.graph.repositoryId !== closure.work.repositoryId ||
    closure.graph.commit !== closure.work.commit ||
    closure.graph.path !== base + "/graph.json"
  )
    throw Error("V3_ARTIFACT_CANONICAL_TASK_SNAPSHOT_REQUIRED");
  const workSource = await readExactV3Source(reader, closure.work),
    taskSource = await readExactV3Source(reader, closure.task);
  assertV3("StoredRecord", workSource.document);
  assertV3("StoredRecord", taskSource.document);
  if (
    workSource.document.kind !== "work" ||
    taskSource.document.kind !== "task" ||
    !same(workSource.document.value, work) ||
    !same(taskSource.document.value, selectedTask)
  )
    throw Error("V3_ARTIFACT_CANONICAL_WORK_TASK_SOURCE_MISMATCH");
  const refs = [
    ...new Map(
      [work.contracts, effectiveTaskRefs(graph, task)]
        .flatMap((set) => Object.values(set))
        .map((ref) => [canonicalV3(ref), ref]),
    ).values(),
  ];
  if (closure.contracts.length !== refs.length)
    throw Error("V3_ARTIFACT_CONTRACT_SOURCE_CLOSURE_MISMATCH");
  const seen = new Set<string>();
  for (const pin of closure.contracts) {
    if (
      pin.repositoryId !== closure.work.repositoryId ||
      pin.commit !== closure.work.commit ||
      seen.has(pin.path)
    )
      throw Error("V3_ARTIFACT_MIXED_OR_DUPLICATE_CONTRACT_SNAPSHOT");
    seen.add(pin.path);
    const source = await readExactV3Source(reader, pin);
    assertV3("Contract", source.document);
    const c = source.document;
    const expectedPath =
      base +
      (c.owner.kind === "task" ? `/tasks/${encodedId(c.owner.taskId)}` : "") +
      `/contracts/${c.kind}/${encodedId(c.id)}@${c.revision}.json`;
    const expected = graph.contracts.find((row) => same(row, c));
    if (
      !expected ||
      pin.path !== expectedPath ||
      !refs.some(
        (ref) =>
          ref.id === c.id &&
          ref.revision === c.revision &&
          ref.kind === c.kind &&
          ref.digest === c.digest &&
          same(ref.owner, c.owner),
      )
    )
      throw Error("V3_ARTIFACT_CONTRACT_SOURCE_CLOSURE_MISMATCH");
  }
  const graphSource = await readExactV3Source(reader, closure.graph);
  assertV3("Graph", graphSource.document);
  const canonical = graphSource.document;
  if (canonical.externalArtifacts?.length || !reader.readAt)
    throw Error("V3_ARTIFACT_CANONICAL_GRAPH_SOURCE_READER_REQUIRED");
  const readRow = async (path: string, expected: unknown) => {
    const source = await reader.readAt!(closure.graph, path);
    verifiedOriginalBytes(source);
    if (
      source.repositoryId !== closure.graph.repositoryId ||
      source.commit !== closure.graph.commit ||
      source.path !== path ||
      source.reference.fragment !== undefined ||
      !same(source.document, expected) ||
      !same(source.value, source.document)
    )
      throw Error("V3_ARTIFACT_CANONICAL_GRAPH_RECORD_MISMATCH");
  };
  for (const row of canonical.works) {
    await readRow(`work/${encodedId(row.projectId)}/${encodedId(row.workId)}/v3/work.json`, {
      schema: "common.git-work-record.v3",
      kind: "work",
      value: row,
    });
    if (!graph.works.some((current) => same(current, row)))
      throw Error("V3_ARTIFACT_CANONICAL_GRAPH_WORK_CHANGED");
  }
  for (const row of canonical.tasks) {
    await readRow(
      `work/${encodedId(row.projectId)}/${encodedId(row.workId)}/v3/tasks/${encodedId(row.taskId)}.json`,
      { schema: "common.git-work-record.v3", kind: "task", value: row },
    );
    if (!graph.tasks.some((current) => same(current, row)))
      throw Error("V3_ARTIFACT_CANONICAL_GRAPH_TASK_CHANGED");
  }
  for (const row of canonical.contracts) {
    await readRow(
      `work/${encodedId(row.owner.projectId)}/${encodedId(row.owner.workId)}/v3` +
        (row.owner.kind === "task" ? `/tasks/${encodedId(row.owner.taskId)}` : "") +
        `/contracts/${row.kind}/${encodedId(row.id)}@${row.revision}.json`,
      row,
    );
    if (!graph.contracts.some((current) => same(current, row)))
      throw Error("V3_ARTIFACT_CANONICAL_GRAPH_CONTRACT_CHANGED");
  }
  if (
    !canonical.tasks.some((row) => same(row, selectedTask)) ||
    !canonical.works.some((row) => same(row, work))
  )
    throw Error("V3_ARTIFACT_CANONICAL_GRAPH_SELECTED_SCOPE_MISSING");
  await qualifyLegacyGraphBindings(canonical, reader);
  return canonical;
}

/** Reads every source; no serialized/source-provider claim can mint this proof. */
export async function qualifySelectedExternalArtifacts(
  graph: Graph,
  reader: ExactV3SourceReader,
): Promise<Graph> {
  assertV3("Graph", graph);
  for (const selected of graph.externalArtifacts ?? []) qualified.delete(selected);
  const graphDigest = digestV3(graph);
  for (const selected of graph.externalArtifacts ?? []) {
    assertV3("SelectedExternalArtifact", selected);
    assertProducerBinding(graph, selected.producer);
    const producerGraph = await qualifyTaskContractSources(
      graph,
      selected.producer.task,
      selected.producerSources,
      reader,
    );
    const consumerGraph = await qualifyTaskContractSources(
      graph,
      selected.selection.consumer.task,
      selected.consumerSources,
      reader,
    );
    if (
      !same(
        selected.selection.consumer,
        consumerContractBinding(graph, selected.selection.consumer.task),
      )
    )
      throw Error("V3_ARTIFACT_CONSUMER_CONTRACT_MISMATCH");
    const packetSource = await readExactV3Source(reader, selected.producerContext);
    const raw = packetSource.document;
    if (
      !raw ||
      typeof raw !== "object" ||
      Array.isArray(raw) ||
      !["common.git-work-context.v3", "common.git-work-context.compact.v3"].includes(
        String(raw.schema),
      )
    )
      throw Error("V3_PRODUCER_CONTEXT_SOURCE_REQUIRED");
    const packet = expandGitWorkV3Context(
      raw as unknown as Parameters<typeof expandGitWorkV3Context>[0],
    );
    const effective = packet.effective;
    // The same canonical resolver is used for actual stored records, current
    // dispatch input and transport. Unsupported/unproved upstream closures fail.
    const canonicalEffective = resolveEffectiveContract(producerGraph, selected.producer.task);
    if (
      !same(canonicalEffective, effective) ||
      !same(resolveEffectiveContract(graph, selected.producer.task), canonicalEffective)
    )
      throw Error("V3_ARTIFACT_COMPLETE_PRODUCER_EFFECTIVE_MISMATCH");
    const semanticGraph = (input: Graph) => {
      const { externalArtifacts: _, ...core } = input;
      return core;
    };
    if (!same(semanticGraph(graph), semanticGraph(consumerGraph)))
      throw Error("V3_ARTIFACT_COMPLETE_CONSUMER_GRAPH_MISMATCH");

    if (
      packet.status !== "source-qualified" ||
      !same(
        {
          task: effective.task,
          workRevision: effective.workRevision,
          taskRevision: effective.taskRevision,
          workContracts: effective.workContracts,
          contracts: effective.contracts,
          effectiveDigest: effective.digest,
        },
        selected.producer,
      )
    )
      throw Error("V3_ARTIFACT_PRODUCER_PACKET_MISMATCH");
    const artifactSource = await readExactV3Source(reader, selected.artifactSource);
    assertV3("SelectedArtifactDocument", artifactSource.document);
    if (
      artifactSource.document.id !== selected.artifact.id ||
      artifactSource.document.revision !== selected.artifact.revision ||
      artifactSource.sha256 !== selected.artifact.digest ||
      !same(artifactSource.document.producer, selected.producer)
    )
      throw Error("V3_ARTIFACT_SOURCE_IDENTITY_MISMATCH");
    const gateSource = await readExactV3Source(reader, selected.selection.definition);
    assertV3("SelectedArtifactGate", gateSource.document);
    const gate = gateSource.document;
    if (
      gate.instanceId !== selected.selection.instanceId ||
      gate.revision !== selected.selection.revision ||
      gate.selectionOwnerId !== selected.selection.selectionOwnerId ||
      !same(gate.consumer, selected.selection.consumer)
    )
      throw Error("V3_ARTIFACT_GATE_REVISION_OR_RECEIVER_MISMATCH");
    if (new Set(gate.alternatives.map((a) => a.id)).size !== gate.alternatives.length)
      throw Error("V3_DUPLICATE_ARTIFACT_ALTERNATIVE");
    const alternative = gate.alternatives.find(
      (a) => a.id === selected.selection.selectedAlternative,
    );
    if (
      !alternative ||
      !same(alternative.producer, selected.producer) ||
      !same(alternative.artifact, selected.artifact) ||
      !alternative.criterionIds.length ||
      new Set(alternative.criterionIds).size !== alternative.criterionIds.length
    )
      throw Error("V3_ARTIFACT_SELECTED_ALTERNATIVE_MISMATCH");
    const selection = await readExactV3Source(reader, selected.selectionReceipt);
    assertV3("ArtifactSelectionReceipt", selection.document);
    if (!same(selection.document.selection, selected.selection))
      throw Error("V3_ARTIFACT_SELECTION_RECEIPT_MISMATCH");
    const milestone = await readExactV3Source(reader, selected.producerMilestone);
    assertV3("ArtifactProducerMilestone", milestone.document);
    if (
      !same(milestone.document.producer, selected.producer) ||
      !same(milestone.document.artifact, selected.artifact) ||
      !same(milestone.document.artifactSource, selected.artifactSource)
    )
      throw Error("V3_ARTIFACT_PRODUCER_MILESTONE_MISMATCH");
    if (milestone.document.milestone === "accepted") {
      if (!milestone.document.acceptanceDecision)
        throw Error("V3_ARTIFACT_SEPARATE_ACCEPTANCE_REQUIRED");
      const acceptance = await readExactV3Source(reader, milestone.document.acceptanceDecision);
      assertV3("ArtifactAcceptanceDecision", acceptance.document);
      if (
        !same(acceptance.document.producer, selected.producer) ||
        !same(acceptance.document.artifact, selected.artifact)
      )
        throw Error("V3_ARTIFACT_ACCEPTANCE_BINDING_MISMATCH");
      for (const evidence of acceptance.document.evidence)
        await readExactV3Source(reader, evidence);
    }
    if (
      new Set(selected.criteria.map((c) => c.criterionId)).size !== selected.criteria.length ||
      !same(
        [...alternative.criterionIds].sort(),
        selected.criteria.map((c) => c.criterionId).sort(),
      )
    )
      throw Error("V3_ARTIFACT_MISSING_OR_DUPLICATE_CRITERION");
    for (const criterion of selected.criteria) {
      const source = await readExactV3Source(reader, criterion.receipt);
      assertV3("ArtifactCriterionReceipt", source.document);
      if (
        source.document.criterionId !== criterion.criterionId ||
        source.document.status !== "pass" ||
        !same(source.document.selection, selected.selection) ||
        !source.document.evidence.length
      )
        throw Error("V3_ARTIFACT_CRITERION_NOT_QUALIFIED");
      for (const evidence of source.document.evidence) await readExactV3Source(reader, evidence);
    }
    const receiver = await readExactV3Source(reader, selected.receiverUse);
    assertV3("ArtifactReceiverUse", receiver.document);
    if (
      !receiver.document.evidence.length ||
      receiver.document.status !== "used" ||
      receiver.document.receiverOwnerId !== selected.receiverOwnerId ||
      !same(receiver.document.selection, selected.selection) ||
      !same(receiver.document.producer, selected.producer) ||
      !same(receiver.document.artifact, selected.artifact)
    )
      throw Error("V3_ARTIFACT_RECEIVER_QUALIFICATION_MISMATCH");
    for (const evidence of receiver.document.evidence) await readExactV3Source(reader, evidence);
    // Verified producer milestone is retained as source semantics, not native permission.
    if (digestV3(graph) !== graphDigest)
      throw Error("V3_ARTIFACT_GRAPH_MUTATED_DURING_QUALIFICATION");
    qualified.set(selected, {
      selected: canonicalV3(selected),
      graphDigest,
      producerDigest: canonicalEffective.digest,
      milestone: milestone.document.milestone,
    });
  }
  return graph;
}
export function selectedArtifactForDependency(
  graph: Graph,
  dependency: Dependency,
): SelectedExternalArtifact | null {
  if (!dependency.artifact) return null;
  const candidates = (graph.externalArtifacts ?? []).filter(
    (a) =>
      same(a.artifact, dependency.artifact) &&
      v3TaskKey(a.producer.task) === v3TaskKey(dependency.producer) &&
      v3TaskKey(a.selection.consumer.task) === v3TaskKey(dependency.consumer),
  );
  if (candidates.length !== 1) throw Error("V3_EXACT_SELECTED_ARTIFACT_REQUIRED");
  const selected = candidates[0]!;
  const proof = qualified.get(selected);
  if (!proof) throw Error("V3_ARTIFACT_ACTUAL_SOURCE_QUALIFICATION_REQUIRED");
  if (proof.selected !== canonicalV3(selected) || proof.graphDigest !== digestV3(graph))
    throw Error("V3_ARTIFACT_QUALIFIED_GRAPH_CHANGED_REQUALIFICATION_REQUIRED");
  if (
    proof.milestone !== dependency.required &&
    !(dependency.required === "verified" && proof.milestone === "accepted")
  )
    throw Error("V3_ARTIFACT_PROOF_CHANGED_OR_MILESTONE_MISMATCH");
  if (resolveEffectiveContract(graph, selected.producer.task).digest !== proof.producerDigest)
    throw Error("V3_ARTIFACT_COMPLETE_PRODUCER_EFFECTIVE_MISMATCH");
  assertProducerBinding(graph, selected.producer);
  if (!same(selected.selection.consumer, consumerContractBinding(graph, dependency.consumer)))
    throw Error("V3_ARTIFACT_CONSUMER_CONTRACT_MISMATCH");
  return selected;
}
