import { isAbsolute, resolve } from "node:path";
import {
  decideWorkMethodAdmission,
  validateWorkMethodPacket,
  validateWorkMethodReceipt,
  validateWorkMethodStanding,
  workMethodPacketSha256,
  type WorkMethodAdmission,
  type WorkMethodAdmissionInput,
  type WorkMethodNativeIds,
  type WorkMethodPacket,
  type WorkMethodReceipt,
  type WorkMethodStanding,
} from "@palantirkc/work-continuity/context";
import { currentWorkOutputPolicy, type CurrentWorkOutputPolicy } from "./output-policy.ts";

/** Explicit owner observations, not authenticated native identities or a session lookup. */
export interface HermesWorkMethodTuple {
  runtime: "native-hermes";
  profileId: string;
  sessionId: string;
  agentId: string | null;
  nativeProjectId: string | null;
  nativeTaskId: string | null;
  nativeRunId: string | null;
  turnId: string | null;
}
export interface HermesWorkMethodSelection {
  schema: "hermes.work-method-owner-selection.v1";
  cwd: string;
  native: HermesWorkMethodTuple;
  actorId: string;
  routeId: string;
  packetSha256: string;
}
export interface HermesWorkMethodRequest {
  schema: "hermes.work-method-owner-request.v1";
  /** Identity-only pre_llm_call fields; never pass prompts, messages or native stores. */
  event: {
    hook_event_name: "pre_llm_call";
    profile: string;
    session_id: string;
    cwd: string;
    extra?: {
      agent_id?: string | null;
      project_id?: string | null;
      task_id?: string | null;
      run_id?: string | null;
      turn_id?: string | null;
    };
  };
  selection: HermesWorkMethodSelection;
  admission: WorkMethodAdmissionInput;
  outputVersion?: "v1" | "v2";
  maxChars?: number;
}
export interface HermesWorkMethodResponse {
  schema: "hermes.work-method-owner-response.v1";
  status: "owner-eligible" | "held";
  holdReasons: string[];
  policy: CurrentWorkOutputPolicy;
  native: (HermesWorkMethodTuple & { cwd: string }) | null;
  admission: WorkMethodAdmission | null;
  /** Full contracts survive serialization unchanged; an oversized response is held, never clipped. */
  projection: {
    standing: WorkMethodStanding;
    packet: WorkMethodPacket;
    receipt: WorkMethodReceipt;
  } | null;
  nativeAuthenticationEstablished: false;
  nativePermissionGranted: false;
  executionAuthorityGranted: false;
}
export interface HermesWorkMethodOptions {
  /** The existing source owner supplies this guard. No data-selected code or live IO here. */
  verifySourceClosure: () => Promise<void>;
}
type Row = Record<string, unknown>;
const tupleKeys = [
  "runtime",
  "profileId",
  "sessionId",
  "agentId",
  "nativeProjectId",
  "nativeTaskId",
  "nativeRunId",
  "turnId",
] as const;
const nativeKeys = [
  "threadId",
  "sessionId",
  "taskId",
  "runId",
  "turnId",
  "parentThreadId",
] as const;
const failures = [
  "INVALID_OWNER_REQUEST",
  "INVALID_OUTPUT_POLICY",
  "UNSUPPORTED_NATIVE_EVENT",
  "EXACT_NATIVE_IDENTITY_REQUIRED",
  "EXACT_NATIVE_CWD_REQUIRED",
  "EXACT_OWNER_SELECTION_REQUIRED",
  "FOREIGN_NATIVE_TUPLE",
  "COMMON_CONTRACT_INVALID",
  "OWNER_PACKET_BINDING_MISMATCH",
  "NATIVE_ACTOR_BINDING_MISMATCH",
  "NATIVE_EVENT_BINDING_MISMATCH",
] as const;
type Failure = (typeof failures)[number];
function fail(reason: Failure): never {
  throw Error(reason);
}
function row(value: unknown): Row {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("INVALID_OWNER_REQUEST");
  return value as Row;
}
function only(value: Row, allowed: readonly string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key))) fail("INVALID_OWNER_REQUEST");
}
function id(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value ||
    value.trim() !== value ||
    value.length > 256 ||
    /[\u0000-\u001f]/u.test(value)
  )
    fail("EXACT_NATIVE_IDENTITY_REQUIRED");
  return value;
}
function optional(value: unknown): string | null {
  return value === undefined || value === null ? null : id(value);
}
function cwd(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length > 2048 ||
    !isAbsolute(value) ||
    resolve(value) !== value ||
    /[\u0000-\u001f]/u.test(value)
  )
    fail("EXACT_NATIVE_CWD_REQUIRED");
  return value;
}
function nativeIds(tuple: HermesWorkMethodTuple): WorkMethodNativeIds {
  return {
    threadId: null,
    sessionId: tuple.sessionId,
    taskId: tuple.nativeTaskId,
    runId: tuple.nativeRunId,
    turnId: tuple.turnId,
    parentThreadId: null,
  };
}
function sameNative(value: unknown, expected: WorkMethodNativeIds): boolean {
  const observed = row(value);
  return nativeKeys.every((key) => observed[key] === expected[key]);
}
function held(reason: string, policy: CurrentWorkOutputPolicy): HermesWorkMethodResponse {
  return {
    schema: "hermes.work-method-owner-response.v1",
    status: "held",
    holdReasons: [reason],
    policy,
    native: null,
    admission: null,
    projection: null,
    nativeAuthenticationEstablished: false,
    nativePermissionGranted: false,
    executionAuthorityGranted: false,
  };
}

/** Identity-only native seam shared by the legacy and explicit source-read routes. */
export function hermesWorkMethodEventIdentity(eventValue: unknown) {
  const event = row(eventValue);
  only(event, ["hook_event_name", "profile", "session_id", "cwd", "extra"]);
  if (event.hook_event_name !== "pre_llm_call") fail("UNSUPPORTED_NATIVE_EVENT");
  const extra = event.extra === undefined || event.extra === null ? {} : row(event.extra);
  only(extra, ["agent_id", "project_id", "task_id", "run_id", "turn_id"]);
  const native: HermesWorkMethodTuple = {
    runtime: "native-hermes",
    profileId: id(event.profile),
    sessionId: id(event.session_id),
    agentId: optional(extra.agent_id),
    nativeProjectId: optional(extra.project_id),
    nativeTaskId: optional(extra.task_id),
    nativeRunId: optional(extra.run_id),
    turnId: optional(extra.turn_id),
  };
  const actualCwd = cwd(event.cwd);
  return { native, actualCwd };
}

/** Additive owner admission only: no generic tool veto, native invocation, fallback or activation. */
async function evaluateHermesWorkMethod(
  value: unknown,
  options: HermesWorkMethodOptions,
): Promise<HermesWorkMethodResponse> {
  let policy = currentWorkOutputPolicy("v1").policy;
  if (!options || typeof options.verifySourceClosure !== "function")
    return held("SOURCE_VERIFIER_REQUIRED", policy);
  let stage: "source" | "input" | "readback" = "source";
  try {
    await options.verifySourceClosure();
    stage = "input";
    const request = row(structuredClone(value));
    only(request, ["schema", "event", "selection", "admission", "outputVersion", "maxChars"]);
    if (request.schema !== "hermes.work-method-owner-request.v1") fail("INVALID_OWNER_REQUEST");
    const version = request.outputVersion ?? "v1";
    if (version !== "v1" && version !== "v2") fail("INVALID_OUTPUT_POLICY");
    const output = currentWorkOutputPolicy(version, request.maxChars as number | undefined);
    if (!output.valid) fail("INVALID_OUTPUT_POLICY");
    policy = output.policy;

    const { native, actualCwd } = hermesWorkMethodEventIdentity(request.event);
    const selection = row(request.selection);
    only(selection, ["schema", "cwd", "native", "actorId", "routeId", "packetSha256"]);
    if (selection.schema !== "hermes.work-method-owner-selection.v1")
      fail("EXACT_OWNER_SELECTION_REQUIRED");
    const selected = row(selection.native);
    only(selected, tupleKeys);
    if (
      cwd(selection.cwd) !== actualCwd ||
      !tupleKeys.every((key) => selected[key] === native[key])
    )
      fail("FOREIGN_NATIVE_TUPLE");

    const admission = row(request.admission);
    if (
      !validateWorkMethodStanding(admission.standing).valid ||
      !validateWorkMethodPacket(admission.packet).valid ||
      !validateWorkMethodReceipt(admission.receipt).valid
    )
      fail("COMMON_CONTRACT_INVALID");
    const input = admission as unknown as WorkMethodAdmissionInput;
    if (
      selection.actorId !== input.expectedActor?.actorId ||
      selection.actorId !== input.receipt.actor.actorId ||
      selection.routeId !== input.routeId ||
      selection.packetSha256 !== workMethodPacketSha256(input.packet)
    )
      fail("OWNER_PACKET_BINDING_MISMATCH");
    const ids = nativeIds(native);
    for (const actor of [input.expectedActor, input.receipt.actor]) {
      if (
        !actor ||
        actor.runtime !== "native-hermes" ||
        actor.profileId !== native.profileId ||
        !sameNative(actor.native, ids)
      )
        fail("NATIVE_ACTOR_BINDING_MISMATCH");
    }
    if (!sameNative(input.packet.native, ids)) fail("NATIVE_ACTOR_BINDING_MISMATCH");
    if (
      input.currentEvent !== null &&
      (!input.currentEvent ||
        input.currentEvent.actorId !== input.expectedActor.actorId ||
        input.currentEvent.packetSha256 !== selection.packetSha256 ||
        input.currentEvent.taskSha256 !== input.packet.task.sha256 ||
        !sameNative(input.currentEvent.native, ids))
    )
      fail("NATIVE_EVENT_BINDING_MISMATCH");

    stage = "readback";
    await options.verifySourceClosure();
    const decision = decideWorkMethodAdmission(input);
    const response: HermesWorkMethodResponse = {
      schema: "hermes.work-method-owner-response.v1",
      status: decision.status,
      holdReasons: decision.status === "held" ? ["COMMON_OWNER_ADMISSION_HELD"] : [],
      policy,
      native: { ...native, cwd: actualCwd },
      admission: decision,
      projection: {
        standing: input.standing,
        packet: input.packet,
        receipt: input.receipt,
      },
      nativeAuthenticationEstablished: false,
      nativePermissionGranted: false,
      executionAuthorityGranted: false,
    };
    // Shared evaluation retains complete contracts; each public wire route applies its own cap.
    await options.verifySourceClosure();
    return response;
  } catch (error) {
    const reason =
      stage === "source" || stage === "readback"
        ? "SOURCE_CLOSURE_NOT_CURRENT"
        : error instanceof Error && failures.includes(error.message as Failure)
          ? error.message
          : "INVALID_OWNER_REQUEST";
    return held(reason, policy);
  }
}

/** Legacy complete response remains lossless and keeps its original whole-response cap. */
export async function readHermesWorkMethod(value: unknown, options: HermesWorkMethodOptions) {
  const response = await evaluateHermesWorkMethod(value, options);
  return JSON.stringify(response).length <= response.policy.maxChars
    ? response
    : held("WORK_METHOD_OUTPUT_EXCEEDS_BOUND", response.policy);
}

/** A source document only; a paginated delivery never establishes native receive/use/admission. */
export async function readHermesWorkMethodSourceDocument(
  value: unknown,
  options: HermesWorkMethodOptions,
) {
  const response = await evaluateHermesWorkMethod(value, options);
  if (!response.projection || !response.native || !response.admission)
    throw Error("OWNER_DOCUMENT_NOT_SOURCE_QUALIFIED");
  return {
    schema: "hermes.work-method-source-document.v1" as const,
    native: response.native,
    admission: response.admission,
    contracts: response.projection,
    nativeAuthenticationEstablished: false as const,
    nativePermissionGranted: false as const,
    executionAuthorityGranted: false as const,
  };
}
