import { createHash } from "node:crypto";
import {
  JEV_MODEL,
  type Json,
  type SemanticEvent,
  type WholeLens,
  type SemanticWireRequest,
  type PrepareResult,
  type ContractPin,
} from "./contract.ts";
import {
  canonicalJson,
  semanticDigest,
  prepareSemanticDecision,
  validateContractPin,
} from "./policy.ts";

export const DIRECT_POLICY = "jev.direct-selected-answer.v2" as const;
export type DirectQuestion =
  | { type: "noul"; instructions: Json; criteria?: { true?: Json; false?: Json } }
  | { type: "choice"; instructions: Json; criteria: Record<string, Json> }
  | { type: "score"; instructions: Json; criteria: Exclude<Json, null | boolean | number>[] }
  | { type: "box"; instructions: Json };
export interface BoxEdges {
  top: number;
  bottom: number;
  left: number;
  right: number;
}
export type DirectAnswer =
  | { type: "noul"; noul: number }
  | { type: "choice"; choice: string; confidence: number; probabilities: Record<string, number> }
  | {
      type: "score";
      score: number;
      confidence: number;
      legend: Record<string, Json>;
      probabilities: Record<string, number>;
    }
  | { type: "box"; box: BoxEdges; confidence: number | null };
export type ImageAsset =
  | { type: "image"; format: "png" | "jpg" | "jpeg"; data_base64: string }
  | { type: "image"; part: string; format?: "png" | "jpg" | "jpeg" };
/** Decoding is performed by the existing image/native owner, not asserted by source metadata alone. */
export interface ImageOwnerEvidence {
  source: ContractPin;
  sha256: string;
  width: number;
  height: number;
  byteLength: number;
  processedByteLength: number;
  format: "png" | "jpg" | "jpeg";
  still: true;
  decoded: true;
  evidenceRefs: string[];
}
export interface BoxBinding {
  assetRef: string;
  presenceQuestionId: string;
  minimumPresence: number;
}
export interface SemanticOpportunity {
  role: "Lead" | "SubLead" | "child";
  unit:
    | "Work"
    | "Task"
    | "subtask"
    | "step"
    | "call"
    | "batch"
    | "item"
    | "chunk"
    | "field"
    | "image-region";
  stableItemId: string;
  revision: string;
  nativeHookId: string;
  consumerId: string;
}
export interface DirectActionBinding {
  actionId: string;
  handlerKey: string;
  effectClass: "read_only" | "native_permitted";
  authorizationRef: string;
  permissionDigest: string;
  preconditionDigest: string;
  sourceRefs: string[];
  /** Optional branch applies only when the exact selected allowed Choice is in this set. */
  when: { questionId: string; choices: string[] } | null;
  arguments: Record<
    string,
    { kind: "literal"; value: Json } | { kind: "answer"; questionId: string }
  >;
}
export interface DirectWireRequest extends Omit<
  SemanticWireRequest,
  "schema" | "questions" | "consumptionPolicy"
> {
  schema: "jev.semantic-request.v2";
  consumptionPolicy: typeof DIRECT_POLICY;
  questions: Record<string, DirectQuestion>;
  assets?: Record<string, ImageAsset>;
  imageOptIn: boolean;
}
export interface PreparedDirectDecision {
  status: "prepared-direct";
  request: DirectWireRequest;
  requestDigest: string;
  reuseKey: string;
  bindingDigest: string;
  sourceRevision: string;
  opportunity: SemanticOpportunity;
  actions: DirectActionBinding[];
  boxes: Record<string, BoxBinding>;
  imageEvidence: Record<string, ImageOwnerEvidence>;
}
export interface DirectSelection {
  schema: "jev.direct-selection.v2";
  requestDigest: string;
  answers: Record<string, DirectAnswer>;
  ignoredQuestionIds: string[];
  executionAuthorityGranted: false;
}
const object = (x: unknown): x is Record<string, unknown> =>
  !!x && typeof x === "object" && !Array.isArray(x);
const id = (x: unknown): x is string =>
  typeof x === "string" &&
  x.trim().length > 0 &&
  x.length <= 1024 &&
  !/[\u0000-\u001f]/u.test(x) &&
  !["__proto__", "constructor", "prototype"].includes(x);
const probability = (x: unknown): x is number =>
  typeof x === "number" && Number.isFinite(x) && x >= 0 && x <= 1;
const refs = (x: unknown): x is string[] =>
  Array.isArray(x) && x.length > 0 && x.length <= 128 && x.every(id);
const hash = (x: unknown): x is string => typeof x === "string" && /^[a-f0-9]{64}$/u.test(x);
const clone = <T>(x: T): T => JSON.parse(canonicalJson(x)) as T;
const sameKeys = (a: object, b: object) =>
  canonicalJson(Object.keys(a).sort()) === canonicalJson(Object.keys(b).sort());
const content = (x: unknown, nullable = true): boolean =>
  (nullable && x === null) || typeof x === "string" || Array.isArray(x) || object(x);
function stringsIn(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(stringsIn);
  if (object(value)) return Object.values(value).flatMap(stringsIn);
  return [];
}
function validateQuestion(q: DirectQuestion): void {
  if (!q || !content(q.instructions) || !["noul", "choice", "score", "box"].includes(q.type))
    throw Error("DIRECT_TYPED_QUESTION_REQUIRED");
  if (
    q.type === "choice" &&
    (!object(q.criteria) ||
      !Object.keys(q.criteria).length ||
      Object.keys(q.criteria).length > 255 ||
      !Object.keys(q.criteria).every(id) ||
      !Object.values(q.criteria).every((value) => content(value)))
  )
    throw Error("DIRECT_CHOICE_CRITERIA");
  if (
    q.type === "noul" &&
    q.criteria !== undefined &&
    (!object(q.criteria) ||
      Object.keys(q.criteria).some((key) => !["true", "false"].includes(key)) ||
      !Object.values(q.criteria).every((value) => content(value)))
  )
    throw Error("DIRECT_NOUL_CRITERIA");
  // API admits 1..10 levels, including the one-level degenerate Score.
  if (
    q.type === "score" &&
    (!Array.isArray(q.criteria) ||
      !q.criteria.length ||
      q.criteria.length > 10 ||
      !q.criteria.every((value) => content(value, false)))
  )
    throw Error("DIRECT_SCORE_CRITERIA");
  const allowed = ["type", "instructions", ...(q.type === "box" ? [] : ["criteria"])];
  if (Object.keys(q).some((key) => !allowed.includes(key))) throw Error("DIRECT_QUESTION_FIELDS");
}
function validateImages(p: PreparedDirectDecision): void {
  const { request: r } = p,
    assets = r.assets ?? {};
  if (r.assets && (!object(r.assets) || !Object.keys(r.assets).length))
    throw Error("DIRECT_ASSETS_NONEMPTY_REQUIRED");
  if (!sameKeys(assets, p.imageEvidence)) throw Error("DIRECT_IMAGE_EVIDENCE_COVERAGE");
  if ((Object.keys(assets).length || Object.keys(p.boxes).length) && r.imageOptIn !== true)
    throw Error("DIRECT_EXPERIMENTAL_IMAGES_OPT_IN_REQUIRED");
  const representedState = Object.fromEntries(
    Object.entries(r.state).filter(([key]) => key !== "directBoundary"),
  );
  // Provider replacement visits question content, never protocol metadata such as type or IDs.
  const questionContent = Object.values(r.questions).map((q) => [
    q.instructions,
    "criteria" in q ? q.criteria : null,
  ]);
  const allStrings = stringsIn([{ ...representedState, scope: r.scope }, questionContent]);
  const projectedState = { ...r.state, scope: r.scope };
  const budgetStrings = stringsIn([projectedState, questionContent]);
  let processed = 0,
    expanded = Buffer.byteLength(canonicalJson([projectedState, r.questions]));
  const parts = new Set<string>();
  let referenceBytes = 0;
  for (const [ref, asset] of Object.entries(assets)) {
    if (
      !id(ref) ||
      Buffer.byteLength(ref) > 1024 ||
      Object.keys(assets).some(
        (other) => other !== ref && (ref.includes(other) || other.includes(ref)),
      )
    )
      throw Error("DIRECT_IMAGE_REFERENCE_INVALID");
    referenceBytes += Buffer.byteLength(ref);
    const occurrences = budgetStrings.reduce((sum, text) => sum + text.split(ref).length - 1, 0);
    if (!allStrings.some((text) => text.includes(ref))) throw Error("DIRECT_IMAGE_NOT_REPRESENTED");
    const e = p.imageEvidence[ref]!;
    validateContractPin(e.source);
    if (
      !hash(e.sha256) ||
      e.source.sha256 !== e.sha256 ||
      !refs(e.evidenceRefs) ||
      e.decoded !== true ||
      e.still !== true ||
      !["png", "jpg", "jpeg"].includes(e.format) ||
      ![e.width, e.height, e.byteLength, e.processedByteLength].every(Number.isSafeInteger) ||
      e.width < 280 ||
      e.width > 1568 ||
      e.height < 280 ||
      e.height > 1568 ||
      e.byteLength <= 0 ||
      e.byteLength > 16777216 ||
      e.processedByteLength <= 0 ||
      e.width * e.height < 78400 ||
      e.width * e.height > 2458624
    )
      throw Error("DIRECT_IMAGE_OWNER_GEOMETRY_OR_BYTES");
    if (asset.type !== "image") throw Error("DIRECT_IMAGE_TYPE");
    if ("part" in asset) {
      if (
        !id(asset.part) ||
        asset.part === "request" ||
        parts.has(asset.part) ||
        "data_base64" in asset ||
        Object.keys(asset).some((key) => !["type", "part", "format"].includes(key))
      )
        throw Error("DIRECT_MULTIPART_ASSET");
      parts.add(asset.part);
    } else {
      if (
        Object.keys(asset).some((key) => !["type", "format", "data_base64"].includes(key)) ||
        !["png", "jpg", "jpeg"].includes(asset.format) ||
        typeof asset.data_base64 !== "string" ||
        !asset.data_base64.length ||
        asset.data_base64.length > 22369624 ||
        asset.data_base64.length % 4 !== 0 ||
        /[^A-Za-z0-9+/=]/u.test(asset.data_base64)
      )
        throw Error("DIRECT_BASE64_ASSET");
      const bytes = Buffer.from(asset.data_base64, "base64");
      if (
        bytes.toString("base64") !== asset.data_base64 ||
        bytes.length !== e.byteLength ||
        createHash("sha256").update(bytes).digest("hex") !== e.sha256
      )
        throw Error("DIRECT_ASSET_BYTES_MISMATCH");
    }
    if (asset.format && asset.format.replace("jpeg", "jpg") !== e.format.replace("jpeg", "jpg"))
      throw Error("DIRECT_ASSET_FORMAT_MISMATCH");
    processed += e.processedByteLength;
    expanded += e.processedByteLength * occurrences;
  }
  if (
    referenceBytes > 65536 ||
    processed > 33554432 ||
    expanded > 33554432 ||
    Buffer.byteLength(canonicalJson(r)) > 33554432 ||
    Buffer.byteLength(
      canonicalJson({ model: "jev-1.14.0", state: projectedState, questions: r.questions, assets }),
    ) > 33554432
  )
    throw Error("DIRECT_IMAGE_REQUEST_BOUND");
  for (const [questionId, binding] of Object.entries(p.boxes)) {
    if (
      r.questions[questionId]?.type !== "box" ||
      !Object.hasOwn(assets, binding.assetRef) ||
      r.questions[binding.presenceQuestionId]?.type !== "noul" ||
      !probability(binding.minimumPresence) ||
      binding.minimumPresence <= 0.5 ||
      !allStrings.some((text) => text.includes(binding.assetRef))
    )
      throw Error("DIRECT_BOX_PRESENCE_BINDING");
  }
  if (
    Object.entries(r.questions).some(([key, q]) => q.type === "box" && !Object.hasOwn(p.boxes, key))
  )
    throw Error("DIRECT_BOX_BINDING_REQUIRED");
}
function validateActions(p: PreparedDirectDecision): void {
  const ids = new Set<string>();
  if (!Array.isArray(p.actions) || p.actions.length > 31) throw Error("DIRECT_ACTION_BOUND");
  for (const a of p.actions) {
    if (
      !id(a.actionId) ||
      ids.has(a.actionId) ||
      !id(a.handlerKey) ||
      !["read_only", "native_permitted"].includes(a.effectClass) ||
      !id(a.authorizationRef) ||
      !hash(a.permissionDigest) ||
      !hash(a.preconditionDigest) ||
      !refs(a.sourceRefs) ||
      !object(a.arguments) ||
      Object.keys(a.arguments).length > 32 ||
      !Object.keys(a.arguments).every(id)
    )
      throw Error("DIRECT_FIXED_ACTION_REQUIRED");
    ids.add(a.actionId);
    if (a.when !== null) {
      const q = p.request.questions[a.when.questionId];
      if (
        !q ||
        q.type !== "choice" ||
        !refs(a.when.choices) ||
        a.when.choices.some((choice) => !Object.hasOwn(q.criteria, choice))
      )
        throw Error("DIRECT_ACTION_BRANCH_INVALID");
    }
    for (const arg of Object.values(a.arguments))
      if (
        !arg ||
        !["literal", "answer"].includes(arg.kind) ||
        (arg.kind === "answer" && !Object.hasOwn(p.request.questions, arg.questionId)) ||
        (arg.kind === "literal" && !Object.hasOwn(arg, "value"))
      )
        throw Error("DIRECT_ACTION_ARGUMENT_INVALID");
  }
}
function intact(p: PreparedDirectDecision): void {
  if (
    p.status !== "prepared-direct" ||
    p.request.schema !== "jev.semantic-request.v2" ||
    p.request.consumptionPolicy !== DIRECT_POLICY ||
    p.request.model !== JEV_MODEL ||
    semanticDigest(p.request) !== p.requestDigest ||
    semanticDigest(p.request.state) !== p.request.stateDigest ||
    semanticDigest(p.request.state.binding) !== p.bindingDigest ||
    p.request.state.sourceRevision !== p.sourceRevision ||
    canonicalJson(p.request.state.directBoundary) !==
      canonicalJson({
        opportunity: p.opportunity,
        actions: p.actions,
        boxes: p.boxes,
        imageEvidence: p.imageEvidence,
      })
  )
    throw Error("DIRECT_PREPARED_BINDING_DRIFT");
  for (const q of Object.values(p.request.questions)) validateQuestion(q);
  validateImages(p);
  validateActions(p);
}
/** Pure common preparation. No provider client, native hook, scheduler or credential bridge. */
export function prepareDirectDecision(input: {
  requestId: string;
  event: SemanticEvent;
  lens: WholeLens;
  opportunity: SemanticOpportunity;
  questions: Record<string, DirectQuestion>;
  actions: DirectActionBinding[];
  boxes?: Record<string, BoxBinding>;
  assets?: Record<string, ImageAsset>;
  imageEvidence?: Record<string, ImageOwnerEvidence>;
  imageOptIn?: boolean;
  previousConsumedReuseKey?: string;
}): PreparedDirectDecision | Exclude<PrepareResult, { status: "prepared" }> {
  const base = prepareSemanticDecision({
    requestId: input.requestId,
    event: input.event,
    lens: input.lens,
    candidates: [],
    consumptionPolicy: DIRECT_POLICY,
  });
  if (base.status !== "prepared") return base;
  const o = input.opportunity;
  if (
    !o ||
    !["Lead", "SubLead", "child"].includes(o.role) ||
    ![
      "Work",
      "Task",
      "subtask",
      "step",
      "call",
      "batch",
      "item",
      "chunk",
      "field",
      "image-region",
    ].includes(o.unit) ||
    ![o.stableItemId, o.revision, o.nativeHookId, o.consumerId].every(id)
  )
    throw Error("DIRECT_OPPORTUNITY_REQUIRED");
  if (
    !object(input.questions) ||
    Object.keys(input.questions).some(
      (key) => !id(key) || Object.hasOwn(base.request.questions, key),
    )
  )
    throw Error("DIRECT_QUESTION_IDS");
  const questions: Record<string, DirectQuestion> = clone({
    ...base.request.questions,
    ...input.questions,
  });
  if (Object.keys(questions).length > 24) throw Error("DIRECT_QUESTION_BOUND");
  const boundary = clone({
    opportunity: o,
    actions: input.actions,
    boxes: input.boxes ?? {},
    imageEvidence: input.imageEvidence ?? {},
  });
  const state = clone({ ...base.request.state, directBoundary: boundary }) as unknown as Record<
    string,
    Json
  >;
  delete state.semanticBoundary;
  const request: DirectWireRequest = {
    ...base.request,
    schema: "jev.semantic-request.v2",
    consumptionPolicy: DIRECT_POLICY,
    state,
    stateDigest: semanticDigest(state),
    questions,
    imageOptIn: input.imageOptIn === true,
    ...(input.assets ? { assets: clone(input.assets) } : {}),
  };
  const prepared: PreparedDirectDecision = {
    status: "prepared-direct",
    request,
    requestDigest: semanticDigest(request),
    reuseKey: semanticDigest({ request: { ...request, requestId: null }, policy: DIRECT_POLICY }),
    bindingDigest: base.bindingDigest,
    sourceRevision: base.sourceRevision,
    ...boundary,
  };
  intact(prepared);
  if (prepared.reuseKey === input.previousConsumedReuseKey)
    return { status: "skipped", reason: "unchanged", reuseKey: prepared.reuseKey };
  return prepared;
}
function distribution(raw: unknown, expected: object): Record<string, number> {
  if (
    !object(raw) ||
    !sameKeys(raw, expected) ||
    !Object.values(raw).every(probability) ||
    Math.abs((Object.values(raw) as number[]).reduce((sum, value) => sum + value, 0) - 1) > 0.03
  )
    throw Error("DIRECT_ANSWER_DISTRIBUTION");
  return clone(raw) as Record<string, number>;
}
/** Selected Choice, numeric Noul/Score and presence-gated Box values are consumed directly.
 * Confidence remains evidence; it never vetoes or replaces a selected allowed answer.
 */
export function normalizeDirectReply(p: PreparedDirectDecision, raw: unknown): DirectSelection {
  intact(p);
  if (
    !object(raw) ||
    raw.schema !== "jev.semantic-result.v2" ||
    raw.consumptionPolicy !== DIRECT_POLICY ||
    raw.status !== "ok" ||
    raw.requestId !== p.request.requestId ||
    raw.stateDigest !== p.request.stateDigest ||
    raw.requestDigest !== p.requestDigest ||
    raw.purpose !== "semantic_bundle" ||
    raw.requestedModel !== JEV_MODEL ||
    ![JEV_MODEL, "jev-1.14.0"].includes(raw.returnedModel as typeof JEV_MODEL) ||
    raw.sourcePostconditionVerified !== true ||
    raw.executionAuthorityGranted !== false ||
    raw.automaticWrite !== false ||
    !object(raw.answers) ||
    !sameKeys(raw.answers, p.request.questions)
  )
    throw Error("DIRECT_RESPONSE_BINDING_OR_SOURCE_HELD");
  canonicalJson(raw);
  const answers: Record<string, DirectAnswer> = Object.create(null),
    ignoredQuestionIds: string[] = [];
  // Box values are processed only after all independently evaluated presence answers.
  for (const [key, q] of Object.entries(p.request.questions).sort(
    (a, b) => Number(a[1].type === "box") - Number(b[1].type === "box"),
  )) {
    const a = raw.answers[key];
    if (!object(a) || a.type !== q.type) throw Error("DIRECT_ANSWER_TYPE");
    if (q.type === "noul") {
      if (!probability(a.noul)) throw Error("DIRECT_NOUL_RANGE");
      answers[key] = { type: "noul", noul: a.noul };
    } else if (q.type === "choice") {
      const probabilities = distribution(a.probabilities, q.criteria);
      if (
        !id(a.choice) ||
        !Object.hasOwn(q.criteria, a.choice) ||
        !probability(a.confidence) ||
        probabilities[a.choice] !== Math.max(...Object.values(probabilities))
      )
        throw Error("DIRECT_CHOICE_MEMBERSHIP");
      answers[key] = { type: "choice", choice: a.choice, confidence: a.confidence, probabilities };
    } else if (q.type === "score") {
      const legend = Object.fromEntries(q.criteria.map((value, index) => [String(index), value]));
      const probabilities = distribution(a.probabilities, legend);
      if (
        canonicalJson(a.legend) !== canonicalJson(legend) ||
        !probability(a.confidence) ||
        typeof a.score !== "number" ||
        !Number.isFinite(a.score) ||
        a.score < 0 ||
        a.score > q.criteria.length - 1 ||
        Math.abs(
          a.score -
            Object.entries(probabilities).reduce(
              (sum, [index, value]) => sum + Number(index) * value,
              0,
            ),
        ) > 0.03
      )
        throw Error("DIRECT_SCORE_VALUE_OR_LEGEND");
      answers[key] = {
        type: "score",
        score: a.score,
        confidence: a.confidence,
        legend: clone(legend),
        probabilities,
      };
    } else {
      const binding = p.boxes[key]!,
        presence = answers[binding.presenceQuestionId];
      if (!presence || presence.type !== "noul" || presence.noul < binding.minimumPresence) {
        ignoredQuestionIds.push(key);
        continue;
      }
      if (
        !object(a.box) ||
        !sameKeys(a.box, { top: 0, bottom: 0, left: 0, right: 0 }) ||
        !Object.values(a.box).every(probability) ||
        (a.box.top as number) >= (a.box.bottom as number) ||
        (a.box.left as number) >= (a.box.right as number) ||
        (a.confidence !== null && !probability(a.confidence))
      )
        throw Error("DIRECT_BOX_GEOMETRY");
      answers[key] = {
        type: "box",
        box: clone(a.box) as unknown as BoxEdges,
        confidence: a.confidence as number | null,
      };
    }
  }
  return {
    schema: "jev.direct-selection.v2",
    requestDigest: p.requestDigest,
    answers,
    ignoredQuestionIds,
    executionAuthorityGranted: false,
  };
}

export interface SemanticAdapterCapability {
  requestSchemas: string[];
  questionTypes: string[];
  assetModes: ("base64" | "multipart")[];
  experimentalImages: boolean;
  source: ContractPin;
}
/** Capability declarations need the native owner's fresh qualification; this is an admission check only. */
export function qualifyDirectAdapter(
  p: PreparedDirectDecision,
  adapter: SemanticAdapterCapability,
): { status: "admitted-source-contract" | "held"; reason: string } {
  try {
    intact(p);
    validateContractPin(adapter.source);
    if (
      !adapter.requestSchemas.includes(p.request.schema) ||
      Object.values(p.request.questions).some((q) => !adapter.questionTypes.includes(q.type)) ||
      Object.values(p.request.assets ?? {}).some(
        (a) => !adapter.assetModes.includes("part" in a ? "multipart" : "base64"),
      ) ||
      ((p.request.imageOptIn || Object.keys(p.boxes).length > 0) && !adapter.experimentalImages)
    )
      return { status: "held", reason: "native-adapter-v2-extension-required" };
    return {
      status: "admitted-source-contract",
      reason: "declared-adapter-contract-covers-request-native-owner-qualification-required",
    };
  } catch {
    return { status: "held", reason: "malformed-adapter-or-request" };
  }
}
/** Pure HTTP/SDK body projection for an admitted adapter. Does not send requests or supply credentials.
 * Multipart framing, actual decoding and processed/expanded transport byte limits remain native obligations.
 */
export function projectDirectRequest(
  p: PreparedDirectDecision,
  adapter: SemanticAdapterCapability,
): {
  body: {
    model: "jev-1.14.0";
    state: Record<string, Json>;
    questions: Record<string, DirectQuestion>;
    assets?: Record<string, ImageAsset>;
  };
  headers: Record<string, string>;
} {
  if (qualifyDirectAdapter(p, adapter).status !== "admitted-source-contract")
    throw Error("NATIVE_ADAPTER_V2_EXTENSION_REQUIRED");
  const headers: Record<string, string> = p.request.imageOptIn
    ? { "TypeSafe-Danger-Zone-Images": "true" }
    : {};
  return clone({
    body: {
      model: "jev-1.14.0",
      state: { ...p.request.state, scope: p.request.scope as unknown as Json },
      questions: p.request.questions,
      ...(p.request.assets ? { assets: p.request.assets } : {}),
    },
    headers,
  });
}

export interface CurrentActionBinding {
  authorized: boolean;
  requestDigest: string;
  bindingDigest: string;
  sourceRevision: string;
  actionDigest: string;
  authorizationRef: string;
  permissionDigest: string;
  preconditionDigest: string;
  occurrenceId: string;
  effectFence: "clear" | "completed" | "unknown";
  evidenceRefs: string[];
}
export interface NativeActionObservation {
  status: "observed" | "failed" | "unknown";
  observationId: string;
  resultRevision: number;
  evidenceRefs: string[];
  postconditionSatisfied: boolean;
  result: Json;
}
export interface DirectActionReceipt {
  schema: "jev.semantic-action-receipt.v2";
  requestId: string;
  requestDigest: string;
  actionId: string;
  actionDigest: string;
  opportunity: SemanticOpportunity;
  status: "held" | "abstained" | "verified-action" | "action-failed" | "action-unknown";
  reason: string;
  dispatched: boolean;
  occurrenceId: string | null;
  observation: NativeActionObservation | null;
  arguments: Record<string, Json> | null;
  evidenceRefs: string[];
  failure: null | {
    boundary: "post-dispatch" | "owner-persistence";
    originalStatus: string;
    observationId: string | null;
  };
  executionAuthorityGranted: false;
  taskCompletionGranted: false;
  retryAllowed: false;
}
/** One injected native-owner action. Existing owners retain effects, immutable receipts and persistence.
 * The selected answer supplies fixed typed arguments only; no model text becomes code or a tool name.
 */
export async function consumeDirectAction(input: {
  prepared: PreparedDirectDecision;
  selection: DirectSelection;
  actionId: string;
  adapter: SemanticAdapterCapability;
  handlers: ReadonlyMap<
    string,
    (action: DirectActionBinding, args: Record<string, Json>) => Promise<NativeActionObservation>
  >;
  observeCurrent(action: DirectActionBinding): Promise<CurrentActionBinding>;
  /** Optional existing owner write, invoked once. Ambiguous writes are never retried. */
  persist?: (serialized: string) => Promise<void>;
}): Promise<DirectActionReceipt> {
  const p = input.prepared;
  const action = p.actions.find((item) => item.actionId === input.actionId);
  const receipt: DirectActionReceipt = {
    schema: "jev.semantic-action-receipt.v2",
    requestId: p.request.requestId,
    requestDigest: p.requestDigest,
    actionId: input.actionId,
    actionDigest: action ? semanticDigest(action) : "",
    opportunity: clone(p.opportunity),
    status: "held",
    reason: "action-binding-or-adapter-held",
    dispatched: false,
    occurrenceId: null,
    observation: null,
    arguments: null,
    evidenceRefs: [],
    failure: null,
    executionAuthorityGranted: false,
    taskCompletionGranted: false,
    retryAllowed: false,
  };
  let rawStatus: string | null = null,
    rawObservationId: string | null = null;
  try {
    intact(p);
    if (
      !action ||
      qualifyDirectAdapter(p, input.adapter).status === "held" ||
      input.selection.schema !== "jev.direct-selection.v2" ||
      input.selection.requestDigest !== p.requestDigest ||
      input.selection.executionAuthorityGranted !== false
    )
      return receipt;
    // Recheck the typed selected values; a changed result cannot bypass answer geometry or membership.
    const checked = normalizeDirectReply(p, {
      schema: "jev.semantic-result.v2",
      consumptionPolicy: DIRECT_POLICY,
      status: "ok",
      requestId: p.request.requestId,
      stateDigest: p.request.stateDigest,
      requestDigest: p.requestDigest,
      purpose: "semantic_bundle",
      requestedModel: JEV_MODEL,
      returnedModel: JEV_MODEL,
      sourcePostconditionVerified: true,
      executionAuthorityGranted: false,
      automaticWrite: false,
      answers: Object.fromEntries(
        Object.entries(p.request.questions).map(([key, q]) => [
          key,
          input.selection.answers[key] ?? (q.type === "box" ? { type: "box" } : null),
        ]),
      ),
    });
    if (canonicalJson(checked) !== canonicalJson(input.selection)) return receipt;
    if (action.when) {
      const selected = checked.answers[action.when.questionId];
      if (selected?.type !== "choice" || !action.when.choices.includes(selected.choice))
        return { ...receipt, status: "abstained", reason: "selected-branch-does-not-apply" };
    }
    const args: Record<string, Json> = Object.create(null);
    for (const [key, binding] of Object.entries(action.arguments)) {
      if (binding.kind === "literal") args[key] = clone(binding.value);
      else {
        const answer = checked.answers[binding.questionId];
        if (!answer)
          return { ...receipt, status: "abstained", reason: "presence-gated-answer-unavailable" };
        args[key] =
          answer.type === "choice"
            ? answer.choice
            : answer.type === "noul"
              ? answer.noul
              : answer.type === "score"
                ? answer.score
                : (clone(answer.box) as unknown as Json);
      }
    }
    receipt.arguments = clone(args);
    const handler = input.handlers.get(action.handlerKey);
    if (!handler) return { ...receipt, reason: "fixed-native-owner-handler-missing" };
    const currentMatches = (current: CurrentActionBinding): boolean =>
      !!current &&
      current.authorized === true &&
      current.requestDigest === p.requestDigest &&
      current.bindingDigest === p.bindingDigest &&
      current.sourceRevision === p.sourceRevision &&
      current.actionDigest === receipt.actionDigest &&
      current.authorizationRef === action.authorizationRef &&
      current.permissionDigest === action.permissionDigest &&
      current.preconditionDigest === action.preconditionDigest &&
      id(current.occurrenceId) &&
      refs(current.evidenceRefs);
    const current = clone(await input.observeCurrent(action));
    if (!currentMatches(current) || current.effectFence !== "clear")
      return { ...receipt, reason: "native-permission-currentness-or-effect-fence-held" };
    // Recheck state and action after the native owner's asynchronous precondition observation.
    intact(p);
    receipt.evidenceRefs = [...current.evidenceRefs];
    receipt.occurrenceId = current.occurrenceId;
    receipt.dispatched = true;
    const rawObservation = await handler(clone(action), clone(args));
    const safeText = (read: () => unknown): string | null => {
      try {
        const value = read();
        return typeof value === "string" ? value.slice(0, 1024) : null;
      } catch {
        return null;
      }
    };
    rawStatus = safeText(() => rawObservation?.status);
    rawObservationId = safeText(() => rawObservation?.observationId);
    const o = clone(rawObservation);
    if (
      !o ||
      !["observed", "failed", "unknown"].includes(o.status) ||
      !id(o.observationId) ||
      !Number.isSafeInteger(o.resultRevision) ||
      o.resultRevision < 1 ||
      !refs(o.evidenceRefs) ||
      typeof o.postconditionSatisfied !== "boolean" ||
      !Object.hasOwn(o, "result")
    )
      throw Error("ACTION_OBSERVATION_INVALID");
    receipt.observation = o;
    receipt.status =
      o.status === "unknown"
        ? "action-unknown"
        : o.status !== "observed" || !o.postconditionSatisfied
          ? "action-failed"
          : "verified-action";
    receipt.reason =
      receipt.status === "verified-action"
        ? "native-action-postcondition-observed"
        : "native-action-outcome-not-verified-no-retry";
    intact(p);
    const after = clone(await input.observeCurrent(action));
    if (
      !currentMatches(after) ||
      after.occurrenceId !== current.occurrenceId ||
      after.effectFence !== "completed"
    ) {
      receipt.status = "action-unknown";
      receipt.reason = "post-action-currentness-or-effect-fence-unknown-no-retry";
    } else receipt.evidenceRefs.push(...after.evidenceRefs);
    intact(p);
  } catch {
    receipt.status = receipt.dispatched ? "action-unknown" : "held";
    receipt.reason = receipt.dispatched
      ? "post-dispatch-failure-no-retry"
      : "pre-dispatch-binding-or-currentness-held";
    if (receipt.dispatched)
      receipt.failure = {
        boundary: "post-dispatch",
        originalStatus: rawStatus ?? "unknown",
        observationId: rawObservationId,
      };
  }
  if (input.persist && receipt.dispatched) {
    try {
      await input.persist(canonicalJson(receipt));
    } catch {
      receipt.failure = {
        boundary: "owner-persistence",
        originalStatus: receipt.status,
        observationId: receipt.observation?.observationId ?? null,
      };
      receipt.status = "action-unknown";
      receipt.reason = "owner-persistence-outcome-unknown-no-retry";
    }
  }
  return receipt;
}
