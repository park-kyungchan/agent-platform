import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  openGitWork,
  assertRecord,
  gitRecordPath,
  taskVerificationTarget,
  validateGitWorkRecords,
  validateGitWorkTransition,
  readGitWorkContext,
  compactGitWorkContext,
  type TaskRecord,
  type GitWorkRecord,
  type LogicalIdentity,
  type PinnedGitSource,
  type SelectedExternalArtifactPrerequisite,
  type ArtifactSelectionReceipt,
  type ExternalArtifactMilestoneReceipt,
} from "./index.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((p) => rm(p, { recursive: true, force: true })));
});
const repositoryId = "fixture/external";
const logical = (workId: string): LogicalIdentity => ({
  domainId: "native-harness",
  projectId: "external",
  workId,
  issueId: "issue",
  specId: "spec",
  specRevision: 1,
  planId: "plan",
  planRevision: 1,
  taskId: "same",
});
const task = (workId: string): TaskRecord => ({
  schema: "common.git-work.v1",
  kind: "task",
  id: "same",
  domainId: "native-harness",
  projectId: "external",
  workId,
  title: "Reusable exact source",
  logical: logical(workId),
  executionClass: "source-only",
  state: {
    execution: "complete",
    verification: "passed",
    localAudit: "result-durable",
    sharedAudit: "delivery-confirmed",
    formalCompletion: "complete",
  },
  dependsOn: [],
  decisionIds: [],
  effectIds: [],
  verificationEvidenceIds: ["proof"],
});
async function fixture(
  milestone: SelectedExternalArtifactPrerequisite["producer"]["milestone"] = "execution-and-verification",
) {
  const root = await mkdtemp(join(tmpdir(), "external-artifact-"));
  roots.push(root);
  const git = async (...args: string[]) => {
    const child = Bun.spawn(["git", "-C", root, ...args], {
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        GIT_CONFIG_GLOBAL: "/dev/null",
        GIT_CONFIG_NOSYSTEM: "1",
        GIT_AUTHOR_NAME: "Source Fixture",
        GIT_AUTHOR_EMAIL: "fixture@example.invalid",
        GIT_COMMITTER_NAME: "Source Fixture",
        GIT_COMMITTER_EMAIL: "fixture@example.invalid",
      },
    });
    const [out, err, exit] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    if (exit) throw Error(err);
    return out.trim();
  };
  const put = async (path: string, value: unknown) => {
    const bytes = JSON.stringify(value);
    await mkdir(dirname(join(root, path)), { recursive: true });
    await writeFile(join(root, path), bytes);
    return createHash("sha256").update(bytes).digest("hex");
  };
  const commit = async () => {
    await git("add", ".");
    await git("commit", "-qm", "Source fixture");
    return git("rev-parse", "HEAD");
  };
  const pin = async (path: string, value: unknown): Promise<PinnedGitSource> => ({
    kind: "git",
    repositoryId,
    path,
    sha256: await put(path, value),
    commit: await commit(),
  });
  const putRecord = (row: GitWorkRecord) =>
    put(
      gitRecordPath(row.projectId, row.workId, {
        kind: row.kind,
        id: row.id,
        ...(row.kind === "spec" || row.kind === "plan" ? { revision: row.revision } : {}),
      }),
      row,
    );
  await git("init", "-q", "-b", "main");
  const producerTask = task("producer"),
    consumerTask = task("consumer");
  for (const t of [producerTask, consumerTask]) {
    const base = {
      schema: "common.git-work.v1" as const,
      domainId: t.domainId,
      projectId: t.projectId,
      workId: t.workId,
    };
    const rows: GitWorkRecord[] = [
      {
        ...base,
        kind: "work",
        id: t.workId,
        repositoryId,
        issueId: "issue",
        purpose: {
          objective: "Reuse source",
          successCriteria: ["Exact consumer binding"],
          inScope: ["Source"],
          outOfScope: ["Native effects"],
        },
        spec: { id: "spec", revision: 1 },
        plan: { id: "plan", revision: 1 },
        governingDecisionIds: [],
        historicalFenceIds: [],
      },
      {
        ...base,
        kind: "spec",
        id: "spec",
        revision: 1,
        outcome: "Reuse source",
        criteria: ["Exact binding"],
      },
      {
        ...base,
        kind: "plan",
        id: "plan",
        revision: 1,
        specId: "spec",
        specRevision: 1,
        objective: "Selected artifact",
      },
      t,
      {
        ...base,
        kind: "evidence",
        id: "proof",
        evidenceKind: "verification",
        source: { kind: "snapshot", path: `proof/${t.workId}.json` },
      },
    ];
    for (const row of rows) await putRecord(row);
    await put(`proof/${t.workId}.json`, {
      schemaVersion: "1.0.0",
      kind: "verification",
      id: `external:verification:${t.workId}`,
      projectId: t.projectId,
      domainId: t.domainId,
      createdAt: "2026-10-21T00:00:00Z",
      payload: {
        targetRefs: [taskVerificationTarget(t)],
        criteria: [
          {
            id: "source",
            status: "pass",
            message: "Synthetic source",
            evidenceRefs: ["fixture:evidence:source"],
          },
        ],
        inputHashes: [{ path: "source.ts", sha256: "a".repeat(64) }],
        runtime: { name: "fixture", version: "1" },
      },
    });
  }
  const initialCommit = await commit();
  const record: PinnedGitSource = {
    kind: "git",
    repositoryId,
    commit: initialCommit,
    path: gitRecordPath("external", "producer", { kind: "task", id: "same" }),
    sha256: createHash("sha256").update(JSON.stringify(producerTask)).digest("hex"),
  };
  const artifact = await pin("artifact.json", { reusable: true });
  const closure = async (t: TaskRecord) =>
    pin(`closure-${t.workId}.json`, {
      schema: "common.git-artifact-source-closure.v1",
      logical: t.logical,
      sources: [artifact],
    });
  const producer: ExternalArtifactMilestoneReceipt["producer"] = {
    logical: producerTask.logical,
    ownerId: "producer-owner",
    record,
    sourceClosure: await closure(producerTask),
    milestone,
    artifact,
  };
  const definition = await pin("gate.json", {
    schema: "common.git-artifact-gate-definition.v1",
    selectionOwnerId: "consumer-owner",
    alternatives: [
      {
        id: "selected",
        producer: producer.logical,
        ownerId: producer.ownerId,
        sourceClosure: producer.sourceClosure,
        artifact,
        milestone,
        criterionIds: ["source"],
      },
    ],
  });
  const selection: ArtifactSelectionReceipt["gate"] = {
    instanceId: "consumer-gate",
    revision: 1,
    definition,
    consumer: consumerTask.logical,
    consumerSourceClosure: await closure(consumerTask),
    selectedAlternative: "selected",
    selectionOwnerId: "consumer-owner",
  };
  const ownerSelectionReceipt = await pin("selection.json", {
    schema: "common.git-artifact-selection-receipt.v1",
    gate: selection,
  });
  const acceptance = {
    schemaVersion: "1.0.0",
    kind: "decision",
    id: "external:decision:acceptance",
    projectId: "external",
    domainId: "native-harness",
    createdAt: "2026-10-21T00:00:00Z",
    payload: {
      title: "Producer acceptance",
      status: "accepted",
      choice: "Accept selected producer",
      rationale: "Synthetic owner decision",
      evidenceRefs: ["fixture:evidence:source"],
      authority: {
        actorType: "project-owner",
        actorId: producer.ownerId,
        sourceEvidenceRef: "fixture:evidence:source",
        scope: "Selected producer",
      },
      details: {
        recordClass: "AcceptanceDecision",
        logical: producer.logical,
        accepted: true,
        ownerId: producer.ownerId,
      },
    },
  };
  const milestoneValue = {
    schema: "common.git-external-milestone-receipt.v1",
    ownerId: producer.ownerId,
    producer,
    ...(milestone === "acceptance"
      ? { acceptanceDecision: await pin("acceptance.json", acceptance) }
      : {}),
  };
  const ownerMilestoneReceipt = await pin("milestone.json", milestoneValue);
  const criterionValue = {
    schema: "common.git-artifact-criterion-receipt.v1",
    selection,
    producer,
    criterionId: "source",
    status: "pass",
    evidence: [artifact],
  };
  const criterionReceipts = [
    { criterionId: "source", evidence: await pin("criterion.json", criterionValue) },
  ];
  const useValue = {
    schema: "common.git-external-artifact-receiver-use.v1",
    selection,
    producer,
    ownerSelectionReceipt,
    ownerMilestoneReceipt,
    criterionReceipts,
    status: "used",
    receiverOwnerId: "consumer-owner",
    evidence: [artifact],
  };
  const receiverUse = await pin("use.json", useValue);
  const prerequisite: SelectedExternalArtifactPrerequisite = {
    schema: "common.git-selected-external-artifact.v1",
    producer: { ...producer, ownerMilestoneReceipt },
    consumerGate: { ...selection, ownerSelectionReceipt, criterionReceipts },
    qualification: { status: "owner-qualified", receiverUse },
    grantsNativePermission: false,
  };
  const save = async (p: SelectedExternalArtifactPrerequisite | null) => {
    const row: TaskRecord = {
      ...consumerTask,
      ...(p ? { schema: "common.git-work-task.v2" as const, externalPrerequisites: [p] } : {}),
    };
    await putRecord(row);
    return openGitWork({
      repositoryRoot: root,
      repositoryId,
      commit: await commit(),
      projectId: "external",
      workId: "consumer",
    });
  };
  return { save, pin, prerequisite, criterionValue, useValue, milestoneValue, acceptance };
}

test("completed external contracts remain fenced during transitions and compact v1 explicitly holds", async () => {
  const f = await fixture(),
    before = await f.save(f.prerequisite);
  expect((await validateGitWorkRecords(before)).valid).toBe(true);
  expect((await validateGitWorkTransition(before, before)).valid).toBe(true);
  const full = await readGitWorkContext(before, { taskId: "same", latestRelevant: "current" });
  expect(full.status).toBe("source-qualified");
  expect(full.records.find((r) => r.kind === "task")?.externalPrerequisites).toEqual(
    JSON.parse(JSON.stringify([f.prerequisite])),
  );
  const compact = compactGitWorkContext(full, { taskId: "same" });
  expect(compact.status).toBe("held");
  expect(compact.holdReasons).toContain("EXTERNAL_PREREQUISITES_REQUIRE_FULL_CONTEXT");
  const after = await f.save(null);
  expect((await validateGitWorkRecords(after)).valid).toBe(true);
  expect((await validateGitWorkTransition(before, after)).findings).toContain(
    "COMPLETED_TASK_CONTRACT_HISTORY_CHANGED:same",
  );
}, 60000);

test("formal completion and owner acceptance stay distinct and a rejected decision cannot qualify", async () => {
  const f = await fixture("acceptance"),
    before = await f.save(f.prerequisite);
  expect(await validateGitWorkRecords(before)).toMatchObject({
    valid: true,
    semanticAcceptanceEstablished: false,
  });
  const decision = structuredClone(f.acceptance);
  decision.payload.status = "rejected";
  const wrongDecision = await f.pin("wrong-decision.json", decision);
  const milestone = await f.pin("wrong-milestone.json", {
    ...f.milestoneValue,
    acceptanceDecision: wrongDecision,
  });
  const p = structuredClone(f.prerequisite);
  p.producer.ownerMilestoneReceipt = milestone;
  const use = await f.pin("wrong-use.json", { ...f.useValue, ownerMilestoneReceipt: milestone });
  p.qualification = { status: "owner-qualified", receiverUse: use };
  const result = await validateGitWorkRecords(await f.save(p));
  expect(result.valid).toBe(false);
  expect(
    result.findings.some((r) => r.startsWith("EXTERNAL_ACCEPTANCE_OWNER_DECISION_MISMATCH")),
  ).toBe(true);
}, 60000);

test("Task-only v2 admission refuses external prerequisites under v1 without widening other record kinds", async () => {
  const f = await fixture(),
    reader = await f.save(f.prerequisite);
  const external = await reader.readRecord({ kind: "task", id: "same" });
  expect(external.schema).toBe("common.git-work-task.v2");
  expect(assertRecord(external)).toEqual(external);
  expect(() => assertRecord({ ...external, schema: "common.git-work.v1" })).toThrow(
    "EXTERNAL_PREREQUISITES_REQUIRE_TASK_V2",
  );
  const local = task("consumer");
  expect(assertRecord(local).schema).toBe("common.git-work.v1");
  expect(assertRecord({ ...local, externalPrerequisites: [] }).schema).toBe("common.git-work.v1");
  expect(reader.work.schema).toBe("common.git-work.v1");
  expect(() => assertRecord({ ...reader.work, schema: "common.git-work-task.v2" })).toThrow(
    "INVALID_RECORD_IDENTITY",
  );
  for (const kind of ["spec", "plan"] as const) {
    const record = await reader.readRecord({ kind, id: kind, revision: 1 });
    expect(record.schema).toBe("common.git-work.v1");
    expect(() => assertRecord({ ...record, schema: "common.git-work-task.v2" })).toThrow(
      "INVALID_RECORD_IDENTITY",
    );
  }
}, 60000);
