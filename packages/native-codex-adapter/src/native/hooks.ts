import { createHash, randomUUID } from "node:crypto";
import {
  existsSync,
  constants,
  openSync,
  closeSync,
  fstatSync,
  readSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
export type NativeContextLoader = () => Promise<{
  index: Pick<
    typeof import("@palantirkc/work-continuity/context"),
    "boundedProjection" | "readCheckpoint" | "recordBoundary"
  >;
  sections: Pick<
    typeof import("@palantirkc/work-continuity/context/sections"),
    "readWorkContextSection"
  >;
  gitWork?: Pick<
    typeof import("@palantirkc/work-continuity/git-work"),
    | "resolveGitWorkSelection"
    | "openGitWork"
    | "readGitWorkContext"
    | "compactGitWorkContext"
    | "decodeCompactGitWorkContext"
  >;
  verifyCurrentSourceClosure(): Promise<void>;
}>;
import {
  attachPlannedNativeChildTool,
  dispatchGuard,
  reserveEvaluationRequest,
  updateEvaluationLedger,
} from "./evaluation-ledger.ts";
import {
  gitWorkDeliveryPolicy,
  selectedGitWorkPointer,
  projectHomeContext,
  projectSelectedGitWork,
} from "./home-context.ts";
import type { ContextRequest, GitWorkSelection } from "@palantirkc/work-continuity/git-work";

export const nativeEvents = [
  "SessionStart",
  "UserPromptSubmit",
  "PreCompact",
  "PostCompact",
  "Stop",
  "Interrupt",
  "SubagentStop",
  "PreToolUse",
] as const;
export type NativeEvent = (typeof nativeEvents)[number];
export interface HookInput {
  hook_event_name: NativeEvent;
  session_id: string;
  cwd: string;
  transcript_path?: string | null;
  model?: string;
  permission_mode?: string;
  turn_id?: string;
  source?: string;
  trigger?: string;
  prompt?: string;
  agent_id?: string;
  agent_type?: string;
  agent_transcript_path?: string | null;
  stop_hook_active?: boolean;
  last_assistant_message?: string | null;
  tool_name?: string;
  tool_use_id?: string;
  tool_input?: unknown;
}
export interface HookBinding {
  projectId: string;
  root: string;
  checkpointPath: string;
  workId?: string;
  /** Explicit source-owned delivery opt-in; native activation is a separate owner gate. */
  gitWorkContextVersion?: "v1" | "v2";
  /** Extra isolated worktree roots must be explicitly admitted; they retain the source owner's checkpoint. */
  allowedCwds?: string[];
  /** Bootstrap entrypoints admit only this canonical cwd, never its descendants. */
  entrypointCwds?: string[];
  entrypointContextPath?: string;
  qualificationLedgerPath?: string;
  childReservationTokens?: number;
}
export interface SessionGitWorkSelection {
  schema: "codex.session-git-work-selection.v1";
  nativeSessionId: string;
  nativeAgentId?: string;
  nativeAgentType?: string;
  cwd: string;
  ownerRoot: string;
  projectId: string;
  workId: string;
  snapshot: GitWorkSelection;
  taskId?: string;
  decisionId?: string;
  /** Caller-declared currentness; never a remote observation or permission grant. */
  latestRelevant?: ContextRequest["latestRelevant"];
}
export interface LegacySessionContextSelection {
  schema: "codex.session-context-selection.v1";
  nativeSessionId: string;
  nativeAgentId?: string;
  nativeAgentType?: string;
  projectId: string;
  workId: string;
  root: string;
  cwd: string;
  checkpointPath: string;
  checkpointSha256: string;
  provenance: { path: string; sha256: string };
}
export type SessionContextSelection = LegacySessionContextSelection;
export interface HookBindings {
  schemaVersion: "1.0.0";
  bindings: HookBinding[];
  contextSelections?: (SessionGitWorkSelection | LegacySessionContextSelection)[];
}
export interface PromptLocator {
  path: string;
  sha256: string;
  bytes: number;
  atUtc: string;
  threadId: string;
  turnId?: string;
}
export interface HookResult {
  status:
    | "no-op"
    | "recorded"
    | "context-injected"
    | "context-held"
    | "failed-open"
    | "budget-reserved"
    | "budget-denied";
  stdout: string;
  exitCode: 0;
  projectId?: string;
  boundary?: { path: string; sha256: string };
  error?: string;
}
export const hashBytes = (value: string | Buffer) =>
  createHash("sha256").update(value).digest("hex");
const within = (root: string, target: string) => {
  const rel = relative(root, target);
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
};
const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`;
function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
const safeId = (value: string) => hashBytes(value).slice(0, 32);

// Ephemeral source-read proof only; no Session lifecycle or execution state is stored.
const bindingSources = new WeakMap<
  HookBindings,
  { path: string; sha256: string; snapshot: HookBindings }
>();
function freezeSnapshot<T>(value: T): T {
  if (value !== null && typeof value === "object") {
    for (const child of Object.values(value)) freezeSnapshot(child);
    Object.freeze(value);
  }
  return value;
}

export function parseHookInput(value: unknown): HookInput {
  if (
    !object(value) ||
    !nativeEvents.includes(value.hook_event_name as NativeEvent) ||
    typeof value.session_id !== "string" ||
    !value.session_id ||
    typeof value.cwd !== "string" ||
    !isAbsolute(value.cwd)
  )
    throw new Error("Unsupported or invalid native hook input");
  for (const key of ["turn_id", "source", "trigger", "prompt", "agent_id", "agent_type"] as const)
    if (value[key] !== undefined && typeof value[key] !== "string")
      throw new Error(`Invalid ${key}`);
  if (value.hook_event_name === "UserPromptSubmit" && typeof value.prompt !== "string")
    throw new Error("UserPromptSubmit requires the original prompt");
  if (
    value.hook_event_name === "SessionStart" &&
    !["startup", "resume", "clear", "compact", "fork"].includes(String(value.source))
  )
    throw new Error("Invalid SessionStart source");
  if (
    ["PreCompact", "PostCompact"].includes(String(value.hook_event_name)) &&
    !["manual", "auto"].includes(String(value.trigger))
  )
    throw new Error("Invalid compaction trigger");
  return value as unknown as HookInput;
}

function validateContextSelections(
  value: unknown,
): asserts value is LegacySessionContextSelection[] {
  if (!Array.isArray(value) || value.length > 128)
    throw Error("CONTEXT_SELECTION_REGISTRY_INVALID");
  const required = [
    "schema",
    "nativeSessionId",
    "projectId",
    "workId",
    "root",
    "cwd",
    "checkpointPath",
    "checkpointSha256",
    "provenance",
  ];
  const allowed = new Set([...required, "nativeAgentId", "nativeAgentType"]);
  for (const row of value) {
    if (
      !object(row) ||
      required.some((key) => !Object.hasOwn(row, key)) ||
      Object.keys(row).some((key) => !allowed.has(key)) ||
      row.schema !== "codex.session-context-selection.v1"
    )
      throw Error("CONTEXT_SELECTION_INVALID");
    for (const key of ["nativeSessionId", "projectId", "workId", "root", "cwd", "checkpointPath"])
      if (typeof row[key] !== "string" || !row[key].trim() || row[key].length > 4096)
        throw Error("CONTEXT_SELECTION_INVALID");
    for (const key of ["nativeAgentId", "nativeAgentType"])
      if (
        row[key] !== undefined &&
        (typeof row[key] !== "string" || !row[key].trim() || row[key].length > 256)
      )
        throw Error("CONTEXT_SELECTION_INVALID");
    if (
      typeof row.checkpointSha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(row.checkpointSha256) ||
      !object(row.provenance) ||
      Object.keys(row.provenance).sort().join(",") !== "path,sha256" ||
      typeof row.provenance.path !== "string" ||
      typeof row.provenance.sha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(row.provenance.sha256)
    )
      throw Error("CONTEXT_SELECTION_INVALID");
    for (const path of [row.root, row.cwd, row.checkpointPath, row.provenance.path])
      if (typeof path !== "string" || !isAbsolute(path) || resolve(path) !== path)
        throw Error("CONTEXT_SELECTION_PATH_INVALID");
  }
}
function selectedContextExpected(input: HookInput, registry: HookBindings): boolean {
  const matches = (rows: unknown) =>
    Array.isArray(rows) &&
    rows.some(
      (row) =>
        object(row) &&
        row.nativeSessionId === input.session_id &&
        row.nativeAgentId === input.agent_id &&
        row.nativeAgentType === input.agent_type &&
        (row.schema !== "codex.session-git-work-selection.v1" || row.cwd === input.cwd),
    );
  // A deleted or rewritten parsed row cannot erase the source-recorded selection.
  return (
    matches(bindingSources.get(registry)?.snapshot.contextSelections) ||
    matches(registry.contextSelections)
  );
}
function contextSelectionSnapshot(registry: HookBindings): HookBindings {
  const source = bindingSources.get(registry);
  if (!source) return registry;
  if (
    (source.snapshot.contextSelections !== undefined || registry.contextSelections !== undefined) &&
    JSON.stringify(source.snapshot) !== JSON.stringify(registry)
  )
    throw Error("CONTEXT_SELECTION_SOURCE_OBJECT_MISMATCH");
  return source.snapshot;
}
function verifyContextPin(root: string, pin: { path: string; sha256: string }): void {
  if (!within(root, pin.path) || realpathSync(pin.path) !== pin.path)
    throw Error("CONTEXT_SELECTION_SOURCE_UNSAFE");
  const before = lstatSync(pin.path);
  if (!before.isFile() || before.isSymbolicLink() || before.size > 2_097_152)
    throw Error("CONTEXT_SELECTION_SOURCE_UNSAFE");
  const fd = openSync(pin.path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const opened = fstatSync(fd);
    if (
      !opened.isFile() ||
      opened.dev !== before.dev ||
      opened.ino !== before.ino ||
      opened.size > 2_097_152
    )
      throw Error("CONTEXT_SELECTION_SOURCE_UNSAFE");
    const buffer = Buffer.alloc(opened.size + 1);
    let count = 0;
    while (count < buffer.length) {
      const next = readSync(fd, buffer, count, buffer.length - count, null);
      if (!next) break;
      count += next;
    }
    const after = fstatSync(fd),
      named = lstatSync(pin.path);
    if (
      count !== opened.size ||
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      named.dev !== opened.dev ||
      named.ino !== opened.ino ||
      named.isSymbolicLink() ||
      realpathSync(pin.path) !== pin.path ||
      hashBytes(buffer.subarray(0, count)) !== pin.sha256
    )
      throw Error("CONTEXT_SELECTION_SOURCE_DRIFT");
  } finally {
    closeSync(fd);
  }
}
function verifyContextSelection(
  selection: LegacySessionContextSelection,
  registry: HookBindings,
): void {
  const registrySource = bindingSources.get(registry);
  if (!registrySource) throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_REQUIRED");
  contextSelectionSnapshot(registry);
  verifyContextPin(selection.root, registrySource);
  verifyContextPin(selection.root, selection.provenance);
  verifyContextPin(selection.root, {
    path: selection.checkpointPath,
    sha256: selection.checkpointSha256,
  });
}
function resolveSessionContext(
  input: HookInput,
  registry: HookBindings,
): {
  binding?: HookBinding;
  selection?: LegacySessionContextSelection;
} {
  const snapshot = contextSelectionSnapshot(registry);
  if (snapshot.contextSelections === undefined)
    return { binding: resolveHookBinding(input.cwd, registry) };
  contextSelections(snapshot.contextSelections);
  const matches = snapshot.contextSelections.filter(
    (row): row is LegacySessionContextSelection =>
      row.schema === "codex.session-context-selection.v1" &&
      row.nativeSessionId === input.session_id &&
      row.nativeAgentId === input.agent_id &&
      row.nativeAgentType === input.agent_type,
  );
  const binding = resolveHookBinding(input.cwd, snapshot);
  const gitSelection = binding
    ? resolveSessionGitWorkSelection(input, registry, binding)
    : undefined;
  if (gitSelection && matches.length) throw Error("CONTEXT_SELECTION_AMBIGUOUS");
  if (!matches.length) return { binding };
  if (matches.length !== 1) throw Error("CONTEXT_SELECTION_AMBIGUOUS");
  const selection = structuredClone(matches[0]!);
  const owner = resolveHookBinding(input.cwd, snapshot);
  if (
    !owner ||
    owner.projectId !== selection.projectId ||
    owner.root !== selection.root ||
    owner.qualificationLedgerPath ||
    owner.entrypointCwds?.includes(input.cwd) ||
    selection.cwd !== input.cwd ||
    realpathSync(selection.cwd) !== selection.cwd ||
    realpathSync(selection.root) !== selection.root ||
    !within(selection.root, selection.cwd)
  )
    throw Error("CONTEXT_SELECTION_SCOPE_MISMATCH");
  verifyContextSelection(selection, registry);
  return {
    selection,
    binding: {
      projectId: selection.projectId,
      root: selection.root,
      workId: selection.workId,
      checkpointPath: selection.checkpointPath,
    },
  };
}

function contextSelections(
  value: unknown,
): (SessionGitWorkSelection | LegacySessionContextSelection)[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.length > 128)
    throw Error("Invalid bounded session selections");
  const seen = new Set<string>();
  const boundedId = (value: unknown): value is string =>
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 1024 &&
    !/[\u0000-\u001f]/.test(value);
  const exactPath = (value: unknown): value is string =>
    boundedId(value) && isAbsolute(value) && resolve(value) === value;
  for (const entry of value) {
    if (object(entry) && entry.schema === "codex.session-context-selection.v1") {
      validateContextSelections([entry]);
      const tuple = JSON.stringify([
        entry.nativeSessionId,
        entry.nativeAgentId ?? null,
        entry.nativeAgentType ?? null,
        entry.cwd,
      ]);
      if (seen.has(tuple)) throw Error("Duplicate native session/agent/type/cwd selection");
      seen.add(tuple);
      continue;
    }
    if (
      !object(entry) ||
      !boundedId(entry.nativeSessionId) ||
      !exactPath(entry.cwd) ||
      (entry.nativeAgentId !== undefined && !boundedId(entry.nativeAgentId)) ||
      (entry.nativeAgentType !== undefined &&
        (!boundedId(entry.nativeAgentType) || entry.nativeAgentId === undefined))
    )
      throw Error("Invalid exact native selection tuple");
    const tuple = JSON.stringify([
      entry.nativeSessionId,
      entry.nativeAgentId ?? null,
      entry.nativeAgentType ?? null,
      entry.cwd,
    ]);
    if (seen.has(tuple)) throw Error("Duplicate native session/agent/cwd selection");
    seen.add(tuple);
    if (
      entry.schema !== "codex.session-git-work-selection.v1" ||
      Object.keys(entry).some(
        (key) =>
          ![
            "schema",
            "nativeSessionId",
            "nativeAgentId",
            "nativeAgentType",
            "cwd",
            "ownerRoot",
            "projectId",
            "workId",
            "snapshot",
            "taskId",
            "decisionId",
            "latestRelevant",
          ].includes(key),
      ) ||
      !exactPath(entry.ownerRoot) ||
      !boundedId(entry.projectId) ||
      !boundedId(entry.workId) ||
      (entry.taskId !== undefined && !boundedId(entry.taskId)) ||
      (entry.decisionId !== undefined && !boundedId(entry.decisionId)) ||
      (entry.latestRelevant !== undefined &&
        !["current", "advanced", "unknown", "unavailable"].includes(
          String(entry.latestRelevant),
        )) ||
      !object(entry.snapshot)
    )
      throw Error("Invalid typed Git Work session selection");
    const snapshot = entry.snapshot;
    if (
      Object.keys(snapshot).some(
        (key) =>
          ![
            "repositoryRoot",
            "repositoryId",
            "commit",
            "projectId",
            "workId",
            "repositories",
          ].includes(key),
      ) ||
      !exactPath(snapshot.repositoryRoot) ||
      !boundedId(snapshot.repositoryId) ||
      snapshot.projectId !== entry.projectId ||
      snapshot.workId !== entry.workId ||
      typeof snapshot.commit !== "string" ||
      !/^(?:[a-f0-9]{40}|[a-f0-9]{64}|refs\/heads\/[A-Za-z0-9][A-Za-z0-9._/-]{0,240})$/.test(
        snapshot.commit,
      ) ||
      (snapshot.commit.startsWith("refs/heads/") &&
        (snapshot.commit.includes("..") ||
          snapshot.commit.includes("//") ||
          snapshot.commit.endsWith("/") ||
          snapshot.commit.endsWith("."))) ||
      (snapshot.repositories !== undefined &&
        (!object(snapshot.repositories) ||
          Object.keys(snapshot.repositories).length > 16 ||
          Object.entries(snapshot.repositories).some(
            ([id, root]) => !boundedId(id) || !exactPath(root),
          )))
    )
      throw Error(
        "Git Work selection requires exact local roots and an explicit immutable commit or branch",
      );
  }
  return value as (SessionGitWorkSelection | LegacySessionContextSelection)[];
}

/** Absence of agent identity is an exact parent tuple, never a child wildcard. */
export function resolveSessionGitWorkSelection(
  input: HookInput,
  registry: HookBindings,
  binding: HookBinding,
): SessionGitWorkSelection | undefined {
  const selected = contextSelections(contextSelectionSnapshot(registry).contextSelections).find(
    (entry) =>
      entry.schema === "codex.session-git-work-selection.v1" &&
      entry.nativeSessionId === input.session_id &&
      entry.nativeAgentId === input.agent_id &&
      entry.nativeAgentType === input.agent_type &&
      entry.cwd === input.cwd,
  );
  if (!selected || selected.schema !== "codex.session-git-work-selection.v1") return undefined;
  const selection = selected as SessionGitWorkSelection;
  if (
    selection.ownerRoot !== binding.root ||
    selection.projectId !== binding.projectId ||
    realpathSync(input.cwd) !== input.cwd ||
    realpathSync(selection.ownerRoot) !== selection.ownerRoot ||
    realpathSync(selection.snapshot.repositoryRoot) !== selection.snapshot.repositoryRoot
  )
    throw Error("Selected session Work differs from the exact native owner/cwd");
  verifyGitSelection(selection, registry);
  return selection;
}
function verifyGitSelection(selection: SessionGitWorkSelection, registry: HookBindings): void {
  const source = bindingSources.get(registry);
  if (!source) throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_REQUIRED");
  contextSelectionSnapshot(registry);
  verifyContextPin(selection.ownerRoot, source);
}
function verifySelectedGitRegistry(
  input: HookInput,
  registry: HookBindings,
  binding: HookBinding,
): void {
  const selection = resolveSessionGitWorkSelection(input, registry, binding);
  if (selection) verifyGitSelection(selection, registry);
}

async function selectedGitWork(
  input: HookInput,
  registry: HookBindings,
  binding: HookBinding,
  provider: Awaited<ReturnType<NativeContextLoader>>,
) {
  const selection = resolveSessionGitWorkSelection(input, registry, binding);
  if (!selection) return undefined;
  if (
    !provider.gitWork ||
    typeof provider.gitWork.compactGitWorkContext !== "function" ||
    typeof provider.gitWork.decodeCompactGitWorkContext !== "function"
  )
    throw Error("Selected Git Work requires the proven code-owner Git context provider");
  await provider.verifyCurrentSourceClosure();
  try {
    const snapshot = await provider.gitWork.resolveGitWorkSelection(selection.snapshot);
    const reader = await provider.gitWork.openGitWork(snapshot);
    const context = await provider.gitWork.readGitWorkContext(reader, {
      ...(selection.taskId ? { taskId: selection.taskId } : {}),
      ...(selection.decisionId ? { decisionId: selection.decisionId } : {}),
      latestRelevant: selection.latestRelevant ?? "unknown",
      maxChars: 24000,
      maxRecords: 256,
    });
    const task = selection.taskId
      ? await reader.readRecord({ kind: "task", id: selection.taskId }).catch(() => undefined)
      : undefined;
    const view = projectSelectedGitWork(
      context,
      {
        taskId: selection.taskId,
        decisionIds:
          task?.kind === "task"
            ? [
                ...new Set([
                  ...task.decisionIds,
                  ...(selection.decisionId ? [selection.decisionId] : []),
                ]),
              ]
            : selection.decisionId
              ? [selection.decisionId]
              : undefined,
      },
      provider.gitWork,
      binding.gitWorkContextVersion ?? "v1",
    );
    // A moving local branch is observed twice; neither observation proves remote currentness.
    const after = await provider.gitWork.resolveGitWorkSelection(selection.snapshot);
    if (after.commit !== snapshot.commit) {
      view.status = "held";
      view.holdReasons.push("SELECTED_LOCAL_REF_ADVANCED_DURING_READ");
      if ("semanticContext" in view) {
        view.semanticContext.status = "held";
        view.semanticContext.holdReasons.push("SELECTED_LOCAL_REF_ADVANCED_DURING_READ");
      }
    }
    await provider.verifyCurrentSourceClosure();
    verifyGitSelection(selection, registry);
    return view;
  } catch {
    throw Error("Selected Git Work source unavailable or invalid; no Work context was inferred");
  }
}

function readBindingSource(path: string): Buffer {
  if (!isAbsolute(path) || resolve(path) !== path || realpathSync(path) !== path)
    throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_UNSAFE");
  const before = lstatSync(path);
  if (!before.isFile() || before.isSymbolicLink() || before.size > 2_097_152)
    throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_UNSAFE");
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const opened = fstatSync(fd);
    if (
      !opened.isFile() ||
      opened.dev !== before.dev ||
      opened.ino !== before.ino ||
      opened.size > 2_097_152
    )
      throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_UNSAFE");
    const buffer = Buffer.alloc(opened.size + 1);
    let count = 0;
    while (count < buffer.length) {
      const next = readSync(fd, buffer, count, buffer.length - count, null);
      if (!next) break;
      count += next;
    }
    const after = fstatSync(fd),
      named = lstatSync(path);
    if (
      count !== opened.size ||
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      named.dev !== opened.dev ||
      named.ino !== opened.ino ||
      named.isSymbolicLink() ||
      realpathSync(path) !== path
    )
      throw Error("CONTEXT_SELECTION_REGISTRY_SOURCE_DRIFT");
    return buffer.subarray(0, count);
  } finally {
    closeSync(fd);
  }
}

export function readHookBindings(path: string): HookBindings {
  const raw = readBindingSource(path);
  const value: unknown = JSON.parse(raw.toString("utf8"));
  if (!object(value) || value.schemaVersion !== "1.0.0" || !Array.isArray(value.bindings))
    throw new Error("Invalid native hook bindings");
  const seen = new Set<string>();
  for (const binding of value.bindings) {
    if (
      !object(binding) ||
      typeof binding.projectId !== "string" ||
      !binding.projectId ||
      typeof binding.root !== "string" ||
      !isAbsolute(binding.root) ||
      typeof binding.checkpointPath !== "string" ||
      !isAbsolute(binding.checkpointPath) ||
      (binding.workId !== undefined && typeof binding.workId !== "string") ||
      (binding.gitWorkContextVersion !== undefined &&
        binding.gitWorkContextVersion !== "v1" &&
        binding.gitWorkContextVersion !== "v2") ||
      (binding.allowedCwds !== undefined &&
        (!Array.isArray(binding.allowedCwds) ||
          !binding.allowedCwds.every((p) => typeof p === "string" && isAbsolute(p)))) ||
      (binding.qualificationLedgerPath !== undefined &&
        (typeof binding.qualificationLedgerPath !== "string" ||
          !isAbsolute(binding.qualificationLedgerPath))) ||
      (binding.childReservationTokens !== undefined &&
        (typeof binding.childReservationTokens !== "number" ||
          !Number.isSafeInteger(binding.childReservationTokens) ||
          binding.childReservationTokens < 0))
    )
      throw new Error("Invalid project hook binding");
    if (
      (binding.entrypointCwds !== undefined &&
        (!Array.isArray(binding.entrypointCwds) ||
          !binding.entrypointCwds.length ||
          !binding.entrypointCwds.every(
            (p) => typeof p === "string" && isAbsolute(p) && resolve(p) === p,
          ))) ||
      (binding.entrypointContextPath !== undefined &&
        (typeof binding.entrypointContextPath !== "string" ||
          !isAbsolute(binding.entrypointContextPath))) ||
      (binding.entrypointCwds !== undefined) !== (binding.entrypointContextPath !== undefined) ||
      (binding.entrypointCwds !== undefined && binding.qualificationLedgerPath !== undefined)
    )
      throw new Error("Invalid exact entrypoint binding; qualification aliases are prohibited");
    const entrypoints = binding.entrypointCwds,
      subtrees = binding.allowedCwds;
    if (
      Array.isArray(entrypoints) &&
      Array.isArray(subtrees) &&
      entrypoints.some((entry) =>
        subtrees.some((path) => within(resolve(path as string), entry as string)),
      )
    )
      throw new Error("Exact entrypoint cannot also admit its parent subtree");
    const identity = `${binding.projectId}\0${binding.root}\0${binding.workId ?? ""}`;
    if (seen.has(identity))
      throw new Error(`Duplicate exact project/work hook binding: ${binding.projectId}`);
    seen.add(identity);
  }
  contextSelections(value.contextSelections);
  const result = value as unknown as HookBindings;
  bindingSources.set(result, {
    path,
    sha256: hashBytes(raw),
    snapshot: freezeSnapshot(structuredClone(result)),
  });
  return result;
}

export function resolveHookBinding(cwd: string, registry: HookBindings): HookBinding | undefined {
  const current = realpathSync(cwd);
  const matching = registry.bindings
    .flatMap((binding) => {
      const exact = (binding.entrypointCwds ?? []).filter(
        (path) =>
          path === cwd && resolve(path) === path && realpathSync(path) === path && current === path,
      );
      if (exact.length && (binding.qualificationLedgerPath || !binding.entrypointContextPath))
        throw new Error(
          "Exact bootstrap entrypoints cannot alias qualification or omit their context contract",
        );
      const declared = [binding.root, ...(binding.allowedCwds ?? [])].filter((path) =>
        within(resolve(path), current),
      );
      if (!declared.length && !exact.length) return [];
      const matched = declared
        .map((path) => realpathSync(path))
        .filter((path) => within(path, current));
      if (!matched.length && !exact.length) return [];
      const root = realpathSync(binding.root);
      const checkpoint = exact.length
        ? resolve(binding.checkpointPath)
        : realpathSync(binding.checkpointPath);
      if (!within(root, checkpoint))
        throw new Error(`Checkpoint escapes source owner: ${binding.projectId}`);
      if (
        exact.length &&
        (root !== binding.root ||
          checkpoint !== binding.checkpointPath ||
          (existsSync(checkpoint) && realpathSync(checkpoint) !== checkpoint) ||
          within(root, current) ||
          binding.allowedCwds?.some((path) => within(resolve(path), current)))
      )
        throw new Error(
          "Exact bootstrap entrypoint requires a distinct canonical project owner and no parent subtree alias",
        );
      return [...matched, ...exact].map((path) => ({
        binding: { ...binding, root, checkpointPath: checkpoint },
        path,
      }));
    })
    .sort((a, b) => b.path.length - a.path.length);
  const specificity = matching[0]?.path.length;
  const identities = new Set(
    matching
      .filter((match) => match.path.length === specificity)
      .map(({ binding }) =>
        JSON.stringify([
          binding.projectId,
          binding.root,
          binding.workId ?? null,
          binding.checkpointPath,
          binding.qualificationLedgerPath ?? null,
          binding.childReservationTokens ?? null,
          binding.entrypointContextPath ?? null,
          binding.gitWorkContextVersion ?? "v1",
        ]),
      ),
  );
  if (identities.size > 1) throw new Error("Ambiguous native hook owner");
  return matching[0]?.binding;
}

function privateDirectory(root: string): string {
  // Existing .private symlinks must not redirect raw prompts outside the admitted owner.
  const parent = resolve(root, ".private");
  if (existsSync(parent) && lstatSync(parent).isSymbolicLink())
    throw new Error("Unsafe private directory");
  mkdirSync(parent, { recursive: true, mode: 0o700 });
  const path = resolve(parent, "native-hook-events");
  if (existsSync(path) && lstatSync(path).isSymbolicLink())
    throw new Error("Unsafe native event directory");
  mkdirSync(path, { recursive: true, mode: 0o700 });
  return path;
}
function latestPath(root: string, session: string) {
  return resolve(privateDirectory(root), `${safeId(session)}.latest-prompt.json`);
}
export function latestPendingPrompt(root: string, session: string): PromptLocator | undefined {
  const path = latestPath(root, session);
  if (!existsSync(path)) return;
  if (lstatSync(path).isSymbolicLink()) throw new Error("Unsafe prompt locator");
  const value: unknown = JSON.parse(readFileSync(path, "utf8"));
  if (
    !object(value) ||
    typeof value.path !== "string" ||
    typeof value.sha256 !== "string" ||
    typeof value.bytes !== "number" ||
    typeof value.atUtc !== "string" ||
    value.threadId !== session
  )
    throw new Error("Invalid prompt locator");
  const prompt = realpathSync(value.path);
  if (!within(privateDirectory(root), prompt) || lstatSync(value.path).isSymbolicLink())
    throw new Error("Unsafe prompt location");
  const bytes = readFileSync(prompt);
  if (bytes.length !== value.bytes || hashBytes(bytes) !== value.sha256)
    throw new Error("Prompt hash mismatch");
  return value as unknown as PromptLocator;
}
function preservePrompt(binding: HookBinding, input: HookInput, atUtc: string): PromptLocator {
  const raw = input.prompt!;
  const location = resolve(
    privateDirectory(binding.root),
    `${safeId(input.session_id)}-${randomUUID()}.prompt.txt`,
  );
  writeFileSync(location, raw, { flag: "wx", mode: 0o600 });
  const locator: PromptLocator = {
    path: location,
    sha256: hashBytes(raw),
    bytes: Buffer.byteLength(raw),
    atUtc,
    threadId: input.session_id,
    ...(input.turn_id ? { turnId: input.turn_id } : {}),
  };
  const target = latestPath(binding.root, input.session_id);
  if (existsSync(target) && lstatSync(target).isSymbolicLink())
    throw new Error("Unsafe latest prompt target");
  const temp = `${target}.${randomUUID()}.tmp`;
  writeFileSync(temp, json(locator), { flag: "wx", mode: 0o600 });
  renameSync(temp, target);
  return locator;
}

export const qualificationChildTools = [
  "Agent",
  "spawn_agent",
  "send_input",
  "followup_task",
  "resume_agent",
] as const;
function denyBudget(reason: string): HookResult {
  return {
    status: "budget-denied",
    stdout: JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: `Native qualification budget: ${reason.slice(0, 500)}`,
      },
    }),
    exitCode: 0,
    error: reason,
  };
}
async function gateQualificationChild(
  binding: HookBinding,
  input: HookInput,
  atUtc: string,
): Promise<HookResult> {
  if (!binding.qualificationLedgerPath) return { status: "no-op", stdout: "", exitCode: 0 };
  if (
    !qualificationChildTools.includes(input.tool_name as (typeof qualificationChildTools)[number])
  )
    return { status: "no-op", stdout: "", exitCode: 0 };
  if (!input.tool_use_id || !input.turn_id || !object(input.tool_input))
    throw new Error("Native child invocation identity or arguments are unavailable");
  const isFollowup = input.tool_name === "send_input" || input.tool_name === "followup_task";
  const isResume = input.tool_name === "resume_agent";
  const message = input.tool_input.message ?? input.tool_input.prompt ?? input.tool_input.task;
  if (!isResume && (typeof message !== "string" || !message.trim()))
    throw new Error("Native child invocation shape is unsupported");
  const target = input.tool_input.id ?? input.tool_input.target ?? input.tool_input.agent_id;
  if ((isFollowup || isResume) && (typeof target !== "string" || !target))
    throw new Error("Native child follow-up target is unavailable");
  const ledgerPath = realpathSync(binding.qualificationLedgerPath);
  if (!within(binding.root, ledgerPath) || ledgerPath !== binding.qualificationLedgerPath)
    throw new Error("Qualification ledger is not an exact source-owned file");
  const argumentHash = hashBytes(JSON.stringify(input.tool_input));
  const id = `native-tool-${hashBytes(`${input.session_id}\0${input.tool_use_id}`).slice(0, 40)}`;
  await updateEvaluationLedger(ledgerPath, (ledger) => {
    if (ledger.projectId !== binding.projectId)
      throw new Error("Qualification ledger belongs to a different project");
    const parent = ledger.requests.findLast(
      (request) =>
        request.threadId === input.session_id &&
        request.status === "dispatched" &&
        (request.turnId === undefined || request.turnId === input.turn_id),
    );
    if (!parent)
      throw new Error("Native parent turn has not been reserved and dispatched by the suite owner");
    const matchesPlannedTarget = (request: typeof parent) =>
      isFollowup || isResume
        ? !!request.plannedNativeChildTargetThreadId &&
          (target === request.plannedNativeChildTargetThreadId ||
            target === request.plannedNativeChildTargetPath) &&
          ledger.threads.some(
            (thread) =>
              thread.threadId === request.plannedNativeChildTargetThreadId &&
              thread.parentThreadId === parent.threadId,
          )
        : request.plannedNativeChildTargetThreadId === undefined;
    if (
      (isFollowup || isResume) &&
      !ledger.threads.some(
        (thread) => thread.threadId === target && thread.parentThreadId === parent.threadId,
      ) &&
      !ledger.requests.some(
        (request) =>
          request.plannedNativeChild &&
          request.parentRequestId === parent.id &&
          request.nativeParentThreadId === input.session_id &&
          request.status === "reserved" &&
          matchesPlannedTarget(request),
      )
    )
      throw new Error(
        "Native child follow-up is outside the exact registered suite lineage and planned target",
      );
    const existing = ledger.requests.filter(
      (request) => request.nativeToolUseId === input.tool_use_id,
    );
    if (existing.length) {
      const request = existing[0]!;
      if (
        existing.length !== 1 ||
        !request.plannedNativeChild ||
        request.kind !== "child" ||
        request.status !== "reserved" ||
        request.parentRequestId !== parent.id ||
        request.nativeParentThreadId !== input.session_id ||
        !matchesPlannedTarget(request) ||
        (request.hookInputSha256 !== undefined && request.hookInputSha256 !== argumentHash)
      )
        throw new Error(
          "Native tool invocation already reserved with a different parent, target, arguments or observed effect; inspect before replay",
        );
      const guard = dispatchGuard(ledger, { now: new Date(atUtc), nativeHooksTrusted: true });
      if (!guard.allowed) throw new Error(guard.reasons.join("; "));
      return attachPlannedNativeChildTool(ledger, {
        requestId: request.id,
        nativeToolUseId: input.tool_use_id!,
        hookInputSha256: argumentHash,
        evidenceRef: `native:PreToolUse:${input.tool_use_id}`,
      });
    }
    if (ledger.requests.some((request) => request.id === id))
      throw new Error(
        "Native tool invocation reservation identity is already occupied; inspect before replay",
      );
    {
      const pendingPlan = ledger.requests.filter(
        (request) =>
          request.plannedNativeChild &&
          request.parentRequestId === parent.id &&
          request.nativeParentThreadId === input.session_id &&
          request.status === "reserved" &&
          !request.nativeToolUseId,
      );
      const planned = pendingPlan.filter(matchesPlannedTarget);
      if (pendingPlan.length && !planned.length)
        throw new Error(
          "Native child invocation differs from its exact planned operation or target",
        );
      if (planned.length > 1) throw new Error("Ambiguous planned native child reservations");
      if (planned[0]) {
        const guard = dispatchGuard(ledger, { now: new Date(atUtc), nativeHooksTrusted: true });
        if (!guard.allowed) throw new Error(guard.reasons.join("; "));
        return attachPlannedNativeChildTool(ledger, {
          requestId: planned[0].id,
          nativeToolUseId: input.tool_use_id!,
          hookInputSha256: argumentHash,
          evidenceRef: `native:PreToolUse:${input.tool_use_id}`,
        });
      }
    }
    const reservation = reserveEvaluationRequest(ledger, {
      kind: "child",
      reservationTokens: binding.childReservationTokens ?? 50_000,
      parentRequestId: parent.id,
      nativeHooksTrusted: true,
      now: new Date(atUtc),
      id,
    });
    Object.assign(reservation.request, {
      nativeToolUseId: input.tool_use_id,
      hookInputSha256: argumentHash,
      nativeParentThreadId: input.session_id,
      evidenceRef: `native:PreToolUse:${input.tool_use_id}`,
    });
    return reservation.ledger;
  });
  return { status: "budget-reserved", stdout: "{}", exitCode: 0, projectId: binding.projectId };
}

export async function handleHook(
  value: unknown,
  options: { bindings: HookBindings; now?: () => Date; maxChars?: number },
  loadCodexOwnerContext: NativeContextLoader,
): Promise<HookResult> {
  let gateActive = false;
  let contextSelectionExpected = false;
  try {
    const input = parseHookInput(value);
    contextSelectionExpected =
      input.hook_event_name !== "PreToolUse" && selectedContextExpected(input, options.bindings);
    const chosen =
      input.hook_event_name === "PreToolUse"
        ? { binding: resolveHookBinding(input.cwd, options.bindings), selection: undefined }
        : resolveSessionContext(input, options.bindings);
    const { binding, selection } = chosen;
    if (!binding) return { status: "no-op", stdout: "", exitCode: 0 };
    if (binding.entrypointCwds?.includes(input.cwd)) {
      // Home is a context entrypoint, not a domain-work capture or evaluation alias.
      if (input.hook_event_name !== "SessionStart" && input.hook_event_name !== "UserPromptSubmit")
        return { status: "no-op", stdout: "", exitCode: 0 };
      const provider = await loadCodexOwnerContext();
      await provider.verifyCurrentSourceClosure();
      const projection = projectHomeContext({
        binding,
        cwd: input.cwd,
        maxChars: options.maxChars,
        selectedWork: await selectedGitWork(input, options.bindings, binding, provider),
      });
      await provider.verifyCurrentSourceClosure();
      verifySelectedGitRegistry(input, options.bindings, binding);
      return {
        status:
          projection.status === "current"
            ? "context-injected"
            : projection.status === "held"
              ? "context-held"
              : "failed-open",
        stdout: JSON.stringify({
          hookSpecificOutput: {
            hookEventName: input.hook_event_name,
            additionalContext: projection.text,
          },
        }),
        exitCode: 0,
        projectId: binding.projectId,
        ...(projection.status !== "current"
          ? {
              error:
                projection.status === "held"
                  ? "Selected Git Work context held"
                  : "Home context source requires repair",
            }
          : {}),
      };
    }
    const atUtc = (options.now ?? (() => new Date()))().toISOString();
    if (input.hook_event_name === "PreToolUse") {
      gateActive = Boolean(
        binding.qualificationLedgerPath &&
        qualificationChildTools.includes(
          input.tool_name as (typeof qualificationChildTools)[number],
        ),
      );
      return await gateQualificationChild(binding, input, atUtc);
    }
    const pending =
      input.hook_event_name === "UserPromptSubmit"
        ? preservePrompt(binding, input, atUtc)
        : latestPendingPrompt(binding.root, input.session_id);
    const provider = await loadCodexOwnerContext();
    await provider.verifyCurrentSourceClosure();
    const { boundedProjection, readCheckpoint, recordBoundary } = provider.index;
    const boundary = await recordBoundary(binding.root, {
      event: input.hook_event_name,
      atUtc,
      threadId: input.session_id,
      ...(input.turn_id ? { turnId: input.turn_id } : {}),
      checkpointPath: binding.checkpointPath,
      ...(pending
        ? { pendingPrompt: { path: pending.path, sha256: pending.sha256, bytes: pending.bytes } }
        : {}),
      ...(input.source ? { nativeSource: input.source } : {}),
      ...(input.trigger ? { trigger: input.trigger } : {}),
      ...(input.agent_id ? { nativeAgentId: input.agent_id } : {}),
      ...(input.agent_type ? { nativeAgentType: input.agent_type } : {}),
      ...((input.agent_transcript_path ?? input.transcript_path)
        ? { nativeTranscriptPath: (input.agent_transcript_path ?? input.transcript_path)! }
        : {}),
    });
    await provider.verifyCurrentSourceClosure();
    // Lifecycle records preserve native identity and locators. They never mark work accepted or effects completed.
    if (input.hook_event_name !== "SessionStart" && input.hook_event_name !== "UserPromptSubmit")
      return {
        status: "recorded",
        stdout: "{}",
        exitCode: 0,
        projectId: binding.projectId,
        boundary,
      };
    const selected = await selectedGitWork(input, options.bindings, binding, provider);
    if (selected) {
      const deliveryPolicy = gitWorkDeliveryPolicy(binding, options.maxChars);
      const text = JSON.stringify({
        schema: "codex.session-git-work.projection.v1",
        deliveryPolicy,
        sourceOwnerProjectRoot: binding.root,
        actualNativeCwd: input.cwd,
        selectedWork: selected,
        authority: "none",
        nativePermissionGranted: false,
      });
      const { maxChars } = deliveryPolicy;
      const held = selected.status === "held" || text.length > maxChars;
      let additionalContext =
        text.length <= maxChars
          ? text
          : JSON.stringify({
              schema: "codex.session-git-work.projection.v1",
              deliveryPolicy,
              requiredContextChars: text.length,
              selectedWork: {
                ...selectedGitWorkPointer(selected),
                status: "held",
                holdReasons: ["NATIVE_CONTEXT_OUTPUT_LIMIT"],
                authority: "none",
                nativePermissionGranted: false,
              },
            });
      if (additionalContext.length > maxChars)
        additionalContext = JSON.stringify({
          schema: "codex.session-git-work.projection.v1",
          deliveryPolicy,
          status: "held",
          holdReasons: ["NATIVE_CONTEXT_OUTPUT_LIMIT"],
          readPolicy: "Read the exact typed session binding and immutable Git Work source.",
          authority: "none",
          nativePermissionGranted: false,
          semanticAcceptanceEstablished: false,
        });
      verifySelectedGitRegistry(input, options.bindings, binding);
      return {
        status: held ? "context-held" : "context-injected",
        stdout: JSON.stringify({
          hookSpecificOutput: { hookEventName: input.hook_event_name, additionalContext },
        }),
        exitCode: 0,
        projectId: binding.projectId,
        boundary,
      };
    }
    const envelope = await readCheckpoint(binding.checkpointPath, {
      root: binding.root,
      verifySources: true,
    });
    if (
      selection &&
      (envelope.sha256 !== selection.checkpointSha256 || envelope.sourceStatus.status !== "current")
    )
      throw Error("CONTEXT_SELECTION_CHECKPOINT_NOT_CURRENT");
    if (
      envelope.checkpoint.projectId !== binding.projectId ||
      (binding.workId !== undefined && envelope.checkpoint.workId !== binding.workId)
    )
      throw new Error("Checkpoint differs from the selected project/work binding");
    const maxChars = Math.min(Math.max(options.maxChars ?? 6000, 2000), 6000);
    const notice = `Source-owned continuity context for ${binding.projectId}${binding.workId ? ` / ${binding.workId}` : ""}. This context does not grant execution authority. Inspect source drift before relying on historical effects.\n`;
    const steering = pending
      ? `\nLatest native user input remains uninterpreted unless its hash appears in the owner's actual decision/checkpoint evidence. Before continuing, compare it with the checkpoint and read only if missing: ${pending.path} (sha256 ${pending.sha256}, ${pending.bytes} bytes). A captured user prompt is not an accepted decision.\n`
      : "";
    const projectionBudget = Math.max(1024, maxChars - notice.length - steering.length - 100);
    let projectionText: string;
    try {
      // Preserve the established complete small-Work projection when it fits.
      projectionText = boundedProjection(envelope.checkpoint, {
        maxChars: projectionBudget,
        sourceStatus: envelope.sourceStatus,
      }).text;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (
        message !==
          "Context budget cannot preserve objective and next action; read the source checkpoint or request a larger budget" &&
        message !== "Context budget cannot preserve required omission metadata"
      )
        throw error;
      // Grown histories use an existing source-owned page with explicit omissions.
      const section = await provider.sections.readWorkContextSection(
        {
          schema: "codex.work-context.request.v1",
          projectId: binding.projectId,
          workId: envelope.checkpoint.workId,
          root: binding.root,
          cwd: binding.root,
          checkpointPath: binding.checkpointPath,
          checkpointSha256: envelope.sha256,
          maxChars: Math.max(1024, maxChars - notice.length - steering.length - 700),
        },
        { section: "pending", offset: 0, limit: 1 },
      );
      projectionText = JSON.stringify({
        ...section,
        omitted: {
          pending: section.omittedAfter,
          completed: envelope.checkpoint.completed.length,
          userDecisions: envelope.checkpoint.userDecisions.length,
          sourcePins: envelope.checkpoint.sourcePins.length,
        },
      });
    }
    const context = `${notice}${projectionText}${steering}`;
    if (context.length > maxChars)
      throw new Error("Continuity context exceeded its bounded budget");
    await provider.verifyCurrentSourceClosure();
    if (selection) {
      verifyContextSelection(selection, options.bindings);
      const final = await readCheckpoint(binding.checkpointPath, {
        root: binding.root,
        verifySources: true,
      });
      if (final.sha256 !== selection.checkpointSha256 || final.sourceStatus.status !== "current")
        throw Error("CONTEXT_SELECTION_CHECKPOINT_NOT_CURRENT");
      await provider.verifyCurrentSourceClosure();
      for (const pin of final.checkpoint.sourcePins) verifyContextPin(selection.root, pin);
      verifyContextSelection(selection, options.bindings);
    }
    return {
      status: "context-injected",
      stdout: JSON.stringify({
        hookSpecificOutput: { hookEventName: input.hook_event_name, additionalContext: context },
      }),
      exitCode: 0,
      projectId: binding.projectId,
      boundary,
    };
  } catch (error) {
    if (gateActive) return denyBudget(error instanceof Error ? error.message : String(error));
    if (contextSelectionExpected)
      return {
        status: "context-held",
        exitCode: 0,
        stdout: JSON.stringify({
          systemMessage:
            "CURRENT_SESSION_CONTEXT_HELD: selected Work context could not be verified; inspect source evidence before dependent work.",
        }),
        error: error instanceof Error ? error.message : String(error),
      };
    // A continuity failure cannot block a user turn, an interruption, or create a Stop continuation.
    return {
      status: "failed-open",
      stdout: JSON.stringify({
        systemMessage: `Codex continuity hook could not load context: ${(error instanceof Error ? error.message : String(error)).slice(0, 240)}`,
      }),
      exitCode: 0,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
