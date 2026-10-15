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
import { readHermesNativeWorkHook } from "./hook-context.ts";

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
    taskDecisions?: number;
    questionlessDecisions?: number[];
    missingQuestionDecision?: number;
    badAnswerDecision?: number;
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
      decisionIds: decisionIds.slice(0, options.taskDecisions ?? decisionIds.length),
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
  for (const [index, id] of decisionIds.entries()) {
    const questionless = options.questionlessDecisions?.includes(index) ?? false;
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
        ...(!questionless && options.missingQuestionDecision !== index
          ? { question: `PRIVATE-QUESTION-SENTINEL-${id}` }
          : {}),
      },
    });
    records.push({
      ...base,
      kind: "decision",
      id,
      status: options.closed ? "superseded" : "accepted",
      answer:
        options.badAnswer || options.badAnswerDecision === index
          ? "BROKEN-ORIGINAL-BINDING"
          : `PRIVATE-ANSWER-SENTINEL-${id}`,
      meaning: "Actual decision rationale retained in the immutable record",
      answerSource: { ...original, fragment: "#/content/answer" },
      ...(!questionless
        ? {
            selectionContext: {
              text: `PRIVATE-QUESTION-SENTINEL-${id}`,
              source: { ...original, fragment: "#/content/question" },
            },
          }
        : {}),
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

test("seventeen governing decisions retain three Task originals with disclosed optional omissions", async () => {
  const f = await fixture({
    decisions: 17,
    taskDecisions: 3,
    questionlessDecisions: [2, 5, 6, 9, 12],
  });
  const value = await f.read();
  expect(value.status).toBe("source-qualified");
  expect(value.holdReasons).toEqual([]);
  expect(value.context?.omitted.decisions).toBeGreaterThan(0);
  expect(value.context?.decisions.slice(0, 3).map((decision) => decision.id)).toEqual([
    "decision-0",
    "decision-1",
    "decision-2",
  ]);
  expect(value.context?.decisions[2]?.question).toBeNull();
  expect(value.context?.decisions.every((decision) => decision.answer?.blobOid)).toBe(true);
  expect(value.context?.governingDecisionIndex).toBe("work.json#/governingDecisionIds");
  expect(value.context?.omitted.decisions! + value.context?.decisions.length!).toBe(17);
  expect(value.context?.purpose.objective).toBe("Preserve the current integrated purpose");
  expect(value.context?.currentTask.title).toBe("Current integrated Task");
  expect(value.context?.omitted.fields).toEqual([]);
  expect(JSON.stringify(value)).not.toContain("PRIVATE-ANSWER-SENTINEL");
  expect(JSON.stringify(value)).not.toContain("PRIVATE-QUESTION-SENTINEL");
  expect(JSON.stringify(value).length).toBeLessThanOrEqual(6000);
});

test("a direct actual answer needs no invented question while a declared missing question holds", async () => {
  const direct = await fixture({ questionlessDecisions: [0] });
  const value = await direct.read();
  expect(value.status).toBe("source-qualified");
  expect(value.context?.decisions[0]?.answer?.fragment).toBe("#/content/answer");
  expect(value.context?.decisions[0]?.question).toBeNull();
  const missing = await fixture({ missingQuestionDecision: 0 });
  const held = await missing.read();
  expect(held.status).toBe("held");
  expect(held.holdReasons).toContain("REQUIRED_DECISION_ORIGINAL_LOCATOR_UNAVAILABLE");
  expect(JSON.stringify(held)).not.toContain("PRIVATE-QUESTION-SENTINEL");
});

test("an omitted nonessential governing original is still fully validated", async () => {
  const f = await fixture({ decisions: 17, taskDecisions: 3, badAnswerDecision: 16 });
  const value = await f.read();
  expect(value.status).toBe("held");
  expect(value.holdReasons).toContain("ACTUAL_ANSWER_FRAGMENT_MISMATCH");
  expect(value.context?.decisions.some((decision) => decision.id === "decision-16")).toBe(false);
  expect(value.context?.omitted.decisions).toBeGreaterThan(0);
  expect(JSON.stringify(value)).not.toContain("BROKEN-ORIGINAL-BINDING");
  expect(JSON.stringify(value).length).toBeLessThanOrEqual(6000);
});

test("essential Task original overflow stays held without truncating purpose or Task text", async () => {
  const f = await fixture({ decisions: 17 });
  const value = await f.read();
  expect(value.status).toBe("held");
  expect(value.holdReasons).toEqual(["REQUIRED_TASK_DECISION_ORIGINALS_OUTPUT_LIMIT"]);
  expect(value.context).toBeNull();
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

function nativePayload(evt: HermesCurrentWorkEvent = event) {
  return {
    hook_event_name: "pre_llm_call",
    profile: evt.profileId,
    session_id: evt.sessionId,
    cwd: "/synthetic/native-cwd",
    tool_name: null,
    tool_input: null,
    extra: {
      agent_id: evt.agentId,
      project_id: evt.nativeProjectId,
      task_id: evt.nativeTaskId,
      run_id: evt.nativeRunId,
      turn_id: "actual-turn-not-native-run",
      user_message: "PRIVATE-NATIVE-PROMPT",
      conversation_history: [{ content: "PRIVATE-NATIVE-HISTORY" }],
    },
  };
}
function hookBindings(f: Awaited<ReturnType<typeof fixture>>) {
  return {
    schema: "hermes.current-git-work-hook-bindings.v1",
    runtimeIndex: f.index,
    selections: [{ cwd: "/synthetic/native-cwd", selection: f.selected }],
  };
}
test("native pre_llm_call protocol returns purpose Task revisions and original pointers without native payloads", async () => {
  const f = await fixture();
  const response = await readHermesNativeWorkHook(nativePayload(), hookBindings(f), f.provider);
  const context = JSON.parse(response.context!);
  expect(Object.keys(response)).toEqual(["context"]);
  expect(context.status).toBe("source-qualified");
  expect(context.context.purpose.objective).toBe("Preserve the current integrated purpose");
  expect(context.context.currentTask.title).toBe("Current integrated Task");
  expect(context.context.upstream.spec.revision).toBe(6);
  expect(context.context.decisions[0].answer.fragment).toBe("#/content/answer");
  expect(response.context!.length).toBeLessThanOrEqual(6000);
  for (const privateText of [
    "PRIVATE-NATIVE-PROMPT",
    "PRIVATE-NATIVE-HISTORY",
    "PRIVATE-ANSWER-SENTINEL",
    "PRIVATE-QUESTION-SENTINEL",
  ])
    expect(JSON.stringify(response)).not.toContain(privateText);
  expect(f.counters()).toEqual({ reads: 2, checks: 4 });
});
test("installed seam missing agent project and run remain exact nulls; turn and logical Task never fill them", async () => {
  const f = await fixture();
  const actual = { ...event, agentId: null, nativeProjectId: null, nativeRunId: null };
  f.selected.native = actual;
  f.index.entries[0] = { ...f.index.entries[0]!, ...actual };
  const payload = nativePayload(actual);
  const { agent_id: _a, project_id: _p, run_id: _r, ...extra } = payload.extra;
  const response = await readHermesNativeWorkHook(
    { ...payload, extra },
    hookBindings(f),
    f.provider,
  );
  expect(JSON.parse(response.context!).context.native).toEqual(actual);
  expect(JSON.parse(response.context!).context.native.nativeRunId).not.toBe(extra.turn_id);
});
test("foreign native tuples and cwd stay unselected without Git reads or legacy fallback", async () => {
  const f = await fixture();
  const original = nativePayload();
  for (const payload of [
    { ...original, profile: "foreign-profile" },
    { ...original, session_id: "foreign-session" },
    { ...original, cwd: "/foreign/cwd" },
    ...["agent_id", "project_id", "task_id", "run_id"].map((key) => ({
      ...original,
      extra: { ...original.extra, [key]: "foreign-native-id" },
    })),
    { ...original, hook_event_name: "on_session_start" },
  ])
    expect(await readHermesNativeWorkHook(payload, hookBindings(f), f.provider)).toEqual({});
  expect(f.counters()).toEqual({ reads: 0, checks: 0 });
  await expect(
    readHermesNativeWorkHook(original, { schema: "hermes-platform-work-v1" }, f.provider),
  ).rejects.toThrow("EXPLICIT_NATIVE_HOOK_BINDINGS_REQUIRED");
});
test("child identity requires its own exact selection and cannot inherit an absent-agent parent row", async () => {
  const f = await fixture(),
    child = { ...event, agentId: "observed-child-agent", sessionId: "observed-child-session" };
  expect(await readHermesNativeWorkHook(nativePayload(child), hookBindings(f), f.provider)).toEqual(
    {},
  );
  expect(f.counters().reads).toBe(0);
  f.selected.native = child;
  f.index.entries[0] = { ...f.index.entries[0]!, ...child };
  expect(
    JSON.parse(
      (await readHermesNativeWorkHook(nativePayload(child), hookBindings(f), f.provider)).context!,
    ).context.native,
  ).toEqual(child);
  expect(
    await readHermesNativeWorkHook(
      nativePayload({ ...child, agentId: null }),
      hookBindings(f),
      f.provider,
    ),
  ).toEqual({});
});
test("native unknown currentness stays visibly held and late async closure drift suppresses protocol output", async () => {
  const f = await fixture();
  f.selected.latestRelevant = "unknown";
  const response = await readHermesNativeWorkHook(nativePayload(), hookBindings(f), f.provider);
  expect(JSON.parse(response.context!).status).toBe("held");
  expect(JSON.parse(response.context!).holdReasons).toContain("REQUIRED_LATEST_CONTEXT_UNKNOWN");
  let checks = 0;
  f.provider.verifySourceClosure = async () => {
    if (++checks === 4) throw Error("PRIVATE-LATE-ASYNC-DRIFT");
  };
  await expect(
    readHermesNativeWorkHook(nativePayload(), hookBindings(f), f.provider),
  ).rejects.toThrow("PRIVATE-LATE-ASYNC-DRIFT");
});
