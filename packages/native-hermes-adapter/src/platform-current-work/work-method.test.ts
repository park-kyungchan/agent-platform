import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  workMethodPacketSha256,
  workMethodRequiredSources,
  type WorkMethodAdmissionInput,
  type WorkMethodNativeIds,
} from "@palantirkc/work-continuity/context";
import {
  readHermesWorkMethod,
  type HermesWorkMethodRequest,
  type HermesWorkMethodTuple,
} from "./work-method.ts";

const sha = (text: string) => createHash("sha256").update(text).digest("hex");
const ref = (path: string) => ({ kind: "snapshot" as const, path, sha256: sha(path) });
const verified = { verifySourceClosure: async () => {} };
const ids = (): WorkMethodNativeIds => ({
  threadId: null,
  sessionId: "native-session-A",
  taskId: null,
  runId: null,
  turnId: null,
  parentThreadId: null,
});
const tuple = (): HermesWorkMethodTuple => ({
  runtime: "native-hermes",
  profileId: "default",
  sessionId: "native-session-A",
  agentId: null,
  nativeProjectId: null,
  nativeTaskId: null,
  nativeRunId: null,
  turnId: null,
});
function rebind(request: HermesWorkMethodRequest): void {
  const input = request.admission;
  input.packet.task.sha256 = sha(input.packet.task.text);
  input.receipt.packetSha256 = workMethodPacketSha256(input.packet);
  input.receipt.taskSha256 = input.packet.task.sha256;
  request.selection.packetSha256 = input.receipt.packetSha256;
  input.advice.packetSha256 = input.receipt.packetSha256;
  input.advice.taskSha256 = input.packet.task.sha256;
  input.sources = workMethodRequiredSources(
    input.packet,
    input.standing,
    input.receipt,
    input.release,
  ).map((source) => ({
    source,
    actualSha256: source.kind === "native" ? null : source.sha256!,
    currentness: "current",
  }));
}
function fixture(): HermesWorkMethodRequest {
  const protocol = { id: "method", version: "1.0.0", sha256: sha("method") };
  const adapter = { id: "hermes", version: "1.0.0", sha256: sha("hermes") };
  const owners = [
    { role: "source" as const, id: "common-owner" },
    { role: "native" as const, id: "hermes-owner" },
  ];
  const logical = { domainId: "d", projectId: "p", workId: "w", taskId: "bootstrap" };
  const actor = {
    actorId: "actor-A",
    environmentId: "hermes349",
    runtime: "native-hermes" as const,
    profileId: "default",
    paneId: null,
    terminalId: null,
    native: ids(),
  };
  const input: WorkMethodAdmissionInput = {
    standing: {
      schema: "common.agent-work-method.standing.v1",
      protocol,
      purpose: "Whole approved useful delivery.",
      acceptedWhy: "Preserve the actual purpose and accepted user decisions.",
      authority: ref("authority"),
      owners,
      routes: [{ id: "owner-task", kind: "task", adapter, supported: true, maxPacketBytes: 12000 }],
    },
    packet: {
      schema: "common.agent-work-method.scoped.v1",
      packetId: "packet-A",
      epochId: "epoch-A",
      protocol,
      adapter,
      logical,
      semantic: { issue: null, spec: null, plan: null },
      task: {
        text: "Prepare compatible Hermes owner projection.",
        sha256: sha("pending"),
        source: ref("task"),
      },
      lens: {
        currentAuthority: { text: "Approved finite bootstrap.", sourceRefs: ["authority"] },
        wholeObjective: "Whole approved useful delivery.",
        taskGoal: "Prepare compatible source only.",
        acceptedWhy: [
          { text: "Preserve actual purpose.", attribution: "current-user", sourceRefs: ["answer"] },
        ],
        acceptedDecisions: [{ id: "common-first", revision: "1", sourceRefs: ["answer"] }],
        owners: [
          ...owners,
          { role: "domain", id: "domain-owner" },
          { role: "data", id: "data-owner" },
        ],
        dependencies: [],
        completedEffectRefs: ["consumed-old"],
        uncertainEffectRefs: ["unknown-old"],
        remainingAcceptance: ["Native use and domain Acceptance remain separate."],
      },
      authority: ref("authority"),
      phase: "bootstrap",
      role: "native owner bootstrap",
      ownerTransitions: [],
      native: ids(),
      references: {
        purpose: [ref("purpose")],
        decisions: [ref("answer")],
        context: [ref("checkpoint")],
        memory: {
          status: "none-applicable",
          sources: [],
          reason: "No additional Memory needed for this finite source Task.",
        },
      },
      source: {
        access: "read-only",
        repositoryId: "common",
        baseCommit: "a".repeat(40),
        headCommit: "b".repeat(40),
        branch: "work/bootstrap",
        worktree: "/source/bootstrap",
        ownedPaths: [],
        resultPath: "/assigned/bootstrap.json",
      },
      fences: {
        completed: ["consumed-old"],
        consumed: ["closed-old"],
        uncertain: ["unknown-old"],
        source: ref("fences"),
        budget: ref("same-usd14"),
        permissions: ref("controls"),
      },
      prerequisites: [],
    },
    receipt: {
      schema: "common.agent-work-method.receipt.v1",
      id: "receipt-A",
      observedAt: "2026-10-20T01:00:00Z",
      actor,
      routeId: "owner-task",
      logical,
      packetId: "packet-A",
      packetSha256: sha("pending"),
      taskSha256: sha("pending"),
      protocol,
      adapter,
      currentness: "current",
      received: true,
      used: true,
      useEvidence: [ref("use")],
      authorityReloaded: false,
      admission: {
        ownerId: "hermes-owner",
        point: "owner-before-effects",
        mechanism: "owner-admission",
        consumedBeforeEffects: true,
        evidence: [ref("admission")],
      },
      transitionRefs: [],
    },
    expectedActor: structuredClone(actor),
    routeId: "owner-task",
    currentness: { protocol, adapter, status: "current" },
    sources: [],
    prerequisites: [],
    concurrent: [],
    advice: {
      status: "held",
      reason: "Exact event and cumulative allowance unknown; paid advice held.",
      actorId: actor.actorId,
      eventId: null,
      packetSha256: sha("pending"),
      taskSha256: sha("pending"),
      sourceSha256: null,
      budgetSource: ref("same-usd14"),
      cumulativeLimitUsd: 14,
      cumulativeUsageUsd: null,
      delivered: false,
      consumed: false,
      evidence: [],
    },
    requiresJudgment: false,
    currentEvent: null,
    release: null,
  };
  // Hash validation must see the exact Task text before receipt/selection binding.
  input.packet.task.sha256 = sha(input.packet.task.text);
  const request: HermesWorkMethodRequest = {
    schema: "hermes.work-method-owner-request.v1",
    event: {
      hook_event_name: "pre_llm_call",
      profile: "default",
      session_id: "native-session-A",
      cwd: "/opt/data",
      extra: {},
    },
    selection: {
      schema: "hermes.work-method-owner-selection.v1",
      cwd: "/opt/data",
      native: tuple(),
      actorId: actor.actorId,
      routeId: input.routeId,
      packetSha256: sha("pending"),
    },
    admission: input,
    outputVersion: "v2",
  };
  rebind(request);
  return request;
}

describe("Hermes owner tuple and common admission", () => {
  test("preserves session and every required contract without inventing native identities", async () => {
    const request = fixture();
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("owner-eligible");
    expect(result.native?.sessionId).toBe("native-session-A");
    expect(result.projection).toEqual({
      standing: request.admission.standing,
      packet: request.admission.packet,
      receipt: request.admission.receipt,
    });
    expect(result.projection?.packet.native).toEqual(ids());
    expect(result.admission?.adviceMode).toBe("deterministic-held-advice");
    expect(result.nativePermissionGranted).toBe(false);
    expect(result.nativeAuthenticationEstablished).toBe(false);
    expect(result.executionAuthorityGranted).toBe(false);
    expect(JSON.stringify(result).length).toBeLessThanOrEqual(12000);
  });

  test.each(["profile", "session_id", "cwd"] as const)("missing %s remains held", async (key) => {
    const request = fixture();
    delete (request.event as unknown as Record<string, unknown>)[key];
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("held");
    expect(result.projection).toBeNull();
  });

  test.each([
    "profileId",
    "sessionId",
    "agentId",
    "nativeProjectId",
    "nativeTaskId",
    "nativeRunId",
    "turnId",
  ] as const)("foreign %s never borrows parent selection", async (key) => {
    const request = fixture();
    request.selection.native[key] = "foreign";
    const result = await readHermesWorkMethod(request, verified);
    expect(result.holdReasons).toEqual(["FOREIGN_NATIVE_TUPLE"]);
    expect(result.projection).toBeNull();
  });

  test("observed Task Run and turn stay distinct, and session is never a thread fallback", async () => {
    const request = fixture();
    request.event.extra = {
      task_id: "task-X",
      run_id: "run-Y",
      turn_id: "turn-Z",
      agent_id: "agent-Q",
      project_id: "project-P",
    };
    Object.assign(request.selection.native, {
      nativeTaskId: "task-X",
      nativeRunId: "run-Y",
      turnId: "turn-Z",
      agentId: "agent-Q",
      nativeProjectId: "project-P",
    });
    for (const native of [
      request.admission.packet.native,
      request.admission.expectedActor.native,
      request.admission.receipt.actor.native,
    ])
      Object.assign(native, { taskId: "task-X", runId: "run-Y", turnId: "turn-Z" });
    rebind(request);
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("owner-eligible");
    expect(result.projection?.packet.native).toEqual({
      ...ids(),
      taskId: "task-X",
      runId: "run-Y",
      turnId: "turn-Z",
    });
    request.admission.expectedActor.native.threadId = "native-session-A";
    expect((await readHermesWorkMethod(request, verified)).holdReasons).toContain(
      "NATIVE_ACTOR_BINDING_MISMATCH",
    );
  });

  test("turn-only event does not supply missing native Task or Run", async () => {
    const request = fixture();
    request.event.extra = { turn_id: "turn-Z" };
    request.selection.native.turnId = "turn-Z";
    for (const native of [
      request.admission.packet.native,
      request.admission.expectedActor.native,
      request.admission.receipt.actor.native,
    ])
      native.turnId = "turn-Z";
    rebind(request);
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("owner-eligible");
    expect(result.projection?.packet.native.taskId).toBeNull();
    expect(result.projection?.packet.native.runId).toBeNull();
  });

  test.each(["actorId", "routeId", "packetSha256"] as const)(
    "foreign selected %s remains held",
    async (key) => {
      const request = fixture();
      request.selection[key] = key === "packetSha256" ? sha("foreign") : "foreign";
      expect((await readHermesWorkMethod(request, verified)).holdReasons).toContain(
        "OWNER_PACKET_BINDING_MISMATCH",
      );
    },
  );

  test("a foreign current native event is held even when advice is deterministic", async () => {
    const request = fixture();
    request.admission.currentEvent = {
      eventId: null,
      actorId: request.selection.actorId,
      packetSha256: request.selection.packetSha256,
      taskSha256: request.admission.packet.task.sha256,
      source: ref("event"),
      native: { ...ids(), sessionId: "another-session" },
    };
    expect((await readHermesWorkMethod(request, verified)).holdReasons).toContain(
      "NATIVE_EVENT_BINDING_MISMATCH",
    );
  });

  test("required paid judgment remains held with the same unknown USD14 allowance", async () => {
    const request = fixture();
    request.admission.requiresJudgment = true;
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("held");
    expect(result.admission?.reasons).toContain("REQUIRED_JUDGMENT_HELD");
    expect(request.admission.advice.cumulativeUsageUsd).toBeNull();
    expect(request.admission.advice.cumulativeLimitUsd).toBe(14);
  });
});

describe("bounded guarded delivery", () => {
  test("v1/v2 cap the entire lossless JSON response and reject rather than clip", async () => {
    const request = fixture();
    const baseline = await readHermesWorkMethod(request, verified);
    expect(baseline.status).toBe("owner-eligible");
    request.admission.packet.task.text += "x".repeat(3300);
    rebind(request);
    request.outputVersion = "v1";
    const small = await readHermesWorkMethod(request, verified);
    expect(small.holdReasons).toEqual(["WORK_METHOD_OUTPUT_EXCEEDS_BOUND"]);
    expect(small.projection).toBeNull();
    expect(JSON.stringify(small).length).toBeLessThanOrEqual(6000);
    request.outputVersion = "v2";
    const large = await readHermesWorkMethod(request, verified);
    expect(large.status).toBe("owner-eligible");
    expect(large.projection?.packet.task.text).toBe(request.admission.packet.task.text);
    expect(JSON.stringify(large).length).toBeLessThanOrEqual(12000);
    request.maxChars = 12001;
    expect((await readHermesWorkMethod(request, verified)).holdReasons).toEqual([
      "INVALID_OUTPUT_POLICY",
    ]);
  });

  test("independently observed source drift remains held", async () => {
    const request = fixture();
    request.admission.sources[0]!.actualSha256 = sha("changed");
    const result = await readHermesWorkMethod(request, verified);
    expect(result.status).toBe("held");
    expect(result.admission?.reasons).toContain("REQUIRED_SOURCE_NOT_CURRENT");
  });

  test("source closure is checked before and after decision; drift never publishes projection", async () => {
    const request = fixture();
    let checks = 0;
    const result = await readHermesWorkMethod(request, {
      verifySourceClosure: async () => {
        checks++;
        if (checks === 3) throw Error("private-source-path");
      },
    });
    expect(checks).toBe(3);
    expect(result.holdReasons).toEqual(["SOURCE_CLOSURE_NOT_CURRENT"]);
    expect(result.projection).toBeNull();
    expect(JSON.stringify(result)).not.toContain("private-source-path");
    expect((await readHermesWorkMethod(request, {} as never)).holdReasons).toEqual([
      "SOURCE_VERIFIER_REQUIRED",
    ]);
  });

  test("request data cannot choose executable roots or import paths", async () => {
    const request = { ...fixture(), providerRoot: "/foreign", implementationPath: "/foreign.ts" };
    expect((await readHermesWorkMethod(request, verified)).holdReasons).toEqual([
      "INVALID_OWNER_REQUEST",
    ]);
  });

  test("identity-only seam refuses prompt or unsupported event payloads", async () => {
    const request = fixture();
    (request.event as unknown as Record<string, unknown>).messages = ["private"];
    const rejected = await readHermesWorkMethod(request, verified);
    expect(rejected.projection).toBeNull();
    expect(JSON.stringify(rejected)).not.toContain("private");
    delete (request.event as unknown as Record<string, unknown>).messages;
    request.event.hook_event_name = "unknown" as "pre_llm_call";
    expect((await readHermesWorkMethod(request, verified)).holdReasons).toEqual([
      "UNSUPPORTED_NATIVE_EVENT",
    ]);
  });

  test("unregistered one-shot reader refuses before implementation import with closed diagnostics", async () => {
    const secret = "synthetic-private-request-marker";
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-env-file",
        new URL("./read-work-method.ts", import.meta.url).pathname,
      ],
      { stdin: new Blob([JSON.stringify({ secret })]), stdout: "pipe", stderr: "pipe" },
    );
    const [stdout, stderr, exit] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    expect(exit).toBe(1);
    expect(JSON.parse(stdout).status).toBe("held");
    expect(JSON.parse(stderr).stage).toBe("registration");
    expect(stdout + stderr).not.toContain(secret);
    expect(stdout + stderr).not.toContain("@palantirkc/work-continuity");
  });
});
