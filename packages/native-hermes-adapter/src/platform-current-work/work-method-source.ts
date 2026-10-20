import { isAbsolute, join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { readExactFile, sourceDigest } from "./provider.ts";
import { currentWorkOutputPolicy, type CurrentWorkOutputPolicy } from "./output-policy.ts";
import {
  hermesWorkMethodEventIdentity,
  readHermesWorkMethodSourceDocument,
  type HermesWorkMethodTuple,
  type HermesWorkMethodOptions,
} from "./work-method.ts";

/** Supplied only by the compiled source owner, never by request data. */
export interface WorkMethodSourceBinding {
  runtimeMapPath: string;
  mapSha256: string;
  dataBase: string;
}
type LogicalTask = { domainId: string; projectId: string; workId: string; taskId: string };
export interface WorkMethodSourceMap {
  schema: "hermes.work-method-source-map.v1";
  logical: LogicalTask;
  actorId: string;
  routeId: string;
  native: HermesWorkMethodTuple;
  cwd: string;
  entries: { id: string; path: string; sha256: string; bytes: number }[];
  /** Complete original/fragment mappings remain in the pinned map, never inferred from a chunk. */
  provenance?: unknown;
}
export interface WorkMethodSourceResponse {
  schema: "hermes.work-method-source-response.v1";
  status: "source-qualified" | "held";
  holdReasons: string[];
  policy: CurrentWorkOutputPolicy;
  binding: {
    sourceMapSha256: string;
    sourceId: string;
    logical: LogicalTask;
    documentSha256: string;
    totalBytes: number;
  } | null;
  chunk: {
    offsetBytes: number;
    nextOffsetBytes: number;
    text: string;
    last: boolean;
  } | null;
  nativeAuthenticationEstablished: false;
  ownerAdmissionEstablishedByDelivery: false;
  receivedOrUsedEstablishedByDelivery: false;
  nativePermissionGranted: false;
  executionAuthorityGranted: false;
}
const fileLimit = 1_048_576;
const digest = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/u.test(value);
class Failure extends Error {}
function need(value: unknown, code: string): asserts value {
  if (!value) throw new Failure(code);
}
function row(value: unknown): Record<string, unknown> {
  need(value && typeof value === "object" && !Array.isArray(value), "INVALID_SOURCE_REQUEST");
  return value as Record<string, unknown>;
}
function closed(value: Record<string, unknown>, keys: string[], optional: string[] = []) {
  need(
    Object.keys(value).every((key) => keys.includes(key) || optional.includes(key)) &&
      keys.every((key) => Object.hasOwn(value, key)),
    "INVALID_SOURCE_REQUEST",
  );
}
function text(value: unknown): asserts value is string {
  need(
    typeof value === "string" &&
      value.length > 0 &&
      value.length <= 256 &&
      value.trim() === value &&
      !/[\u0000-\u001f]/u.test(value),
    "INVALID_SOURCE_REQUEST",
  );
}
function logical(value: unknown): asserts value is LogicalTask {
  const object = row(value);
  closed(object, ["domainId", "projectId", "workId", "taskId"]);
  for (const field of Object.values(object)) text(field);
}
function exactPath(value: unknown): asserts value is string {
  need(
    typeof value === "string" &&
      value.length <= 2048 &&
      isAbsolute(value) &&
      resolve(value) === value &&
      !/[\u0000-\u001f]/u.test(value),
    "INVALID_SOURCE_BINDING",
  );
}
function contained(base: string, path: string) {
  need(path.startsWith(base + "/"), "SOURCE_PATH_ESCAPE");
}
function mapValue(bytes: Uint8Array): WorkMethodSourceMap {
  const raw = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  const value = row(JSON.parse(raw));
  // Reject duplicate keys/lossy spellings; owner emits one of these complete generated encodings.
  need(
    raw === JSON.stringify(value) + "\n" || raw === JSON.stringify(value, null, 2) + "\n",
    "UNAMBIGUOUS_SOURCE_MAP_REQUIRED",
  );
  closed(
    value,
    ["schema", "logical", "actorId", "routeId", "native", "cwd", "entries"],
    ["provenance"],
  );
  need(value.schema === "hermes.work-method-source-map.v1", "INVALID_SOURCE_MAP");
  logical(value.logical);
  text(value.actorId);
  text(value.routeId);
  exactPath(value.cwd);
  const native = row(value.native);
  closed(native, [
    "runtime",
    "profileId",
    "sessionId",
    "agentId",
    "nativeProjectId",
    "nativeTaskId",
    "nativeRunId",
    "turnId",
  ]);
  need(native.runtime === "native-hermes", "INVALID_SOURCE_MAP");
  text(native.profileId);
  text(native.sessionId);
  for (const key of ["agentId", "nativeProjectId", "nativeTaskId", "nativeRunId", "turnId"])
    if (native[key] !== null) text(native[key]);
  need(Array.isArray(value.entries) && value.entries.length <= 2048, "SOURCE_MAP_LIMIT");
  const ids = new Set<string>();
  let total = 0;
  for (const item of value.entries) {
    const entry = row(item);
    closed(entry, ["id", "path", "sha256", "bytes"]);
    text(entry.id);
    need(/^[A-Za-z0-9_.:-]+$/u.test(entry.id) && !ids.has(entry.id), "INVALID_SOURCE_ID");
    ids.add(entry.id);
    need(
      typeof entry.path === "string" &&
        entry.path.length <= 512 &&
        !entry.path.includes("\\") &&
        !/[\u0000-\u001f]/u.test(entry.path) &&
        !entry.path.split("/").some((part) => ["", ".", ".."].includes(part)) &&
        !isAbsolute(entry.path) &&
        digest(entry.sha256) &&
        Number.isSafeInteger(entry.bytes) &&
        Number(entry.bytes) >= 0 &&
        Number(entry.bytes) <= fileLimit,
      "INVALID_SOURCE_MAP_ENTRY",
    );
    total += Number(entry.bytes);
  }
  need(total <= 33_554_432, "SOURCE_MAP_TOTAL_LIMIT");
  return value as unknown as WorkMethodSourceMap;
}
function response(policy: CurrentWorkOutputPolicy): WorkMethodSourceResponse {
  return {
    schema: "hermes.work-method-source-response.v1",
    status: "source-qualified",
    holdReasons: [],
    policy,
    binding: null,
    chunk: null,
    nativeAuthenticationEstablished: false,
    ownerAdmissionEstablishedByDelivery: false,
    receivedOrUsedEstablishedByDelivery: false,
    nativePermissionGranted: false,
    executionAuthorityGranted: false,
  };
}

/** Stateless exact source delivery. It observes no native stores and creates no receipt/use/admission. */
export async function readHermesWorkMethodSource(
  input: unknown,
  bindingValue: WorkMethodSourceBinding | undefined,
  options: HermesWorkMethodOptions,
): Promise<WorkMethodSourceResponse> {
  let policy = currentWorkOutputPolicy("v2").policy;
  let stage = "source";
  try {
    need(options && typeof options.verifySourceClosure === "function", "SOURCE_VERIFIER_REQUIRED");
    await options.verifySourceClosure();
    stage = "input";
    const request = row(structuredClone(input));
    closed(
      request,
      ["schema", "event", "selection", "sourceMapSha256", "target", "cursor", "outputVersion"],
      ["maxChars"],
    );
    need(
      request.schema === "hermes.work-method-source-request.v1" && request.outputVersion === "v2",
      "INVALID_SOURCE_REQUEST",
    );
    const selectedPolicy = currentWorkOutputPolicy("v2", request.maxChars as number | undefined);
    need(selectedPolicy.valid, "INVALID_OUTPUT_POLICY");
    policy = selectedPolicy.policy;
    need(bindingValue, "SOURCE_MAP_NOT_REGISTERED");
    const binding = row(structuredClone(bindingValue));
    closed(binding, ["runtimeMapPath", "mapSha256", "dataBase"]);
    exactPath(binding.runtimeMapPath);
    exactPath(binding.dataBase);
    contained(binding.dataBase, binding.runtimeMapPath);
    need(
      digest(binding.mapSha256) && request.sourceMapSha256 === binding.mapSha256,
      "SOURCE_MAP_PIN_MISMATCH",
    );
    const identity = hermesWorkMethodEventIdentity(request.event);
    const selection = row(request.selection);
    closed(selection, ["cwd", "native", "actorId", "routeId", "logical"]);
    logical(selection.logical);
    need(
      selection.cwd === identity.actualCwd && isDeepStrictEqual(selection.native, identity.native),
      "FOREIGN_NATIVE_TUPLE",
    );
    stage = "map";
    const mapBytes = await readExactFile(binding.runtimeMapPath, fileLimit, binding.mapSha256);
    const map = mapValue(mapBytes);
    need(
      selection.actorId === map.actorId &&
        selection.routeId === map.routeId &&
        selection.cwd === map.cwd &&
        isDeepStrictEqual(selection.logical, map.logical) &&
        isDeepStrictEqual(identity.native, map.native),
      "FOREIGN_SOURCE_SELECTION",
    );
    const target = row(request.target);
    const cursor = row(request.cursor);
    closed(cursor, ["offsetBytes", "expectedDocumentSha256"]);
    need(
      Number.isSafeInteger(cursor.offsetBytes) && Number(cursor.offsetBytes) >= 0,
      "INVALID_SOURCE_CURSOR",
    );
    need(
      digest(cursor.expectedDocumentSha256) ||
        (cursor.offsetBytes === 0 && cursor.expectedDocumentSha256 === null),
      "CONTINUATION_DIGEST_REQUIRED",
    );
    let bytes: Uint8Array;
    let sourceId: string;
    let selectedFile: { path: string; sha256: string } | null = null;
    if (target.kind === "mapped-source") {
      closed(target, ["kind", "id"]);
      text(target.id);
      sourceId = target.id;
      if (target.id === "@map") bytes = mapBytes;
      else {
        const entry = map.entries.find((item) => item.id === target.id);
        need(entry, "UNKNOWN_SOURCE_ID");
        const path = join(binding.dataBase, entry.path);
        contained(binding.dataBase, path);
        selectedFile = { path, sha256: entry.sha256 };
        stage = "file";
        bytes = await readExactFile(path, fileLimit, entry.sha256);
        need(bytes.length === entry.bytes, "SOURCE_FILE_LENGTH_MISMATCH");
      }
    } else {
      closed(target, ["kind", "request"]);
      need(target.kind === "validated-owner-document", "INVALID_SOURCE_TARGET");
      const ownerRequest = row(target.request);
      const ownerAdmission = row(ownerRequest.admission);
      const ownerPacket = row(ownerAdmission.packet);
      const ownerSelection = row(ownerRequest.selection);
      need(
        isDeepStrictEqual(ownerRequest.event, request.event) &&
          isDeepStrictEqual(ownerPacket.logical, map.logical) &&
          ownerSelection.actorId === map.actorId &&
          ownerSelection.routeId === map.routeId,
        "FOREIGN_OWNER_DOCUMENT",
      );
      stage = "source";
      const document = await readHermesWorkMethodSourceDocument(ownerRequest, options);
      bytes = new TextEncoder().encode(JSON.stringify(document));
      sourceId = "@validated-owner-document";
    }
    need(bytes.length <= fileLimit, "SOURCE_DOCUMENT_LIMIT");
    stage = "input";
    const textValue = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
    const documentSha256 = sourceDigest(bytes);
    need(
      cursor.expectedDocumentSha256 === null || cursor.expectedDocumentSha256 === documentSha256,
      "SOURCE_DOCUMENT_CHANGED",
    );
    const boundaries = [0];
    let count = 0;
    for (const scalar of textValue) {
      count += Buffer.byteLength(scalar, "utf8");
      boundaries.push(count);
    }
    const start = boundaries.indexOf(Number(cursor.offsetBytes));
    need(start >= 0, "UNALIGNED_SOURCE_OFFSET");
    const value = response(policy);
    value.binding = {
      sourceMapSha256: binding.mapSha256,
      sourceId,
      logical: map.logical,
      documentSha256,
      totalBytes: bytes.length,
    };
    const makeChunk = (index: number) => {
      const end = boundaries[index]!;
      value.chunk = {
        offsetBytes: Number(cursor.offsetBytes),
        nextOffsetBytes: end,
        text: Buffer.from(bytes.subarray(Number(cursor.offsetBytes), end)).toString("utf8"),
        last: end === bytes.length,
      };
    };
    makeChunk(start);
    need(JSON.stringify(value).length <= policy.maxChars, "SOURCE_METADATA_OUTPUT_LIMIT");
    let low = start,
      high = boundaries.length - 1;
    while (low < high) {
      const middle = Math.ceil((low + high) / 2);
      makeChunk(middle);
      if (JSON.stringify(value).length <= policy.maxChars) low = middle;
      else high = middle - 1;
    }
    makeChunk(low);
    need(low > start || Number(cursor.offsetBytes) === bytes.length, "SOURCE_CHUNK_OUTPUT_LIMIT");
    stage = "source";
    await options.verifySourceClosure();
    stage = "map";
    await readExactFile(binding.runtimeMapPath, fileLimit, binding.mapSha256);
    if (selectedFile) {
      stage = "file";
      await readExactFile(selectedFile.path, fileLimit, selectedFile.sha256);
    }
    return value;
  } catch (error) {
    const value = response(policy);
    value.status = "held";
    value.holdReasons = [
      error instanceof Failure
        ? error.message
        : stage === "source"
          ? "SOURCE_CLOSURE_NOT_CURRENT"
          : stage === "map"
            ? "SOURCE_MAP_NOT_CURRENT"
            : stage === "file"
              ? "SOURCE_FILE_NOT_CURRENT"
              : "INVALID_SOURCE_REQUEST",
    ];
    return value;
  }
}
