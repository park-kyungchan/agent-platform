import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, writeFile, readFile, rm, symlink } from "node:fs/promises";
import { join, dirname } from "node:path";
import { tmpdir } from "node:os";
import type { Graph, GitSourcePin, LegacyMigrationRequest, SelectedExternalArtifact, Run } from "./generated/v3.ts";
import type { GitWorkRecord, TaskRecord, GitWorkSelection, SelectedExternalArtifactPrerequisite } from "./types.ts";
import { canonicalV3, digestV3 } from "./v3-codec.ts";
import { gitRecordPath, openGitWork } from "./source.ts";
import { validateGitWorkTransition } from "./records.ts";
import { readLegacyGitWorkProjection, readVersionedLegacyGitWork, admitLegacyGitWork, recoverLegacyCreateRequest, readMigratedGitWorkV3, type NativeLegacyMigrationOwner } from "./legacy-bridge.ts";
import { validateGitWorkAliases, resolveGitWorkAlias } from "./aliases.ts";
import { resolveEffectiveContract, readGitWorkV3Context, expandGitWorkV3Context, readSourceQualifiedGitWorkV3Context, v3ContractRef } from "./effective-contract.ts";
import { openExactV3SourceReader, consumerContractBinding } from "./external-artifacts.ts";
import { allocateGitWorkIdentity, type GitAllocationTarget } from "./allocation.ts";
import { sourceSha256 } from "./git-owner-transaction.ts";
import { fixtureGit as git, fixtureScope, fixtureTask, fixtureRef, fixtureGraph, fixtureOwner, fixtureTaskRequest, fixtureWorkRequest, fixtureContract, seedFixture } from "./fixtures/v3-owner.ts";

const roots: string[] = [];
afterEach(async () => { await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true }))); });
async function directory() { const root = await mkdtemp(join(tmpdir(), "v3-correction-synthetic-")); roots.push(root); return root; }
async function put(root: string, path: string, value: unknown) { const bytes = canonicalV3(value) + "\n"; await mkdir(dirname(join(root,path)), { recursive: true }); await writeFile(join(root,path),bytes); return sourceSha256(bytes); }
function commit(root: string) { git(root,"add","work"); git(root,"commit","-qm","Synthetic source successor"); return git(root,"rev-parse","HEAD"); }
async function pin(root: string, path: string, value: unknown): Promise<GitSourcePin> { const sha256 = await put(root,path,value); return { repositoryId: "synthetic-repository", path, sha256, commit: commit(root) }; }
const legacyScope = { domainId: "synthetic-domain", projectId: "synthetic-project", workId: "continuing-work-slug" };
const legacyBase = { schema: "common.git-work.v1" as const, ...legacyScope };
const oldTaskId = "parent-task-slug", newTaskId = "18a81525-5cb1-4e22-a235-bf221bf6837b";
const legacyLogical = (id: string, specRevision: number, planRevision: number) => ({ ...legacyScope, issueId: "legacy-issue", specId: "legacy-spec", specRevision, planId: "legacy-plan", planRevision, taskId: id });
const legacyTask = (id: string, specRevision: number, planRevision: number): TaskRecord => ({ ...legacyBase, kind: "task", id, title: "Source fixture Task", logical: legacyLogical(id,specRevision,planRevision), state: { execution: "planned", verification: "pending", localAudit: "intent-durable", sharedAudit: "pending", formalCompletion: "pending" }, executionClass: "source-only", dependsOn: [], decisionIds: ["original-answer"], effectIds: [], pendingEffectIds: [], verificationEvidenceIds: [] });
async function legacyFixture() {
  const root = await directory(); git(root,"init","--initial-branch=work/synthetic-v3"); git(root,"config","user.name","Synthetic native source owner"); git(root,"config","user.email","synthetic@example.invalid");
  const parent = legacyTask(oldTaskId,1,4), child = legacyTask(newTaskId,2,5), external = legacyTask("external-v2",1,4);
  const placeholder = { kind: "git" as const, repositoryId: "synthetic-repository", commit: "0".repeat(40), path: "work/external/source.json", sha256: "1".repeat(64) };
  const prerequisite: SelectedExternalArtifactPrerequisite = { schema: "common.git-selected-external-artifact.v1", producer: { logical: { ...legacyLogical("producer",1,4), workId: "external-work" }, ownerId: "external-owner", record: placeholder, sourceClosure: placeholder, milestone: "execution-and-verification", artifact: placeholder, ownerMilestoneReceipt: placeholder }, consumerGate: { instanceId: "external-gate", revision: 2, definition: placeholder, consumer: external.logical, consumerSourceClosure: placeholder, selectedAlternative: "source-A", selectionOwnerId: "receiver", ownerSelectionReceipt: placeholder, criterionReceipts: [{ criterionId: "external-criterion", evidence: placeholder }] }, qualification: { status: "unresolved", receiverUse: null }, grantsNativePermission: false };
  external.schema = "common.git-work-task.v2"; external.externalPrerequisites = [prerequisite]; external.executionClass = "native-effect"; external.pendingEffectIds = ["proposed-effect"]; external.effectIds = ["already-uncertain"];
  const meaning = { executionClass: "source-only", purpose: "Implement source contracts", spec: { id: "legacy-spec", revision: 2 }, plan: { id: "legacy-plan", revision: 5 }, approvedManifestSha256: "a".repeat(64) };
  const bootstrap = { ...legacyBase, kind: "input" as const, id: "original-create-request", inputKind: "source-owner-record" as const, origin: { ownerId: "synthetic-root", locator: "synthetic:native-source-capture", kind: "source-owner-capture" as const }, content: { owner: { repositoryId: "synthetic-repository", projectId: legacyScope.projectId, workId: legacyScope.workId }, kind: "Task", createRequestId: "a69cd343-7ac5-4190-9bb9-7a021c41c16b", proposedTaskId: newTaskId, meaning, meaningSha256: digestV3(meaning) } };
  const rows: GitWorkRecord[] = [
    { ...legacyBase, kind: "work", id: legacyScope.workId, repositoryId: "synthetic-repository", issueId: "legacy-issue", purpose: { objective: "Original Work objective, never Issue text", successCriteria: ["preserve original"], inScope: ["source"], outOfScope: ["native adoption"] }, spec: { id: "legacy-spec", revision: 2 }, plan: { id: "legacy-plan", revision: 5 }, governingDecisionIds: ["original-answer"], historicalFenceIds: ["original-fence"] },
    ...[1,2].map((revision) => ({ ...legacyBase, kind: "spec" as const, id: "legacy-spec", revision, outcome: `Original outcome ${revision}`, criteria: ["Exact original criterion", `Revision ${revision} criterion`] })),
    ...[4,5].map((revision) => ({ ...legacyBase, kind: "plan" as const, id: "legacy-plan", revision, specId: "legacy-spec", specRevision: revision - 3, objective: `Exact Plan ${revision}` })), parent, child, external, bootstrap,
    { ...legacyBase, kind: "decision", id: "original-answer", status: "accepted", answer: "Original answer bytes", meaning: "Original source meaning", answerSource: { kind: "snapshot", path: "work/synthetic-project/continuing-work-slug/inputs/original-create-request.json", fragment: "#/content/meaning" }, supersedes: [] },
    { ...legacyBase, kind: "effect-fence", id: "original-fence", completedEffectIds: ["already-completed"], uncertainEffectIds: ["already-uncertain"], source: { kind: "snapshot", path: "work/synthetic-project/continuing-work-slug/inputs/original-create-request.json", fragment: "#/content" } },
  ];
  for (const row of rows) await put(root,gitRecordPath(row.projectId,row.workId,{ kind: row.kind, id: row.id, ...("revision" in row ? { revision: row.revision } : {}) }),row);
  const head = commit(root); git(root,"checkout","--detach");
  const selection: GitWorkSelection = { repositoryRoot: root, repositoryId: "synthetic-repository", commit: head, projectId: legacyScope.projectId, workId: legacyScope.workId };
  const projection = await readLegacyGitWorkProjection(selection);
  const request: LegacyMigrationRequest = { schema: "common.git-legacy-migration-request.v3", ownerId: "synthetic-owner", createRequestId: "migration-admission-request", snapshot: { repositoryId: selection.repositoryId, commit: head, projectId: selection.projectId, workId: selection.workId }, expectedInventoryDigest: projection.inventoryDigest, bootstrapRequests: projection.sources.filter((p) => p.path.endsWith("/inputs/original-create-request.json")), aliases: projection.aliases };
  const target: GitAllocationTarget = { repositoryRoot: root, ownerRef: fixtureRef, expectedHead: head, expectedAuthorityRevision: 7 };
  const owner: NativeLegacyMigrationOwner = { async withCurrentMigrationAuthority(input, operation) { return operation({ ownerId: request.ownerId, actorId: "synthetic-observed-native-actor", scope: { projectId: legacyScope.projectId, workId: legacyScope.workId }, operation: "admit-existing-legacy-work", revision: 7, authorityRef: "synthetic-current-admission", expiresAtUtc: new Date(Date.now()+120_000).toISOString(), membership: true, gitHookPolicy: { mode: "native-qualified-no-git-hooks", repositoryRoot: root, ownerRef: fixtureRef, head: input.head, ownerId: request.ownerId, authorityRevision: 7, qualificationRef: "synthetic-current-hook-qualification", expiresAtUtc: new Date(Date.now()+120_000).toISOString(), requiredNativeControlsSatisfied: true } }); } };
  return { root, selection, projection, request, target, owner, meaningSha256: digestV3(meaning) };
}

test("R01 production-shaped v1/v2 source bridge preserves Task1/4 versus Work2/5, unknown Issue and held original prerequisites", async () => {
  const f = await legacyFixture(); const target = { ...legacyScope, taskId: oldTaskId };
  const effective = resolveEffectiveContract(f.projection.graph,target);
  expect(effective.workContracts.spec.revision).toBe(2); expect(effective.contracts.spec.revision).toBe(1); expect(effective.contracts.plan.revision).toBe(4);
  const issue = f.projection.graph.contracts.find((c) => c.kind === "issue")!; expect(issue.body.issueTextStatus).toBe("unknown"); expect(issue.body.problem).toBeUndefined();
  for (const mode of ["full","compact","dispatch","resume"] as const) expect(expandGitWorkV3Context(readGitWorkV3Context({ graph: f.projection.graph, task: target, mode, expectedDigest: effective.digest })).effective).toEqual(effective);
  const packet = expandGitWorkV3Context(readGitWorkV3Context({ graph: f.projection.graph, task: { ...legacyScope, taskId: "external-v2" }, mode: "full" }));
  expect(packet.status).toBe("held"); expect(packet.holdReasons).toContain("LEGACY_V2_EXTERNAL_PREREQUISITES_REQUIRE_VERSIONED_SOURCE_SELECTION");
  expect(packet.effective.legacyBinding!.protocol).toBe("common.git-work-task.v2");
  expect(() => resolveEffectiveContract(structuredClone(f.projection.graph), target)).toThrow("ACTUAL_SOURCE_QUALIFICATION");
  const missing = structuredClone(f.projection.graph); missing.tasks[0]!.legacyBinding!.source.sha256 = "0".repeat(64);
  await expect(readSourceQualifiedGitWorkV3Context({ graph: missing, task: target, mode: "full", reader: await openExactV3SourceReader(f.selection) })).rejects.toThrow();
});

test("R01 exact admission/restart/concurrent CAS imports original bootstrap ID and fences old writer without changing decision/effect source bytes", async () => {
  const f = await legacyFixture(); const before = f.projection.sources;
  const results = await Promise.all([admitLegacyGitWork(f.request,f.target,f.owner),admitLegacyGitWork(f.request,f.target,f.owner)]);
  expect(results.every((r) => ["admitted","reconciled","existing"].includes(r.status))).toBe(true);
  const head = git(f.root,"rev-parse",fixtureRef); expect(git(f.root,"rev-list","--count",f.selection.commit+".."+head)).toBe("1");
  const replay = await admitLegacyGitWork(f.request,{ ...f.target, expectedHead: head },f.owner); expect(replay.status).toBe("existing");
  const current = { ...f.selection, commit: head }; const historical = await readVersionedLegacyGitWork(current); expect(historical.sources).toEqual(before);
  for (const pin of before) expect(sourceSha256(git(f.root,"show",head+":"+pin.path)+"\n")).toBe(pin.sha256);
  const migrated = await readMigratedGitWorkV3(current); expect(migrated.works[0]!.leadActorId).toBe("synthetic-observed-native-actor"); expect(migrated.tasks.map((t) => t.taskId).sort()).toEqual([oldTaskId,newTaskId,"external-v2"].sort());
  const recovery = { selection: current, ownerId: f.request.ownerId, legacyOwner: { repositoryId: current.repositoryId, projectId: current.projectId, workId: current.workId }, createRequestId: "a69cd343-7ac5-4190-9bb9-7a021c41c16b", kind: "task" as const, meaningSha256: f.meaningSha256 };
  expect((await recoverLegacyCreateRequest(recovery)).id).toBe(newTaskId);
  await expect(recoverLegacyCreateRequest({ ...recovery, meaningSha256: "0".repeat(64) })).rejects.toThrow("CONFLICT");
  await expect(recoverLegacyCreateRequest({ ...recovery, legacyOwner: { ...recovery.legacyOwner, workId: "foreign" } })).rejects.toThrow("CONFLICT");
  expect((await validateGitWorkTransition(await openGitWork(f.selection),await openGitWork(current))).findings).toContain("LEGACY_AUTHORITATIVE_WRITER_RETIRED_USE_VERSIONED_MIGRATION_READER");
  const alias = f.request.aliases.find((a) => a.from.kind === "task")!; expect(resolveGitWorkAlias(f.request.aliases,alias.from)).toEqual(alias.to);
  expect(() => validateGitWorkAliases([...f.request.aliases,alias])).toThrow("AMBIGUOUS");
  expect(() => validateGitWorkAliases([{ ...alias, to: { ...alias.to, kind: "work" } }])).toThrow("WRONG_KIND");
  expect(() => validateGitWorkAliases([alias,{ ...alias, from: alias.to, to: alias.from }])).toThrow("CYCLE");
  const wrongAlias = structuredClone(f.request); wrongAlias.aliases[0]!.to.key += "-foreign";
  await expect(admitLegacyGitWork(wrongAlias,{ ...f.target,expectedHead: head },f.owner)).rejects.toThrow("CONFLICT");
  // Original source bytes remain authoritative history; modification is detected.
  git(f.root,"checkout","--detach",head); await put(f.root,before[0]!.path,{ corrupt: "synthetic" }); const changed = commit(f.root);
  await expect(readVersionedLegacyGitWork({ ...current,commit: changed })).rejects.toThrow("CHANGED_ORIGINAL");
}, 30_000);

test("R01 missing inventory, omitted bootstrap, wrong meaning and competing writer reject before ref mutation", async () => {
  const f = await legacyFixture();
  await expect(admitLegacyGitWork({ ...f.request,expectedInventoryDigest: "0".repeat(64) },f.target,f.owner)).rejects.toThrow("INVENTORY");
  await expect(admitLegacyGitWork({ ...f.request,bootstrapRequests: [] },f.target,f.owner)).rejects.toThrow("ALL_LEGACY_BOOTSTRAP");
  const newRequest = { ...fixtureTaskRequest("a69cd343-7ac5-4190-9bb9-7a021c41c16b"), ownerId: f.request.ownerId };
  const applied = await admitLegacyGitWork(f.request,f.target,f.owner);
  const native = fixtureOwner(f.root,{ ownerId: f.request.ownerId });
  // Native policy must match too; the independent generic path still cannot consume this key.
  native.withCurrentAuthority = async (input, operation) => operation({ ownerId: f.request.ownerId, actorId: "synthetic", scope: { kind: "project",projectId: fixtureScope.projectId }, kinds: ["task"], revision: 7, authorityRef: "synthetic", membership: true, expiresAtUtc: new Date(Date.now()+60_000).toISOString(), gitHookPolicy: { mode: "native-qualified-no-git-hooks", repositoryRoot: f.root,ownerRef: fixtureRef,head: input.head,ownerId: f.request.ownerId,authorityRevision: 7,qualificationRef: "synthetic",expiresAtUtc: new Date(Date.now()+60_000).toISOString(),requiredNativeControlsSatisfied: true } });
  await expect(allocateGitWorkIdentity(newRequest,{ ...f.target,expectedHead: applied.head },native)).rejects.toThrow("LEGACY_REQUEST_REQUIRES_VERSIONED_RECOVERY");
  expect(git(f.root,"rev-parse",fixtureRef)).toBe(applied.head);
});

async function artifactFixture() {
  const root = await directory(); git(root,"init","--initial-branch=work/synthetic-v3"); git(root,"config","user.name","Synthetic artifact owner"); git(root,"config","user.email","synthetic@example.invalid");
  const graph = fixtureGraph();
  const producer = { ...fixtureTask,workId: "producer-work",taskId: "producer-task" };
  const producerGraph = JSON.parse(canonicalV3(graph).replaceAll(fixtureScope.workId,producer.workId).replaceAll(fixtureTask.taskId,producer.taskId)) as Graph;
  // Real Task-specific producer Spec/Plan: caller cannot substitute the Work contract.
  const spec = fixtureContract("spec","producer-task-spec",{ kind: "task",...producer },{ refinesSpec: producerGraph.works[0]!.contracts.spec,acceptanceCriteria: [{ id: "P-1",covers: ["AC-1"] }] });
  const plan = fixtureContract("plan","producer-task-plan",{ kind: "task",...producer },{ methodRef: producerGraph.works[0]!.contracts.plan,appliesToSpec: v3ContractRef(spec) });
  producerGraph.contracts.push(spec,plan); producerGraph.tasks[0]!.contractOverrides = { spec: v3ContractRef(spec),plan: v3ContractRef(plan) }; producerGraph.tasks[0]!.acceptanceCriteria = ["P-1"];
  // Rehash moved Work-owned contracts and their Plan-to-Spec reference.
  const workSpec = producerGraph.contracts.find((c) => c.kind === "spec" && c.owner.kind === "work")!;
  const workIssue = producerGraph.contracts.find((c) => c.kind === "issue")!;
  const workPlan = producerGraph.contracts.find((c) => c.kind === "plan" && c.owner.kind === "work")!;
  for (const c of [workIssue,workSpec]) { const { digest: _,...body } = c; c.digest=digestV3(body); }
  workPlan.body.appliesToSpec = v3ContractRef(workSpec); { const { digest: _,...body } = workPlan; workPlan.digest = digestV3(body); }
  producerGraph.works[0]!.contracts = { issue: v3ContractRef(workIssue),spec: v3ContractRef(workSpec),plan: v3ContractRef(workPlan) };
  spec.body.refinesSpec = v3ContractRef(workSpec); { const { digest: _,...body }=spec; spec.digest=digestV3(body); }
  plan.body.methodRef = v3ContractRef(workPlan); plan.body.appliesToSpec=v3ContractRef(spec); { const { digest: _,...body }=plan; plan.digest=digestV3(body); }
  producerGraph.tasks[0]!.contractOverrides = { spec: v3ContractRef(spec),plan: v3ContractRef(plan) };
  graph.works.push(...producerGraph.works); graph.tasks.push(...producerGraph.tasks); graph.contracts.push(...producerGraph.contracts);
  const contractPath = (c: Graph["contracts"][number]) => `work/${encodeURIComponent(c.owner.projectId)}/${encodeURIComponent(c.owner.workId)}/v3` + (c.owner.kind === "task" ? `/tasks/${encodeURIComponent(c.owner.taskId)}` : "") + `/contracts/${c.kind}/${encodeURIComponent(c.id)}@${c.revision}.json`;
  const sourceHashes = new Map<string,string>();
  for (const work of graph.works) { const path = `work/${work.projectId}/${work.workId}/v3/work.json`; sourceHashes.set(path,await put(root,path,{ schema: "common.git-work-record.v3",kind: "work",value: work })); }
  for (const task of graph.tasks) { const path = `work/${task.projectId}/${task.workId}/v3/tasks/${task.taskId}.json`; sourceHashes.set(path,await put(root,path,{ schema: "common.git-work-record.v3",kind: "task",value: task })); }
  for (const c of graph.contracts) sourceHashes.set(contractPath(c),await put(root,contractPath(c),c));
  const sourceCommit = commit(root);
  const sourcePin = (path: string): GitSourcePin => ({ repositoryId: "synthetic-repository",commit: sourceCommit,path,sha256: sourceHashes.get(path)! });
  const sourceClosure = (selectedGraph: Graph) => ({ work: sourcePin(`work/${selectedGraph.works[0]!.projectId}/${selectedGraph.works[0]!.workId}/v3/work.json`),task: sourcePin(`work/${selectedGraph.tasks[0]!.projectId}/${selectedGraph.tasks[0]!.workId}/v3/tasks/${selectedGraph.tasks[0]!.taskId}.json`),contracts: selectedGraph.contracts.map((c) => sourcePin(contractPath(c))) });
  const consumerSources = sourceClosure(fixtureGraph()), producerSources = sourceClosure(producerGraph);
  const effective = resolveEffectiveContract(producerGraph,producer);
  const producerBinding = { task: producer,workRevision: effective.workRevision,taskRevision: effective.taskRevision,workContracts: effective.workContracts,contracts: effective.contracts,effectiveDigest: effective.digest };
  const producerContext = await pin(root,"work/artifact-sources/producer-context.json",readGitWorkV3Context({ graph: producerGraph,task: producer,mode: "compact" }));
  const artifactSource = await pin(root,"work/artifact-sources/artifact.json",{ schema: "common.git-artifact.v3",id: "selected-artifact",revision: 3,producer: producerBinding,body: { synthetic: "useful output" } });
  const artifact = { id: "selected-artifact",revision: 3,digest: artifactSource.sha256 };
  const consumer = consumerContractBinding(graph,fixtureTask);
  const definition = await pin(root,"work/artifact-sources/gate.json",{ schema: "common.git-artifact-gate.v3",instanceId: "consumer-gate",revision: 4,consumer,selectionOwnerId: "consumer-owner",alternatives: [{ id: "selected-A",producer: producerBinding,artifact,criterionIds: ["integrates"] }] });
  const selection = { instanceId: "consumer-gate",revision: 4,definition,consumer,selectedAlternative: "selected-A",selectionOwnerId: "consumer-owner" };
  const evidence = await pin(root,"work/artifact-sources/evidence.json",{ synthetic: "bounded independent source observation" });
  const producerMilestone = await pin(root,"work/artifact-sources/milestone.json",{ schema: "common.git-artifact-producer-milestone.v3",producer: producerBinding,artifact,artifactSource,milestone: "verified" });
  const selectionReceipt = await pin(root,"work/artifact-sources/selection.json",{ schema: "common.git-artifact-selection.v3",selection });
  const criterion = await pin(root,"work/artifact-sources/criterion.json",{ schema: "common.git-artifact-criterion.v3",selection,criterionId: "integrates",status: "pass",evidence: [evidence] });
  const receiverUse = await pin(root,"work/artifact-sources/receiver.json",{ schema: "common.git-artifact-receiver-use.v3",selection,producer: producerBinding,artifact,receiverOwnerId: "consumer-native-owner",status: "used",evidence: [evidence] });
  const selected: SelectedExternalArtifact = { schema: "common.git-selected-external-artifact.v3",producer: producerBinding,producerContext,producerSources,consumerSources,artifact,artifactSource,producerMilestone,selection,selectionReceipt,criteria: [{ criterionId: "integrates",receipt: criterion }],receiverOwnerId: "consumer-native-owner",receiverUse };
  graph.externalArtifacts = [selected]; graph.dependencies = [{ producer,consumer: fixtureTask,required: "verified",artifact }];
  const sourceSelection: GitWorkSelection = { repositoryRoot: root,repositoryId: "synthetic-repository",commit: git(root,"rev-parse","HEAD"),...fixtureScope };
  return { root,graph,selected,sourceSelection,producer };
}
test("R02 actual selected source/producer Task override/consumer gate/evidence/receiver roundtrip through every context path", async () => {
  const f = await artifactFixture();
  expect(() => resolveEffectiveContract(f.graph,fixtureTask)).toThrow("ACTUAL_SOURCE_QUALIFICATION");
  const full = expandGitWorkV3Context(await readSourceQualifiedGitWorkV3Context({ graph: f.graph,task: fixtureTask,mode: "full",reader: await openExactV3SourceReader(f.sourceSelection) }));
  expect(full.effective.dependencyTasks[0]!.contracts.spec.id).toBe("producer-task-spec");
  expect(full.effective.externalArtifacts[0]!.producer.contracts.spec.id).toBe("producer-task-spec");
  for (const mode of ["compact","dispatch","resume"] as const) expect(expandGitWorkV3Context(await readSourceQualifiedGitWorkV3Context({ graph: f.graph,task: fixtureTask,mode,expectedDigest: full.effective.digest,reader: await openExactV3SourceReader(f.sourceSelection) })).effective).toEqual(full.effective);
});
test.each(["nonexistent","wrong-producer","wrong-receiver","stale-gate","missing-criterion","changed-consumer","forged-producer-source","forged-consumer-source"])("R02 rejects %s pinned selected artifact", async (fault) => {
  const f = await artifactFixture();
  if (fault === "nonexistent") f.selected.artifactSource.path += ".missing";
  if (fault === "wrong-producer") f.selected.producer.contracts = f.selected.producer.workContracts;
  if (fault === "wrong-receiver") f.selected.receiverOwnerId = "foreign-native-owner";
  if (fault === "stale-gate") f.selected.selection.revision++;
  if (fault === "missing-criterion") f.selected.criteria=[];
  if (fault === "forged-producer-source") f.selected.producerSources.task.sha256="0".repeat(64);
  if (fault === "forged-consumer-source") f.selected.consumerSources.work.sha256="0".repeat(64);
  if (fault === "changed-consumer") f.graph.tasks[0]!.revision++;
  await expect(readSourceQualifiedGitWorkV3Context({ graph: f.graph,task: fixtureTask,mode: "full",reader: await openExactV3SourceReader(f.sourceSelection) })).rejects.toThrow();
});

async function ownerFixture() { const root=await directory(); const head=await seedFixture(root); return { root,target: { repositoryRoot: root,ownerRef: fixtureRef,expectedHead: head,expectedAuthorityRevision: 1 } }; }
async function evolve(root: string, head: string, path: string, change: (record: { value: Record<string,unknown> }) => void) { git(root,"checkout","--detach",head); const record=JSON.parse(git(root,"show",head+":"+path)); change(record); await put(root,path,record); const next=commit(root); git(root,"update-ref",fixtureRef,next,head); return next; }
test.each(["work","task","run"] as const)("R03 immutable %s creation replay survives legitimate record evolution and rejects foreign identity", async (kind) => {
  const f=await ownerFixture();
  const run: Run = { runId: "placeholder",task: fixtureTask,attempt: 1,status: "reserved",effectiveContractDigest: resolveEffectiveContract(fixtureGraph(),fixtureTask).digest,sourceSnapshotId: "synthetic-source",parentAuthorityRef: "synthetic-authority",nativeBindings: [],effectRefs: [] };
  const request = kind === "work" ? fixtureWorkRequest() : kind === "task" ? fixtureTaskRequest() : { ...fixtureTaskRequest("run-evolution"),kind: "run" as const,scope: { kind: "task" as const,...fixtureTask },record: { ...run,runId: { $allocatedId: true } } };
  const created=await allocateGitWorkIdentity(request,f.target,fixtureOwner(f.root));
  const path=kind === "work" ? `work/${fixtureScope.projectId}/${created.id}/v3/work.json` : `work/${fixtureScope.projectId}/${fixtureScope.workId}/v3/${kind === "task" ? "tasks" : "runs"}/${created.id}.json`;
  const next=await evolve(f.root,created.head,path,(row) => { row.value.status = kind === "work" ? "admitted" : kind === "task" ? "ready" : "started"; if (kind !== "run") { row.value.revision=2; row.value.title="Legitimate revision"; } else { row.value.nativeBindings=[{ runtimeId: "synthetic-runtime",nativeSessionId: "observed-synthetic",nativeTaskId: null,nativeRunId: null,paneId: null }]; } });
  const replay=await allocateGitWorkIdentity(request,{ ...f.target,expectedHead: next },fixtureOwner(f.root)); expect(replay.id).toBe(created.id); expect(replay.status).toBe("existing"); expect(replay.head).toBe(next);
  const tampered=await evolve(f.root,next,path,(row) => { if (kind === "run") row.value.runId="foreign"; else if (kind === "task") row.value.taskId="foreign"; else row.value.workId="foreign"; });
  await expect(allocateGitWorkIdentity(request,{ ...f.target,expectedHead: tampered },fixtureOwner(f.root))).rejects.toThrow();
});
test("R03 immutable ledger edits remain conflicts even if restored at current head", async () => {
  const f=await ownerFixture(); const request=fixtureTaskRequest(); const created=await allocateGitWorkIdentity(request,f.target,fixtureOwner(f.root));
  const path=`work/.identity-requests/${sourceSha256(request.ownerId)}/${sourceSha256(request.createRequestId)}.json`; const old=git(f.root,"show",created.head+":"+path);
  git(f.root,"checkout","--detach",created.head); await writeFile(join(f.root,path),old+" \n"); const changed=commit(f.root); await writeFile(join(f.root,path),old+"\n"); const restored=commit(f.root); git(f.root,"update-ref",fixtureRef,restored,created.head); void changed;
  await expect(allocateGitWorkIdentity(request,{ ...f.target,expectedHead: restored },fixtureOwner(f.root))).rejects.toThrow("LEDGER_REWRITTEN");
});

test("R04 missing/wrong native hook policy rejects before mutation; qualified invocation leaves Git/native hook configuration intact", async () => {
  const f=await ownerFixture(); const marker=join(f.root,"prepared"); const hooks=join(f.root,"synthetic-hooks"); await mkdir(hooks); const hook=join(hooks,"reference-transaction"); await symlink("/usr/bin/touch",hook); git(f.root,"config","core.hooksPath",hooks);
  const before=git(f.root,"count-objects","-v");
  await expect(allocateGitWorkIdentity(fixtureTaskRequest(),f.target,fixtureOwner(f.root,{ gitHookPolicy: undefined }))).rejects.toThrow("GIT_HOOK_POLICY");
  expect(git(f.root,"rev-parse",fixtureRef)).toBe(f.target.expectedHead); expect(git(f.root,"count-objects","-v")).toBe(before);
  const malicious=fixtureOwner(f.root); const original=malicious.withCurrentAuthority.bind(malicious); malicious.withCurrentAuthority=(input,operation) => original(input,(authority) => operation({ ...authority,gitHookPolicy: { ...authority.gitHookPolicy,head: "0".repeat(40) } }));
  await expect(allocateGitWorkIdentity(fixtureTaskRequest(),f.target,malicious)).rejects.toThrow("GIT_HOOK_POLICY");
  const input=join(f.root,"worker-input.json"); await writeFile(input,JSON.stringify({ request: fixtureTaskRequest(),target: f.target,syntheticLostCasAcknowledgement: true }));
  const worker=Bun.spawn([process.execPath,"--no-env-file","--no-install",new URL("./fixtures/v3-worker.ts",import.meta.url).pathname,input],{ stdout: "pipe",stderr: "pipe" });
  const receipt=JSON.parse(await new Response(worker.stdout).text()); expect(await worker.exited).toBe(0); expect(receipt.status).toBe("reconciled");
  expect(git(f.root,"rev-list","--count",f.target.expectedHead+".."+fixtureRef)).toBe("1");
  expect(git(f.root,"config","--get","core.hooksPath")).toBe(hooks); await expect(readFile(marker)).rejects.toThrow();
  // Prove the harmless fixture hook itself works under an ordinary native Git call.
  git(f.root,"update-ref","refs/heads/synthetic-hook-positive",receipt.head); expect(await readFile(marker,"utf8")).toBe("");
});
