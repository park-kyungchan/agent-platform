import { createHash } from "node:crypto";
import {
  workMethodPacketSha256,
  workMethodRequiredSources,
  type WorkMethodAdmissionInput,
  type WorkMethodNativeIds,
} from "@palantirkc/work-continuity/context";
import { type HermesWorkMethodRequest, type HermesWorkMethodTuple } from "../work-method.ts";

export const sha = (text: string | Uint8Array) => createHash("sha256").update(text).digest("hex");
export const ref = (path: string) => ({ kind: "snapshot" as const, path, sha256: sha(path) });
export const verified = { verifySourceClosure: async () => {} };
export const ids = (): WorkMethodNativeIds => ({
  threadId: null,
  sessionId: "native-session-A",
  taskId: null,
  runId: null,
  turnId: null,
  parentThreadId: null,
});
export const tuple = (): HermesWorkMethodTuple => ({
  runtime: "native-hermes",
  profileId: "default",
  sessionId: "native-session-A",
  agentId: null,
  nativeProjectId: null,
  nativeTaskId: null,
  nativeRunId: null,
  turnId: null,
});
export function rebind(request: HermesWorkMethodRequest): void {
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
export function fixture(): HermesWorkMethodRequest {
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
