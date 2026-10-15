import { isAbsolute, resolve } from "node:path";
import {
  assertLogicalIdentity,
  openGitWork,
  readGitWorkContext,
  resolveGitWorkSelection,
  type ContextSourceProjection,
  type DecisionRecord,
  type GitWorkContext,
  type GitWorkSelection,
  type LogicalIdentity,
  type TaskRecord,
} from "@palantirkc/work-continuity/git-work";
import {
  selectRuntimeReference,
  type NativeReferenceScope,
} from "@palantirkc/work-continuity/runtime-index";

/** These are caller-supplied native references, never generated native identities. */
export interface HermesCurrentWorkEvent {
  runtime: "native-hermes";
  profileId: string;
  sessionId: string;
  agentId: string | null;
  nativeProjectId: string | null;
  nativeTaskId: string | null;
  nativeRunId: string | null;
}
export interface HermesCurrentWorkSelection {
  schema: "hermes.current-git-work-selection.v1";
  native: HermesCurrentWorkEvent;
  logical: LogicalIdentity;
  snapshot: GitWorkSelection;
  latestRelevant: "current" | "advanced" | "unknown" | "unavailable";
}
export interface HermesCurrentWorkSource {
  path: string;
  fragment?: string;
  blobOid?: string;
  repositoryId?: string;
  commit?: string;
}
export interface HermesCurrentWorkDecision {
  id: string;
  status: DecisionRecord["status"];
  supersedes: string[];
  revisionIdentity: "immutable-record-id-and-supersession";
  record: HermesCurrentWorkSource;
  answer: HermesCurrentWorkSource | null;
  question: HermesCurrentWorkSource | null;
  rationale: HermesCurrentWorkSource;
}
export interface HermesCurrentWorkView {
  snapshot: { repositoryId: string; commit: string; sourcePathBase: string };
  logical: LogicalIdentity;
  native: HermesCurrentWorkEvent;
  purpose: { objective?: string; source: string };
  currentTask: {
    title?: string;
    state: TaskRecord["state"];
    source: string;
  };
  upstream: {
    spec: { id: string; revision: number; source: string };
    plan: { id: string; revision: number; source: string };
  };
  decisions: HermesCurrentWorkDecision[];
  governingDecisionIndex: string;
  directDependencies: (TaskRecord["dependsOn"][number] & { source: string })[];
  historicalEffects: GitWorkContext["historicalEffects"];
  omitted: { decisions: number; directDependencies: number; fields: string[] };
}
export interface HermesCurrentWorkResult {
  schema: "hermes.current-git-work-context.v1";
  status: "source-qualified" | "held";
  holdReasons: string[];
  context: HermesCurrentWorkView | null;
  projection: "bounded-derived-pointer-view";
  currentness: "caller-declared-not-remote-verified";
  verificationClass: "git-object-source-only";
  runtimeReferenceClass: "source-reference-only";
  nativeAuthorshipVerified: false;
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
export type HermesCurrentWorkGitApi = Pick<
  typeof import("@palantirkc/work-continuity/git-work"),
  "resolveGitWorkSelection" | "openGitWork" | "readGitWorkContext"
>;
export interface HermesCurrentWorkOptions {
  /** The owning loader verifies its exact source closure before and after reading. */
  verifySourceClosure: () => Promise<void>;
  gitWork?: HermesCurrentWorkGitApi;
  maxChars?: number;
}
type Row = Record<string, unknown>;
const nativeKeys = [
  "runtime",
  "profileId",
  "sessionId",
  "agentId",
  "nativeProjectId",
  "nativeTaskId",
  "nativeRunId",
] as const;
const logicalKeys = [
  "domainId",
  "projectId",
  "workId",
  "issueId",
  "specId",
  "specRevision",
  "planId",
  "planRevision",
  "taskId",
] as const;
function row(value: unknown, keys: readonly string[]): Row {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw Error("INVALID_SELECTION");
  const result = value as Row;
  if (Object.keys(result).length !== keys.length || keys.some((key) => !Object.hasOwn(result, key)))
    throw Error("INVALID_SELECTION_FIELDS");
  return result;
}
function id(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value ||
    value.trim() !== value ||
    value.length > 256 ||
    /[\u0000-\u001f]/u.test(value)
  )
    throw Error("INVALID_SELECTION_ID");
  return value;
}
function path(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length > 2048 ||
    !isAbsolute(value) ||
    resolve(value) !== value ||
    /[\u0000-\u001f]/u.test(value)
  )
    throw Error("INVALID_LOCAL_SOURCE_ROOT");
  return value;
}
function native(value: unknown): HermesCurrentWorkEvent {
  const data = row(value, nativeKeys);
  if (data.runtime !== "native-hermes") throw Error("INVALID_NATIVE_RUNTIME");
  const nullable = (value: unknown) => (value === null ? null : id(value));
  return {
    runtime: "native-hermes",
    profileId: id(data.profileId),
    sessionId: id(data.sessionId),
    agentId: nullable(data.agentId),
    nativeProjectId: nullable(data.nativeProjectId),
    nativeTaskId: nullable(data.nativeTaskId),
    nativeRunId: nullable(data.nativeRunId),
  };
}
function logical(value: unknown): LogicalIdentity {
  const data = row(value, logicalKeys);
  if (typeof data.specRevision !== "number" || typeof data.planRevision !== "number")
    throw Error("INVALID_LOGICAL_REVISION");
  const result: LogicalIdentity = {
    domainId: id(data.domainId),
    projectId: id(data.projectId),
    workId: id(data.workId),
    issueId: id(data.issueId),
    specId: id(data.specId),
    specRevision: data.specRevision,
    planId: id(data.planId),
    planRevision: data.planRevision,
    taskId: id(data.taskId),
  };
  assertLogicalIdentity(result);
  return result;
}
function selection(value: unknown): HermesCurrentWorkSelection {
  const data = row(value, ["schema", "native", "logical", "snapshot", "latestRelevant"]);
  if (data.schema !== "hermes.current-git-work-selection.v1")
    throw Error("INVALID_SELECTION_SCHEMA");
  const identity = logical(data.logical);
  if (!["current", "advanced", "unknown", "unavailable"].includes(String(data.latestRelevant)))
    throw Error("INVALID_CONTEXT_CURRENTNESS");
  if (!data.snapshot || typeof data.snapshot !== "object" || Array.isArray(data.snapshot))
    throw Error("INVALID_GIT_SELECTION");
  const snapshot = data.snapshot as Row;
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
    )
  )
    throw Error("INVALID_GIT_SELECTION_FIELDS");
  const commit = id(snapshot.commit);
  if (
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64}|refs\/heads\/[A-Za-z0-9][A-Za-z0-9._/-]{0,240})$/u.test(
      commit,
    ) ||
    (commit.startsWith("refs/heads/") &&
      (commit.includes("..") ||
        commit.includes("//") ||
        commit.endsWith("/") ||
        commit.endsWith(".")))
  )
    throw Error("INVALID_LOCAL_GIT_COMMIT_OR_REF");
  if (snapshot.projectId !== identity.projectId || snapshot.workId !== identity.workId)
    throw Error("SELECTED_GIT_LOGICAL_SCOPE_MISMATCH");
  let repositories: Record<string, string> | undefined;
  if (snapshot.repositories !== undefined) {
    if (
      !snapshot.repositories ||
      typeof snapshot.repositories !== "object" ||
      Array.isArray(snapshot.repositories) ||
      Object.keys(snapshot.repositories).length > 16
    )
      throw Error("INVALID_SOURCE_REPOSITORIES");
    repositories = Object.fromEntries(
      Object.entries(snapshot.repositories).map(([key, value]) => [id(key), path(value)]),
    );
  }
  return {
    schema: "hermes.current-git-work-selection.v1",
    native: native(data.native),
    logical: identity,
    snapshot: {
      repositoryRoot: path(snapshot.repositoryRoot),
      repositoryId: id(snapshot.repositoryId),
      commit,
      projectId: identity.projectId,
      workId: identity.workId,
      ...(repositories ? { repositories } : {}),
    },
    latestRelevant: data.latestRelevant as HermesCurrentWorkSelection["latestRelevant"],
  };
}
function result(
  reasons: string[],
  context: HermesCurrentWorkView | null = null,
): HermesCurrentWorkResult {
  return {
    schema: "hermes.current-git-work-context.v1",
    status: reasons.length ? "held" : "source-qualified",
    holdReasons: [...new Set(reasons)],
    context,
    projection: "bounded-derived-pointer-view",
    currentness: "caller-declared-not-remote-verified",
    verificationClass: "git-object-source-only",
    runtimeReferenceClass: "source-reference-only",
    nativeAuthorshipVerified: false,
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
}
const sameLogical = (a: LogicalIdentity, b: LogicalIdentity) =>
  logicalKeys.every((key) => a[key] === b[key]);
function source(value: ContextSourceProjection | undefined): HermesCurrentWorkSource | null {
  return value
    ? {
        path: value.path,
        blobOid: value.blobOid,
        ...(value.fragment ? { fragment: value.fragment } : {}),
        ...(value.repositoryId ? { repositoryId: value.repositoryId, commit: value.commit } : {}),
      }
    : null;
}
const recordSource = (kind: string, value: string) => `${kind}/${encodeURIComponent(value)}.json`;
function bounded(
  view: HermesCurrentWorkView,
  reasons: string[],
  maxChars: number,
  essentialDecisionIds: Set<string>,
): HermesCurrentWorkResult {
  const fits = () => JSON.stringify(result(reasons, view)).length <= maxChars;
  while (!fits()) {
    const optional = view.decisions.findLastIndex(
      (decision) => !essentialDecisionIds.has(decision.id),
    );
    if (optional < 0) break;
    // Every original was validated before projection. The Git governing index retains
    // omitted governing records; the selected Task's originals remain essential.
    view.decisions.splice(optional, 1);
    view.omitted.decisions++;
  }
  while (!fits() && view.directDependencies.length) {
    view.directDependencies.pop();
    view.omitted.directDependencies++;
    if (!reasons.includes("UNEXPANDED_DIRECT_DEPENDENCIES"))
      reasons.push("UNEXPANDED_DIRECT_DEPENDENCIES");
  }
  if (!fits()) {
    // Never shrink essential originals, purpose or Task text into a qualified view.
    return result(["REQUIRED_STARTUP_CONTEXT_OUTPUT_LIMIT"]);
  }
  return result(reasons, view);
}

/** Explicit local source selection only. No native store, model loop, fetch or runtime mutation. */
export async function readHermesCurrentWorkContext(
  eventValue: unknown,
  selectionValue: unknown,
  runtimeIndex: unknown,
  options: HermesCurrentWorkOptions,
): Promise<HermesCurrentWorkResult> {
  let selected: HermesCurrentWorkSelection;
  let event: HermesCurrentWorkEvent;
  const maxChars = options?.maxChars ?? 6000;
  if (!Number.isSafeInteger(maxChars) || maxChars < 1024 || maxChars > 6000)
    return result(["INVALID_STARTUP_CONTEXT_BUDGET"]);
  if (!options || typeof options.verifySourceClosure !== "function")
    return result(["SOURCE_CLOSURE_VERIFIER_REQUIRED"]);
  try {
    event = native(eventValue);
    selected = selection(selectionValue);
  } catch {
    return result(["EXPLICIT_NATIVE_WORK_SELECTION_REQUIRED"]);
  }
  if (!nativeKeys.every((key) => event[key] === selected.native[key]))
    return result(["FOREIGN_NATIVE_WORK_TUPLE"]);
  const scope: NativeReferenceScope = { ...selected.native, logical: selected.logical };
  const reference = selectRuntimeReference(runtimeIndex, scope);
  if (reference.status !== "matched") return result(reference.holdReasons);
  const api = options.gitWork ?? { resolveGitWorkSelection, openGitWork, readGitWorkContext };
  try {
    await options.verifySourceClosure();
    try {
      const snapshot = await api.resolveGitWorkSelection(selected.snapshot);
      const reader = await api.openGitWork(snapshot);
      if (
        reader.work.domainId !== selected.logical.domainId ||
        reader.work.projectId !== selected.logical.projectId ||
        reader.work.workId !== selected.logical.workId ||
        reader.work.issueId !== selected.logical.issueId ||
        reader.work.spec.id !== selected.logical.specId ||
        reader.work.spec.revision !== selected.logical.specRevision ||
        reader.work.plan.id !== selected.logical.planId ||
        reader.work.plan.revision !== selected.logical.planRevision
      )
        return result(["CURRENT_WORK_LOGICAL_CONTRACT_MISMATCH"]);
      const context = await api.readGitWorkContext(reader, {
        taskId: selected.logical.taskId,
        latestRelevant: selected.latestRelevant,
        maxChars: 24000,
        maxRecords: 256,
      });
      if (!context.records.length)
        return result(
          context.holdReasons.length
            ? safeReasons(context.holdReasons)
            : ["REQUIRED_CONTEXT_UNAVAILABLE"],
        );
      const task = await reader.readRecord({ kind: "task", id: selected.logical.taskId });
      if (task.kind !== "task" || !sameLogical(task.logical, selected.logical))
        return result(["CURRENT_TASK_LOGICAL_CONTRACT_MISMATCH"]);
      const decisions: HermesCurrentWorkDecision[] = [];
      const reasons = safeReasons(context.holdReasons);
      const projectedDecisions = context.records.filter((item) => item.kind === "decision");
      // Prioritize the current Task's decisions before governing/upstream revisions.
      projectedDecisions.sort(
        (a, b) => Number(task.decisionIds.includes(b.id)) - Number(task.decisionIds.includes(a.id)),
      );
      for (const item of projectedDecisions) {
        const record = await reader.readRecord({ kind: "decision", id: item.id });
        if (record.kind !== "decision") return result(["INVALID_DECISION_CONTEXT"]);
        const answer = source(context.sources.find((row) => row.id === item.answerSource));
        const question = source(
          context.sources.find((row) => row.id === item.selectionContextSource),
        );
        if (!answer || (record.selectionContext && !question))
          reasons.push("REQUIRED_DECISION_ORIGINAL_LOCATOR_UNAVAILABLE");
        decisions.push({
          id: record.id,
          status: record.status,
          supersedes: record.supersedes,
          revisionIdentity: "immutable-record-id-and-supersession",
          record: { path: recordSource("decisions", record.id) },
          answer,
          question,
          rationale: { path: recordSource("decisions", record.id), fragment: "#/meaning" },
        });
      }
      const view: HermesCurrentWorkView = {
        snapshot: {
          repositoryId: context.repositoryId,
          commit: context.commit,
          sourcePathBase: context.sourcePathBase,
        },
        logical: selected.logical,
        native: selected.native,
        purpose: { objective: reader.work.purpose.objective, source: "work.json#/purpose" },
        currentTask: {
          title: task.title,
          state: task.state,
          source: recordSource("tasks", task.id),
        },
        upstream: {
          spec: {
            ...reader.work.spec,
            source: `specs/${encodeURIComponent(reader.work.spec.id)}@${reader.work.spec.revision}.json`,
          },
          plan: {
            ...reader.work.plan,
            source: `plans/${encodeURIComponent(reader.work.plan.id)}@${reader.work.plan.revision}.json`,
          },
        },
        decisions,
        governingDecisionIndex: "work.json#/governingDecisionIds",
        directDependencies: task.dependsOn.map((dependency) => ({
          ...dependency,
          source: recordSource("tasks", dependency.taskId),
        })),
        historicalEffects: context.historicalEffects,
        omitted: { decisions: 0, directDependencies: 0, fields: [] },
      };
      const after = await api.resolveGitWorkSelection(selected.snapshot);
      if (after.commit !== snapshot.commit) reasons.push("SELECTED_LOCAL_REF_ADVANCED_DURING_READ");
      return bounded(view, reasons, maxChars, new Set(task.decisionIds));
    } finally {
      await options.verifySourceClosure();
    }
  } catch {
    // Parser/provider error strings may contain original inputs. Never echo them.
    return result(["SELECTED_WORK_SOURCE_UNAVAILABLE_OR_DRIFTED"]);
  }
}
function safeReasons(reasons: string[]): string[] {
  return reasons.map((reason) =>
    /^[A-Z][A-Z0-9_]{0,120}(?::|$)/u.test(reason)
      ? reason.split(":")[0]!
      : "REQUIRED_SOURCE_UNAVAILABLE",
  );
}
