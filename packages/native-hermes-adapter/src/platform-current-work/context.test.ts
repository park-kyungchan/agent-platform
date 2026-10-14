import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import * as gitWork from "@palantirkc/work-continuity/git-work";
import type {
  GitWorkRecord,
  LogicalIdentity,
  TaskRecord,
} from "@palantirkc/work-continuity/git-work";
import type { RuntimeIndex } from "@palantirkc/work-continuity/runtime-index";
import {
  readHermesCurrentWorkContext,
  type HermesCurrentWorkEvent,
  type HermesCurrentWorkSelection,
  type HermesCurrentWorkOptions,
} from "./context.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
async function git(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    env: {
      ...gitWork.gitReadEnvironment(),
      GIT_AUTHOR_NAME: "Synthetic",
      GIT_AUTHOR_EMAIL: "fixture@example.invalid",
      GIT_COMMITTER_NAME: "Synthetic",
      GIT_COMMITTER_EMAIL: "fixture@example.invalid",
    },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error(stderr);
  return stdout.trim();
}
const identity: LogicalIdentity = {
  domainId: "native-harness",
  projectId: "synthetic-project",
  workId: "synthetic-current-work",
  issueId: "synthetic-issue",
  specId: "synthetic-spec",
  specRevision: 6,
  planId: "synthetic-plan",
  planRevision: 6,
  taskId: "synthetic-task",
};
const event: HermesCurrentWorkEvent = {
  runtime: "native-hermes",
  profileId: "synthetic-profile",
  sessionId: "actual-synthetic-session",
  agentId: null,
  nativeProjectId: "actual-synthetic-project",
  nativeTaskId: "actual-synthetic-task",
  nativeRunId: "actual-synthetic-run",
};
const state: TaskRecord["state"] = {
  execution: "planned",
  verification: "pending",
  localAudit: "intent-durable",
  sharedAudit: "pending",
  formalCompletion: "pending",
};
async function fixture(
  options: {
    decisions?: number;
    dependencies?: number;
    badAnswer?: boolean;
    closed?: boolean;
    superseded?: boolean;
  } = {},
) {
  const root = await realpath(await mkdtemp(join(tmpdir(), "hermes-current-git-work-")));
  roots.push(root);
  const path = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
    gitWork.gitRecordPath(identity.projectId, identity.workId, {
      kind,
      id,
      ...(revision ? { revision } : {}),
    });
  const base = {
    schema: "common.git-work.v1" as const,
    domainId: identity.domainId,
    projectId: identity.projectId,
    workId: identity.workId,
  };
  const decisionIds = Array.from({ length: options.decisions ?? 1 }, (_, i) => `decision-${i}`);
  const dependencyIds = Array.from(
    { length: options.dependencies ?? 0 },
    (_, i) => `dependency-${i}`,
  );
  const records: GitWorkRecord[] = [
    {
      ...base,
      kind: "work",
      id: identity.workId,
      repositoryId: "synthetic/common",
      issueId: identity.issueId,
      purpose: {
        objective: "Preserve the current integrated purpose",
        successCriteria: ["Exact logical/native selection"],
        inScope: ["Source context"],
        outOfScope: ["Native authority"],
      },
      spec: { id: identity.specId, revision: 6 },
      plan: { id: identity.planId, revision: 6 },
      governingDecisionIds: decisionIds,
      historicalFenceIds: [],
    },
    {
      ...base,
      kind: "spec",
      id: identity.specId,
      revision: 6,
      outcome: "Current integrated source",
      criteria: ["Retain originals"],
    },
    {
      ...base,
      kind: "plan",
      id: identity.planId,
      revision: 6,
      specId: identity.specId,
      specRevision: 6,
      objective: "Read current source",
    },
    {
      ...base,
      kind: "task",
      id: identity.taskId,
      logical: identity,
      title: "Current integrated Task",
      state,
      executionClass: "source-only",
      decisionIds,
      dependsOn: dependencyIds.map((taskId) => ({
        taskId,
        milestone: "execution-and-verification" as const,
      })),
      effectIds: [],
      verificationEvidenceIds: [],
    },
    ...dependencyIds.map((id): TaskRecord => ({
      ...base,
      kind: "task",
      id,
      logical: { ...identity, taskId: id },
      title: `Direct dependency ${id}`,
      state,
      executionClass: "source-only",
      decisionIds: [],
      dependsOn: [],
      effectIds: [],
      verificationEvidenceIds: [],
    })),
  ];
  for (const id of decisionIds) {
    const original = { kind: "snapshot" as const, path: path("input", `${id}-original`) };
    records.push({
      ...base,
      kind: "input",
      id: `${id}-original`,
      inputKind: "actual-user-answer",
      origin: {
        ownerId: "synthetic-user",
        locator: `synthetic:private-original-${id}`,
        kind: "native-user-message",
      },
      content: {
        answer: `PRIVATE-ANSWER-SENTINEL-${id}`,
        question: `PRIVATE-QUESTION-SENTINEL-${id}`,
      },
    });
    records.push({
      ...base,
      kind: "decision",
      id,
      status: options.closed ? "superseded" : "accepted",
      answer: options.badAnswer ? "BROKEN-ORIGINAL-BINDING" : `PRIVATE-ANSWER-SENTINEL-${id}`,
      meaning: "Actual decision rationale retained in the immutable record",
      answerSource: { ...original, fragment: "#/content/answer" },
      selectionContext: {
        text: `PRIVATE-QUESTION-SENTINEL-${id}`,
        source: { ...original, fragment: "#/content/question" },
      },
      supersedes: options.superseded ? [`${id}-previous`] : [],
    });
    if (options.superseded)
      records.push({
        ...base,
        kind: "decision",
        id: `${id}-previous`,
        status: "superseded",
        answer: `PRIVATE-ANSWER-SENTINEL-${id}`,
        meaning: "Previous declared decision revision",
        answerSource: { ...original, fragment: "#/content/answer" },
        selectionContext: {
          text: `PRIVATE-QUESTION-SENTINEL-${id}`,
          source: { ...original, fragment: "#/content/question" },
        },
        supersedes: [],
      });
  }
  for (const record of records) {
    const target = join(
      root,
      path(
        record.kind,
        record.id,
        record.kind === "spec" || record.kind === "plan" ? record.revision : undefined,
      ),
    );
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, JSON.stringify(record));
  }
  await git(root, "init", "-q", "-b", "work");
  await git(root, "add", ".");
  await git(root, "commit", "-qm", "Synthetic source");
  const commit = await git(root, "rev-parse", "HEAD");
  const selected: HermesCurrentWorkSelection = {
    schema: "hermes.current-git-work-selection.v1",
    native: structuredClone(event),
    logical: structuredClone(identity),
    latestRelevant: "current",
    snapshot: {
      repositoryRoot: root,
      repositoryId: "synthetic/common",
      projectId: identity.projectId,
      workId: identity.workId,
      commit: "refs/heads/work",
    },
  };
  const index: RuntimeIndex = {
    schema: "native-harness.runtime-index.v1",
    entries: [
      {
        ...event,
        logical: identity,
        observation: {
          class: "declared",
          observedAtUtc: null,
          evidenceRefs: [],
          currentness: "current",
        },
        sourceOnly: true,
      },
    ],
  };
  let reads = 0,
    checks = 0;
  const provider: HermesCurrentWorkOptions = {
    verifySourceClosure: async () => {
      checks++;
    },
    gitWork: {
      ...gitWork,
      resolveGitWorkSelection: async (value) => {
        reads++;
        return gitWork.resolveGitWorkSelection(value);
      },
    },
  };
  const read = (evt: unknown = event, choice: unknown = selected, idx: unknown = index) =>
    readHermesCurrentWorkContext(evt, choice, idx, provider);
  return { root, commit, selected, index, provider, read, counters: () => ({ reads, checks }) };
}

test("exact native tuple and revision-six Work retain bounded immutable originals without raw input", async () => {
  const f = await fixture();
  const value = await f.read();
  expect(value.status).toBe("source-qualified");
  expect(f.counters()).toEqual({ reads: 2, checks: 2 });
  expect(value.context?.snapshot.commit).toBe(f.commit);
  expect(value.context?.logical).toEqual(identity);
  expect(value.context?.purpose.objective).toBe("Preserve the current integrated purpose");
  expect(value.context?.currentTask.title).toBe("Current integrated Task");
  expect(value.context?.upstream.spec).toEqual({
    id: identity.specId,
    revision: 6,
    source: "specs/synthetic-spec@6.json",
  });
  expect(value.context?.upstream.plan.revision).toBe(6);
  expect(value.context?.decisions[0]?.answer?.fragment).toBe("#/content/answer");
  expect(value.context?.decisions[0]?.question?.fragment).toBe("#/content/question");
  expect(value.context?.decisions[0]?.rationale.fragment).toBe("#/meaning");
  expect(value.context?.decisions[0]?.revisionIdentity).toBe(
    "immutable-record-id-and-supersession",
  );
  expect(value.context?.decisions[0]?.answer?.blobOid).toMatch(/^[a-f0-9]{40}$/u);
  expect(value.authority).toBe("none");
  expect(value.nativePermissionGranted).toBe(false);
  expect(value.nativeAuthorshipVerified).toBe(false);
  expect(value.semanticAcceptanceEstablished).toBe(false);
  expect(JSON.stringify(value)).not.toContain("PRIVATE-ANSWER-SENTINEL");
  expect(JSON.stringify(value)).not.toContain("PRIVATE-QUESTION-SENTINEL");
  expect(JSON.stringify(value)).not.toContain("private-original");
  expect(JSON.stringify(value).length).toBeLessThanOrEqual(6000);
});

test("missing, unknown and foreign selections do not read Git or source closures", async () => {
  const f = await fixture();
  for (const choice of [
    null,
    undefined,
    {},
    { ...f.selected, schema: "unknown" },
    { ...f.selected, extra: true },
    { ...f.selected, native: { ...event, agentId: undefined } },
    { ...f.selected, snapshot: { ...f.selected.snapshot, projectId: "foreign" } },
  ]) {
    expect((await readHermesCurrentWorkContext(event, choice, f.index, f.provider)).status).toBe(
      "held",
    );
  }
  for (const key of [
    "profileId",
    "sessionId",
    "agentId",
    "nativeProjectId",
    "nativeTaskId",
    "nativeRunId",
  ] as const) {
    const foreign = { ...event, [key]: "different-actual-reference" };
    expect((await f.read(foreign)).holdReasons).toEqual(["FOREIGN_NATIVE_WORK_TUPLE"]);
  }
  expect((await f.read({ ...event, runtime: "native-codex" })).status).toBe("held");
  expect((await f.read({ ...event, sessionId: null })).status).toBe("held");
  expect(f.counters()).toEqual({ reads: 0, checks: 0 });
});

test("RuntimeIndex missing, foreign, ambiguous and unknown references hold before Git reads", async () => {
  const f = await fixture();
  for (const index of [
    null,
    { schema: "native-harness.runtime-index.v1", entries: [] },
    { ...f.index, entries: [{ ...f.index.entries[0]!, nativeTaskId: "foreign-task" }] },
    { ...f.index, entries: [f.index.entries[0]!, f.index.entries[0]!] },
    {
      ...f.index,
      entries: [
        {
          ...f.index.entries[0]!,
          observation: { ...f.index.entries[0]!.observation, currentness: "unknown" },
        },
      ],
    },
  ]) {
    expect((await f.read(event, f.selected, index)).status).toBe("held");
  }
  expect(f.counters()).toEqual({ reads: 0, checks: 0 });
});

test("caller unknown currentness retains explicit hold rather than claiming remote freshness", async () => {
  const f = await fixture();
  f.selected.latestRelevant = "unknown";
  const value = await f.read();
  expect(value.status).toBe("held");
  expect(value.holdReasons).toContain("REQUIRED_LATEST_CONTEXT_UNKNOWN");
  expect(value.context?.logical.specRevision).toBe(6);
  expect(value.currentness).toBe("caller-declared-not-remote-verified");
});

test("current Work and selected Task require the exact logical Spec/Plan join", async () => {
  const f = await fixture();
  f.selected.logical.specRevision = 5;
  f.index.entries[0]!.logical = { ...f.selected.logical };
  const value = await f.read();
  expect(value.holdReasons).toEqual(["CURRENT_WORK_LOGICAL_CONTRACT_MISMATCH"]);
  expect(f.counters()).toEqual({ reads: 1, checks: 2 });
});

test("closed governing decision and broken original binding remain held", async () => {
  for (const option of [{ closed: true }, { badAnswer: true }]) {
    const f = await fixture(option);
    const value = await f.read();
    expect(value.status).toBe("held");
    expect(value.holdReasons).toContain(
      option.closed ? "GOVERNING_DECISION_NOT_CURRENT" : "ACTUAL_ANSWER_FRAGMENT_MISMATCH",
    );
    expect(JSON.stringify(value)).not.toContain("PRIVATE-ANSWER-SENTINEL");
    expect(JSON.stringify(value)).not.toContain("BROKEN-ORIGINAL-BINDING");
  }
});

test("accepted decision revisions preserve their actual superseded original chain", async () => {
  const f = await fixture({ superseded: true });
  const value = await f.read();
  expect(value.status).toBe("source-qualified");
  expect(value.context?.decisions.find((row) => row.id === "decision-0")?.supersedes).toEqual([
    "decision-0-previous",
  ]);
  expect(value.context?.decisions.find((row) => row.id === "decision-0-previous")?.status).toBe(
    "superseded",
  );
  expect(
    value.context?.decisions.find((row) => row.id === "decision-0-previous")?.answer?.fragment,
  ).toBe("#/content/answer");
  expect(JSON.stringify(value)).not.toContain("PRIVATE-ANSWER-SENTINEL");
});

test("moving local ref and source-closure drift produce truthful holds with safe errors", async () => {
  const f = await fixture();
  const original = f.provider.gitWork!.resolveGitWorkSelection;
  let count = 0;
  f.provider.gitWork!.resolveGitWorkSelection = async (value) => {
    if (++count === 2) {
      await writeFile(join(f.root, "synthetic-extra.json"), "{}");
      await git(f.root, "add", ".");
      await git(f.root, "commit", "-qm", "Synthetic ref advance");
    }
    return original(value);
  };
  expect((await f.read()).holdReasons).toContain("SELECTED_LOCAL_REF_ADVANCED_DURING_READ");
  let checks = 0;
  f.provider.verifySourceClosure = async () => {
    if (++checks === 2) throw Error("PRIVATE-CLOSURE-ERROR-SENTINEL");
  };
  const drift = await f.read();
  expect(drift.holdReasons).toEqual(["SELECTED_WORK_SOURCE_UNAVAILABLE_OR_DRIFTED"]);
  expect(JSON.stringify(drift)).not.toContain("PRIVATE-CLOSURE-ERROR-SENTINEL");
});

test("startup projection omits originals explicitly and never exceeds its requested bound", async () => {
  const f = await fixture({ decisions: 10 });
  const value = await f.read();
  expect(value.status).toBe("held");
  expect(value.context?.omitted.decisions).toBeGreaterThan(0);
  expect(value.holdReasons).toContain("UNEXPANDED_DECISION_ORIGINALS");
  expect(value.context?.decisions.length).toBeGreaterThan(0);
  expect(JSON.stringify(value).length).toBeLessThanOrEqual(6000);
  f.provider.maxChars = 1200;
  const tiny = await f.read();
  expect(tiny.status).toBe("held");
  expect(JSON.stringify(tiny).length).toBeLessThanOrEqual(1200);
});

test("direct dependency projection has explicit omission counts while prerequisites remain held", async () => {
  const f = await fixture({ dependencies: 16 });
  f.provider.maxChars = 3000;
  const value = await f.read();
  expect(value.status).toBe("held");
  expect(value.context?.omitted.directDependencies).toBeGreaterThan(0);
  expect(value.holdReasons).toContain("UNEXPANDED_DIRECT_DEPENDENCIES");
  expect(value.context?.upstream.spec.revision).toBe(6);
  expect(JSON.stringify(value).length).toBeLessThanOrEqual(3000);
});

test("missing closure verifier, bad budgets and parser errors expose no original payload", async () => {
  const f = await fixture();
  for (const maxChars of [0, 1000, 6001, NaN]) {
    f.provider.maxChars = maxChars;
    expect((await f.read()).holdReasons).toEqual(["INVALID_STARTUP_CONTEXT_BUDGET"]);
  }
  delete f.provider.maxChars;
  expect(
    (await readHermesCurrentWorkContext(event, f.selected, f.index, {} as HermesCurrentWorkOptions))
      .holdReasons,
  ).toEqual(["SOURCE_CLOSURE_VERIFIER_REQUIRED"]);
  f.provider.gitWork!.readGitWorkContext = async () => {
    throw Error("PRIVATE-PARSER-PAYLOAD-SENTINEL");
  };
  const value = await f.read();
  expect(value.holdReasons).toEqual(["SELECTED_WORK_SOURCE_UNAVAILABLE_OR_DRIFTED"]);
  expect(JSON.stringify(value)).not.toContain("PRIVATE-PARSER-PAYLOAD-SENTINEL");
});
