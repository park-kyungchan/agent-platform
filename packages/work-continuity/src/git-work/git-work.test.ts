import { afterEach, describe, expect, test } from "bun:test";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Artifact } from "@palantirkc/ontology";
import {
  gitRecordPath,
  openGitWork,
  qualifyGitWorkStart,
  readGitWorkContext,
  validateGitWorkRecords,
  validateGitWorkTransition,
  taskVerificationTarget,
} from "./index.ts";
import type {
  GitWorkRecord,
  InputRecord,
  LogicalIdentity,
  TaskOperationalState,
  TaskRecord,
} from "./types.ts";

const temporaryRoots: string[] = [];
afterEach(async () => {
  await Promise.all(
    temporaryRoots.splice(0).map((path) => rm(path, { recursive: true, force: true })),
  );
});
const scope = { domainId: "native-harness", projectId: "fixture", workId: "portable-work" };
const base = { schema: "common.git-work.v1" as const, ...scope };
const state: TaskOperationalState = {
  execution: "planned",
  verification: "pending",
  localAudit: "intent-durable",
  sharedAudit: "pending",
  formalCompletion: "pending",
};
const done: TaskOperationalState = {
  execution: "complete",
  verification: "passed",
  localAudit: "result-durable",
  sharedAudit: "pending",
  formalCompletion: "audit-synchronization-pending",
};
const pathOf = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
  gitRecordPath(scope.projectId, scope.workId, {
    kind,
    id,
    ...(revision === undefined ? {} : { revision }),
  });
const ref = (id: string, fragment = "#/content") => ({
  kind: "snapshot" as const,
  path: pathOf("input", id),
  fragment,
});
const logical = (id: string): LogicalIdentity => ({
  ...scope,
  issueId: "issue",
  specId: "spec",
  specRevision: 1,
  planId: "plan",
  planRevision: 1,
  taskId: id,
});
const task = (id: string): TaskRecord => ({
  ...base,
  kind: "task",
  id,
  title: `Fixture Task ${id}`,
  logical: logical(id),
  state: { ...state },
  executionClass: "source-only",
  dependsOn: [],
  decisionIds: ["direction"],
  effectIds: [],
  verificationEvidenceIds: [],
});
const input = (id: string, content: InputRecord["content"]): InputRecord => ({
  ...base,
  kind: "input",
  id,
  inputKind: "source-owner-record",
  origin: { ownerId: "fixture", locator: `fixture:${id}`, kind: "source-owner-capture" },
  content,
});
function verification(id: string, record: TaskRecord = task(id)): Artifact {
  return {
    schemaVersion: "1.0.0",
    kind: "verification",
    id: `fixture:verification:${id}`,
    projectId: scope.projectId,
    domainId: scope.domainId,
    createdAt: "2026-10-14T00:00:00Z",
    payload: {
      targetRefs: [taskVerificationTarget(record)],
      criteria: [
        {
          id: "source-proof",
          status: "pass",
          message: "Synthetic source proof only.",
          evidenceRefs: ["fixture:evidence:proof"],
        },
      ],
      inputHashes: [{ path: "synthetic.json", sha256: "0".repeat(64) }],
      runtime: { name: "fixture", version: "1" },
    },
  };
}
function completed(id: string): GitWorkRecord[] {
  const row = task(id);
  row.state = { ...done };
  row.verificationEvidenceIds = [`proof-${id}`];
  row.artifact = {
    schemaVersion: "1.0.0",
    kind: "task",
    id: `fixture:task:${id}`,
    projectId: scope.projectId,
    domainId: scope.domainId,
    createdAt: "2026-10-14T00:00:00Z",
    payload: {
      title: row.title,
      objective: "Source fixture prerequisite",
      status: "complete",
      acceptanceCriteria: ["Source proof passes"],
      verificationRefs: [`fixture:verification:${id}`],
      details: { logical: { ...row.logical } },
    },
  };
  return [
    row,
    input(`verification-${id}`, JSON.parse(JSON.stringify(verification(id, row)))),
    {
      ...base,
      kind: "evidence",
      id: `proof-${id}`,
      evidenceKind: "verification",
      source: ref(`verification-${id}`),
    },
  ];
}
function initialRecords(): GitWorkRecord[] {
  return [
    {
      ...base,
      kind: "work",
      id: scope.workId,
      repositoryId: "fixture/repo",
      issueId: "issue",
      purpose: {
        objective: "Portable source-owned Work",
        successCriteria: ["Keep meaning and original proof"],
        inScope: ["Common semantics"],
        outOfScope: ["Native execution authority"],
      },
      spec: { id: "spec", revision: 1 },
      plan: { id: "plan", revision: 1 },
      governingDecisionIds: ["direction"],
      historicalFenceIds: ["history"],
    },
    {
      ...base,
      kind: "spec",
      id: "spec",
      revision: 1,
      outcome: "Git-owned records",
      criteria: ["Bounded source closure"],
    },
    {
      ...base,
      kind: "plan",
      id: "plan",
      revision: 1,
      specId: "spec",
      specRevision: 1,
      objective: "Read exact Git objects",
    },
    {
      ...input("answer", { text: "Use Git", question: "Which canonical source?" }),
      inputKind: "actual-user-answer",
      origin: { ownerId: "user", locator: "fixture:user-message:1", kind: "native-user-message" },
    },
    {
      ...base,
      kind: "decision",
      id: "direction",
      status: "accepted",
      answer: "Use Git",
      meaning: "Use Git as canonical Work storage; retain native owners.",
      answerSource: ref("answer", "#/content/text"),
      selectionContext: {
        text: "Which canonical source?",
        source: ref("answer", "#/content/question"),
      },
      supersedes: [],
    },
    input("fences", { completedEffectIds: ["already"], uncertainEffectIds: ["unknown"] }),
    {
      ...base,
      kind: "effect-fence",
      id: "history",
      completedEffectIds: ["already"],
      uncertainEffectIds: ["unknown"],
      source: ref("fences"),
    },
    task("a"),
    task("b"),
  ];
}
async function git(root: string, ...args: string[]): Promise<string> {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Fixture",
      GIT_AUTHOR_EMAIL: "fixture@example.invalid",
      GIT_COMMITTER_NAME: "Fixture",
      GIT_COMMITTER_EMAIL: "fixture@example.invalid",
    },
  });
  const [out, err, status] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (status !== 0) throw Error(`fixture git ${args[0]}: ${err}`);
  return out.trim();
}
async function put(root: string, row: GitWorkRecord): Promise<void> {
  const path = join(
    root,
    pathOf(row.kind, row.id, row.kind === "spec" || row.kind === "plan" ? row.revision : undefined),
  );
  await mkdir(join(path, ".."), { recursive: true });
  await writeFile(path, JSON.stringify(row));
}
async function commit(root: string): Promise<string> {
  await git(root, "add", ".");
  await git(root, "commit", "-qm", "Fixture revision");
  return git(root, "rev-parse", "HEAD");
}
async function fixture(rows = initialRecords()): Promise<{ root: string; oid: string }> {
  const root = await mkdtemp(join(tmpdir(), "common-git-work-"));
  temporaryRoots.push(root);
  await git(root, "init", "-q", "-b", "main");
  for (const row of rows) await put(root, row);
  return { root, oid: await commit(root) };
}
const open = (root: string, oid: string) =>
  openGitWork({
    repositoryRoot: root,
    repositoryId: "fixture/repo",
    commit: oid,
    projectId: scope.projectId,
    workId: scope.workId,
  });
const byTask = (rows: GitWorkRecord[], id: string) =>
  rows.find((row) => row.kind === "task" && row.id === id) as TaskRecord;

async function advancingContractsFixture(): Promise<{
  root: string;
  beforeOid: string;
  afterOid: string;
}> {
  const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "a"));
  rows.push(
    ...completed("a"),
    ...Array.from({ length: 9 }, (_, index) => task(`historical-${index}`)),
  );
  for (const row of rows) {
    if (row.kind === "work") {
      row.spec.revision = 5;
      row.plan.revision = 5;
    }
    if (row.kind === "spec" || row.kind === "plan") row.revision = 5;
    if (row.kind === "plan") row.specRevision = 5;
    if (row.kind === "task") {
      row.logical.specRevision = 5;
      row.logical.planRevision = 5;
      if (row.artifact?.kind === "task" && row.artifact.payload.details)
        row.artifact.payload.details.logical = { ...row.logical };
    }
  }
  const { root, oid: beforeOid } = await fixture(rows);
  const oldSpec = rows.find((row) => row.kind === "spec");
  const oldPlan = rows.find((row) => row.kind === "plan");
  const work = rows.find((row) => row.kind === "work");
  if (oldSpec?.kind !== "spec" || oldPlan?.kind !== "plan" || work?.kind !== "work")
    throw Error("fixture");
  await put(root, { ...oldSpec, revision: 6, outcome: "Revised source contract" });
  await put(root, {
    ...oldPlan,
    revision: 6,
    specRevision: 6,
    objective: "Implement revised source contract",
  });
  work.spec.revision = 6;
  work.plan.revision = 6;
  await put(root, work);
  const next = task("new-6");
  next.logical.specRevision = 6;
  next.logical.planRevision = 6;
  await put(root, next);
  return { root, beforeOid, afterOid: await commit(root) };
}

describe("semantic contract revisions", () => {
  test("same Work advances Spec/Plan 5 to 6 while eleven historical Tasks retain revision 5", async () => {
    const { root, beforeOid, afterOid } = await advancingContractsFixture();
    const before = await open(root, beforeOid),
      after = await open(root, afterOid);
    expect((await validateGitWorkRecords(after)).valid).toBe(true);
    expect((await validateGitWorkTransition(before, after)).valid).toBe(true);
    expect((await after.listRecords()).filter((row) => row.kind === "task")).toHaveLength(12);
    expect(await after.readRecord({ kind: "task", id: "a" })).toMatchObject({
      logical: { specRevision: 5, planRevision: 5 },
    });
    for (const id of ["a", "new-6"]) {
      const context = await readGitWorkContext(after, { taskId: id, latestRelevant: "current" });
      expect(context.status).toBe("source-qualified");
      expect(
        context.records
          .filter((row) => row.kind === "spec")
          .map((row) => row.revision)
          .sort(),
      ).toEqual(id === "a" ? [5, 6] : [6]);
    }
    expect(await after.readRecord({ kind: "spec", id: "spec", revision: 5 })).toMatchObject({
      revision: 5,
    });
    expect(await after.readRecord({ kind: "plan", id: "plan", revision: 6 })).toMatchObject({
      revision: 6,
      specRevision: 6,
    });
  });
  test.each(["spec", "plan"] as const)(
    "rejects deletion or rewriting of historical %s revision",
    async (kind) => {
      const { root, afterOid } = await advancingContractsFixture();
      const before = await open(root, afterOid);
      const old = await before.readRecord({ kind, id: kind, revision: 5 });
      if (old.kind === "spec") old.outcome = "Rewritten historical outcome";
      if (old.kind === "plan") old.objective = "Rewritten historical objective";
      await put(root, old);
      const mutated = await open(root, await commit(root));
      expect((await validateGitWorkRecords(mutated)).valid).toBe(true);
      expect((await validateGitWorkTransition(before, mutated)).findings).toContain(
        `IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED:${kind}:${kind}@5`,
      );
      await rm(join(root, pathOf(kind, kind, 5)));
      const deleted = await open(root, await commit(root));
      expect((await validateGitWorkTransition(before, deleted)).findings).toContain(
        `IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED:${kind}:${kind}@5`,
      );
    },
  );
  test("Spec/Plan reference and filename revisions must be explicit and match payloads", async () => {
    const { root, oid } = await fixture();
    const reader = await open(root, oid);
    await expect(reader.readRecord({ kind: "spec", id: "spec" })).rejects.toThrow(
      "CONTRACT_REVISION_REFERENCE_REQUIRED",
    );
    const spec = await reader.readRecord({ kind: "spec", id: "spec", revision: 1 });
    await writeFile(join(root, pathOf("spec", "spec", 2)), JSON.stringify(spec));
    const mismatch = await open(root, await commit(root));
    await expect(mismatch.readRecord({ kind: "spec", id: "spec", revision: 2 })).rejects.toThrow(
      "RECORD_IDENTITY_OR_SCOPE_MISMATCH",
    );
    await writeFile(join(root, "work/fixture/portable-work/specs/flat.json"), JSON.stringify(spec));
    const flat = await open(root, await commit(root));
    await expect(flat.listRecords()).rejects.toThrow("CONTRACT_REVISION_FILENAME_REQUIRED");
  });
  test("completed Task keeps its historical contract and presence while audit completion can advance", async () => {
    const { root, afterOid } = await advancingContractsFixture();
    const before = await open(root, afterOid);
    const a = await before.readRecord({ kind: "task", id: "a" });
    if (a.kind !== "task") throw Error("fixture");
    a.state.sharedAudit = "delivery-confirmed";
    a.state.formalCompletion = "complete";
    await put(root, a);
    expect(
      (await validateGitWorkTransition(before, await open(root, await commit(root)))).valid,
    ).toBe(true);
    a.logical.specRevision = 6;
    a.logical.planRevision = 6;
    if (a.artifact?.kind === "task" && a.artifact.payload.details)
      a.artifact.payload.details.logical = { ...a.logical };
    await put(root, a);
    const rebound = await open(root, await commit(root));
    expect((await validateGitWorkRecords(rebound)).valid).toBe(true);
    expect((await validateGitWorkTransition(before, rebound)).findings).toContain(
      "COMPLETED_TASK_CONTRACT_HISTORY_CHANGED:a",
    );
    await rm(join(root, pathOf("task", "a")));
    const deleted = await open(root, await commit(root));
    expect((await validateGitWorkRecords(deleted)).valid).toBe(true);
    expect((await validateGitWorkTransition(before, deleted)).findings).toContain(
      "COMPLETED_TASK_CONTRACT_HISTORY_CHANGED:a",
    );
  });
});

describe("immutable Git Work source", () => {
  test("portable clone and dirty checkout retain exact source meaning without native authority", async () => {
    const { root, oid } = await fixture();
    const reader = await open(root, oid);
    const local = await readGitWorkContext(reader, { taskId: "a", latestRelevant: "current" });
    expect(local.status).toBe("source-qualified");
    expect(local.nativePermissionGranted).toBe(false);
    const dirty = input("answer", { text: "Wrong working-copy text" });
    await put(root, dirty);
    expect((await reader.readSource(ref("answer", "#/content/text"))).value).toBe("Use Git");
    const cloneRoot = await mkdtemp(join(tmpdir(), "common-git-clone-"));
    temporaryRoots.push(cloneRoot);
    await git(cloneRoot, "clone", "-q", "--no-hardlinks", root, ".");
    expect(
      await readGitWorkContext(await open(cloneRoot, oid), {
        taskId: "a",
        latestRelevant: "current",
      }),
    ).toEqual(local);
  });
  test("captured selection and Work cannot be changed after opening", async () => {
    const { root, oid } = await fixture();
    const selection = {
      repositoryRoot: root,
      repositoryId: "fixture/repo",
      commit: oid,
      projectId: scope.projectId,
      workId: scope.workId,
      repositories: { foreign: root },
    };
    const reader = await openGitWork(selection);
    selection.commit = "f".repeat(40);
    selection.workId = "elsewhere";
    selection.repositories.foreign = "/missing";
    expect(reader.commit).toBe(oid);
    expect(reader.selection.workId).toBe(scope.workId);
    expect(reader.selection.repositories?.foreign).toBe(root);
    expect(() => {
      reader.work.purpose.objective = "Mutated";
    }).toThrow();
    expect((await reader.readRecord({ kind: "task", id: "a" })).id).toBe("a");
    const sourceReference = ref("answer", "#/content/text");
    const pendingSource = reader.readSource(sourceReference);
    sourceReference.path = pathOf("input", "fences");
    sourceReference.fragment = "#/content";
    expect((await pendingSource).value).toBe("Use Git");
    const recordReference = { kind: "task" as const, id: "a" };
    const pendingRecord = reader.readRecord(recordReference);
    recordReference.id = "b";
    expect((await pendingRecord).id).toBe("a");
    const contextRequest = { taskId: "a", latestRelevant: "current" as const };
    const pendingContext = readGitWorkContext(reader, contextRequest);
    contextRequest.taskId = "b";
    expect(
      (await pendingContext).records.some((row) => row.kind === "task" && row.id === "a"),
    ).toBe(true);
  });
  test("Git replace refs cannot substitute the exact selected commit or blob", async () => {
    const { root, oid } = await fixture();
    const oldBlob = await git(root, "rev-parse", `${oid}:${pathOf("input", "answer")}`);
    await put(root, {
      ...input("answer", { text: "Replacement" }),
      inputKind: "actual-user-answer",
    });
    const newer = await commit(root);
    const newBlob = await git(root, "rev-parse", `${newer}:${pathOf("input", "answer")}`);
    await git(root, "replace", oid, newer);
    await git(root, "replace", oldBlob, newBlob);
    const reader = await open(root, oid);
    expect((await reader.readSource(ref("answer", "#/content/text"))).value).toBe("Use Git");
  });
  test("rejects mutable refs, traversal, foreign mappings below repository root and nonregular blobs", async () => {
    const { root, oid } = await fixture();
    await expect(open(root, "HEAD")).rejects.toThrow("IMMUTABLE_COMMIT_REQUIRED");
    const reader = await open(root, oid);
    await expect(reader.readSource({ kind: "snapshot", path: "../input.json" })).rejects.toThrow(
      "UNSAFE_GIT_PATH",
    );
    await mkdir(join(root, "subdir"));
    await writeFile(join(root, "subdir/input.json"), '"wrong address"');
    await symlink(pathOf("input", "answer"), join(root, "link.json"));
    const next = await commit(root);
    const external = await openGitWork({
      ...reader.selection,
      commit: next,
      repositories: { foreign: join(root, "subdir") },
    });
    await expect(
      external.readSource({
        kind: "git",
        repositoryId: "foreign",
        commit: next,
        path: "input.json",
      }),
    ).rejects.toThrow("EXACT_REPOSITORY_ROOT_REQUIRED");
    await expect(external.readSource({ kind: "snapshot", path: "link.json" })).rejects.toThrow(
      "REGULAR_GIT_BLOB_REQUIRED",
    );
  });
  test("bounds selected source bytes and reports original receipt identity", async () => {
    const { root } = await fixture();
    await writeFile(join(root, "large.txt"), "x".repeat(9000));
    const oid = await commit(root);
    const reader = await openGitWork({
      repositoryRoot: root,
      repositoryId: "fixture/repo",
      commit: oid,
      projectId: scope.projectId,
      workId: scope.workId,
      maxBlobBytes: 4096,
    });
    await expect(reader.readSource({ kind: "snapshot", path: "large.txt" })).rejects.toThrow(
      "SELECTED_BLOB_LIMIT",
    );
    const source = await reader.readSource(ref("answer", "#/content/text"));
    expect(source.commit).toBe(oid);
    expect(source.sha256).toHaveLength(64);
    expect(source.blobOid).toHaveLength(40);
    expect(source.nativeAuthorshipVerified).toBe(false);
    await expect(reader.readSource({ ...ref("answer"), sha256: "0".repeat(64) })).rejects.toThrow(
      "SOURCE_CONTENT_MISMATCH",
    );
  });
});

describe("actual decisions, bounded context and historical fences", () => {
  test.each(["answer", "source-class", "selection"] as const)(
    "rejects mismatched decision provenance: %s",
    async (kind) => {
      const rows = initialRecords();
      const decision = rows.find((row) => row.kind === "decision");
      if (decision?.kind !== "decision") throw Error("fixture");
      if (kind === "answer") decision.answer = "Unanswered proposal";
      if (kind === "source-class") {
        const answer = rows.find(
          (row) => row.kind === "input" && row.id === "answer",
        ) as InputRecord;
        answer.inputKind = "source-owner-record";
      }
      if (kind === "selection") decision.selectionContext!.text = "Different question";
      const { root, oid } = await fixture(rows);
      expect((await validateGitWorkRecords(await open(root, oid))).valid).toBe(false);
    },
  );
  test("unreferenced invalid history is excluded but required source/currentness cannot silently disappear", async () => {
    const { root } = await fixture();
    const broken = join(root, pathOf("task", "unrelated"));
    await writeFile(broken, "{bad json");
    const oid = await commit(root);
    const reader = await open(root, oid);
    expect(
      (await readGitWorkContext(reader, { taskId: "a", latestRelevant: "current" })).status,
    ).toBe("source-qualified");
    expect((await validateGitWorkRecords(reader)).valid).toBe(false);
    expect(
      (await readGitWorkContext(reader, { taskId: "a", latestRelevant: "advanced" })).status,
    ).toBe("held");
    expect(
      (await readGitWorkContext(reader, { taskId: "a", latestRelevant: "current", maxRecords: 2 }))
        .holdReasons,
    ).toContain("REQUIRED_CONTEXT_RECORD_LIMIT");
    const bounded = await readGitWorkContext(reader, {
      taskId: "a",
      latestRelevant: "current",
      maxChars: 1024,
    });
    expect(bounded.status).toBe("held");
    expect(JSON.stringify(bounded).length).toBeLessThanOrEqual(1024);
  });
  test.each(["already", "unknown"])(
    "blocks replay of completed/uncertain effect %s",
    async (effectId) => {
      const rows = initialRecords();
      const a = byTask(rows, "a");
      a.executionClass = "native-effect";
      a.pendingEffectIds = [effectId];
      const { root, oid } = await fixture(rows);
      const result = await validateGitWorkRecords(await open(root, oid));
      expect(result.findings).toContain(`EFFECT_REPLAY_OR_UNCERTAIN:${effectId}`);
    },
  );
  test("transition preserves original inputs, accepted meaning and historical fences", async () => {
    const { root, oid } = await fixture();
    const before = await open(root, oid);
    const answer = JSON.parse(
      await readFile(join(root, pathOf("input", "answer")), "utf8"),
    ) as InputRecord;
    answer.content = { text: "Rewritten original", question: "Which canonical source?" };
    await put(root, answer);
    const after = await open(root, await commit(root));
    const checked = await validateGitWorkTransition(before, after);
    expect(checked.valid).toBe(false);
    expect(
      checked.findings.some((value) => value.includes("IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED")),
    ).toBe(true);
  });
});

describe("semantic verification, dependencies and Git concurrency", () => {
  test.each([
    "received",
    "received-without-artifact",
    "missing-receiver",
    "wrong-occurrence",
    "wrong-payload",
  ] as const)("native prerequisite source and receiver joins: %s", async (variant) => {
    const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "a"));
    rows.push(...completed("a"));
    const a = byTask(rows, "a"),
      b = byTask(rows, "b");
    if (variant === "received-without-artifact") {
      delete a.artifact;
      (
        rows.find((row) => row.kind === "input" && row.id === "verification-a") as InputRecord
      ).content = JSON.parse(JSON.stringify(verification("a", a)));
    }
    a.executionClass = "native-effect";
    a.runtimeId = "wsl";
    a.occurrence = { runtimeId: "wsl", attemptId: "attempt-1", occurrenceId: "occurrence-1" };
    a.executionEvidenceId = "native-a";
    b.executionClass = "native-effect";
    b.runtimeId = "vps";
    b.dependsOn = [
      {
        taskId: "a",
        milestone: "execution-and-verification",
        receiptEvidenceId: "native-a",
        ...(variant === "missing-receiver" ? {} : { receiverEvidenceId: "received-a" }),
      },
    ];
    // Reordered fields are semantically equal across host JSON writers.
    const reorderedLogical = Object.fromEntries(Object.entries(a.logical).reverse());
    const reorderedState = Object.fromEntries(Object.entries(a.state).reverse());
    rows.push(
      input("intent", { task: "a", phase: "intent" }),
      input("result", { task: "a", phase: "result" }),
      input("native-receipt", {
        schema: "common.git-prerequisite-receipt.v1",
        logical: reorderedLogical,
        state: reorderedState,
        occurrence: { ...a.occurrence },
        intent: ref("intent"),
        result: ref("result"),
        verification: ref("verification-a"),
      }),
      {
        ...base,
        kind: "evidence",
        id: "native-a",
        evidenceKind: "native-execution",
        source: ref("native-receipt"),
      },
      input("received", {
        schema: "common.git-receiver-receipt.v1",
        receiverRuntimeId: "vps",
        prerequisiteTaskId: "a",
        producer: {
          ...a.occurrence,
          occurrenceId: variant === "wrong-occurrence" ? "different" : "occurrence-1",
        },
        payload: ref(variant === "wrong-payload" ? "intent" : "native-receipt"),
      }),
      {
        ...base,
        kind: "evidence",
        id: "received-a",
        evidenceKind: "receiver",
        source: ref("received"),
      },
    );
    const { root, oid } = await fixture(rows);
    const reader = await open(root, oid);
    const result = await readGitWorkContext(reader, { taskId: "b", latestRelevant: "current" });
    const passed = variant === "received" || variant === "received-without-artifact";
    expect(result.status).toBe(passed ? "source-qualified" : "held");
    expect(result.nativePermissionGranted).toBe(false);
    if (passed) expect((await validateGitWorkRecords(reader)).valid).toBe(true);
  });
  test("source-only dependencies use exact verified source without invented native occurrences", async () => {
    const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "a"));
    rows.push(...completed("a"));
    byTask(rows, "b").dependsOn = [{ taskId: "a", milestone: "execution-and-verification" }];
    const { root, oid } = await fixture(rows);
    const reader = await open(root, oid);
    expect(await validateGitWorkRecords(reader)).toMatchObject({ valid: true });
    expect(
      (await readGitWorkContext(reader, { taskId: "b", latestRelevant: "current" })).status,
    ).toBe("source-qualified");
  });
  test.each(["same-work", "different-work"] as const)(
    "artifact-free verification retains Work scope: %s",
    async (variant) => {
      const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "a"));
      rows.push(...completed("a"));
      const a = byTask(rows, "a");
      delete a.artifact;
      byTask(rows, "b").dependsOn = [{ taskId: "a", milestone: "execution-and-verification" }];
      const otherWork = {
        ...a,
        workId: "different-work",
        logical: { ...a.logical, workId: "different-work" },
      };
      expect(taskVerificationTarget(a)).toMatch(/^common-harness:task:[a-f0-9]{64}$/);
      expect(taskVerificationTarget(a)).not.toBe(taskVerificationTarget(otherWork));
      (
        rows.find((row) => row.kind === "input" && row.id === "verification-a") as InputRecord
      ).content = JSON.parse(
        JSON.stringify(verification("a", variant === "same-work" ? a : otherWork)),
      );
      const { root, oid } = await fixture(rows);
      const reader = await open(root, oid);
      const checked = await validateGitWorkRecords(reader);
      expect(checked.valid).toBe(variant === "same-work");
      expect(
        (await readGitWorkContext(reader, { taskId: "b", latestRelevant: "current" })).status,
      ).toBe(variant === "same-work" ? "source-qualified" : "held");
      if (variant === "different-work")
        expect(
          checked.findings.some((value) =>
            value.includes("TASK_VERIFICATION_TARGET_OR_STATUS_MISMATCH"),
          ),
        ).toBe(true);
      expect(await reader.readRecord({ kind: "task", id: "a" })).toMatchObject({
        id: "a",
        logical: { taskId: "a", workId: scope.workId },
      });
    },
  );
  test.each(["occurrence", "effects", "execution-receipt"] as const)(
    "source-only Task cannot claim native %s",
    async (kind) => {
      const rows = initialRecords();
      const a = byTask(rows, "a");
      if (kind === "occurrence")
        a.occurrence = { runtimeId: "wsl", attemptId: "1", occurrenceId: "1" };
      if (kind === "effects") a.effectIds = ["fake"];
      if (kind === "execution-receipt") a.executionEvidenceId = "fake";
      const { root, oid } = await fixture(rows);
      expect((await validateGitWorkRecords(await open(root, oid))).valid).toBe(false);
    },
  );
  test.each(["failed", "wrong-target", "wrong-owner"] as const)(
    "passed Task cannot use verification with %s",
    async (kind) => {
      const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "a"));
      rows.push(...completed("a"));
      const proof = verification("a", byTask(rows, "a"));
      if (proof.kind !== "verification") throw Error("fixture");
      if (kind === "failed") proof.payload.criteria[0]!.status = "fail";
      if (kind === "wrong-target") proof.payload.targetRefs = ["fixture:task:other"];
      if (kind === "wrong-owner") proof.domainId = "other-domain";
      (
        rows.find((row) => row.kind === "input" && row.id === "verification-a") as InputRecord
      ).content = JSON.parse(JSON.stringify(proof));
      const { root, oid } = await fixture(rows);
      const result = await validateGitWorkRecords(await open(root, oid));
      expect(result.valid).toBe(false);
      expect(
        result.findings.some((value) =>
          value.includes("TASK_VERIFICATION_TARGET_OR_STATUS_MISMATCH"),
        ),
      ).toBe(true);
    },
  );
  test("Task relation dependsOn must match milestone-bearing dependency authority", async () => {
    const rows = initialRecords();
    rows.push({
      ...base,
      kind: "relation",
      id: "hidden",
      relation: "dependsOn",
      from: { kind: "task", id: "a" },
      to: { kind: "task", id: "b" },
    });
    const { root, oid } = await fixture(rows);
    expect((await validateGitWorkRecords(await open(root, oid))).findings).toContain(
      "TASK_RELATION_DEPENDENCY_CONTRADICTION:hidden",
    );
  });
  test("independent Task files merge cleanly in Git but semantic dependency cycle is rejected", async () => {
    const { root, oid } = await fixture();
    const a = task("a");
    a.dependsOn = [{ taskId: "b", milestone: "execution-and-verification" }];
    await put(root, a);
    const left = await commit(root);
    expect((await validateGitWorkRecords(await open(root, left))).valid).toBe(true);
    await git(root, "checkout", "-q", "-b", "right", oid);
    const b = task("b");
    b.dependsOn = [{ taskId: "a", milestone: "execution-and-verification" }];
    await put(root, b);
    const right = await commit(root);
    expect((await validateGitWorkRecords(await open(root, right))).valid).toBe(true);
    const tree = (await git(root, "merge-tree", "--write-tree", left, right)).split("\n")[0]!;
    const merged = await git(
      root,
      "commit-tree",
      tree,
      "-p",
      left,
      "-p",
      right,
      "-m",
      "Pure semantic merge fixture",
    );
    const result = await validateGitWorkRecords(await open(root, merged));
    expect(result.valid).toBe(false);
    expect(result.findings.some((value) => value.includes("SEMANTIC_DEPENDENCY_CYCLE"))).toBe(true);
    expect(await git(root, "rev-parse", "HEAD")).toBe(right);
  });
  test("purpose relation edges survive Task graph insertion order", async () => {
    const rows = initialRecords();
    rows.push(
      {
        ...base,
        kind: "relation",
        id: "ab",
        relation: "purpose",
        from: { kind: "task", id: "a" },
        to: { kind: "task", id: "b" },
      },
      {
        ...base,
        kind: "relation",
        id: "ba",
        relation: "purpose",
        from: { kind: "task", id: "b" },
        to: { kind: "task", id: "a" },
      },
    );
    const { root, oid } = await fixture(rows);
    expect(
      (await validateGitWorkRecords(await open(root, oid))).findings.some((value) =>
        value.includes("SEMANTIC_DEPENDENCY_CYCLE"),
      ),
    ).toBe(true);
  });
});

describe("source qualification does not grant startup or native authority", () => {
  test("new Work requires a same-start fresh main observation; continuation preserves selected identity", () => {
    const oid = "a".repeat(40);
    const request = {
      mode: "new" as const,
      selectedCommit: oid,
      repositoryId: "fixture/repo",
      remoteMain: {
        repositoryId: "fixture/repo",
        commit: oid,
        observedAtUtc: "2026-10-14T00:00:00Z",
        checkedForStartId: "start",
        synchronized: true as const,
        receipt: "owner-observation",
      },
      startId: "start",
      nowUtc: "2026-10-14T00:00:01Z",
      maxObservationAgeMs: 1000,
    };
    expect(qualifyGitWorkStart(request).valid).toBe(true);
    expect(qualifyGitWorkStart({ ...request, nowUtc: "2026-10-14T00:00:02Z" }).valid).toBe(false);
    expect(qualifyGitWorkStart({ ...request, startId: "later-start" }).valid).toBe(false);
    expect(
      qualifyGitWorkStart({
        mode: "continue",
        repositoryId: "fixture/repo",
        selectedCommit: oid,
        previous: scope,
        current: scope,
        branch: "work/continuing",
      }),
    ).toMatchObject({ valid: true, authority: "none", nativePermissionGranted: false });
    expect(
      qualifyGitWorkStart({
        mode: "continue",
        repositoryId: "fixture/repo",
        selectedCommit: oid,
        previous: scope,
        current: { ...scope, workId: "different" },
        branch: "work/continuing",
      }).valid,
    ).toBe(false);
  });
  test("real canonical Work is useful at the default budget and remains source-only", async () => {
    const canonical = join(
      import.meta.dir,
      "../../../..",
      "work/codex-harness-engineering/ontology-driven-harness-operation",
    );
    const root = await mkdtemp(join(tmpdir(), "common-real-context-"));
    temporaryRoots.push(root);
    await git(root, "init", "-q", "-b", "main");
    const target = join(root, "work/codex-harness-engineering/ontology-driven-harness-operation");
    await mkdir(join(target, ".."), { recursive: true });
    await cp(canonical, target, { recursive: true });
    const oid = await commit(root);
    const reader = await openGitWork({
      repositoryRoot: root,
      repositoryId: "park-kyungchan/agent-platform",
      commit: oid,
      projectId: "codex-harness-engineering",
      workId: "ontology-driven-harness-operation",
    });
    for (const taskId of ["GHA-22", "GHA-27", "WSL2-LEAD-HANDOFF"]) {
      const result = await readGitWorkContext(reader, { taskId, latestRelevant: "current" });
      expect(result.status).toBe("source-qualified");
      expect(result.records.some((row) => row.kind === "task" && row.id === taskId)).toBe(true);
      expect(JSON.stringify(result).length).toBeLessThanOrEqual(24000);
      expect(result.semanticAcceptanceEstablished).toBe(false);
    }
  });
});
