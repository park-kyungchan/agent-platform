// Synthetic test data only; no actual Work, native runtime or owner policy is read.
import { mkdir, mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Artifact } from "@palantirkc/ontology";
import {
  gitRecordPath,
  openGitWork,
  taskVerificationTarget,
} from "@palantirkc/work-continuity/git-work";
import type {
  GitWorkRecord,
  InputRecord,
  LogicalIdentity,
  TaskOperationalState,
  TaskRecord,
  WorkRecord,
  SourceReference,
} from "@palantirkc/work-continuity/git-work";
import { prepareGitWorkMemoryArtifacts } from "@palantirkc/work-continuity/memory/recall-request";
import { episodeFingerprint, sourceKey } from "@palantirkc/work-continuity/memory/qualification";
import { sourceDigest } from "../provider.ts";
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

export async function syntheticMemoryOwner() {
  const rows = initialRecords().filter((row) => !(row.kind === "task" && row.id === "b"));
  rows.push(...completed("b"));
  const work = rows.find((row) => row.kind === "work") as WorkRecord;
  work.continuity = {
    schema: "common.git-work-continuity.v1",
    purposeSources: [ref("purpose")],
    effectInventorySources: [ref("fences")],
  };
  rows.push(input("purpose", JSON.parse(JSON.stringify(work.purpose))));
  byTask(rows, "a").dependsOn = [{ taskId: "b", milestone: "execution-and-verification" }];
  const f = await fixture(rows);
  const original = await open(f.root, f.oid);
  const descriptor = {
    topicId: "synthetic-topic",
    generationId: "synthetic-generation",
    taskId: "a",
    receiptTaskIds: ["b"],
    claims: [{ key: "storage", record: { kind: "decision", id: "direction" } }],
    inputIds: { request: "memory-request", dataset: "memory-dataset", policy: "memory-policy" },
    query: "Git",
    claimKey: "storage",
    maxChars: 16000,
    policy: {
      schema: "codex.local-memory-policy.v1",
      projectId: scope.projectId,
      workId: scope.workId,
      activeGenerationId: "synthetic-generation",
      policy: {
        revision: "synthetic-policy",
        excludedSourceHashes: [],
        excludedFragmentIds: [],
        aliases: [],
        commonAdmissions: [],
      },
    },
  };
  const candidate = await prepareGitWorkMemoryArtifacts(original, descriptor);
  const review = input(
    "memory-review",
    JSON.parse(
      JSON.stringify({
        schema: "common.git-work-memory-owner-review.v1",
        ownerId: "fixture",
        bindingSha256: candidate.dataset.reviewBindingSha256,
        status: "reviewed",
        receipts: candidate.dataset.memories.map((memory) => ({
          memoryId: memory.id,
          episodeId: memory.episodeId,
          generationId: memory.generationId,
          policyRevision: memory.policyRevision,
          contentHash: memory.contentHash,
          sourceKeys: memory.sourceKeys,
          provenance: "owner-verified",
          claim: memory.claim,
          scope: memory.scope,
          episodeFingerprint: episodeFingerprint(candidate.dataset.episodes[0]!),
        })),
        authoritativeReceipts: candidate.dataset.authoritativeClaims.map((claim) => ({
          sourceKey: sourceKey(claim.source),
          key: claim.key,
          value: claim.value,
          recordClass: claim.recordClass,
          status: claim.status,
          provenance: "owner-verified",
        })),
      }),
    ),
  );
  await put(f.root, review);
  const reviewCommit = await commit(f.root);
  const reviewPath = pathOf("input", review.id);
  const reviewRef: SourceReference = {
    kind: "git",
    repositoryId: "fixture/repo",
    commit: reviewCommit,
    path: reviewPath,
    sha256: sourceDigest(await readFile(join(f.root, reviewPath))),
    fragment: "#/content",
  };
  // Historical original closure stays fixed. The declared test review is a separate Git object.
  const compiled = await prepareGitWorkMemoryArtifacts(original, descriptor, reviewRef);
  const dataset = input("memory-dataset", JSON.parse(JSON.stringify(compiled.dataset)));
  const policy = input("memory-policy", JSON.parse(JSON.stringify(compiled.policy)));
  await put(f.root, dataset);
  await put(f.root, policy);
  const pin = async (id: string) => ({
    ...ref(id),
    sha256: sourceDigest(await readFile(join(f.root, pathOf("input", id)))),
  });
  const request = input("memory-request", {
    ...compiled.request,
    dataset: await pin(dataset.id),
    policy: await pin(policy.id),
  });
  await put(f.root, request);
  const admitted = await commit(f.root);
  const payload = {
    snapshot: { ...original.selection, commit: admitted },
    requestRef: await pin(request.id),
    ownerSelection: {
      owningRef: "refs/heads/main",
      admittedCommit: admitted,
      policyRevision: "synthetic-policy",
      activeGenerationId: "synthetic-generation",
      state: "current",
    },
  };
  return {
    payload,
    root: f.root,
    compiled,
    advance: () => git(f.root, "commit", "--allow-empty", "-qm", "Synthetic owner advanced"),
    cleanup: () => rm(f.root, { recursive: true, force: true }),
  };
}
