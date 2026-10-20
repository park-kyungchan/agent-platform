import { isAbsolute, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { readExactFile } from "./provider.ts";
import { hermesWorkMethodEventIdentity } from "./work-method.ts";
import { readHermesWorkMethodSource, type WorkMethodSourceBinding } from "./work-method-source.ts";
import { currentWorkOutputPolicy } from "./output-policy.ts";
import {
  readHermesCurrentWorkContext,
  readHermesCurrentWorkContextV2,
  type HermesCurrentWorkEvent,
  type HermesCurrentWorkOptions,
} from "./context.ts";

export interface HermesNativeWorkHookResponse {
  context?: string;
}
export interface HermesNativeWorkHookOptions extends HermesCurrentWorkOptions {
  outputVersion?: "v1" | "v2";
  workMethodSourceBinding?: WorkMethodSourceBinding;
  compiledRegistrationSha256?: string;
  bindingsSha256?: string;
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
function closed(value: Row, fields: string[]) {
  if (Object.keys(value).sort().join("|") !== fields.slice().sort().join("|"))
    throw Error("EXACT_CALLBACK_SOURCE_FIELDS_REQUIRED");
}
function digest(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{64}$/u.test(value);
}
async function readRealCallbackSource(
  payload: Row,
  bindings: Row,
  options: HermesNativeWorkHookOptions,
): Promise<HermesNativeWorkHookResponse> {
  closed(bindings, ["schema", "kind", "shellSelection", "sourceSelection"]);
  const shell = object(bindings.shellSelection),
    selected = object(bindings.sourceSelection);
  closed(shell, ["profileName", "sessionId", "cwd"]);
  id(shell.profileName);
  id(shell.sessionId);
  exactPath(shell.cwd);
  closed(selected, ["scope", "logical", "actorId", "routeId"]);
  id(selected.actorId);
  id(selected.routeId);
  const scope = object(selected.scope),
    logical = object(selected.logical);
  closed(scope, ["kind", "runtime", "profileId", "sessionId"]);
  if (scope.kind !== "session-source" || scope.runtime !== "native-hermes")
    throw Error("EXACT_CALLBACK_SOURCE_SCOPE_REQUIRED");
  id(scope.profileId);
  id(scope.sessionId);
  closed(logical, ["domainId", "projectId", "workId", "taskId"]);
  for (const value of Object.values(logical)) id(value);
  if (
    bindings.kind !== "stableSessionSource" ||
    options.outputVersion !== "v2" ||
    options.metadataEncoding !== undefined ||
    !options.workMethodSourceBinding ||
    typeof options.verifySourceClosure !== "function" ||
    !digest(options.compiledRegistrationSha256) ||
    !digest(options.bindingsSha256)
  )
    throw Error("EXPLICIT_CALLBACK_SOURCE_OPT_IN_REQUIRED");
  const policy = currentWorkOutputPolicy("v2", options.maxChars);
  if (!policy.valid) throw Error("INVALID_CALLBACK_SOURCE_OUTPUT_BUDGET");
  const binding = structuredClone(options.workMethodSourceBinding);
  closed(object(binding), ["runtimeMapPath", "mapSha256", "dataBase"]);
  exactPath(binding.runtimeMapPath);
  exactPath(binding.dataBase);
  if (!binding.runtimeMapPath.startsWith(binding.dataBase + "/") || !digest(binding.mapSha256))
    throw Error("EXACT_CALLBACK_SOURCE_BINDING_REQUIRED");
  const verify = options.verifySourceClosure;
  const extra = payload.extra === undefined || payload.extra === null ? {} : object(payload.extra);
  // Copy only observed identity, never prompt/history/tool/provider fields.
  const wireEvent = {
    hook_event_name: "pre_llm_call",
    profile: payload.profile,
    session_id: payload.session_id,
    cwd: payload.cwd,
    extra: {
      agent_id: optional(extra.agent_id),
      project_id: optional(extra.project_id),
      task_id: optional(extra.task_id),
      run_id: optional(extra.run_id),
      turn_id: optional(extra.turn_id),
    },
  };
  const actual = hermesWorkMethodEventIdentity(wireEvent);
  // The declared source scope is not an authenticated canonical-to-shell profile mapping.
  if (
    actual.native.profileId !== shell.profileName ||
    actual.native.sessionId !== shell.sessionId ||
    actual.actualCwd !== shell.cwd
  )
    return {};
  const inner = {
    schema: "hermes.real-callback-identity-context.v1",
    kind: "stableSessionSource",
    logical,
    actorId: selected.actorId,
    routeId: selected.routeId,
    wireEvent,
    declaredSourceScope: scope,
    sourceProvenance: {
      compiledRegistrationSha256: options.compiledRegistrationSha256,
      bindingsSha256: options.bindingsSha256,
      sourceMapSha256: binding.mapSha256,
    },
    nativeInvocationVerified: false,
    ownerProfileMappingVerified: false,
    nativeAuthenticationEstablished: false,
    receivedOrUsedEstablishedByProjection: false,
    ownerAdmissionEstablished: false,
    nativePermissionGranted: false,
    executionAuthorityGranted: false,
  };
  const context = JSON.stringify(inner),
    response = { context };
  if (
    context.length > policy.policy.maxChars ||
    JSON.stringify(response).length > policy.policy.maxChars
  )
    throw Error("BOUNDED_NATIVE_CONTEXT_REQUIRED");
  // The existing map API validates the whole closed source selector. Its chunk is discarded.
  // Its metadata budget is independent of the selected identity-context output budget.
  const source = await readHermesWorkMethodSource(
    {
      schema: "hermes.work-method-session-source-request.v1",
      scope,
      logical,
      actorId: selected.actorId,
      routeId: selected.routeId,
      sourceMapSha256: binding.mapSha256,
      target: { kind: "mapped-source", id: "@map" },
      cursor: { offsetBytes: 0, expectedDocumentSha256: null },
      outputVersion: "v2",
    },
    binding,
    { verifySourceClosure: verify },
  );
  if (
    source.status !== "source-qualified" ||
    !source.binding ||
    source.binding.sourceId !== "@map" ||
    source.binding.sourceMapSha256 !== binding.mapSha256 ||
    source.binding.documentSha256 !== binding.mapSha256 ||
    !isDeepStrictEqual(source.binding.logical, logical)
  )
    throw Error("CALLBACK_SOURCE_SELECTION_HELD");
  await verify();
  await readExactFile(binding.runtimeMapPath, 1048576, binding.mapSha256);
  return response;
}
/** Installed shell-hook protocol: profile/session/cwd are top-level; task and optional identities are extra kwargs.
 * Missing agent/project/run stay null. turn_id and logical Task IDs never fabricate those native references.
 */
export async function readHermesNativeWorkHook(
  payloadValue: unknown,
  bindingsValue: unknown,
  options: HermesNativeWorkHookOptions,
): Promise<HermesNativeWorkHookResponse> {
  const payload = object(payloadValue);
  if (payload.hook_event_name !== "pre_llm_call") return {};
  if (
    bindingsValue &&
    typeof bindingsValue === "object" &&
    !Array.isArray(bindingsValue) &&
    (bindingsValue as Row).schema === "hermes.real-callback-source-hook-bindings.v1"
  )
    return readRealCallbackSource(payload, object(structuredClone(bindingsValue)), options);
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
  if (options.outputVersion !== undefined && !["v1", "v2"].includes(options.outputVersion))
    throw Error("EXPLICIT_CURRENT_WORK_OUTPUT_VERSION_REQUIRED");
  const read =
    options.outputVersion === "v2" ? readHermesCurrentWorkContextV2 : readHermesCurrentWorkContext;
  const projection = await read(event, selected, bindings.runtimeIndex, options);
  await options.verifySourceClosure();
  const context = JSON.stringify(projection);
  if (
    context.length >
    currentWorkOutputPolicy(options.outputVersion ?? "v1", options.maxChars).policy.maxChars
  )
    throw Error("BOUNDED_NATIVE_CONTEXT_REQUIRED");
  return { context };
}
