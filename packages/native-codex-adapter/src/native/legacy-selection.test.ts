import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as core from "@palantirkc/work-continuity/context";
import * as sections from "@palantirkc/work-continuity/context/sections";
import {
  handleHook,
  readHookBindings,
  type HookBindings,
  type LegacySessionContextSelection,
  type NativeContextLoader,
} from "./hooks.ts";

const temporaryRoots: string[] = [];
const hash = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
afterEach(async () => {
  await Promise.all(
    temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

function ownerLoader(
  verify: (check: number) => void | Promise<void> = () => {},
): NativeContextLoader {
  let checks = 0;
  return async () => ({
    index: core,
    sections,
    async verifyCurrentSourceClosure() {
      await verify(++checks);
    },
  });
}

async function fixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), "codex-legacy-selection-")));
  temporaryRoots.push(root);
  const source = join(root, "observed.ts");
  await writeFile(source, "export const observed = true;\n");
  const provenancePath = join(root, "accepted-context-direction.json");
  await writeFile(provenancePath, '{"scope":"context-only","authority":"none"}\n');
  const checkpoint = (workId: string): core.ContinuityCheckpoint => ({
    schema: "codex.continuity.checkpoint.v1",
    projectId: "fixture",
    workId,
    root,
    owner: { agentId: "/fixture-owner", threadId: "retained-owner" },
    version: 1,
    updatedAt: "2026-10-14T00:00:00.000Z",
    purpose: {
      objective: `Purpose for ${workId}`,
      successCriteria: ["Keep native ownership"],
      inScope: ["Context"],
      outOfScope: ["Permissions"],
    },
    userDecisions: [],
    completed: [],
    pending: [{ id: "next", action: "Read relevant source", dependsOn: [], status: "ready" }],
    nextAction: { taskId: "next", action: "Read relevant source" },
    nativeThreads: [],
    evidenceRefs: [],
    sourcePins: [{ path: source, sha256: hash("export const observed = true;\n"), role: "code" }],
    uncertainEffects: [],
    uninterpretedInputs: [],
    authority: "none",
    nativePermissionGranted: false,
  });
  const historical = checkpoint("historical-work"),
    selected = checkpoint("selected-work"),
    oldPath = join(root, "state/continuity/old.json"),
    selectedPath = join(root, "state/continuity/selected.json");
  await core.writeCheckpoint(oldPath, historical, { root, expectedVersion: 0 });
  const selectedPin = await core.writeCheckpoint(selectedPath, selected, {
    root,
    expectedVersion: 0,
  });
  const row: LegacySessionContextSelection = {
    schema: "codex.session-context-selection.v1",
    nativeSessionId: "actual-root",
    projectId: "fixture",
    workId: "selected-work",
    root,
    cwd: root,
    checkpointPath: selectedPath,
    checkpointSha256: selectedPin.sha256,
    provenance: { path: provenancePath, sha256: hash(await readFile(provenancePath)) },
  };
  const value: HookBindings = {
    schemaVersion: "1.0.0",
    bindings: [{ projectId: "fixture", workId: "historical-work", root, checkpointPath: oldPath }],
    contextSelections: [row],
  };
  const bindingsPath = join(root, "architecture/bindings.json");
  await mkdir(join(root, "architecture"));
  const save = async () => {
    await writeFile(bindingsPath, `${JSON.stringify(value, null, 2)}\n`);
    return readHookBindings(bindingsPath);
  };
  const event = (name = "SessionStart", extra: Record<string, unknown> = {}) => ({
    hook_event_name: name,
    session_id: "actual-root",
    cwd: root,
    ...(name === "SessionStart" ? { source: "resume" } : {}),
    ...extra,
  });
  return {
    root,
    source,
    oldPath,
    selectedPath,
    selected,
    row,
    value,
    bindingsPath,
    provenancePath,
    save,
    event,
  };
}

function expectHeld(result: Awaited<ReturnType<typeof handleHook>>) {
  expect(result.status).toBe("context-held");
  expect(result.exitCode).toBe(0);
  const output = JSON.parse(result.stdout);
  expect(output.hookSpecificOutput).toBeUndefined();
  expect(output.continue).toBeUndefined();
  expect(output.decision).toBeUndefined();
  expect(result.stdout).not.toContain("historical-work");
  expect(result.stdout).not.toContain("permissionDecision");
  expect(result.stdout).not.toContain("PRIVATE_USER_BYTES");
}

async function replaceSource(path: string, change: "bytes" | "delete" | "symlink") {
  const original = await readFile(path);
  if (change === "bytes") {
    await writeFile(path, Buffer.concat([original, Buffer.from(" ")]));
    return;
  }
  await rm(path);
  if (change === "symlink") {
    const retained = `${path}.retained`;
    await writeFile(retained, original);
    await symlink(retained, path);
  }
}

describe("source-bound legacy session context selection", () => {
  test("the exact Root receives its selected checkpoint with retained owner and bounded context", async () => {
    const f = await fixture(),
      before = await readFile(f.selectedPath),
      bindings = await f.save();
    const result = await handleHook(f.event(), { bindings }, ownerLoader());
    expect(result.status).toBe("context-injected");
    expect(result.stdout).toContain("selected-work");
    expect(result.stdout).not.toContain("historical-work");
    const context = JSON.parse(result.stdout).hookSpecificOutput.additionalContext;
    expect(context.length).toBeLessThanOrEqual(6000);
    expect(context).toContain("retained-owner");
    expect(context).toContain('"authority":"none"');
    expect(context).toContain('"nativePermissionGranted":false');
    expect(await readFile(f.selectedPath)).toEqual(before);
    expect(JSON.parse(await readFile(result.boundary!.path, "utf8")).checkpointPath).toBe(
      f.selectedPath,
    );
  });

  test.each([
    { session_id: "other-root" },
    { agent_id: "actual-child", agent_type: "reviewer" },
    { agent_type: "worker" },
  ])("an unmatched native identity does not revive Root selection: %j", async (extra) => {
    const f = await fixture();
    const result = await handleHook(
      f.event("SessionStart", extra),
      { bindings: await f.save() },
      ownerLoader(),
    );
    expect(result.status).toBe("context-injected");
    expect(result.stdout).toContain("historical-work");
    expect(result.stdout).not.toContain("selected-work");
  });

  test("an explicit child row requires both actual agent identity and exact agent type", async () => {
    const f = await fixture();
    f.row.nativeAgentId = "actual-child";
    f.row.nativeAgentType = "reviewer";
    const bindings = await f.save();
    const accepted = await handleHook(
      f.event("SessionStart", { agent_id: "actual-child", agent_type: "reviewer" }),
      { bindings },
      ownerLoader(),
    );
    expect(accepted.status).toBe("context-injected");
    expect(accepted.stdout).toContain("selected-work");
    for (const extra of [
      { agent_id: "actual-child", agent_type: "worker" },
      { agent_id: "actual-child" },
      { agent_id: "another-child", agent_type: "reviewer" },
    ]) {
      const rejected = await handleHook(
        f.event("SessionStart", extra),
        { bindings },
        ownerLoader(),
      );
      expect(rejected.stdout).not.toContain("selected-work");
    }
  });

  test("an exact identity at a different cwd holds instead of choosing another Work", async () => {
    const f = await fixture(),
      child = join(f.root, "child");
    await mkdir(child);
    expectHeld(
      await handleHook(
        f.event("SessionStart", { cwd: child }),
        { bindings: await f.save() },
        ownerLoader(),
      ),
    );
  });

  test("selection cannot change the admitted project ownership", async () => {
    const f = await fixture();
    f.row.projectId = "foreign-project";
    expectHeld(await handleHook(f.event(), { bindings: await f.save() }, ownerLoader()));
  });

  test("duplicate exact rows are refused rather than silently choosing one", async () => {
    const f = await fixture();
    f.value.contextSelections!.push(structuredClone(f.row));
    await expect(f.save()).rejects.toThrow();
  });

  for (const target of ["bindingsPath", "provenancePath", "selectedPath"] as const) {
    for (const change of ["bytes", "delete", "symlink"] as const) {
      test(`${target} ${change} before hook execution holds the recorded selection`, async () => {
        const f = await fixture(),
          bindings = await f.save();
        await replaceSource(f[target], change);
        expectHeld(await handleHook(f.event(), { bindings }, ownerLoader()));
      });

      test(`${target} ${change} during async source verification holds the recorded selection`, async () => {
        const f = await fixture(),
          bindings = await f.save();
        const changing = ownerLoader(async (check) => {
          if (check === 1) await replaceSource(f[target], change);
        });
        expectHeld(await handleHook(f.event(), { bindings }, changing));
      });
    }
  }

  test("registry symlink cannot acquire a fresh source-read proof", async () => {
    const f = await fixture();
    await f.save();
    const alias = join(f.root, "architecture/alias.json");
    await symlink(f.bindingsPath, alias);
    expect(() => readHookBindings(alias)).toThrow();
  });

  test("mutating a parsed selection cannot replace the recorded checkpoint", async () => {
    const f = await fixture(),
      bindings = await f.save(),
      sourceBytes = await readFile(f.bindingsPath);
    Object.assign(bindings.contextSelections![0]!, {
      workId: "historical-work",
      checkpointPath: f.oldPath,
      checkpointSha256: hash(await readFile(f.oldPath)),
    });
    expectHeld(await handleHook(f.event(), { bindings }, ownerLoader()));
    expect(await readFile(f.bindingsPath)).toEqual(sourceBytes);
  });

  test("deleting parsed selections cannot enable an older checkpoint fallback", async () => {
    const f = await fixture(),
      bindings = await f.save(),
      sourceBytes = await readFile(f.bindingsPath);
    delete bindings.contextSelections;
    expectHeld(await handleHook(f.event(), { bindings }, ownerLoader()));
    expect(await readFile(f.bindingsPath)).toEqual(sourceBytes);
  });

  test("rewriting a parsed binding cannot move the selected owner", async () => {
    const f = await fixture(),
      bindings = await f.save();
    bindings.bindings[0]!.workId = "selected-work";
    bindings.bindings[0]!.checkpointPath = f.selectedPath;
    expectHeld(await handleHook(f.event(), { bindings }, ownerLoader()));
  });

  test("a selection without a source-bound registry cannot inject context", async () => {
    const f = await fixture();
    expectHeld(await handleHook(f.event(), { bindings: f.value }, ownerLoader()));
  });

  test("transitive checkpoint source drift before the read holds selected context", async () => {
    const f = await fixture(),
      bindings = await f.save();
    await writeFile(f.source, "export const observed = false;\n");
    expectHeld(await handleHook(f.event(), { bindings }, ownerLoader()));
  });

  test("transitive checkpoint source drift after projection is caught by final currentness", async () => {
    const f = await fixture(),
      bindings = await f.save();
    let initialReadCompleted = false,
      changed = false;
    const changing: NativeContextLoader = async () => ({
      index: {
        ...core,
        async readCheckpoint(...args) {
          const result = await core.readCheckpoint(...args);
          initialReadCompleted = true;
          return result;
        },
      },
      sections,
      async verifyCurrentSourceClosure() {
        if (initialReadCompleted && !changed) {
          changed = true;
          await writeFile(f.source, "export const observed = false;\n");
        }
      },
    });
    expectHeld(await handleHook(f.event(), { bindings }, changing));
    expect(initialReadCompleted).toBe(true);
    expect(changed).toBe(true);
  });

  test("late registry drift after final checkpoint read cannot publish stale context", async () => {
    const f = await fixture(),
      bindings = await f.save();
    let reads = 0,
      changed = false;
    const changing: NativeContextLoader = async () => ({
      index: {
        ...core,
        async readCheckpoint(...args) {
          const result = await core.readCheckpoint(...args);
          reads++;
          return result;
        },
      },
      sections,
      async verifyCurrentSourceClosure() {
        if (reads >= 2 && !changed) {
          changed = true;
          await replaceSource(f.bindingsPath, "bytes");
        }
      },
    });
    expectHeld(await handleHook(f.event(), { bindings }, changing));
    expect(reads).toBeGreaterThanOrEqual(2);
    expect(changed).toBe(true);
  });

  test("late transitive source drift during final provider verification holds selected context", async () => {
    const f = await fixture(),
      bindings = await f.save();
    let reads = 0,
      changed = false;
    const changing: NativeContextLoader = async () => ({
      index: {
        ...core,
        async readCheckpoint(...args) {
          const result = await core.readCheckpoint(...args);
          reads++;
          expect(result.sourceStatus.status).toBe("current");
          return result;
        },
      },
      sections,
      async verifyCurrentSourceClosure() {
        if (reads >= 2 && !changed) {
          changed = true;
          await writeFile(f.source, "export const observed = false;\n");
        }
      },
    });
    expectHeld(await handleHook(f.event(), { bindings }, changing));
    expect(reads).toBeGreaterThanOrEqual(2);
    expect(changed).toBe(true);
  });

  test("selected context cannot bypass a failing code-owner closure", async () => {
    const f = await fixture();
    const denied: NativeContextLoader = async () => {
      throw Error("OWNER_SOURCE_CLOSURE_DRIFT");
    };
    expectHeld(await handleHook(f.event(), { bindings: await f.save() }, denied));
  });

  test("indivisible selected context overflow holds without truncation or older Work fallback", async () => {
    const f = await fixture();
    f.selected.purpose.objective = "selected indivisible purpose ".repeat(700);
    await writeFile(f.selectedPath, JSON.stringify(f.selected));
    f.row.checkpointSha256 = hash(await readFile(f.selectedPath));
    expectHeld(
      await handleHook(f.event(), { bindings: await f.save(), maxChars: 2000 }, ownerLoader()),
    );
  });

  test("raw user prompt stays private and its selected boundary grants no permission", async () => {
    const f = await fixture();
    const result = await handleHook(
      f.event("UserPromptSubmit", { turn_id: "actual-turn", prompt: "PRIVATE_USER_BYTES" }),
      { bindings: await f.save() },
      ownerLoader(),
    );
    expect(result.status).toBe("context-injected");
    expect(result.stdout).not.toContain("PRIVATE_USER_BYTES");
    expect(result.stdout).toContain("not an accepted decision");
    const output = JSON.parse(result.stdout);
    expect(output.continue).toBeUndefined();
    expect(output.decision).toBeUndefined();
    expect(result.stdout).not.toContain("permissionDecision");
    const boundary = JSON.parse(await readFile(result.boundary!.path, "utf8"));
    expect(boundary.checkpointPath).toBe(f.selectedPath);
    expect(boundary.turnId).toBe("actual-turn");
    expect(await readFile(boundary.pendingPrompt.path, "utf8")).toBe("PRIVATE_USER_BYTES");
  });

  test("context selection grants neither tool admission nor an evaluation reservation", async () => {
    const f = await fixture(),
      before = await readFile(f.selectedPath);
    const result = await handleHook(
      f.event("PreToolUse", {
        tool_name: "spawn_agent",
        tool_use_id: "actual-tool",
        tool_input: { message: "Synthetic source task" },
      }),
      { bindings: await f.save() },
      ownerLoader(),
    );
    expect(result).toEqual({ status: "no-op", stdout: "", exitCode: 0 });
    expect(await readFile(f.selectedPath)).toEqual(before);
  });

  test.each(["Stop", "Interrupt"])(
    "%s preserves native lifecycle and the selected boundary checkpoint",
    async (name) => {
      const f = await fixture();
      const result = await handleHook(f.event(name), { bindings: await f.save() }, ownerLoader());
      expect(result.status).toBe("recorded");
      expect(result.stdout).toBe("{}");
      expect(JSON.parse(await readFile(result.boundary!.path, "utf8")).checkpointPath).toBe(
        f.selectedPath,
      );
    },
  );

  test.each([
    "nativePermissionGranted",
    "qualificationLedgerPath",
    "childReservationTokens",
    "newModelBudgetGranted",
    "trusted",
  ])("legacy selection cannot carry the forbidden %s field", async (field) => {
    const f = await fixture();
    Object.assign(f.row, { [field]: true });
    await expect(f.save()).rejects.toThrow();
  });
});
