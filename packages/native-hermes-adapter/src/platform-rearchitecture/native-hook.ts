import { readJsonRequest } from "./input.ts";
import { open } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import {
  readCurrentWork,
  configuredNativeManifestPin,
  currentWorkContextMaxChars,
  openExactDirectory,
  readExactFile,
} from "./work-reader";
import { constants } from "node:fs";

const root = "/opt/data/workspace/hermes-harness-audit/platform/current";
const manifestPath = join(root, "current-work.json");
const events = new Set([
  "pre_llm_call",
  "post_llm_call",
  "on_session_start",
  "on_session_end",
  "post_tool_call",
  "pre_api_request",
  "post_api_request",
  "api_request_error",
  "pre_auxiliary_call",
  "post_auxiliary_call",
]);
const hash = (x: Uint8Array | string) => createHash("sha256").update(x).digest("hex");
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
function metadata(value: unknown): string | null {
  if (typeof value !== "string") return null;
  if (value.length > 300 || /[\u0000-\u001f]/u.test(value))
    throw Error("Bounded native metadata required");
  return value;
}
/** Native shell observer only. No semantic decisions, new dispatcher, transcript store or model call. */
export async function observeNativeBoundary(
  payload: unknown,
  options: { manifestPath: string; receiptRoot: string; expectedManifestSha256?: string },
) {
  if (
    !object(payload) ||
    typeof payload.hook_event_name !== "string" ||
    !events.has(payload.hook_event_name) ||
    typeof payload.profile !== "string" ||
    !/^[a-zA-Z0-9_-]{1,80}$/u.test(payload.profile) ||
    typeof payload.session_id !== "string" ||
    !payload.session_id ||
    !/^[a-zA-Z0-9_-]{1,160}$/.test(payload.session_id)
  )
    throw Error("Explicit native boundary identity required");
  const extra = object(payload.extra) ? payload.extra : {};
  const toolName = metadata(payload.tool_name);
  const execution = {
    completed: typeof extra.completed === "boolean" ? extra.completed : null,
    failed: typeof extra.failed === "boolean" ? extra.failed : null,
    interrupted: typeof extra.interrupted === "boolean" ? extra.interrupted : null,
    turnId: metadata(extra.turn_id),
    taskId: metadata(extra.task_id),
    model: metadata(extra.model),
    provider: metadata(extra.provider),
  };
  const requestIdentity = Object.fromEntries(
    [
      "api_request_id",
      "api_call_count",
      "response_model",
      "aux_task",
      "retry_count",
      "turn_exit_reason",
    ].flatMap<[string, string | number | null]>((key) => {
      const value = extra[key];
      if (typeof value === "string") return [[key, metadata(value)]];
      return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
        ? [[key, value]]
        : [];
    }),
  );
  const projection = await readCurrentWork(
    options.manifestPath,
    payload.profile,
    payload.session_id,
    currentWorkContextMaxChars,
    options.expectedManifestSha256,
  );
  // Never persist arbitrary hook payloads: user_message, history, args, tool outputs and credentials remain native-owned.
  const selected = projection.selectedProductWork as { status: string; binding?: unknown };
  const usage = object(extra.usage)
    ? Object.fromEntries(
        [
          "input_tokens",
          "output_tokens",
          "prompt_tokens",
          "completion_tokens",
          "total_tokens",
          "cached_tokens",
          "reasoning_tokens",
        ].flatMap((key) => {
          const value = (extra.usage as Record<string, unknown>)[key];
          return typeof value === "number" && Number.isFinite(value) && value >= 0
            ? [[key, value]]
            : [];
        }),
      )
    : null;
  const observation = {
    schema: "hermes.native-boundary-observation.v1",
    profile: payload.profile,
    sessionId: payload.session_id,
    event: payload.hook_event_name,
    observedAtUtc: new Date().toISOString(),
    manifestPin: projection.manifestPin,
    engineeringWorkId: projection.engineeringWork.workId,
    selectedProductWork: { status: selected.status, binding: selected.binding },
    toolName,
    execution,
    usage: ["post_api_request", "post_auxiliary_call"].includes(payload.hook_event_name)
      ? usage
      : null,
    originalCustody: {
      owner: "native-hermes",
      capture: "not-inferred-from-hook",
      apiContentDistinct: true,
    },
    share: {
      status: "pending-owner-reconciliation",
      reason: "A local observer cannot certify cross-environment sharing or independent recovery.",
    },
    semanticAcceptance: "not-inferred",
    missedBoundaryPolicy:
      "Reconcile native durable task/run/session records at the next observed selected Work boundary; never replay ambiguous effects.",
  };
  if (resolve(options.receiptRoot) !== options.receiptRoot)
    throw Error("Absolute receipt root required");
  const record = {
    ...observation,
    requestIdentity,
    nativeErrorPresent: extra.error !== undefined && extra.error !== null,
    usageCompleteness:
      "Not inferred from an observer hook; reconcile actual native request ledger before additional model dispatch.",
  };
  const bytes = Buffer.from(JSON.stringify(record) + "\n"),
    name = hash(bytes) + ".json";
  if (bytes.length > 1048576) throw Error("Native boundary receipt too large");
  const dir = join(options.receiptRoot, payload.profile, payload.session_id),
    directory = await openExactDirectory(dir, true),
    path = join(dir, name);
  try {
    const file = await open(
      `/proc/self/fd/${directory.fd}/${name}`,
      constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
      0o600,
    );
    try {
      await file.writeFile(bytes);
      await file.sync();
    } finally {
      await file.close();
    }
    await directory.sync();
  } finally {
    await directory.close();
  }
  await readExactFile(path, 1048576, hash(bytes));
  return {
    context:
      payload.hook_event_name === "pre_llm_call"
        ? JSON.stringify({
            currentWork: projection,
            boundaryReceipt: { path, sha256: hash(bytes) },
            policy: "Current pinned source context; no native permission or semantic acceptance.",
          })
        : undefined,
    observationPath: path,
  };
}
if (import.meta.main) {
  try {
    const input = await readJsonRequest(2097152);
    const result = await observeNativeBoundary(input, {
      manifestPath,
      receiptRoot: join(root, "boundary-observations"),
      expectedManifestSha256: configuredNativeManifestPin(),
    });
    console.log(JSON.stringify(result.context ? { context: result.context } : {}));
  } catch {
    console.error(
      JSON.stringify({
        schema: "hermes.native-boundary-observer-failure.v1",
        error: "NATIVE_BOUNDARY_INPUT_OR_SOURCE_FAILED",
        semanticAcceptance: false,
        reconcileRequired: true,
      }),
    );
    console.log("{}");
    process.exitCode = 0;
  }
}
