import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  gitRecordPath,
  openGitWork,
  taskVerificationTarget,
  validateGitWorkRecords,
  readGitWorkContext,
  type GitWorkRecord,
  type LogicalIdentity,
  type PinnedGitSource,
  type SelectedExternalArtifactPrerequisite,
  type TaskRecord,
} from "./index.ts";
import {
  JEV_MODEL,
  type ChildAssignment,
  type CurrentChildAuthority,
  type NativeBinding,
  type WholeLens,
} from "../jev/contract.ts";
import {
  canonicalJson,
  semanticDigest,
  prepareChildSemanticPacket,
  prepareLogicalAssignmentSemanticPacket,
  qualifyChildAssignmentAuthority,
} from "../jev/policy.ts";
import {
  prepareWorkflowDecision,
  normalizeWorkflowReply,
  recordNativeDelegationReceipt,
  recordChildArtifactDisposition,
  type CurrentWorkflowBinding,
  type NativeDelegationObservation,
  type PreparedWorkflowDecision,
  type WorkflowInput,
} from "../jev/workflows.ts";

// Independent synthetic source fixtures. Nothing here observes or authorizes native execution.
const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
const repositoryId = "independent/w01";
const identity = (workId: string, taskId = "same-task"): LogicalIdentity => ({
  domainId: "native-harness",
  projectId: "w01-independent",
  workId,
  taskId,
  issueId: "issue",
  specId: "spec",
  specRevision: 1,
  planId: "plan",
  planRevision: 1,
});
const hash = (value: string) => createHash("sha256").update(value).digest("hex");
function sourceTask(workId: string, id = "same-task", complete = false): TaskRecord {
  return {
    schema: "common.git-work.v1",
    kind: "task",
    id,
    domainId: "native-harness",
    projectId: "w01-independent",
    workId,
    title: "Independent source contract fixture",
    logical: identity(workId, id),
    state: {
      execution: complete ? "complete" : "planned",
      verification: complete ? "passed" : "pending",
      localAudit: complete ? "result-durable" : "intent-durable",
      sharedAudit: "pending",
      formalCompletion: complete ? "audit-synchronization-pending" : "pending",
    },
    executionClass: "source-only",
    dependsOn: [],
    decisionIds: [],
    effectIds: [],
    verificationEvidenceIds: complete ? [`proof-${id}`] : [],
  };
}
function records(workId: string, tasks: TaskRecord[]): GitWorkRecord[] {
  const base = {
    schema: "common.git-work.v1" as const,
    domainId: "native-harness",
    projectId: "w01-independent",
    workId,
  };
  const result: GitWorkRecord[] = [
    {
      ...base,
      kind: "work",
      id: workId,
      repositoryId,
      issueId: "issue",
      purpose: {
        objective: "Verify bounded source contracts",
        successCriteria: ["Exact source identity"],
        inScope: ["Source"],
        outOfScope: ["Native authority"],
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
      outcome: "Source contracts",
      criteria: ["Exact identity"],
    },
    {
      ...base,
      kind: "plan",
      id: "plan",
      revision: 1,
      specId: "spec",
      specRevision: 1,
      objective: "Independent imported source verification",
    },
    ...tasks,
  ];
  for (const task of tasks.filter((t) => t.state.execution === "complete")) {
    const artifact = {
      schemaVersion: "1.0.0",
      kind: "verification",
      id: `w01-independent:verification:${task.id}`,
      projectId: base.projectId,
      domainId: base.domainId,
      createdAt: "2026-10-21T00:00:00Z",
      payload: {
        targetRefs: [taskVerificationTarget(task)],
        criteria: [
          {
            id: "source",
            status: "pass",
            message: "Synthetic source fixture",
            evidenceRefs: ["fixture:evidence:source"],
          },
        ],
        inputHashes: [{ path: "fixture.json", sha256: "a".repeat(64) }],
        runtime: { name: "independent-fixture", version: "1" },
      },
    };
    result.push({
      ...base,
      kind: "input",
      id: `verification-${task.id}`,
      inputKind: "source-owner-record",
      origin: {
        ownerId: "independent-fixture",
        locator: "fixture:source",
        kind: "source-owner-capture",
      },
      content: JSON.parse(JSON.stringify(artifact)),
    });
    result.push({
      ...base,
      kind: "evidence",
      id: `proof-${task.id}`,
      evidenceKind: "verification",
      source: {
        kind: "snapshot",
        path: gitRecordPath(base.projectId, workId, {
          kind: "input",
          id: `verification-${task.id}`,
        }),
        fragment: "#/content",
      },
    });
    if (task.executionClass === "native-effect") {
      const ref = (id: string) => ({
        kind: "snapshot" as const,
        path: gitRecordPath(base.projectId, workId, { kind: "input", id }),
        fragment: "#/content",
      });
      const input = (id: string, content: unknown): GitWorkRecord => ({
        ...base,
        kind: "input",
        id,
        inputKind: "source-owner-record",
        origin: {
          ownerId: "independent-fixture",
          locator: `fixture:${id}`,
          kind: "source-owner-capture",
        },
        content: JSON.parse(JSON.stringify(content)),
      });
      result.push(
        input(`intent-${task.id}`, { task: task.id, phase: "intent", synthetic: true }),
        input(`result-${task.id}`, { task: task.id, phase: "result", synthetic: true }),
        input(`execution-receipt-${task.id}`, {
          schema: "common.git-prerequisite-receipt.v1",
          logical: task.logical,
          occurrence: task.occurrence,
          state: task.state,
          intent: ref(`intent-${task.id}`),
          result: ref(`result-${task.id}`),
          verification: ref(`verification-${task.id}`),
        }),
        {
          ...base,
          kind: "evidence",
          id: task.executionEvidenceId!,
          evidenceKind: "native-execution",
          source: ref(`execution-receipt-${task.id}`),
        },
      );
    }
  }
  return result;
}
async function git(root: string, ...args: string[]): Promise<string> {
  const process = Bun.spawn(["git", "-C", root, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...globalThis.process.env,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Independent Fixture",
      GIT_AUTHOR_EMAIL: "independent@example.invalid",
      GIT_COMMITTER_NAME: "Independent Fixture",
      GIT_COMMITTER_EMAIL: "independent@example.invalid",
    },
  });
  const [stdout, stderr, exit] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);
  if (exit !== 0) throw Error(`fixture git ${args[0]}: ${stderr}`);
  return stdout.trim();
}
async function put(root: string, path: string, value: unknown): Promise<string> {
  const bytes = JSON.stringify(value);
  await mkdir(dirname(join(root, path)), { recursive: true });
  await writeFile(join(root, path), bytes);
  return hash(bytes);
}
async function fixture(rows: GitWorkRecord[]) {
  const root = await mkdtemp(join(tmpdir(), "w01-independent-"));
  roots.push(root);
  await git(root, "init", "-q", "-b", "main");
  for (const row of rows)
    await put(
      root,
      gitRecordPath(row.projectId, row.workId, {
        kind: row.kind,
        id: row.id,
        ...(row.kind === "spec" || row.kind === "plan" ? { revision: row.revision } : {}),
      }),
      row,
    );
  const commit = async () => {
    await git(root, "add", ".");
    await git(root, "commit", "-qm", "Independent source fixture");
    return git(root, "rev-parse", "HEAD");
  };
  return { root, commit, oid: await commit() };
}
const open = (root: string, commit: string, workId: string) =>
  openGitWork({ repositoryRoot: root, repositoryId, commit, projectId: "w01-independent", workId });

async function externalFixture(nativeProducer = false, consumingRuntime?: string) {
  const producerTask = sourceTask("producer-work", "same-task", true);
  const consumerTask = sourceTask("consumer-work", "same-task", true);
  if (nativeProducer) {
    producerTask.executionClass = "native-effect";
    producerTask.runtimeId = "producer-runtime";
    producerTask.occurrence = {
      runtimeId: "producer-runtime",
      attemptId: "producer-attempt",
      occurrenceId: "producer-occurrence",
    };
    producerTask.executionEvidenceId = "original-v1-execution";
  }
  if (consumingRuntime !== undefined) consumerTask.runtimeId = consumingRuntime;
  const f = await fixture([
    ...records("producer-work", [producerTask]),
    ...records("consumer-work", [consumerTask]),
  ]);
  const pin = async (path: string, value: unknown): Promise<PinnedGitSource> => {
    const sha256 = await put(f.root, path, value);
    return { kind: "git", repositoryId, commit: await f.commit(), path, sha256 };
  };
  const recordPath = gitRecordPath(producerTask.projectId, producerTask.workId, {
    kind: "task",
    id: producerTask.id,
  });
  const record: PinnedGitSource = {
    kind: "git",
    repositoryId,
    commit: f.oid,
    path: recordPath,
    sha256: hash(JSON.stringify(producerTask)),
  };
  const artifact = await pin("proof/artifact.json", {
    meaning: "Independently pinned reusable source artifact",
  });
  const producerClosure = await pin("proof/producer-closure.json", {
    schema: "common.git-artifact-source-closure.v1",
    logical: producerTask.logical,
    sources: [record, artifact],
  });
  const consumerClosure = await pin("proof/consumer-closure.json", {
    schema: "common.git-artifact-source-closure.v1",
    logical: consumerTask.logical,
    sources: [artifact],
  });
  const producer = {
    logical: producerTask.logical,
    ownerId: "producer-owner",
    record,
    sourceClosure: producerClosure,
    milestone: "execution-and-verification" as const,
    artifact,
  };
  const definition = await pin("proof/definition.json", {
    schema: "common.git-artifact-gate-definition.v1",
    selectionOwnerId: "consumer-owner",
    alternatives: [
      {
        id: "legacy-source",
        producer: producerTask.logical,
        ownerId: producer.ownerId,
        sourceClosure: producerClosure,
        milestone: producer.milestone,
        artifact,
        criterionIds: ["exact-source"],
      },
    ],
  });
  const selection = {
    instanceId: "selected-consumer-gate",
    revision: 1,
    definition,
    consumer: consumerTask.logical,
    consumerSourceClosure: consumerClosure,
    selectedAlternative: "legacy-source",
    selectionOwnerId: "consumer-owner",
  };
  const ownerSelectionReceipt = await pin("proof/selection.json", {
    schema: "common.git-artifact-selection-receipt.v1",
    gate: selection,
  });
  const ownerMilestoneReceipt = await pin("proof/milestone.json", {
    schema: "common.git-external-milestone-receipt.v1",
    ownerId: "producer-owner",
    producer,
  });
  const criterion = await pin("proof/criterion.json", {
    schema: "common.git-artifact-criterion-receipt.v1",
    selection,
    producer,
    criterionId: "exact-source",
    status: "pass",
    evidence: [artifact],
  });
  const criterionReceipts = [{ criterionId: "exact-source", evidence: criterion }];
  const receiverUse = await pin("proof/use.json", {
    schema: "common.git-external-artifact-receiver-use.v1",
    selection,
    producer,
    ownerSelectionReceipt,
    ownerMilestoneReceipt,
    criterionReceipts,
    status: "used",
    receiverOwnerId: "consumer-owner",
    evidence: [artifact],
  });
  const prerequisite: SelectedExternalArtifactPrerequisite = {
    schema: "common.git-selected-external-artifact.v1",
    producer: { ...producer, ownerMilestoneReceipt },
    consumerGate: { ...selection, ownerSelectionReceipt, criterionReceipts },
    qualification: { status: "owner-qualified", receiverUse },
    grantsNativePermission: false,
  };
  const check = async (change?: (value: SelectedExternalArtifactPrerequisite) => void) => {
    const dependency = structuredClone(prerequisite);
    change?.(dependency);
    const task: TaskRecord = {
      ...consumerTask,
      schema: "common.git-work-task.v2",
      externalPrerequisites: [dependency],
    };
    await put(
      f.root,
      gitRecordPath(task.projectId, task.workId, { kind: "task", id: task.id }),
      task,
    );
    const reader = await open(f.root, await f.commit(), "consumer-work");
    return { reader, validation: await validateGitWorkRecords(reader) };
  };
  return { ...f, check, prerequisite, pin };
}

const nativeBinding = (child = false): NativeBinding => ({
  runtime: "native-codex",
  sessionId: child ? "independent-child" : "independent-parent",
  agentId: null,
  agentType: null,
  parentSessionId: child ? "independent-parent" : null,
  rootSessionId: "independent-parent",
  lineageSessionIds: child ? ["independent-parent", "independent-child"] : ["independent-parent"],
  profileId: null,
  nativeTaskId: null,
  nativeRunId: null,
  logical: identity("delegated-work", child ? "child-task" : "parent-task"),
  admissionRef: "fixture:admission",
});
const wholeLens = (): WholeLens => ({
  currentAuthority: { text: "Verify this bounded source assignment", sourceRefs: ["fixture:user"] },
  wholeObjective: "Preserve exact task and authority bindings",
  taskGoal: "Verify source contracts",
  acceptedWhy: [
    {
      text: "Keep source evidence useful across parent changes",
      attribution: "current-user",
      sourceRefs: ["fixture:why"],
    },
  ],
  acceptedDecisions: [{ id: "fixture:decision", revision: "1", sourceRefs: ["fixture:source"] }],
  owners: [{ role: "native", id: "codex" }],
  dependencies: [],
  completedEffectRefs: ["fixture:completed"],
  uncertainEffectRefs: ["fixture:unknown"],
  remainingAcceptance: ["fixture:owner-acceptance"],
});
const assignment = (): ChildAssignment => ({
  schema: "jev.child-assignment.v1",
  assignmentId: "legacy-child-assignment",
  revision: 1,
  status: "admitted",
  actorId: "independent-child",
  logical: identity("delegated-work", "child-task"),
  sourceRevision: "independent-source-v1",
  sourceClosure: { path: "fixture/source-closure.json", sha256: "a".repeat(64) },
  packetDigest: "b".repeat(64),
  parentAuthority: {
    assignmentId: "parent-assignment",
    revision: 1,
    domainId: "native-harness",
    projectId: "w01-independent",
    workId: "delegated-work",
    ownerId: "parent-owner",
    status: "active",
    authorityReference: { path: "fixture/authority.json", sha256: "c".repeat(64) },
  },
  route: { kind: "native-child" },
});
const assignmentReference = (a: ChildAssignment) => ({
  assignmentId: a.assignmentId,
  revision: a.revision,
  assignmentDigest: semanticDigest(a),
});
const authority = (a: ChildAssignment): CurrentChildAuthority => ({
  parentAuthority: structuredClone(a.parentAuthority),
  childAssignment: assignmentReference(a),
  childStatus: "admitted",
  carryForward: null,
});
function delegationFixture(withExpected = true) {
  const a = assignment();
  const workflow: Extract<WorkflowInput, { kind: "delegation" }> = {
    kind: "delegation",
    taskPacket: {
      id: "independent-task-packet",
      goal: "Verify exact source contracts",
      packetDigest: a.packetDigest,
      requiredPostconditionIds: ["source-checked"],
      ...(withExpected ? { expectedChild: a } : {}),
    },
    profiles: ["high", "low"].map((effort) => ({
      profileId: `sol-${effort}`,
      model: "gpt-6.1-sol",
      effort,
      availability: "available",
      applicability: "applicable",
      authorizationRef: "fixture:profiles",
      sourceRefs: ["fixture:profile-catalog"],
    })),
  };
  const prepared = prepareWorkflowDecision({
    requestId: "independent-delegation",
    event: {
      eventId: "independent-result",
      kind: "worker_result",
      binding: nativeBinding(),
      sourceRevision: a.sourceRevision,
      materialRevision: "result-v1",
      material: true,
      privacy: "admitted",
      externalEvidence: false,
      facts: { observed: "Synthetic independent source result" },
    },
    lens: wholeLens(),
    workflow,
  });
  if (prepared.status !== "prepared")
    throw Error(`independent fixture preparation: ${canonicalJson(prepared)}`);
  const p: PreparedWorkflowDecision = prepared;
  const current: CurrentWorkflowBinding = {
    authorized: true,
    bindingDigest: p.bindingDigest,
    sourceRevision: p.sourceRevision,
    workflowDigest: p.workflowDigest,
    evidenceRefs: ["fixture:current-owner"],
    childAuthority: authority(a),
  };
  const raw = {
    schema: "jev.semantic-result.v1",
    status: "ok",
    requestId: p.request.requestId,
    stateDigest: p.request.stateDigest,
    purpose: "semantic_bundle",
    requestedModel: JEV_MODEL,
    returnedModel: JEV_MODEL,
    sourcePostconditionVerified: true,
    executionAuthorityGranted: false,
    automaticWrite: false,
    answers: Object.fromEntries(
      Object.entries(p.request.questions).map(([key, q]) => [
        key,
        q.type === "noul"
          ? { type: "noul", noul: p.questionMeanings[key]?.endsWith(":conflict") ? 0.01 : 0.99 }
          : {
              type: "choice",
              choice: "sol-high",
              confidence: 0.99,
              probabilities: Object.fromEntries(
                Object.keys(q.criteria).map((id) => [id, id === "sol-high" ? 1 : 0]),
              ),
            },
      ]),
    ),
  };
  const advice = normalizeWorkflowReply(p, raw, current);
  const observation: NativeDelegationObservation = {
    observationId: "fixture:observed",
    taskPacketDigest: a.packetDigest,
    requestedProfile: { profileId: "sol-high", model: "gpt-6.1-sol", effort: "high" },
    resolvedProfile: { profileId: "sol-high", model: "gpt-6.1-sol", effort: "high" },
    childBinding: nativeBinding(true),
    assignment: assignmentReference(a),
    status: "observed",
    postconditions: [{ id: "source-checked", satisfied: true, evidenceRefs: ["fixture:checked"] }],
    evidenceRefs: ["fixture:observation"],
  };
  return {
    a,
    p,
    current,
    advice,
    observation,
    receipt: (o = observation, c = current) =>
      recordNativeDelegationReceipt({ prepared: p, advice, current: c, observation: o }),
  };
}

describe("W01 independent public source verification", () => {
  test("a legitimate v1 local dependency remains valid through real openGitWork", async () => {
    const producer = sourceTask("consumer-work", "producer", true);
    const consumer = sourceTask("consumer-work", "consumer", true);
    consumer.dependsOn = [{ taskId: producer.id, milestone: "execution-and-verification" }];
    const f = await fixture(records("consumer-work", [producer, consumer]));
    expect(await validateGitWorkRecords(await open(f.root, f.oid, "consumer-work"))).toMatchObject({
      valid: true,
      findings: [],
      nativePermissionGranted: false,
    });
  });
  test("exact external reuse qualifies with same-named Tasks in two Works and no foreign-map copy", async () => {
    const f = await externalFixture();
    const { reader, validation } = await f.check();
    expect(validation).toMatchObject({ valid: true, findings: [], nativePermissionGranted: false });
    expect((await reader.listRecords()).filter((r) => r.kind === "task")).toHaveLength(1);
    const context = await readGitWorkContext(reader, {
      taskId: "same-task",
      latestRelevant: "current",
    });
    expect(context.status).toBe("source-qualified");
    expect(context.records.filter((r) => r.kind === "task")).toHaveLength(1);
  }, 30_000);
  test.each([undefined, "different-consumer-runtime"])(
    "an external native-effect producer qualifies through original v1 proof and external receiver-use with consumer runtime %s",
    async (consumingRuntime) => {
      const f = await externalFixture(true, consumingRuntime);
      const producerReader = await open(f.root, f.oid, "producer-work");
      expect(await validateGitWorkRecords(producerReader)).toMatchObject({
        valid: true,
        findings: [],
        nativePermissionGranted: false,
      });
      const producerTask = await producerReader.readRecord({ kind: "task", id: "same-task" });
      expect(producerTask).toMatchObject({
        executionClass: "native-effect",
        runtimeId: "producer-runtime",
        executionEvidenceId: "original-v1-execution",
      });
      const { reader, validation } = await f.check();
      expect(validation).toMatchObject({
        valid: true,
        findings: [],
        nativePermissionGranted: false,
      });
      const consumer = await reader.readRecord({ kind: "task", id: "same-task" });
      expect(consumer.kind === "task" && consumer.runtimeId).toBe(consumingRuntime);
      expect(
        (await readGitWorkContext(reader, { taskId: "same-task", latestRelevant: "current" }))
          .status,
      ).toBe("source-qualified");
      expect(
        (await reader.listRecords()).some((record) => record.id === "original-v1-execution"),
      ).toBe(false);
      const missingReceiver = await f.check((p) => {
        p.qualification = { status: "unresolved", receiverUse: null };
      });
      expect(missingReceiver.validation.valid).toBe(false);
      expect(
        missingReceiver.validation.findings.some((finding) =>
          finding.startsWith("EXTERNAL_RECEIVER_USE_REQUIRED"),
        ),
      ).toBe(true);
    },
    30_000,
  );
  test("external producer, consumer, source, milestone and receiver-use mismatches are held", async () => {
    const f = await externalFixture();
    const changes: Array<[string, (p: SelectedExternalArtifactPrerequisite) => void]> = [
      [
        "producer Work",
        (p) => {
          p.producer.logical.workId = "consumer-work";
        },
      ],
      [
        "producer Task",
        (p) => {
          p.producer.logical.taskId = "foreign-task";
        },
      ],
      [
        "producer owner",
        (p) => {
          p.producer.ownerId = "foreign-owner";
        },
      ],
      [
        "consumer Work",
        (p) => {
          p.consumerGate.consumer.workId = "producer-work";
        },
      ],
      [
        "consumer revision",
        (p) => {
          p.consumerGate.consumer.specRevision = 2;
        },
      ],
      [
        "source bytes",
        (p) => {
          p.producer.artifact.sha256 = "0".repeat(64);
        },
      ],
      [
        "source snapshot",
        (p) => {
          p.producer.sourceClosure.commit = "0".repeat(40);
        },
      ],
      [
        "milestone",
        (p) => {
          p.producer.milestone = "formal-task-completion";
        },
      ],
      [
        "receiver-use absence",
        (p) => {
          p.qualification = { status: "unresolved", receiverUse: null };
        },
      ],
      [
        "receiver-use rejected",
        (p) => {
          p.qualification.status = "rejected";
        },
      ],
      [
        "selection revision",
        (p) => {
          p.consumerGate.revision = 2;
        },
      ],
    ];
    for (const [label, change] of changes) {
      const { reader, validation: result } = await f.check(change);
      expect(result.valid, label).toBe(false);
      expect(result.findings.length, label).toBeGreaterThan(0);
      expect(
        (await readGitWorkContext(reader, { taskId: "same-task", latestRelevant: "current" }))
          .status,
        label,
      ).toBe("held");
    }
  }, 30_000);
  test("correctly pinned rejected or foreign receiver-use payloads cannot satisfy the selected consumer", async () => {
    const f = await externalFixture();
    const { reader } = await f.check();
    const original = (await reader.readSource(f.prerequisite.qualification.receiverUse!)).value;
    for (const [field, value] of [
      ["status", "rejected"],
      ["receiverOwnerId", "foreign-owner"],
    ]) {
      const receipt = await f.pin(`proof/wrong-${field}.json`, {
        ...(original as Record<string, unknown>),
        [field!]: value,
      });
      const { validation } = await f.check((p) => {
        p.qualification = { status: "owner-qualified", receiverUse: receipt };
      });
      expect(validation.valid).toBe(false);
      expect(
        validation.findings.some((finding) =>
          finding.startsWith("EXTERNAL_RECEIVER_USE_BINDING_MISMATCH"),
        ),
      ).toBe(true);
    }
  }, 30_000);
  test("the real existing delegation call accepts an exact separately admitted child with nullable native IDs", () => {
    const f = delegationFixture();
    expect(f.receipt()).toMatchObject({
      status: "verified-native-delegation",
      executionAuthorityGranted: false,
      taskCompletionGranted: false,
      retryAllowed: false,
    });
    expect(
      prepareChildSemanticPacket({
        parentBinding: nativeBinding(),
        childBinding: nativeBinding(true),
        parentLens: wholeLens(),
        childTaskGoal: "Verify exact source",
      }).binding.logical?.taskId,
    ).toBe("child-task");
  });
  test("the real existing delegation call holds missing expected-child and null or foreign logical results", () => {
    expect(delegationFixture(false).receipt().status).toBe("held");
    const f = delegationFixture();
    for (const key of [
      "domainId",
      "projectId",
      "workId",
      "taskId",
      "issueId",
      "specId",
      "planId",
      "specRevision",
      "planRevision",
    ] as const) {
      const o = structuredClone(f.observation);
      const logical = o.childBinding!.logical!;
      if (key === "specRevision" || key === "planRevision") logical[key] = 2;
      else logical[key] = `foreign-${key}`;
      expect(f.receipt(o).status, key).toBe("held");
    }
    const o = structuredClone(f.observation);
    o.childBinding!.logical = null;
    expect(f.receipt(o).status).toBe("held");
  });
  test("stale assignment, packet, source and parent authority cannot verify a delegation receipt", () => {
    const f = delegationFixture();
    const staleAssignment = structuredClone(f.observation);
    staleAssignment.assignment!.revision = 2;
    expect(f.receipt(staleAssignment).status).toBe("held");
    const staleDigest = structuredClone(f.observation);
    staleDigest.assignment!.assignmentDigest = "0".repeat(64);
    expect(f.receipt(staleDigest).status).toBe("held");
    const stalePacket = structuredClone(f.observation);
    stalePacket.taskPacketDigest = "0".repeat(64);
    expect(f.receipt(stalePacket).status).toBe("held");
    expect(f.receipt(f.observation, { ...f.current, sourceRevision: "stale-source" }).status).toBe(
      "held",
    );
    for (const status of ["revoked", "retired"] as const) {
      const c = structuredClone(f.current);
      c.childAuthority!.parentAuthority.status = status;
      expect(f.receipt(f.observation, c).status).toBe("held");
      const child = structuredClone(f.current);
      child.childAuthority!.childStatus = status;
      expect(f.receipt(f.observation, child).status).toBe("held");
    }
    const staleParent = structuredClone(f.current);
    staleParent.childAuthority!.parentAuthority.revision = 2;
    expect(f.receipt(f.observation, staleParent).status).toBe("held");
  });
  test("explicit successor carry-forward preserves old assignment and blocks replay", () => {
    const f = delegationFixture();
    const before = canonicalJson(f.a);
    const current = authority(f.a);
    current.parentAuthority.revision = 2;
    current.parentAuthority.ownerId = "successor-owner";
    current.carryForward = {
      schema: "jev.child-carry-forward.v1",
      from: structuredClone(f.a.parentAuthority),
      to: structuredClone(current.parentAuthority),
      childAssignment: assignmentReference(f.a),
      successorOwnerDecision: { path: "fixture/successor.json", sha256: "d".repeat(64) },
      pendingAndUnknownEffects: [{ path: "fixture/unknown.json", sha256: "e".repeat(64) }],
      permitsReplay: false,
    };
    expect(qualifyChildAssignmentAuthority(f.a, current)).toMatchObject({
      status: "carried-forward",
      executionAuthorityGranted: false,
      permitsReplay: false,
    });
    expect(f.receipt(f.observation, { ...f.current, childAuthority: current }).status).toBe(
      "verified-native-delegation",
    );
    expect(canonicalJson(f.a)).toBe(before);
    const bad = structuredClone(current);
    bad.carryForward!.childAssignment.revision = 2;
    expect(qualifyChildAssignmentAuthority(f.a, bad).status).toBe("held");
  });
  test("late artifacts survive revoked ownership as evidence without continued effect permission", () => {
    const a = assignment();
    const original = {
      assignment: assignmentReference(a),
      logical: a.logical,
      sourceRevision: a.sourceRevision,
      sourceClosure: a.sourceClosure,
      packetDigest: a.packetDigest,
      artifact: { path: "fixture/late-result.json", sha256: "f".repeat(64) },
    };
    const current = authority(a);
    current.parentAuthority.status = "revoked";
    const receipt = recordChildArtifactDisposition({
      assignment: a,
      result: original,
      readoption: {
        current,
        ownerDecision: { path: "fixture/late-owner.json", sha256: "a".repeat(64) },
      },
    });
    expect(receipt).toMatchObject({
      status: "historical-evidence-only",
      result: original,
      continuedEffectsAuthorizedByResult: false,
      executionAuthorityGranted: false,
      permitsReplay: false,
    });
    const foreign = { ...original, logical: identity("foreign-work", "child-task") };
    expect(
      recordChildArtifactDisposition({ assignment: a, result: foreign, readoption: null }),
    ).toMatchObject({ status: "held", result: foreign, continuedEffectsAuthorizedByResult: false });
  });
  test("explicit logical assignment accepts independent native sessions without fabricated parent ancestry", () => {
    const parent = nativeBinding();
    const child: NativeBinding = {
      ...nativeBinding(true),
      parentSessionId: null,
      rootSessionId: "independent-child",
      lineageSessionIds: ["independent-child"],
    };
    const a = assignment();
    a.route = {
      kind: "logical-assignment",
      parentBindingDigest: semanticDigest(parent),
      childBindingDigest: semanticDigest(child),
      relationEvidenceRefs: ["fixture:explicit-relation"],
    };
    const packet = prepareLogicalAssignmentSemanticPacket({
      parentBinding: parent,
      childBinding: child,
      parentLens: wholeLens(),
      childTaskGoal: "Verify independent source",
      assignment: a,
    });
    expect(packet.binding.parentSessionId).toBeNull();
    expect(packet.binding.lineageSessionIds).toEqual(["independent-child"]);
    expect(packet.logicalParentSessionId).toBe(parent.sessionId);
    expect(() =>
      prepareChildSemanticPacket({
        parentBinding: parent,
        childBinding: child,
        parentLens: wholeLens(),
        childTaskGoal: "Verify independent source",
      }),
    ).toThrow();
  });
});
