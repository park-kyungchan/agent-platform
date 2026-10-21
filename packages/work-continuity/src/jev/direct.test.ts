import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import {
  JEV_MODEL,
  DIRECT_POLICY,
  prepareDirectDecision,
  normalizeDirectReply,
  projectDirectRequest,
  qualifyDirectAdapter,
  consumeDirectAction,
  prepareSemanticDecision,
  normalizeSemanticReply,
  semanticDigest,
  type SemanticEvent,
  type WholeLens,
  type PreparedDirectDecision,
  type DirectActionBinding,
  type SemanticAdapterCapability,
  type CurrentActionBinding,
  type NativeActionObservation,
  type DirectQuestion,
} from "./index.ts";
const pin = (path = "source:fixed") => ({ path, sha256: "a".repeat(64) });
const event = (): SemanticEvent => ({
  eventId: "event:direct",
  kind: "engineering_result",
  sourceRevision: "source-v2",
  materialRevision: "result-1",
  material: true,
  privacy: "admitted",
  externalEvidence: false,
  facts: { report: "Feature degraded; workaround exists." },
  binding: {
    runtime: "native-codex",
    sessionId: "native-root",
    agentId: null,
    agentType: null,
    parentSessionId: null,
    rootSessionId: "native-root",
    lineageSessionIds: ["native-root"],
    profileId: null,
    nativeTaskId: null,
    nativeRunId: null,
    logical: null,
    admissionRef: "owner:admission",
  },
});
const lens = (): WholeLens => ({
  currentAuthority: {
    text: "Consume an allowed bounded action using source fixtures.",
    sourceRefs: ["user:authority"],
  },
  wholeObjective: "Preserve owners and direct typed judgments.",
  taskGoal: "Route one bounded source item.",
  acceptedWhy: [
    {
      text: "Source proof never grants native permission.",
      attribution: "current-user",
      sourceRefs: ["user:why"],
    },
  ],
  acceptedDecisions: [],
  owners: [{ role: "native", id: "codex" }],
  dependencies: [{ id: "source", revision: "source-v2", currentness: "current" }],
  completedEffectRefs: ["old:completed"],
  uncertainEffectRefs: ["old:unknown"],
  remainingAcceptance: ["native:activation"],
});
const action = (): DirectActionBinding => ({
  actionId: "route",
  handlerKey: "owner.route-bounded-item",
  effectClass: "native_permitted",
  authorizationRef: "user:bounded-effect",
  permissionDigest: "b".repeat(64),
  preconditionDigest: "c".repeat(64),
  sourceRefs: ["owner:fixed-handler"],
  when: { questionId: "route", choices: ["repair"] },
  arguments: {
    route: { kind: "answer", questionId: "route" },
    severity: { kind: "answer", questionId: "severity" },
    sufficient: { kind: "answer", questionId: "sufficient" },
    item: { kind: "literal", value: "item:stable" },
  },
});
const questions = (): Record<string, DirectQuestion> => ({
  route: {
    type: "choice",
    instructions: "Which allowed route fits this report?",
    criteria: { repair: { description: "Repair the source" }, defer: null },
  },
  severity: {
    type: "score",
    instructions: "How severe is the reported defect?",
    criteria: [
      "Cosmetic",
      { impact: "Feature degraded with workaround" },
      "Blocking with no workaround",
    ],
  },
  sufficient: {
    type: "noul",
    instructions: "Is current source evidence sufficient?",
    criteria: {
      true: "Direct source supports the scoped claim",
      false: "Required source is missing",
    },
  },
});
function prepare(
  overrides: Partial<Parameters<typeof prepareDirectDecision>[0]> = {},
): PreparedDirectDecision {
  const result = prepareDirectDecision({
    requestId: "request-direct",
    event: event(),
    lens: lens(),
    opportunity: {
      role: "child",
      unit: "item",
      stableItemId: "item:stable",
      revision: "1",
      nativeHookId: "owner:item-hook",
      consumerId: "owner:route-consumer",
    },
    questions: questions(),
    actions: [action()],
    ...overrides,
  });
  if (result.status !== "prepared-direct") throw Error("fixture-not-prepared");
  return result;
}
const adapter = (): SemanticAdapterCapability => ({
  requestSchemas: ["jev.semantic-request.v2"],
  questionTypes: ["choice", "noul", "score", "box"],
  assetModes: ["base64", "multipart"],
  experimentalImages: true,
  source: pin("owner:future-adapter"),
});
function reply(p: PreparedDirectDecision): Record<string, unknown> {
  return {
    schema: "jev.semantic-result.v2",
    consumptionPolicy: DIRECT_POLICY,
    requestId: p.request.requestId,
    stateDigest: p.request.stateDigest,
    requestDigest: p.requestDigest,
    purpose: "semantic_bundle",
    requestedModel: JEV_MODEL,
    returnedModel: "jev-1.14.0",
    status: "ok",
    sourcePostconditionVerified: true,
    executionAuthorityGranted: false,
    automaticWrite: false,
    answers: Object.fromEntries(
      Object.entries(p.request.questions).map(([key, q]) => [
        key,
        q.type === "choice"
          ? {
              type: "choice",
              choice: "repair",
              confidence: 0,
              probabilities: { repair: 0.5, defer: 0.5 },
            }
          : q.type === "score"
            ? {
                type: "score",
                score: 1.43,
                confidence: 0.01,
                legend: {
                  "0": "Cosmetic",
                  "1": { impact: "Feature degraded with workaround" },
                  "2": "Blocking with no workaround",
                },
                probabilities: { "0": 0, "1": 0.57, "2": 0.43 },
              }
            : q.type === "box"
              ? {
                  type: "box",
                  box: { top: 0.25, bottom: 0.82, left: 0.44, right: 0.84 },
                  confidence: null,
                }
              : { type: "noul", noul: 0.96 },
      ]),
    ),
  };
}
const replyAnswers = (raw: Record<string, unknown>) =>
  raw.answers as Record<string, Record<string, unknown>>;
const current = (
  p: PreparedDirectDecision,
  fence: CurrentActionBinding["effectFence"] = "clear",
): CurrentActionBinding => ({
  authorized: true,
  requestDigest: p.requestDigest,
  bindingDigest: p.bindingDigest,
  sourceRevision: p.sourceRevision,
  actionDigest: semanticDigest(p.actions[0]),
  authorizationRef: action().authorizationRef,
  permissionDigest: action().permissionDigest,
  preconditionDigest: action().preconditionDigest,
  occurrenceId: "native:occurrence-one",
  effectFence: fence,
  evidenceRefs: ["owner:fresh-permission"],
});
const observation = (): NativeActionObservation => ({
  status: "observed",
  observationId: "owner:effect-result",
  resultRevision: 2,
  evidenceRefs: ["owner:postcondition"],
  postconditionSatisfied: true,
  result: { recorded: true },
});
const call = (
  p: PreparedDirectDecision,
  overrides: Partial<Parameters<typeof consumeDirectAction>[0]> = {},
) => {
  let observes = 0;
  return consumeDirectAction({
    prepared: p,
    selection: normalizeDirectReply(p, reply(p)),
    actionId: "route",
    adapter: adapter(),
    handlers: new Map([[action().handlerKey, async () => observation()]]),
    observeCurrent: async () => current(p, ++observes === 1 ? "clear" : "completed"),
    ...overrides,
  });
};
describe("W02 official typed direct boundary", () => {
  test("allowed tied Choice and fractional Score survive zero confidence into one fixed native action and owner receipt", async () => {
    const p = prepare();
    let calls = 0,
      writes = 0;
    const result = await call(p, {
      handlers: new Map([
        [
          action().handlerKey,
          async (bound, args) => {
            calls++;
            expect(bound.handlerKey).toBe("owner.route-bounded-item");
            expect(args).toEqual({
              route: "repair",
              severity: 1.43,
              sufficient: 0.96,
              item: "item:stable",
            });
            return observation();
          },
        ],
      ]),
      persist: async (serialized) => {
        writes++;
        const stored = JSON.parse(serialized);
        expect(stored.dispatched).toBe(true);
        expect(stored.observation.resultRevision).toBe(2);
        expect(stored.opportunity.unit).toBe("item");
      },
    });
    expect(result.status).toBe("verified-action");
    expect(result.dispatched).toBe(true);
    expect(result.executionAuthorityGranted).toBe(false);
    expect(result.retryAllowed).toBe(false);
    expect(calls).toBe(1);
    expect(writes).toBe(1);
    const projected = projectDirectRequest(p, adapter());
    expect(projected.body.model).toBe("jev-1.14.0");
    expect(projected.body.state.scope).toBeDefined();
    expect(projected.body.questions.severity?.type).toBe("score");
  });
  test("malformed, wrong-version, API-error and missing answers are held independently of confidence", () => {
    const p = prepare();
    for (const patch of [
      { schema: "jev.semantic-result.v1" },
      { consumptionPolicy: "other" },
      { status: "error" },
      { requestDigest: "wrong" },
      { stateDigest: "wrong" },
      { returnedModel: "jev-preview" },
      { sourcePostconditionVerified: false },
      { answers: {} },
    ])
      expect(() => normalizeDirectReply(p, { ...reply(p), ...patch })).toThrow();
    for (const patch of [
      { score: 2.1 },
      { score: 0.5 },
      { confidence: 2 },
      { legend: { "0": "wrong" } },
    ]) {
      const r = reply(p);
      Object.assign(replyAnswers(r).severity!, patch);
      expect(() => normalizeDirectReply(p, r)).toThrow();
    }
    const r = reply(p);
    replyAnswers(r).route!.choice = "unoffered";
    expect(() => normalizeDirectReply(p, r)).toThrow();
  });
  test("v1 native capability is an explicit v2 hold, before handler execution", async () => {
    const p = prepare(),
      old = {
        ...adapter(),
        requestSchemas: ["jev.semantic-request.v1"],
        questionTypes: ["noul", "choice"],
        assetModes: [],
      };
    expect(qualifyDirectAdapter(p, old).reason).toBe("native-adapter-v2-extension-required");
    expect(() => projectDirectRequest(p, old)).toThrow("NATIVE_ADAPTER_V2_EXTENSION_REQUIRED");
    const result = await call(p, {
      adapter: old,
      handlers: new Map([
        [
          action().handlerKey,
          async () => {
            throw Error("must not run");
          },
        ],
      ]),
    });
    expect(result.status).toBe("held");
    expect(result.dispatched).toBe(false);
  });
  test("pinned real predecessor rejects v2 while the exact current caller/helper admits v1 and direct v2", () => {
    const base = prepareSemanticDecision({
      requestId: "request-v1",
      event: event(),
      lens: lens(),
      candidates: [],
    });
    if (base.status !== "prepared") throw Error("fixture");
    const parser = new URL(
      "../../../native-hermes-adapter/native/jev/semantic_caller.py",
      import.meta.url,
    ).pathname;
    const fixture = new URL(
      "../../../native-hermes-adapter/native/jev/fixtures/predecessor_v1_caller.py",
      import.meta.url,
    );
    const pin = JSON.parse(
      readFileSync(new URL("./predecessor_v1_caller.pin.json", fixture), "utf8"),
    );
    expect(createHash("sha256").update(readFileSync(fixture)).digest("hex")).toBe(pin.sha256);
    const run = (source: string, request: unknown) =>
      Bun.spawnSync(
        [
          "python3",
          "-B",
          "-c",
          "import importlib.util,pathlib,runpy,sys; p=pathlib.Path(sys.argv[1]); h=p.with_name('v2_contract.py'); s=importlib.util.spec_from_file_location('v2_contract',h) if h.is_file() else None; m=importlib.util.module_from_spec(s) if s else None; sys.modules.update({'v2_contract':m} if s else {}); s.loader.exec_module(m) if s else None; runpy.run_path(str(p),run_name='source_contract_fixture')['parse_request'](sys.stdin.buffer.read())",
          source,
        ],
        { stdin: Buffer.from(JSON.stringify(request)), stdout: "pipe", stderr: "pipe" },
      ).exitCode;
    expect(run(fixture.pathname, base.request)).toBe(0);
    expect(run(fixture.pathname, prepare().request)).not.toBe(0);
    expect(run(parser, base.request)).toBe(0);
    expect(run(parser, prepare().request)).toBe(0);
    expect(() => normalizeSemanticReply(base, reply(prepare()))).toThrow();
  });
  test("fixed permissions and completed/unknown fences cannot be reused or changed by model output", async () => {
    for (const patch of [
      { authorized: false },
      { sourceRevision: "stale" },
      { authorizationRef: "foreign" },
      { permissionDigest: "d".repeat(64) },
      { effectFence: "unknown" },
      { effectFence: "completed" },
    ]) {
      const p = prepare();
      const result = await call(p, {
        observeCurrent: async () => ({ ...current(p), ...patch }) as CurrentActionBinding,
      });
      expect(result.dispatched).toBe(false);
      expect(result.status).toBe("held");
    }
    const p = prepare();
    let observations = 0;
    const result = await call(p, {
      observeCurrent: async () => (++observations === 1 ? current(p) : (null as never)),
    });
    expect(result.status).toBe("action-unknown");
    expect(result.dispatched).toBe(true);
    expect(result.retryAllowed).toBe(false);
  });
  test("post-action reused mutable binding cannot replace the original occurrence", async () => {
    const p = prepare(),
      reused = current(p);
    let observes = 0;
    const result = await call(p, {
      observeCurrent: async () => {
        if (++observes > 1) {
          reused.occurrenceId = "native:foreign-occurrence";
          reused.effectFence = "completed";
        }
        return reused;
      },
    });
    expect(result.status).toBe("action-unknown");
    expect(result.occurrenceId).toBe("native:occurrence-one");
  });
  test("cyclic raw native outcome and ambiguous existing owner writes keep primitive original metadata", async () => {
    const p = prepare();
    const result = await call(p, {
      handlers: new Map([
        [
          action().handlerKey,
          async () => {
            const raw = observation();
            Object.assign(raw, { loop: raw });
            return raw;
          },
        ],
      ]),
    });
    expect(result.status).toBe("action-unknown");
    expect(result.dispatched).toBe(true);
    expect(result.failure).toMatchObject({
      originalStatus: "observed",
      observationId: "owner:effect-result",
    });
    expect(() => JSON.stringify(result)).not.toThrow();
    let writes = 0;
    const failed = await call(prepare(), {
      persist: async () => {
        writes++;
        throw Error("ambiguous owner receipt");
      },
    });
    expect(failed.status).toBe("action-unknown");
    expect(failed.failure?.boundary).toBe("owner-persistence");
    expect(failed.failure?.originalStatus).toBe("verified-action");
    expect(writes).toBe(1);
  });
  test("unused fan-out branch does not execute its fixed action", async () => {
    const p = prepare();
    const raw = reply(p);
    replyAnswers(raw).route!.choice = "defer";
    expect((await call(p, { selection: normalizeDirectReply(p, raw) })).status).toBe("abstained");
    for (const unit of [
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
    ] as const)
      expect(prepare({ opportunity: { ...p.opportunity, unit } }).opportunity.unit).toBe(unit);
  });
});

const imageRef = "opaque-image-70fa";
function imageFixture(mode: "base64" | "multipart" = "multipart") {
  // Synthetic owner evidence tests this source boundary, never image decoding or native acceptance.
  const bytes = Buffer.from("synthetic-owner-decoded-image");
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  return {
    assets: {
      [imageRef]:
        mode === "multipart"
          ? { type: "image" as const, part: "photo", format: "png" as const }
          : {
              type: "image" as const,
              format: "png" as const,
              data_base64: bytes.toString("base64"),
            },
    },
    imageEvidence: {
      [imageRef]: {
        source: { path: "owner:synthetic-image", sha256 },
        sha256,
        width: 800,
        height: 600,
        byteLength: bytes.length,
        processedByteLength: 4096,
        format: "png" as const,
        still: true as const,
        decoded: true as const,
        evidenceRefs: ["owner:synthetic-decoder"],
      },
    },
  };
}
function imagePrepare(overrides: Partial<Parameters<typeof prepareDirectDecision>[0]> = {}) {
  const a = action();
  a.arguments.region = { kind: "answer", questionId: "region" };
  return prepare({
    ...imageFixture(),
    imageOptIn: true,
    event: { ...event(), facts: { photo: imageRef } },
    questions: {
      ...questions(),
      region: { type: "box", instructions: "The defect region in `facts.photo`" },
      present: { type: "noul", instructions: "Is the defect visible in `facts.photo`?" },
    },
    boxes: { region: { assetRef: imageRef, presenceQuestionId: "present", minimumPresence: 0.6 } },
    actions: [a],
    ...overrides,
  });
}
describe("W02 experimental assets and presence-before-Box consumption", () => {
  test("Box geometry is used as exact normalized edges only after the separate presence answer", async () => {
    const p = imagePrepare(),
      selection = normalizeDirectReply(p, reply(p));
    expect(selection.answers.region).toEqual({
      type: "box",
      box: { top: 0.25, bottom: 0.82, left: 0.44, right: 0.84 },
      confidence: null,
    });
    expect(projectDirectRequest(p, adapter()).headers).toEqual({
      "TypeSafe-Danger-Zone-Images": "true",
    });
    const r = reply(p);
    replyAnswers(r).present!.noul = 0.1;
    const absent = normalizeDirectReply(p, r);
    expect(absent.answers.region).toBeUndefined();
    expect(absent.ignoredQuestionIds).toEqual(["region"]);
    expect((await call(p, { selection: absent })).status).toBe("abstained");
    const invalid = reply(p);
    (replyAnswers(invalid).region!.box as Record<string, unknown>).left = 0.99;
    expect(() => normalizeDirectReply(p, invalid)).toThrow("DIRECT_BOX_GEOMETRY");
  });
  test("image opt-in, actual representation, presence binding and owner geometry are mandatory", () => {
    expect(() => imagePrepare({ imageOptIn: false })).toThrow(
      "DIRECT_EXPERIMENTAL_IMAGES_OPT_IN_REQUIRED",
    );
    expect(() => imagePrepare({ event: event() })).toThrow("DIRECT_IMAGE_NOT_REPRESENTED");
    expect(() => imagePrepare({ boxes: {} })).toThrow("DIRECT_BOX_BINDING_REQUIRED");
    for (const width of [279, 1569, 300.5]) {
      const fixture = imageFixture();
      fixture.imageEvidence[imageRef]!.width = width;
      expect(() => imagePrepare(fixture)).toThrow("DIRECT_IMAGE_OWNER_GEOMETRY_OR_BYTES");
    }
    const huge = imageFixture();
    huge.imageEvidence[imageRef]!.processedByteLength = 33554432;
    expect(() => imagePrepare(huge)).toThrow("DIRECT_IMAGE_REQUEST_BOUND");
  });
  test("base64 is canonical and bytes/hash-bound; multipart dual-mode is rejected explicitly", () => {
    expect(imagePrepare(imageFixture("base64")).request.assets?.[imageRef]).toBeDefined();
    const invalid = imageFixture("base64");
    (invalid.assets[imageRef] as { data_base64: string }).data_base64 += "\n";
    expect(() => imagePrepare(invalid)).toThrow("DIRECT_BASE64_ASSET");
    const wrong = imageFixture("base64");
    wrong.imageEvidence[imageRef]!.sha256 = "f".repeat(64);
    wrong.imageEvidence[imageRef]!.source.sha256 = "f".repeat(64);
    expect(() => imagePrepare(wrong)).toThrow("DIRECT_ASSET_BYTES_MISMATCH");
    const dual = imageFixture();
    Object.assign(dual.assets[imageRef], { data_base64: "AAAA" });
    expect(() => imagePrepare(dual)).toThrow("DIRECT_MULTIPART_ASSET");
  });
  test("all-state image expansion counts direct-boundary metadata and projected scope", () => {
    const p = imagePrepare(),
      fixture = imageFixture();
    // One represented state use plus the Box binding's state metadata use exceeds 32 MiB.
    fixture.imageEvidence[imageRef]!.processedByteLength = 18000000;
    expect(() => imagePrepare(fixture)).toThrow("DIRECT_IMAGE_REQUEST_BOUND");
    fixture.imageEvidence[imageRef]!.processedByteLength = 9000000;
    const withScope = lens();
    withScope.taskGoal += ` ${imageRef}`;
    expect(() => imagePrepare({ ...fixture, lens: withScope })).toThrow(
      "DIRECT_IMAGE_REQUEST_BOUND",
    );
    expect(p.imageEvidence[imageRef]!.processedByteLength).toBe(4096);
  });
});
