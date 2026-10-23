import { test, expect, afterAll } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, rm, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  prepareInitialContinuityBundle,
  readWorkContinuity,
  type ContinuityEndpointRegistration,
} from "./work-continuity.ts";
import type { WorkRecordAdmission } from "./work-records.ts";
import type { CurrentWorkManifest } from "./work-reader.ts";
import { initializeCanonicalHandoff } from "@palantirkc/work-continuity/handoff";
import { createLegacyHermesOwnerReaders } from "./owner-readers.ts";
import { readCurrentEngineeringIdentity } from "./work-identity.ts";
import { currentEngineeringRecordAdmission } from "./work-records.ts";
import { observeNativeBoundary } from "./native-hook.ts";

const sha = (b: Uint8Array | string) => createHash("sha256").update(b).digest("hex");
const modes = ["continuity-record", "source-review", "ocp-review"] as const;
const marker = "SYNTHETIC_PRIVATE_STDIN_MARKER_97531";
const observations: Array<Record<string, unknown>> = [];

test("legacy owner facades require the original explicit owner instead of inferring data from canonical code location", async () => {
  expect(() => createLegacyHermesOwnerReaders("relative")).toThrow(
    "EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED",
  );
  await expect(readCurrentEngineeringIdentity("")).rejects.toThrow(
    "EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED",
  );
  await expect(currentEngineeringRecordAdmission("")).rejects.toThrow(
    "EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED",
  );
  const owner = createLegacyHermesOwnerReaders("/tmp/synthetic-original-owner");
  expect(owner.sourceOwnerRoot).toBe("/tmp/synthetic-original-owner");
  expect(owner.authority).toBe("none");
  expect(owner.nativePermissionGranted).toBe(false);
});

test("native observer rejects oversized selected metadata before creating any durable receipt", async () => {
  const f = await fixture();
  try {
    const receiptRoot = join(f.root, "receipts"),
      options = {
        manifestPath: f.manifestPath,
        receiptRoot,
        expectedManifestSha256: sha(await readFile(f.manifestPath)),
      };
    const base = {
      hook_event_name: "post_tool_call",
      profile: "default",
      session_id: "fixture-session",
    };
    const oversized = "x".repeat(1100000);
    await expect(observeNativeBoundary({ ...base, tool_name: oversized }, options)).rejects.toThrow(
      "Bounded native metadata",
    );
    for (const key of [
      "turn_id",
      "task_id",
      "model",
      "provider",
      "api_request_id",
      "response_model",
      "aux_task",
      "turn_exit_reason",
    ])
      await expect(
        observeNativeBoundary({ ...base, extra: { [key]: "x".repeat(301) } }, options),
      ).rejects.toThrow("Bounded native metadata");
    await expect(observeNativeBoundary({ ...base, profile: "../escape" }, options)).rejects.toThrow(
      "Explicit native boundary identity",
    );
    await expect(access(receiptRoot)).rejects.toMatchObject({ code: "ENOENT" });
    const result = await observeNativeBoundary(
      {
        ...base,
        tool_name: "terminal",
        extra: {
          model: "synthetic-model",
          provider: "fixture-provider",
          turn_id: "fixture-turn",
          task_id: "fixture-native-task",
          api_request_id: "fixture-request",
          error: "SYNTHETIC_PRIVATE_ERROR_NOT_CAPTURED",
        },
      },
      options,
    );
    const bytes = await readFile(result.observationPath),
      record = JSON.parse(bytes.toString());
    expect(bytes.length).toBeLessThan(1048576);
    expect(record.toolName).toBe("terminal");
    expect(record.execution.model).toBe("synthetic-model");
    expect(record.nativeErrorPresent).toBe(true);
    expect(bytes.toString()).not.toContain("SYNTHETIC_PRIVATE_ERROR_NOT_CAPTURED");
  } finally {
    await f.cleanup();
  }
}, 150_000);

/** Entire fixture is synthetic. Original historical cp15 data is never copied or rewritten. */
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "hermes-canonical-cli-")),
    source = join(root, "work"),
    endpoint = join(root, "endpoint"),
    adapter = join(root, "adapter");
  for (const path of [source, endpoint, adapter]) await mkdir(path, { mode: 0o700 });
  const workId = "hermes-platform-rearchitecture",
    projectId = "hermes-harness-engineering",
    issueId = "fixture-issue",
    specId = "fixture-spec",
    planId = "fixture-plan";
  const plan = {
    workId,
    status: "implementation-authorized",
    stages: [
      { id: "fixture-stage", outcome: "Synthetic source-only regression", dependencies: [] },
    ],
    architecture: { codeRepositories: [], layers: [], leads: [], nativeExecution: {}, models: {} },
    scope: {},
    budget: { closed: true },
    validation: {},
    effects: {},
    sourcePromotion: {},
    git: {},
    runtimeSetup: {},
  };
  const decisions = {
    decisions: [{ id: "D9", choice: "Synthetic fixture, no actual user authority claimed." }],
  };
  const authorization = {
    kind: "decision",
    projectId,
    payload: {
      status: "accepted",
      details: {
        programId: workId,
        priorCompletedWorkReopened: false,
        implementationResultsAccepted: false,
      },
    },
  };
  const raw = (v: unknown) => JSON.stringify(v, null, 2) + "\n";
  const pin = (role: string, v: unknown) => ({
    path: join(source, role + ".json"),
    sha256: sha(raw(v)),
  });
  const purposePins = [
      pin("plan", plan),
      pin("decisions", decisions),
      pin("authorization", authorization),
    ],
    authPin = pin("authorization", authorization);
  const common = {
      projectId,
      ontologyVersion: "0.2.0",
      createdAt: "2026-10-15T00:00:00.000Z",
      recordClass: "current-source-projection-of-approved-scope-not-new-user-acceptance",
    },
    joins = { workId, issueId, specId, planId };
  const issue = {
    ...common,
    schema: "hermes.work-issue.v1",
    kind: "issue",
    id: issueId,
    payload: {
      ...joins,
      authorizingDecision: authPin,
      sourcePins: purposePins,
      nativeTaskIds: [],
      nativeSessionIds: [],
      historicalAttributionInvented: false,
    },
  };
  const spec = {
    ...common,
    schema: "hermes.work-spec.v1",
    kind: "spec",
    id: specId,
    payload: {
      ...joins,
      revision: 1,
      authorizingDecision: authPin,
      sourcePins: purposePins,
      implementationResultsAccepted: false,
      nativePermissionGranted: false,
      requirements: plan.stages.map((s) => ({
        id: specId + ":" + s.id,
        stageId: s.id,
        outcome: s.outcome,
        dependsOn: s.dependencies,
      })),
      acceptedPlanSections: Object.fromEntries(
        [
          "architecture",
          "scope",
          "budget",
          "validation",
          "effects",
          "sourcePromotion",
          "git",
          "runtimeSetup",
        ].map((k) => [k, plan[k as keyof typeof plan]]),
      ),
    },
  };
  const binding = {
    ...common,
    schema: "hermes.work-plan-binding.v1",
    kind: "plan",
    id: planId,
    payload: { ...joins, revision: 1, source: pin("plan", plan), authorizingDecision: authPin },
  };
  const all = [
    ["issue", issue],
    ["spec", spec],
    ["plan-binding", binding],
    ["plan", plan],
    ["decisions", decisions],
    ["authorization", authorization],
  ] as const;
  const immutableSources = [];
  for (const [role, value] of all) {
    const path = role + ".json",
      bytes = raw(value);
    await writeFile(join(source, path), bytes);
    immutableSources.push({ role, path, sha256: sha(bytes), originPath: join(source, path) });
  }
  const checkpoint = {
    schema: "hermes.platform-rearchitecture.continuity.v1",
    workId,
    version: 15,
    status: "paused",
    completed: ["fixture-preserved-effect"],
    pending: ["Fixture successor"],
    uncertain: [],
    activeWriters: [],
    nextAction: "Read fixture only",
    updatedAtUtc: "2026-10-15T00:00:00.000Z",
  };
  await writeFile(join(source, "current-continuity.json"), raw(checkpoint));
  const admission: WorkRecordAdmission = {
    schema: "hermes.work-record-admission.v2",
    ownerRoot: root,
    ownerProjectId: projectId,
    workId,
    immutablePurposeSources: immutableSources,
    supplements: [],
    currentRecords: [
      {
        role: "current-continuity",
        path: "current-continuity.json",
        originPath: join(source, "current-continuity.json"),
        maxBytes: 262144,
        schemaPointer: "/schema",
        schema: checkpoint.schema,
        workPointer: "/workId",
      },
    ],
    decisionGroups: [],
    joins: [],
  };
  const registration: ContinuityEndpointRegistration = {
    schema: "hermes.work-continuity-endpoint.v1",
    endpointId: "fixture-cli-privacy",
    ownerProjectId: projectId,
    workId,
    checkpointId: "fixture-checkpoint",
    nativeRoot: endpoint,
    immutableSources,
    maxStateBytes: 1048576,
    records: admission,
  };
  await writeFile(
    join(source, "work-state.json"),
    raw({
      schema: "hermes.mutable-owner-work-state.v1",
      selection: null,
      boundaries: [],
      missedHookBoundaryIds: [],
      verifiedShares: [],
    }),
  );
  await initializeCanonicalHandoff(
    endpoint,
    "fixture-data-owner",
    await prepareInitialContinuityBundle(source, registration, issueId),
  );
  const pins = [];
  for (const [dest, origin] of [
    ["plan.json", "plan.json"],
    ["interview.json", "decisions.json"],
    ["checkpoint.json", "current-continuity.json"],
  ]) {
    const bytes = await readFile(join(source, origin!));
    await writeFile(join(adapter, dest!), bytes);
    pins.push({ path: join(adapter, dest!), sha256: sha(bytes) });
  }
  const manifest: CurrentWorkManifest & { sourceSets: unknown[] } = {
    schema: "hermes.platform-current-work.v1",
    workId,
    version: 15,
    ownerRoot: adapter,
    sourcePins: pins,
    engineering: {
      plan: pins[0]!,
      interview: pins[1]!,
      checkpoint: pins[2]!,
      continuityRegistration: registration,
    },
    profiles: [
      {
        handle: "default",
        role: "Synthetic CLI source test",
        nativeHome: adapter,
        nativeProjectIds: [],
        workspace: adapter,
        semanticStatus: "active-owner",
      },
    ],
    selections: [],
    nativeSourceRevision: "fixture-no-native-execution",
    sourceSets: [
      {
        id: "fixture-source-set",
        ownerRoot: adapter,
        profiles: ["default"],
        workIds: [workId],
        pins: [pins[0]!],
      },
    ],
  };
  const manifestPath = join(adapter, "current-work.json"),
    bytes = raw(manifest);
  await writeFile(manifestPath, bytes);
  async function build(entry: string, name: string) {
    const program = join(root, name);
    const compiler = Bun.spawn(
      [
        process.execPath,
        "build",
        entry,
        "--target=bun",
        "--outfile",
        program,
        "--define",
        "PLATFORM_NATIVE_MANIFEST_SHA256=" + JSON.stringify(sha(bytes)),
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    const [code] = await Promise.all([
      compiler.exited,
      new Response(compiler.stdout).text(),
      new Response(compiler.stderr).text(),
    ]);
    expect(code).toBe(0);
    return program;
  }
  const program = await build(join(import.meta.dir, "read-context.ts"), "successor.js");
  async function run(mode: string, input?: Uint8Array | string, entry = program) {
    const p = Bun.spawn(
      [
        process.execPath,
        "--no-env-file",
        entry,
        mode,
        "--manifest",
        manifestPath,
        "--profile",
        "default",
      ],
      {
        stdin:
          input === undefined
            ? "ignore"
            : new Blob([typeof input === "string" ? input : new Uint8Array(input).buffer]),
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const timer = setTimeout(() => p.kill("SIGKILL"), 50_000);
    try {
      const [code, out, error] = await Promise.all([
        p.exited,
        new Response(p.stdout).text(),
        new Response(p.stderr).text(),
      ]);
      return { code, out, error };
    } finally {
      clearTimeout(timer);
    }
  }
  return {
    root,
    source,
    adapter,
    registration,
    manifestPath,
    pins,
    run,
    build,
    cleanup: () => rm(root, { recursive: true, force: true }),
  };
}
test("all stdin commands reject malformed, invalid UTF-8, non-object and oversized bytes without echo or mutation", async () => {
  const f = await fixture();
  try {
    const before = await readWorkContinuity(f.registration);
    const samples: Array<{ id: string; input: string | Uint8Array; error: string }> = [
      { id: "unquoted-sensitive-token", input: marker, error: "INVALID_REQUEST_JSON" },
      {
        id: "malformed-sensitive-field",
        input: '{"private":"' + marker + '", "broken":}',
        error: "INVALID_REQUEST_JSON",
      },
      { id: "empty", input: "", error: "INVALID_REQUEST_JSON" },
      {
        id: "invalid-utf8",
        input: Buffer.concat([Buffer.from('{"private":"' + marker), Buffer.from([0xc3, 0x28])]),
        error: "INVALID_REQUEST_JSON",
      },
      { id: "null", input: "null", error: "INVALID_REQUEST_OBJECT" },
      { id: "array", input: JSON.stringify([marker]), error: "INVALID_REQUEST_OBJECT" },
      { id: "scalar", input: JSON.stringify(marker), error: "INVALID_REQUEST_OBJECT" },
      {
        id: "oversized-ascii",
        input: JSON.stringify({ private: marker, padding: "x".repeat(1048576) }),
        error: "REQUEST_TOO_LARGE",
      },
      // UTF-16 length is below 1 MiB; UTF-8 bytes exceed the admitted byte bound.
      {
        id: "oversized-multibyte",
        input: JSON.stringify({ private: marker, padding: "한".repeat(349526) }),
        error: "REQUEST_TOO_LARGE",
      },
    ];
    for (const mode of modes)
      for (const sample of samples) {
        const r = await f.run(mode, sample.input);
        expect(r.code).toBe(1);
        expect(r.out).toBe("");
        expect(r.error).toBe(sample.error + "\n");
        expect(r.error + r.out).not.toContain(marker);
        expect(r.error).not.toContain("SyntaxError");
        expect(r.error).not.toContain(" at ");
        observations.push({
          mode,
          case: sample.id,
          code: r.code,
          stderrCode: sample.error,
          inputBytes: Buffer.byteLength(sample.input),
          stderrBytes: Buffer.byteLength(r.error),
          secretEcho: false,
        });
      }
    expect((await readWorkContinuity(f.registration)).digest).toBe(before.digest);
  } finally {
    await f.cleanup();
  }
}, 300_000);

test("valid source pages, supplied evidence, context and continuity CAS retain their existing semantics", async () => {
  const f = await fixture();
  try {
    const ok = async (mode: string, input?: unknown) => {
      const r = await f.run(mode, input === undefined ? undefined : JSON.stringify(input));
      expect(r.code).toBe(0);
      expect(r.error).toBe("");
      return JSON.parse(r.out);
    };
    const before = await ok("continuity-read");
    const source = await ok("source-review", {
      mode: "pinned-source",
      sourceSetId: "fixture-source-set",
      paths: [f.pins[0]!.path],
      offset: 0,
      limit: 73,
    });
    expect(source.mode).toBe("pinned-source");
    expect(source.documents[0].content).toBe(
      (await readFile(f.pins[0]!.path, "utf8")).slice(0, 73),
    );
    expect(source.documents[0].complete).toBe(false);
    expect(source.authority).toBe("none");
    const content = "Synthetic evidence document";
    const packet = {
      review_id: "fixture-review",
      documents: [
        { id: "fixture-document", source: "fixture://source", content, sha256: sha(content) },
      ],
    };
    const evidence = await ok("ocp-review", {
      mode: "supplied-evidence",
      packet,
      packetSha256: sha(JSON.stringify(packet)),
      document_id: "fixture-document",
      offset: 10,
      limit: 8,
    });
    expect(evidence.documents[0].content).toBe(content.slice(10, 18));
    expect(evidence.packetDigestVerifiedByThisUtility).toBe(false);
    expect(evidence.liveSourceVerified).toBe(false);
    const boundary = {
      boundaryId: "fixture-cli-valid",
      kind: "verification",
      atUtc: "2026-10-15T00:00:00.000Z",
      sourceRefs: ["fixture://source"],
      completedEffectRefs: [],
      pendingWorkRefs: ["fixture://next"],
      decisionRefs: [],
      nativeTaskRunRefs: [],
    };
    const request = {
      expected: before.canonical.token,
      expectedDigest: before.canonical.sha256,
      boundary,
      core: {
        version: before.checkpoint.document.version + 1,
        updatedAtUtc: "2026-10-15T00:00:00.000Z",
        pending: ["Fixture successor action"],
        nextAction: "Read fixture successor",
      },
    };
    const recorded = await ok("continuity-record", request);
    expect(recorded.status).toBe("recorded");
    expect(recorded.checkpoint.document.pending).toEqual(["Fixture successor action"]);
    const replay = await ok("continuity-record", {
      ...request,
      expected: recorded.canonical.token,
      expectedDigest: recorded.canonical.sha256,
    });
    expect(replay.status).toBe("already-recorded");
    const context = await ok("context");
    expect(context.engineeringWork.checkpointVersion).toBe(request.core.version);
    expect(context.engineeringWork.pending).toEqual(request.core.pending);
    expect(context.nativePermissionGranted).toBe(false);
    const after = await readWorkContinuity(f.registration);
    for (const p of f.registration.immutableSources)
      expect(after.canonical.bundle.files.find((f) => f.path === p.path)!.sha256).toBe(p.sha256);
    // Exactly 1 MiB, including whitespace, remains a valid request.
    const exact = JSON.stringify({
      mode: "pinned-source",
      sourceSetId: "fixture-source-set",
      paths: [],
    });
    const atLimit = await f.run(
      "source-review",
      " ".repeat(1048576 - Buffer.byteLength(exact)) + exact,
    );
    expect(atLimit.code).toBe(0);
    expect(atLimit.error).toBe("");
    expect(JSON.parse(atLimit.out).documents).toHaveLength(1);
  } finally {
    await f.cleanup();
  }
}, 150_000);

test("mode/schema/owner failures stay non-echoing and stale continuity requests never write", async () => {
  const f = await fixture();
  try {
    const before = await readWorkContinuity(f.registration);
    for (const [mode, input] of [
      ["source-review", { mode: marker, paths: [] }],
      ["ocp-review", { mode: "pinned-source", private: marker }],
      ["continuity-record", { core: { version: 16 }, [marker]: marker }],
      [
        "continuity-record",
        {
          expected: before.token,
          expectedDigest: "0".repeat(64),
          boundary: { boundaryId: marker },
          core: { version: 16 },
        },
      ],
      ["source-review", { mode: "pinned-source", sourceSetId: marker, paths: [] }],
    ] as const) {
      const r = await f.run(mode, JSON.stringify(input));
      expect(r.code).toBe(1);
      expect(r.out).toBe("");
      expect(r.error).toBe("CONTEXT_COMMAND_FAILED\n");
      expect(r.error).not.toContain(marker);
    }
    expect((await readWorkContinuity(f.registration)).digest).toBe(before.digest);
  } finally {
    await f.cleanup();
  }
}, 150_000);

test("an uncertain canonical write remains a structured outcome that requires reconciliation", async () => {
  const f = await fixture();
  try {
    const before = await readWorkContinuity(f.registration);
    const request = {
      expected: before.token,
      expectedDigest: before.digest,
      boundary: {
        boundaryId: "fixture-write-lock",
        kind: "verification",
        atUtc: "2026-10-15T00:00:00.000Z",
        sourceRefs: ["fixture://source"],
        completedEffectRefs: [],
        pendingWorkRefs: ["fixture://next"],
        decisionRefs: [],
        nativeTaskRunRefs: [],
      },
      core: { version: 16, updatedAtUtc: "2026-10-15T00:00:00.000Z" },
    };
    await mkdir(join(f.registration.nativeRoot, ".writer-lock"), { mode: 0o700 });
    const r = await f.run("continuity-record", JSON.stringify(request));
    expect(r.code).toBe(0);
    expect(r.error).toBe("");
    const result = JSON.parse(r.out);
    expect(result.status).toBe("effect-unknown-reconcile-before-retry");
    expect(result.authority).toBe("none");
    expect(result.nativePermissionGranted).toBe(false);
    expect(result.executionReady).toBe(false);
    expect(result.expected).toEqual(before.token);
    expect((await readWorkContinuity(f.registration)).digest).toBe(before.digest);
  } finally {
    await f.cleanup();
  }
}, 150_000);

test.skipIf(!process.env.HERMES_CLI_PRIVACY_PREDECESSOR)(
  "preserved predecessor demonstrates the source-only parser disclosure regression",
  async () => {
    const predecessor = process.env.HERMES_CLI_PRIVACY_PREDECESSOR;
    if (!predecessor) return;
    const f = await fixture();
    try {
      const baseline = await f.build(predecessor, "predecessor.js");
      for (const mode of modes) {
        const r = await f.run(mode, marker, baseline);
        expect(r.code).not.toBe(0);
        const echo = (r.out + r.error).includes(marker);
        expect(echo).toBe(true);
        observations.push({
          mode,
          case: "preserved-predecessor",
          code: r.code,
          secretEcho: echo,
          stderrBytes: Buffer.byteLength(r.error),
          stderrSha256: sha(r.error),
        });
      }
    } finally {
      await f.cleanup();
    }
  },
  30000,
);

afterAll(async () => {
  const receipt = process.env.HERMES_CLI_PRIVACY_TEST_RECEIPT;
  if (receipt)
    await writeFile(
      receipt,
      JSON.stringify(
        {
          schema: "hermes.cli-input-privacy-subprocess-observations.v1",
          bunVersion: Bun.version,
          observations,
          privateInputsUsed: false,
          nativeEffects: false,
          modelEvalRequests: 0,
        },
        null,
        2,
      ) + "\n",
      { mode: 0o600 },
    );
});
