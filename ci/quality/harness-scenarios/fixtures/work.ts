import { mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import * as work from "@palantirkc/work-continuity/git-work";
import type {
  GitWorkRecord,
  InputRecord,
  TaskRecord,
  WorkRecord,
} from "@palantirkc/work-continuity/git-work";

// Fictional identities are test data; none describe a native occurrence.
export const scope = { domainId: "native-harness", projectId: "oracle", workId: "recovery" };
export const base = { schema: "common.git-work.v1" as const, ...scope };
export const objective =
  "Recover the original purpose while preserving consumed effects and useful source work";
export const nextAction =
  "Read the durable delegated result; inspect the independent source patch; keep the uncertain publication held";
export const clause =
  "Never replay a completed or uncertain publication; preserve the original answer and question";
export const pathOf = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
  work.gitRecordPath(scope.projectId, scope.workId, {
    kind,
    id,
    ...(revision ? { revision } : {}),
  });
export const reference = (id: string, fragment = "#/content") => ({
  kind: "snapshot" as const,
  path: pathOf("input", id),
  fragment,
});
export const logical = (taskId: string) => ({
  ...scope,
  issueId: "issue",
  specId: "spec",
  specRevision: 1,
  planId: "plan",
  planRevision: 1,
  taskId,
});
export const task = (id: string): TaskRecord => ({
  ...base,
  kind: "task",
  id,
  title: `Independent ${id}`,
  logical: logical(id),
  nextAction,
  state: {
    execution: "planned",
    verification: "pending",
    localAudit: "intent-durable",
    sharedAudit: "pending",
    formalCompletion: "pending",
  },
  executionClass: "source-only",
  dependsOn: [],
  decisionIds: ["decision-0", "decision-1", "decision-2", "decision-3"],
  effectIds: [],
  verificationEvidenceIds: [],
});
export const input = (id: string, content: InputRecord["content"]): InputRecord => ({
  ...base,
  kind: "input",
  id,
  inputKind: "source-owner-record",
  origin: { ownerId: "fixture", locator: `fictional:${id}`, kind: "source-owner-capture" },
  content,
});
export function records(): GitWorkRecord[] {
  const purpose: WorkRecord["purpose"] = {
    objective,
    successCriteria: ["Recover exact meaning and next action"],
    inScope: ["Independent source review"],
    outOfScope: ["Native execution or activation"],
  };
  const inventory = {
    completedEffectIds: ["published", ...Array.from({ length: 67 }, (_, i) => `completed-${i}`)],
    uncertainEffectIds: ["uncertain", "uncertain-2", "uncertain-3"],
  };
  const rows: GitWorkRecord[] = [
    {
      ...base,
      kind: "work",
      id: scope.workId,
      repositoryId: "fictional/oracles",
      issueId: "issue",
      purpose,
      spec: { id: "spec", revision: 2 },
      plan: { id: "plan", revision: 2 },
      governingDecisionIds: ["decision-0", "decision-1", "decision-2", "decision-3"],
      historicalFenceIds: ["history"],
      continuity: {
        schema: "common.git-work-continuity.v1",
        purposeSources: [reference("purpose")],
        effectInventorySources: [reference("inventory")],
      },
    },
    input("purpose", purpose),
    input("inventory", inventory),
    { ...base, kind: "effect-fence", id: "history", ...inventory, source: reference("inventory") },
    ...[1, 2].flatMap((revision): GitWorkRecord[] => [
      {
        ...base,
        kind: "spec",
        id: "spec",
        revision,
        outcome: `Revision ${revision} source outcome`,
        criteria: [clause],
      },
      {
        ...base,
        kind: "plan",
        id: "plan",
        revision,
        specId: "spec",
        specRevision: revision,
        objective: `Revision ${revision} plan`,
        clauses: [clause],
      },
    ]),
    task("review"),
    task("dependency"),
    input("delegated-result", {
      summary: "Independent review identified a source-boundary issue",
      oldPane: null,
      nativeOccurrence: null,
    }),
  ];
  const prerequisite = rows.find((r) => r.kind === "task" && r.id === "dependency") as TaskRecord;
  prerequisite.state = {
    execution: "complete",
    verification: "passed",
    localAudit: "result-durable",
    sharedAudit: "pending",
    formalCompletion: "audit-synchronization-pending",
  };
  prerequisite.verificationEvidenceIds = ["dependency-proof"];
  rows.push(
    input("dependency-verification", {
      schemaVersion: "1.0.0",
      kind: "verification",
      id: "oracle:verification:dependency",
      projectId: scope.projectId,
      domainId: scope.domainId,
      createdAt: "2026-10-18T00:00:00Z",
      payload: {
        targetRefs: [work.taskVerificationTarget(prerequisite)],
        criteria: [
          {
            id: "source-proof",
            status: "pass",
            message: "Fictional durable source prerequisite",
            evidenceRefs: ["oracle:evidence:source"],
          },
        ],
        inputHashes: [{ path: "fixture.json", sha256: "0".repeat(64) }],
        runtime: { name: "fixture", version: "1" },
      },
    }),
    {
      ...base,
      kind: "evidence",
      id: "dependency-proof",
      evidenceKind: "verification",
      source: reference("dependency-verification"),
    },
  );
  const review = rows.find((r) => r.kind === "task" && r.id === "review") as TaskRecord;
  review.dependsOn = [{ taskId: "dependency", milestone: "execution-and-verification" }];
  for (let i = 0; i < 5; i++) {
    rows.push({
      ...input(`original-${i}`, { answer: `Exact answer ${i}`, question: `Exact question ${i}` }),
      inputKind: "actual-user-answer",
    });
    rows.push({
      ...base,
      kind: "decision",
      id: `decision-${i}`,
      status: i === 4 ? "superseded" : "accepted",
      answer: `Exact answer ${i}`,
      meaning: `Retain distinct original reason ${i}`,
      answerSource: reference(`original-${i}`, "#/content/answer"),
      selectionContext: {
        text: `Exact question ${i}`,
        source: reference(`original-${i}`, "#/content/question"),
      },
      supersedes: i === 0 ? ["decision-4"] : [],
    });
  }
  return rows;
}
export async function git(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...work.gitReadEnvironment(),
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Codex",
      GIT_AUTHOR_EMAIL: "codex@local.invalid",
      GIT_COMMITTER_NAME: "Codex",
      GIT_COMMITTER_EMAIL: "codex@local.invalid",
    },
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error(`Fixture Git ${args[0]}: ${stderr}`);
  return stdout.trim();
}
export async function put(root: string, record: GitWorkRecord) {
  const target = join(
    root,
    pathOf(
      record.kind,
      record.id,
      record.kind === "spec" || record.kind === "plan" ? record.revision : undefined,
    ),
  );
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, JSON.stringify(record) + "\n");
}
export async function commit(root: string, message = "Fictional independent oracle input") {
  await git(root, "add", ".");
  await git(root, "commit", "-qm", message);
  return git(root, "rev-parse", "HEAD");
}
export function selection(root: string, commit: string) {
  return {
    repositoryRoot: root,
    repositoryId: "fictional/oracles",
    commit,
    projectId: scope.projectId,
    workId: scope.workId,
  };
}
export async function fixture(rows = records()) {
  const root = await realpath(await mkdtemp(join(tmpdir(), "pt10-quality-oracle-")));
  await git(root, "init", "-q", "-b", "work");
  for (const row of rows) await put(root, row);
  const oid = await commit(root);
  return {
    root,
    oid,
    rows,
    open: (oidOverride = oid) => work.openGitWork(selection(root, oidOverride)),
    cleanup: () => rm(root, { recursive: true, force: true }),
  };
}
