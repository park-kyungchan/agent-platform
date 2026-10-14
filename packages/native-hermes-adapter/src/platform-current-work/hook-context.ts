import { isAbsolute, resolve } from "node:path";
import {
  readHermesCurrentWorkContext,
  type HermesCurrentWorkEvent,
  type HermesCurrentWorkOptions,
} from "./context.ts";

export interface HermesNativeWorkHookResponse {
  context?: string;
}
type Row = Record<string, unknown>;
const keys = [
  "runtime",
  "profileId",
  "sessionId",
  "agentId",
  "nativeProjectId",
  "nativeTaskId",
  "nativeRunId",
] as const;
function object(value: unknown): Row {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("HOOK_RECORD_REQUIRED");
  return value as Row;
}
function id(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value ||
    value.trim() !== value ||
    value.length > 256 ||
    /[\u0000-\u001f]/u.test(value)
  )
    throw Error("EXACT_NATIVE_HOOK_ID_REQUIRED");
  return value;
}
function optional(value: unknown): string | null {
  return value === undefined || value === null ? null : id(value);
}
function exactPath(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length > 2048 ||
    !isAbsolute(value) ||
    resolve(value) !== value ||
    /[\u0000-\u001f]/u.test(value)
  )
    throw Error("EXACT_NATIVE_HOOK_CWD_REQUIRED");
  return value;
}
/** Installed shell-hook protocol: profile/session/cwd are top-level; task and optional identities are extra kwargs.
 * Missing agent/project/run stay null. turn_id and logical Task IDs never fabricate those native references.
 */
export async function readHermesNativeWorkHook(
  payloadValue: unknown,
  bindingsValue: unknown,
  options: HermesCurrentWorkOptions,
): Promise<HermesNativeWorkHookResponse> {
  const payload = object(payloadValue);
  if (payload.hook_event_name !== "pre_llm_call") return {};
  const extra = payload.extra === undefined || payload.extra === null ? {} : object(payload.extra);
  const event: HermesCurrentWorkEvent = {
    runtime: "native-hermes",
    profileId: id(payload.profile),
    sessionId: id(payload.session_id),
    agentId: optional(extra.agent_id),
    nativeProjectId: optional(extra.project_id),
    nativeTaskId: optional(extra.task_id),
    nativeRunId: optional(extra.run_id),
  };
  const cwd = exactPath(payload.cwd),
    bindings = object(structuredClone(bindingsValue));
  if (
    bindings.schema !== "hermes.current-git-work-hook-bindings.v1" ||
    Object.keys(bindings).sort().join("|") !== "runtimeIndex|schema|selections" ||
    !Array.isArray(bindings.selections) ||
    bindings.selections.length > 128
  )
    throw Error("EXPLICIT_NATIVE_HOOK_BINDINGS_REQUIRED");
  const matches = bindings.selections.filter((input: unknown) => {
    const binding = object(input);
    if (Object.keys(binding).sort().join("|") !== "cwd|selection")
      throw Error("EXACT_HOOK_SELECTION_FIELDS_REQUIRED");
    const selected = object(binding.selection),
      native = object(selected.native);
    return exactPath(binding.cwd) === cwd && keys.every((key) => native[key] === event[key]);
  });
  // No parent/child wildcard, logical Task inference, stale legacy fallback, or Git read for unselected tuples.
  if (matches.length !== 1) return {};
  const selected = object(matches[0]).selection;
  await options.verifySourceClosure();
  const projection = await readHermesCurrentWorkContext(
    event,
    selected,
    bindings.runtimeIndex,
    options,
  );
  await options.verifySourceClosure();
  const context = JSON.stringify(projection);
  if (context.length > (options.maxChars ?? 6000)) throw Error("BOUNDED_NATIVE_CONTEXT_REQUIRED");
  return { context };
}
