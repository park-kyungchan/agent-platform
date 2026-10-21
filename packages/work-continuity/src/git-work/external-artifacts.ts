import type { Graph, GitSourcePin, SelectedExternalArtifact, ConsumerContractBinding, ProducerContractBinding, Dependency, TaskRef, ContractSet } from "./generated/v3.ts";
import type { GitWorkSelection, ResolvedSource } from "./types.ts";
import { openGitSourceSnapshot, verifiedOriginalBytes } from "./source.ts";
import { assertV3, canonicalV3, digestV3 } from "./v3-codec.ts";
import { expandGitWorkV3Context, v3TaskKey, v3WorkKey } from "./effective-contract.ts";

export interface ExactV3SourceReader { read(pin: GitSourcePin): Promise<ResolvedSource> }
export async function openExactV3SourceReader(selection: GitWorkSelection): Promise<ExactV3SourceReader> {
  const snapshot = await openGitSourceSnapshot(selection);
  return { async read(pin) { assertV3("GitSourcePin", pin); return snapshot.readSource({ kind: "git", ...pin }); } };
}
export async function readExactV3Source(reader: ExactV3SourceReader, pin: GitSourcePin): Promise<ResolvedSource> {
  assertV3("GitSourcePin", pin);
  const source = await reader.read(pin); verifiedOriginalBytes(source);
  if (source.repositoryId !== pin.repositoryId || source.commit !== pin.commit || source.path !== pin.path || source.sha256 !== pin.sha256 || source.reference.fragment !== undefined || canonicalV3(source.value) !== canonicalV3(source.document)) throw Error("V3_PINNED_SOURCE_RECEIPT_MISMATCH");
  return source;
}
const qualified = new WeakMap<object, string>();
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
  return { task: { projectId: target.projectId, workId: target.workId, taskId: target.taskId }, workRevision: work.revision, taskRevision: task.revision, workContracts: work.contracts, contracts: effectiveTaskRefs(graph, target), acceptanceCriteria: [...task.acceptanceCriteria].sort() };
}
function assertProducerBinding(graph: Graph, producer: ProducerContractBinding): void {
  const core = consumerContractBinding(graph, producer.task);
  for (const key of ["task", "workRevision", "taskRevision", "workContracts", "contracts"] as const) if (!same(core[key], producer[key])) throw Error("V3_ARTIFACT_PRODUCER_CONTRACT_MISMATCH");
}

/** Reads every source; no serialized/source-provider claim can mint this proof. */
export async function qualifySelectedExternalArtifacts(graph: Graph, reader: ExactV3SourceReader): Promise<Graph> {
  for (const selected of graph.externalArtifacts ?? []) {
    assertV3("SelectedExternalArtifact", selected);
    assertProducerBinding(graph, selected.producer);
    if (!same(selected.selection.consumer, consumerContractBinding(graph, selected.selection.consumer.task))) throw Error("V3_ARTIFACT_CONSUMER_CONTRACT_MISMATCH");
    const packetSource = await readExactV3Source(reader, selected.producerContext);
    const raw = packetSource.document;
    if (!raw || typeof raw !== "object" || Array.isArray(raw) || !["common.git-work-context.v3", "common.git-work-context.compact.v3"].includes(String(raw.schema))) throw Error("V3_PRODUCER_CONTEXT_SOURCE_REQUIRED");
    const packet = expandGitWorkV3Context(raw as unknown as Parameters<typeof expandGitWorkV3Context>[0]);
    const effective = packet.effective;
    if (packet.status !== "source-qualified" || !same({ task: effective.task, workRevision: effective.workRevision, taskRevision: effective.taskRevision, workContracts: effective.workContracts, contracts: effective.contracts, effectiveDigest: effective.digest }, selected.producer)) throw Error("V3_ARTIFACT_PRODUCER_PACKET_MISMATCH");
    const artifactSource = await readExactV3Source(reader, selected.artifactSource);
    assertV3("SelectedArtifactDocument", artifactSource.document);
    if (artifactSource.document.id !== selected.artifact.id || artifactSource.document.revision !== selected.artifact.revision || artifactSource.sha256 !== selected.artifact.digest || !same(artifactSource.document.producer, selected.producer)) throw Error("V3_ARTIFACT_SOURCE_IDENTITY_MISMATCH");
    const gateSource = await readExactV3Source(reader, selected.selection.definition); assertV3("SelectedArtifactGate", gateSource.document);
    const gate = gateSource.document;
    if (gate.instanceId !== selected.selection.instanceId || gate.revision !== selected.selection.revision || gate.selectionOwnerId !== selected.selection.selectionOwnerId || !same(gate.consumer, selected.selection.consumer)) throw Error("V3_ARTIFACT_GATE_REVISION_OR_RECEIVER_MISMATCH");
    if (new Set(gate.alternatives.map((a) => a.id)).size !== gate.alternatives.length) throw Error("V3_DUPLICATE_ARTIFACT_ALTERNATIVE");
    const alternative = gate.alternatives.find((a) => a.id === selected.selection.selectedAlternative);
    if (!alternative || !same(alternative.producer, selected.producer) || !same(alternative.artifact, selected.artifact) || !alternative.criterionIds.length || new Set(alternative.criterionIds).size !== alternative.criterionIds.length) throw Error("V3_ARTIFACT_SELECTED_ALTERNATIVE_MISMATCH");
    const selection = await readExactV3Source(reader, selected.selectionReceipt); assertV3("ArtifactSelectionReceipt", selection.document);
    if (!same(selection.document.selection, selected.selection)) throw Error("V3_ARTIFACT_SELECTION_RECEIPT_MISMATCH");
    const milestone = await readExactV3Source(reader, selected.producerMilestone); assertV3("ArtifactProducerMilestone", milestone.document);
    if (!same(milestone.document.producer, selected.producer) || !same(milestone.document.artifact, selected.artifact) || !same(milestone.document.artifactSource, selected.artifactSource)) throw Error("V3_ARTIFACT_PRODUCER_MILESTONE_MISMATCH");
    if (milestone.document.milestone === "accepted") {
      if (!milestone.document.acceptanceDecision) throw Error("V3_ARTIFACT_SEPARATE_ACCEPTANCE_REQUIRED");
      const acceptance = await readExactV3Source(reader, milestone.document.acceptanceDecision); assertV3("ArtifactAcceptanceDecision", acceptance.document);
      if (!same(acceptance.document.producer, selected.producer) || !same(acceptance.document.artifact, selected.artifact)) throw Error("V3_ARTIFACT_ACCEPTANCE_BINDING_MISMATCH");
      for (const evidence of acceptance.document.evidence) await readExactV3Source(reader, evidence);
    }
    if (new Set(selected.criteria.map((c) => c.criterionId)).size !== selected.criteria.length || !same([...alternative.criterionIds].sort(), selected.criteria.map((c) => c.criterionId).sort())) throw Error("V3_ARTIFACT_MISSING_OR_DUPLICATE_CRITERION");
    for (const criterion of selected.criteria) {
      const source = await readExactV3Source(reader, criterion.receipt); assertV3("ArtifactCriterionReceipt", source.document);
      if (source.document.criterionId !== criterion.criterionId || source.document.status !== "pass" || !same(source.document.selection, selected.selection) || !source.document.evidence.length) throw Error("V3_ARTIFACT_CRITERION_NOT_QUALIFIED");
      for (const evidence of source.document.evidence) await readExactV3Source(reader, evidence);
    }
    const receiver = await readExactV3Source(reader, selected.receiverUse); assertV3("ArtifactReceiverUse", receiver.document);
    if (receiver.document.status !== "used" || receiver.document.receiverOwnerId !== selected.receiverOwnerId || !same(receiver.document.selection, selected.selection) || !same(receiver.document.producer, selected.producer) || !same(receiver.document.artifact, selected.artifact)) throw Error("V3_ARTIFACT_RECEIVER_QUALIFICATION_MISMATCH");
    for (const evidence of receiver.document.evidence) await readExactV3Source(reader, evidence);
    // Verified producer milestone is retained as source semantics, not native permission.
    qualified.set(selected, canonicalV3({ selected, milestone: milestone.document.milestone }));
  }
  return graph;
}
export function selectedArtifactForDependency(graph: Graph, dependency: Dependency): SelectedExternalArtifact | null {
  if (!dependency.artifact) return null;
  const candidates = (graph.externalArtifacts ?? []).filter((a) => same(a.artifact, dependency.artifact) && v3TaskKey(a.producer.task) === v3TaskKey(dependency.producer) && v3TaskKey(a.selection.consumer.task) === v3TaskKey(dependency.consumer));
  if (candidates.length !== 1) throw Error("V3_EXACT_SELECTED_ARTIFACT_REQUIRED");
  const selected = candidates[0]!;
  const proof = qualified.get(selected);
  if (!proof) throw Error("V3_ARTIFACT_ACTUAL_SOURCE_QUALIFICATION_REQUIRED");
  const expected = canonicalV3({ selected, milestone: dependency.required });
  // Accepted source milestones satisfy verified dependency selection as well.
  if (proof !== expected && !(dependency.required === "verified" && proof === canonicalV3({ selected, milestone: "accepted" }))) throw Error("V3_ARTIFACT_PROOF_CHANGED_OR_MILESTONE_MISMATCH");
  assertProducerBinding(graph, selected.producer);
  if (!same(selected.selection.consumer, consumerContractBinding(graph, dependency.consumer))) throw Error("V3_ARTIFACT_CONSUMER_CONTRACT_MISMATCH");
  return selected;
}
