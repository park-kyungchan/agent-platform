import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import * as gitWork from "@palantirkc/work-continuity/git-work";
import type { GitWorkRecord } from "@palantirkc/work-continuity/git-work";
import {
  handleHook,
  readHookBindings,
  parseHookInput,
  resolveSessionGitWorkSelection,
  type HookBinding,
  type HookBindings,
  type HookInput,
  type NativeContextLoader,
  type SessionGitWorkSelection,
} from "./hooks.ts";
import type { HomeContextContract } from "./home-context.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
async function command(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...gitWork.gitReadEnvironment(),
      GIT_AUTHOR_NAME: "Synthetic",
      GIT_AUTHOR_EMAIL: "fixture@example.invalid",
      GIT_COMMITTER_NAME: "Synthetic",
      GIT_COMMITTER_EMAIL: "fixture@example.invalid",
    },
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error(stderr);
  return stdout.trim();
}
async function fixture() {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "codex-selected-work-")));
  roots.push(parent);
  const home = join(parent, "home"),
    owner = join(parent, "owner"),
    repositoryRoot = join(parent, "common");
  await mkdir(join(home, ".codex/harness"), { recursive: true });
  await mkdir(join(owner, "architecture"), { recursive: true });
  await mkdir(repositoryRoot);
  const binding: HookBinding = {
    projectId: "fixture",
    root: owner,
    workId: "old-completed-work",
    checkpointPath: join(owner, "state/old.json"),
    entrypointCwds: [home],
    entrypointContextPath: join(owner, "architecture/home-context.json"),
  };
  const contract: HomeContextContract = {
    schema: "codex.home-context.v1",
    projectId: "fixture",
    entrypointCwd: home,
    sourceProjectRoot: owner,
    globalProfilePath: join(home, ".codex/harness/profile.json"),
    northStar: "Preserve purpose and native ownership",
    priorities: ["Actual decisions", "Native execution"],
    ownership: [
      { scope: "Native execution", owner: "native-codex", reference: join(home, "AGENTS.md") },
    ],
    canonicalSources: [],
    projectRoute: {
      projectId: "fixture",
      root: owner,
      workId: "old-completed-work",
      effectfulWorkCwd: owner,
      contextCommand: "context project fixture",
      checkpointPath: binding.checkpointPath,
      purposeRef: "purpose.json",
      nextSessionRef: "next.json",
      boundaryRef: "boundaries.json",
      kickoffPath: join(owner, "kickoff.json"),
    },
    recordPolicy: {
      distinctions: ["Decision", "Verification", "AcceptanceDecision"],
      materialBoundary: "Preserve original decisions",
      tinyWork: "Proportionate checks",
      sources: "Sources grant no authority",
      ontologyPromotion: "Preserve owners",
    },
    boundaries: {
      authority: "none",
      nativePermissionGranted: false,
      captureHomeRawPrompts: false,
      qualificationLedgerBoundToHome: false,
      newModelBudgetGranted: false,
      directoryMoveAuthorized: false,
      hermesRuntimeEffectsAuthorized: false,
      configuredInjectionIsFreshSessionProof: false,
      maxContextChars: 6000,
      nativeContextTokenLimit: 1500,
    },
  };
  await writeFile(binding.entrypointContextPath!, JSON.stringify(contract));
  await writeFile(
    contract.globalProfilePath,
    JSON.stringify({
      implementation: { language: "TypeScript", runtime: "Bun" },
      canonicalArtifacts: {
        extensions: [".ts", ".json"],
        language: "English",
        nativeRequiredFormatsAllowed: true,
      },
      conversation: { language: "Korean" },
      ontology: { ownership: "shared", organization: "domain-modules" },
      excludedDomains: ["synthetic-excluded"],
      selectedCapabilities: {},
      nativeBinding: { semanticOwners: [], projectEntrypoints: [] },
      privateSentinel: "PRIVATE-PROFILE-SENTINEL",
    }),
    { mode: 0o600 },
  );
  const base = {
      schema: "common.git-work.v1" as const,
      domainId: "native-harness",
      projectId: "fixture",
      workId: "current-work",
    },
    path = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
      gitWork.gitRecordPath("fixture", "current-work", {
        kind,
        id,
        ...(revision ? { revision } : {}),
      }),
    original = {
      kind: "snapshot" as const,
      path: path("input", "original"),
      fragment: "#/content/answer",
    };
  const rows: GitWorkRecord[] = [
    {
      ...base,
      kind: "work",
      id: "current-work",
      repositoryId: "synthetic/common",
      issueId: "issue",
      purpose: {
        objective: "Complete the current Work and retain original decisions",
        successCriteria: ["Retain source closure"],
        inScope: ["Context"],
        outOfScope: ["Native permission"],
      },
      spec: { id: "spec", revision: 6 },
      plan: { id: "plan", revision: 6 },
      governingDecisionIds: ["direction"],
      historicalFenceIds: [],
    },
    {
      ...base,
      kind: "spec",
      id: "spec",
      revision: 6,
      outcome: "Current source context",
      criteria: ["Exact tuple"],
    },
    {
      ...base,
      kind: "plan",
      id: "plan",
      revision: 6,
      specId: "spec",
      specRevision: 6,
      objective: "Read current source",
    },
    {
      ...base,
      kind: "input",
      id: "original",
      inputKind: "actual-user-answer",
      origin: { ownerId: "user", locator: "synthetic:original", kind: "native-user-message" },
      content: { answer: "PRIVATE-ORIGINAL-SENTINEL", question: "Synthetic question" },
    },
    {
      ...base,
      kind: "decision",
      id: "direction",
      status: "accepted",
      answer: "PRIVATE-ORIGINAL-SENTINEL",
      meaning: "Retain the original source",
      answerSource: original,
      selectionContext: {
        text: "Synthetic question",
        source: { ...original, fragment: "#/content/question" },
      },
      supersedes: [],
    },
    {
      ...base,
      kind: "task",
      id: "current-task",
      title: "Preserve current purpose",
      logical: {
        domainId: "native-harness",
        projectId: "fixture",
        workId: "current-work",
        issueId: "issue",
        specId: "spec",
        specRevision: 6,
        planId: "plan",
        planRevision: 6,
        taskId: "current-task",
      },
      state: {
        execution: "running",
        verification: "pending",
        localAudit: "intent-durable",
        sharedAudit: "pending",
        formalCompletion: "pending",
      },
      executionClass: "source-only",
      dependsOn: [],
      decisionIds: ["direction"],
      effectIds: [],
      verificationEvidenceIds: [],
    },
  ];
  for (const row of rows) {
    const file = join(
      repositoryRoot,
      path(row.kind, row.id, row.kind === "spec" || row.kind === "plan" ? row.revision : undefined),
    );
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, JSON.stringify(row));
  }
  await command(repositoryRoot, "init", "-q", "-b", "work");
  await command(repositoryRoot, "add", ".");
  await command(repositoryRoot, "commit", "-qm", "Synthetic source");
  const commit = await command(repositoryRoot, "rev-parse", "HEAD");
  const selection: SessionGitWorkSelection = {
    schema: "codex.session-git-work-selection.v1",
    nativeSessionId: "parent",
    cwd: home,
    ownerRoot: owner,
    projectId: "fixture",
    workId: "current-work",
    snapshot: {
      repositoryId: "synthetic/common",
      repositoryRoot,
      commit: "refs/heads/work",
      projectId: "fixture",
      workId: "current-work",
    },
    taskId: "current-task",
    latestRelevant: "current",
  };
  const registry: HookBindings = {
    schemaVersion: "1.0.0",
    bindings: [binding],
    contextSelections: [selection],
  };
  const registryPath = join(owner, "architecture/native-hook-bindings.json");
  await writeFile(registryPath, JSON.stringify(registry));
  const boundRegistry = readHookBindings(registryPath);
  const input: HookInput = {
    hook_event_name: "UserPromptSubmit",
    session_id: "parent",
    cwd: home,
    prompt: "PRIVATE-PROMPT-SENTINEL",
  };
  let gitReads = 0,
    checks = 0;
  const unavailable = () => {
    throw Error("Legacy checkpoint must not be read for selected Home Work");
  };
  const provider: Awaited<ReturnType<NativeContextLoader>> = {
    index: {
      boundedProjection: unavailable,
      readCheckpoint: unavailable,
      recordBoundary: unavailable,
    },
    sections: { readWorkContextSection: unavailable },
    gitWork: {
      ...gitWork,
      resolveGitWorkSelection: async (value) => {
        gitReads++;
        return gitWork.resolveGitWorkSelection(value);
      },
    },
    verifyCurrentSourceClosure: async () => {
      checks++;
    },
  };
  const load: NativeContextLoader = async () => provider;
  return {
    parent,
    home,
    owner,
    repositoryRoot,
    binding,
    registry: boundRegistry,
    registryPath,
    input,
    selection: boundRegistry.contextSelections![0] as SessionGitWorkSelection,
    commit,
    load,
    provider,
    counters: () => ({ gitReads, checks }),
  };
}
async function persistBindings(f: Awaited<ReturnType<typeof fixture>>) {
  await writeFile(f.registryPath, JSON.stringify(f.registry));
  f.registry = readHookBindings(f.registryPath);
  f.selection = f.registry.contextSelections![0] as SessionGitWorkSelection;
}
function context(stdout: string) {
  const injected = JSON.parse(stdout).hookSpecificOutput.additionalContext as string;
  return { text: injected, value: JSON.parse(injected.slice(injected.indexOf("\n") + 1)) };
}

test("selected native tuple injects immutable current Task and decision originals without payloads", async () => {
  const f = await fixture();
  const before = await readFile(f.binding.entrypointContextPath!);
  for (const input of [
    f.input,
    { ...f.input, hook_event_name: "SessionStart" as const, source: "resume" },
  ]) {
    const result = await handleHook(input, { bindings: f.registry }, f.load);
    expect(result.status).toBe("context-injected");
    const c = context(result.stdout);
    expect(c.text.length).toBeLessThanOrEqual(6000);
    expect(c.value.selectedWork.status).toBe("source-qualified");
    expect(c.value.selectedWork.pointer.commit).toBe(f.commit);
    expect(c.value.selectedWork.purpose.objective).toContain("current Work");
    expect(c.value.selectedWork.purpose.specRevision).toBe(6);
    expect(c.value.selectedWork.currentTask.id).toBe("current-task");
    expect(c.value.selectedWork.currentTask.state.execution).toBe("running");
    expect(c.value.selectedWork.currentness).toBe("caller-declared-not-remote-verified");
    const original = c.value.selectedWork.decisionOriginals.directPointers[0];
    const address = c.value.selectedWork.decisionOriginals.sources.find(
      (row: { id: string }) => row.id === original.answer.source,
    );
    expect(address.path).toBe("inputs/original.json");
    expect(original.answer.fragment).toBe("#/content/answer");
    expect(address.blobOid).toMatch(/^[a-f0-9]{40}$/);
    expect(original.selectionContext.fragment).toBe("#/content/question");
    expect(c.value.selectedWork.nativePermissionGranted).toBe(false);
    expect(c.value.selectedWork.semanticAcceptanceEstablished).toBe(false);
    expect(c.value.preferences.excludedDomains).toEqual(["synthetic-excluded"]);
    expect(c.value.projectRoute).toBeUndefined();
    expect(c.value.defaultProjectRoute?.role).toBe(
      "default-bootstrap-route-not-selected-session-work",
    );
    for (const sentinel of [
      "PRIVATE-PROMPT-SENTINEL",
      "PRIVATE-ORIGINAL-SENTINEL",
      "PRIVATE-PROFILE-SENTINEL",
    ])
      expect(c.text).not.toContain(sentinel);
  }
  expect(f.counters()).toEqual({ gitReads: 4, checks: 8 });
  expect(await readFile(f.binding.entrypointContextPath!)).toEqual(before);
  expect(await command(f.repositoryRoot, "status", "--porcelain")).toBe("");
});

test("unselected, foreign and child tuples make no Git reads; explicit child identity is required", async () => {
  const f = await fixture();
  for (const input of [
    { ...f.input, session_id: "foreign" },
    { ...f.input, agent_id: "child" },
  ]) {
    const result = await handleHook(input, { bindings: f.registry }, f.load);
    expect(result.status).toBe("context-injected");
    expect(context(result.stdout).value.selectedWork).toBeUndefined();
  }
  expect(
    resolveSessionGitWorkSelection({ ...f.input, cwd: f.owner }, f.registry, f.binding),
  ).toBeUndefined();
  expect(f.counters().gitReads).toBe(0);
  f.registry.contextSelections!.push({ ...f.selection, nativeAgentId: "child" });
  await persistBindings(f);
  expect(
    resolveSessionGitWorkSelection({ ...f.input, agent_id: "child" }, f.registry, f.binding)
      ?.nativeAgentId,
  ).toBe("child");
  const selected = await handleHook(
    { ...f.input, agent_id: "child" },
    { bindings: f.registry },
    f.load,
  );
  expect(context(selected.stdout).value.selectedWork.currentTask.id).toBe("current-task");
  expect(f.counters().gitReads).toBe(2);
});

test("unknown currentness and local ref movement remain held rather than fresh or executable", async () => {
  const f = await fixture();
  delete f.selection.latestRelevant;
  await persistBindings(f);
  const unknown = context((await handleHook(f.input, { bindings: f.registry }, f.load)).stdout)
    .value.selectedWork;
  expect(unknown.status).toBe("held");
  expect(unknown.holdReasons).toContain("REQUIRED_LATEST_CONTEXT_UNKNOWN");
  expect(unknown.purpose.objective).toContain("current Work");
  f.selection.latestRelevant = "current";
  await persistBindings(f);
  const original = f.provider.gitWork!.resolveGitWorkSelection;
  let reads = 0;
  f.provider.gitWork!.resolveGitWorkSelection = async (value) => {
    const resolved = await original(value);
    return ++reads === 2 ? { ...resolved, commit: "a".repeat(40) } : resolved;
  };
  const advanced = context((await handleHook(f.input, { bindings: f.registry }, f.load)).stdout)
    .value.selectedWork;
  expect(advanced.status).toBe("held");
  expect(advanced.holdReasons).toContain("SELECTED_LOCAL_REF_ADVANCED_DURING_READ");
  expect(advanced.pointer.commit).toBe(f.commit);
});

test("duplicate tuples, revision expressions, owner mismatch and missing proven Git capability refuse selection", async () => {
  const f = await fixture(),
    registryPath = join(f.owner, "bindings.json");
  await writeFile(registryPath, JSON.stringify(f.registry));
  expect(readHookBindings(registryPath).contextSelections).toHaveLength(1);
  for (const mutate of [
    (v: HookBindings) => {
      v.contextSelections!.push({ ...f.selection });
    },
    (v: HookBindings) => {
      (v.contextSelections![0] as SessionGitWorkSelection).snapshot.commit = "HEAD~1";
    },
  ]) {
    const invalid = structuredClone(f.registry);
    mutate(invalid);
    await writeFile(registryPath, JSON.stringify(invalid));
    expect(() => readHookBindings(registryPath)).toThrow();
  }
  const invalid = structuredClone(f.registry);
  (invalid.contextSelections![0] as SessionGitWorkSelection).ownerRoot = f.repositoryRoot;
  expect((await handleHook(f.input, { bindings: invalid }, f.load)).status).toBe("context-held");
  delete f.provider.gitWork;
  const missing = await handleHook(f.input, { bindings: f.registry }, f.load);
  expect(missing.status).toBe("context-held");
  expect(missing.stdout).not.toContain("PRIVATE-");
});

test("source closure drift refuses injection and bounded derived overflow stays explicitly held", async () => {
  const f = await fixture();
  const large = structuredClone(f.selection);
  // Many relevant originals require a pointer view rather than dropping owner preferences.
  f.registry.contextSelections = [large];
  await persistBindings(f);
  const originalRead = f.provider.gitWork!.readGitWorkContext;
  f.provider.gitWork!.readGitWorkContext = async (reader, request) => {
    const value = await originalRead(reader, request);
    value.records.find((row) => row.kind === "task")!.title = "S".repeat(7000);
    return value;
  };
  const bounded = context((await handleHook(f.input, { bindings: f.registry }, f.load)).stdout);
  expect(bounded.text.length).toBeLessThanOrEqual(6000);
  expect(bounded.value.selectedWork.status).toBe("held");
  expect(bounded.value.selectedWork.holdReasons).toEqual(["NATIVE_CONTEXT_OUTPUT_LIMIT"]);
  expect(bounded.value.preferences.implementation.runtime).toBe("Bun");
  expect(bounded.value.northStar).toBe("Preserve purpose and native ownership");
  f.provider.verifyCurrentSourceClosure = async () => {
    throw Error("Pinned code closure changed");
  };
  const changed = await handleHook(f.input, { bindings: f.registry }, f.load);
  expect(changed.status).toBe("context-held");
  expect(JSON.parse(changed.stdout).hookSpecificOutput).toBeUndefined();
});

test("shared local resolver pins a branch snapshot and ignores ambient Git directory overrides", async () => {
  const f = await fixture(),
    previous = process.env.GIT_DIR;
  process.env.GIT_DIR = join(f.parent, "nonexistent");
  try {
    expect((await gitWork.resolveGitWorkSelection(f.selection.snapshot)).commit).toBe(f.commit);
  } finally {
    if (previous === undefined) delete process.env.GIT_DIR;
    else process.env.GIT_DIR = previous;
  }
  await writeFile(join(f.repositoryRoot, "new.txt"), "Synthetic advancement");
  await command(f.repositoryRoot, "add", ".");
  await command(f.repositoryRoot, "commit", "-qm", "Advance local branch");
  expect((await gitWork.resolveGitWorkSelection(f.selection.snapshot)).commit).not.toBe(f.commit);
  expect((await gitWork.openGitWork({ ...f.selection.snapshot, commit: f.commit })).commit).toBe(
    f.commit,
  );
});

test("grown verified decision history keeps Work purpose and Task inside6k with explicit unexpanded original pointers", async () => {
  const f = await fixture();
  const file = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
    join(
      f.repositoryRoot,
      gitWork.gitRecordPath("fixture", "current-work", {
        kind,
        id,
        ...(revision ? { revision } : {}),
      }),
    );
  const taskPath = file("task", "current-task"),
    workPath = file("work", "current-work");
  const task = JSON.parse(await readFile(taskPath, "utf8")),
    work = JSON.parse(await readFile(workPath, "utf8"));
  for (let index = 0; index < 12; index++) {
    const id = `synthetic-required-direction-${index}`,
      originalId = `synthetic-original-answer-${index}`;
    task.decisionIds.push(id);
    work.governingDecisionIds.push(id);
    const base = {
      schema: "common.git-work.v1",
      domainId: "native-harness",
      projectId: "fixture",
      workId: "current-work",
    };
    await writeFile(
      file("input", originalId),
      JSON.stringify({
        ...base,
        kind: "input",
        id: originalId,
        inputKind: "actual-user-answer",
        origin: { ownerId: "user", locator: `synthetic:${index}`, kind: "native-user-message" },
        content: { text: "PRIVATE-GROWN-ANSWER", question: "Synthetic decision question" },
      }),
    );
    await writeFile(
      file("decision", id),
      JSON.stringify({
        ...base,
        kind: "decision",
        id,
        status: "accepted",
        answer: "PRIVATE-GROWN-ANSWER",
        meaning: "Retain exact original pointers in growing history",
        answerSource: {
          kind: "snapshot",
          path: gitWork.gitRecordPath("fixture", "current-work", { kind: "input", id: originalId }),
          fragment: "#/content/text",
        },
        supersedes: [],
      }),
    );
  }
  await writeFile(taskPath, JSON.stringify(task));
  await writeFile(workPath, JSON.stringify(work));
  await command(f.repositoryRoot, "add", ".");
  await command(f.repositoryRoot, "commit", "-qm", "Grow synthetic decision history");
  const reader = await gitWork.openGitWork(
    await gitWork.resolveGitWorkSelection(f.selection.snapshot),
  );
  const full = await gitWork.readGitWorkContext(reader, {
    taskId: "current-task",
    latestRelevant: "current",
  });
  expect(full.status).toBe("source-qualified");
  const c = context((await handleHook(f.input, { bindings: f.registry }, f.load)).stdout);
  expect(c.text.length).toBeLessThanOrEqual(6000);
  expect(c.value.selectedWork.purpose.objective).toContain("current Work");
  expect(c.value.selectedWork.currentTask.id).toBe("current-task");
  expect(c.value.selectedWork.status).toBe("held");
  expect(c.value.selectedWork.holdReasons).toContain("NATIVE_DECISION_POINTERS_UNEXPANDED");
  expect(c.value.selectedWork.decisionOriginals.directPointers.length).toBeGreaterThan(0);
  expect(c.value.selectedWork.decisionOriginals.directPointers.length).toBeLessThan(13);
  expect(c.value.selectedWork.decisionOriginals.unexpandedDecisionCount).toBe(
    13 - c.value.selectedWork.decisionOriginals.directPointers.length,
  );
  expect(c.value.selectedWork.decisionOriginals.governingIndex).toBe(
    "work.json#/governingDecisionIds",
  );
  expect(c.text).not.toContain("PRIVATE-GROWN-ANSWER");
});

test("historical Task contract pins remain distinct from the current Work revision", async () => {
  const f = await fixture();
  const file = (kind: GitWorkRecord["kind"], id: string, revision?: number) =>
    join(
      f.repositoryRoot,
      gitWork.gitRecordPath("fixture", "current-work", {
        kind,
        id,
        ...(revision ? { revision } : {}),
      }),
    );
  const taskPath = file("task", "current-task"),
    task = JSON.parse(await readFile(taskPath, "utf8"));
  task.logical.specRevision = 5;
  task.logical.planRevision = 5;
  await writeFile(taskPath, JSON.stringify(task));
  for (const kind of ["spec", "plan"] as const) {
    const old = JSON.parse(await readFile(file(kind, kind, 6), "utf8"));
    old.revision = 5;
    if (kind === "plan") old.specRevision = 5;
    await writeFile(file(kind, kind, 5), JSON.stringify(old));
  }
  await command(f.repositoryRoot, "add", ".");
  await command(f.repositoryRoot, "commit", "-qm", "Retain historical Task pins");
  const c = context((await handleHook(f.input, { bindings: f.registry }, f.load)).stdout).value
    .selectedWork;
  expect(c.status).toBe("source-qualified");
  expect(c.purpose.specRevision).toBe(6);
  expect(c.purpose.planRevision).toBe(6);
  expect(c.currentTask.specRevision).toBe(5);
  expect(c.currentTask.planRevision).toBe(5);
  expect(c.currentTask.contractSource).toBe("tasks/current-task.json#/logical");
});

test("malformed selected source cannot leak private source bytes through hook failure text", async () => {
  const f = await fixture();
  await writeFile(
    join(
      f.repositoryRoot,
      gitWork.gitRecordPath("fixture", "current-work", { kind: "work", id: "current-work" }),
    ),
    "PRIVATE-MALFORMED-SOURCE",
  );
  await command(f.repositoryRoot, "add", ".");
  await command(f.repositoryRoot, "commit", "-qm", "Malformed synthetic source");
  const result = await handleHook(f.input, { bindings: f.registry }, f.load);
  expect(result.status).toBe("context-held");
  expect(result.stdout).not.toContain("PRIVATE-MALFORMED-SOURCE");
  expect(result.error).not.toContain("PRIVATE-MALFORMED-SOURCE");
  expect(JSON.parse(result.stdout).hookSpecificOutput).toBeUndefined();
});

test("documented fork starts preserve exact parent and child tuples with no inferred Work", async () => {
  const f = await fixture();
  const fork: HookInput = {
    hook_event_name: "SessionStart",
    source: "fork",
    session_id: "parent",
    cwd: f.home,
    agent_id: "selected-child",
    agent_type: "synthetic-worker",
  };
  expect(parseHookInput(fork)).toEqual(fork);
  // Official common input uses the parent session id for child hooks. The
  // existing parent selection must never act as a wildcard for this child.
  for (const input of [
    fork,
    { ...fork, session_id: "foreign" },
    { ...fork, agent_id: "foreign-child" },
  ]) {
    const result = await handleHook(input, { bindings: f.registry }, f.load);
    expect(result.status).toBe("context-injected");
    expect(JSON.parse(result.stdout).hookSpecificOutput.hookEventName).toBe("SessionStart");
    expect(context(result.stdout).value.selectedWork).toBeUndefined();
  }
  expect(f.counters().gitReads).toBe(0);
  f.registry.contextSelections!.push({
    ...f.selection,
    nativeAgentId: "selected-child",
    nativeAgentType: "synthetic-worker",
  });
  await persistBindings(f);
  const selected = await handleHook(fork, { bindings: f.registry }, f.load);
  const projected = context(selected.stdout);
  expect(projected.value.selectedWork.currentTask.id).toBe("current-task");
  expect(projected.value.selectedWork.pointer.commit).toBe(f.commit);
  expect(projected.value.selectedWork.nativePermissionGranted).toBe(false);
  expect(projected.value.selectedWork.semanticAcceptanceEstablished).toBe(false);
  expect(projected.text.length).toBeLessThanOrEqual(6000);
  expect(f.counters().gitReads).toBe(2);
  // Fork also describes a native forked root. Absence of agent identity still
  // matches only the explicitly selected parent tuple.
  const parent = await handleHook(
    { ...fork, agent_id: undefined, agent_type: undefined },
    { bindings: f.registry },
    f.load,
  );
  expect(context(parent.stdout).value.selectedWork.currentTask.id).toBe("current-task");
  expect(f.counters().gitReads).toBe(4);
});

test("fork support retains legacy SessionStart sources and rejects malformed fork payloads", async () => {
  const f = await fixture();
  for (const source of ["startup", "resume", "clear", "compact", "fork"]) {
    const input: HookInput = {
      hook_event_name: "SessionStart",
      source,
      session_id: "parent",
      cwd: f.home,
    };
    expect(parseHookInput(input).source).toBe(source);
    const result = await handleHook(input, { bindings: f.registry }, f.load);
    expect(result.status).toBe("context-injected");
    expect(context(result.stdout).value.selectedWork.currentTask.id).toBe("current-task");
  }
  for (const input of [
    { hook_event_name: "SessionStart", source: "fork", session_id: 7, cwd: f.home },
    { hook_event_name: "SessionStart", source: "fork", session_id: "parent", cwd: "relative" },
    {
      hook_event_name: "SessionStart",
      source: "fork",
      session_id: "parent",
      cwd: f.home,
      agent_id: 7,
    },
    {
      hook_event_name: "SessionStart",
      source: "future-unknown",
      session_id: "parent",
      cwd: f.home,
    },
  ]) {
    expect(() => parseHookInput(input)).toThrow();
    expect((await handleHook(input, { bindings: f.registry }, f.load)).status).toBe("failed-open");
  }
  expect(f.counters().gitReads).toBe(10);
});

test("Git selection source snapshot cannot be erased or redirected by mutable objects", async () => {
  for (const mutate of [
    (f: Awaited<ReturnType<typeof fixture>>) => {
      delete f.registry.contextSelections;
    },
    (f: Awaited<ReturnType<typeof fixture>>) => {
      f.selection.workId = "foreign-work";
    },
    (f: Awaited<ReturnType<typeof fixture>>) => {
      f.registry.bindings = [];
    },
  ]) {
    const f = await fixture();
    mutate(f);
    const result = await handleHook(f.input, { bindings: f.registry }, f.load);
    expect(result.status).toBe("context-held");
    expect(f.counters().gitReads).toBe(0);
    expect(JSON.parse(result.stdout).hookSpecificOutput).toBeUndefined();
  }
});

test("Git source registry mutation during async read holds rather than injecting stale selection", async () => {
  const f = await fixture(),
    original = f.provider.gitWork!.readGitWorkContext;
  f.provider.gitWork!.readGitWorkContext = async (reader, request) => {
    const context = await original(reader, request);
    await writeFile(f.registryPath, JSON.stringify({ ...f.registry, contextSelections: [] }));
    return context;
  };
  const result = await handleHook(f.input, { bindings: f.registry }, f.load);
  expect(result.status).toBe("context-held");
  expect(JSON.parse(result.stdout).hookSpecificOutput).toBeUndefined();
});

test("Git parent and child agent types match exactly and never borrow another tuple", async () => {
  const f = await fixture();
  for (const input of [
    { ...f.input, agent_type: "worker" },
    { ...f.input, agent_id: "child", agent_type: "worker" },
  ]) {
    expect(
      context((await handleHook(input, { bindings: f.registry }, f.load)).stdout).value
        .selectedWork,
    ).toBeUndefined();
  }
  expect(f.counters().gitReads).toBe(0);
  f.registry.contextSelections!.push({
    ...f.selection,
    nativeAgentId: "child",
    nativeAgentType: "worker",
  });
  await persistBindings(f);
  for (const input of [
    { ...f.input, agent_id: "child" },
    { ...f.input, agent_id: "child", agent_type: "foreign-type" },
  ]) {
    expect(
      context((await handleHook(input, { bindings: f.registry }, f.load)).stdout).value
        .selectedWork,
    ).toBeUndefined();
  }
  expect(f.counters().gitReads).toBe(0);
  const result = await handleHook(
    { ...f.input, agent_id: "child", agent_type: "worker" },
    { bindings: f.registry },
    f.load,
  );
  expect(context(result.stdout).value.selectedWork.currentTask.id).toBe("current-task");
});
