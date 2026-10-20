import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  decideWorkMethodAdmission,
  evaluateWorkMethodCoverage,
  validateWorkMethodPacket,
  validateWorkMethodReceipt,
  validateWorkMethodStanding,
  workMethodPacketSha256,
  workMethodRequiredSources,
  workMethodAdviceRequiredSources,
  workMethodCoverageRequiredSources,
  type WorkMethodSourceObservation,
  type WorkMethodAdmissionInput,
  type WorkMethodCoverageEpoch,
  type WorkMethodNativeIds,
  type WorkMethodPacket,
  type WorkMethodReceipt,
  type WorkMethodStanding,
} from "./work-method.ts";
import type { SourceReference } from "./git-work/types.ts";

const sha = (text: string) => createHash("sha256").update(text).digest("hex");
const ref = (path: string): SourceReference => ({ kind: "snapshot", path, sha256: sha(path) });
const native = (): WorkMethodNativeIds => ({
  threadId: null,
  sessionId: null,
  taskId: null,
  runId: null,
  turnId: null,
  parentThreadId: null,
});
const protocol = { id: "common-work-method", version: "1.0.0", sha256: sha("protocol") };
const adapter = { id: "native-codex", version: "1.0.0", sha256: sha("adapter") };
const task = {
  domainId: "native-harness",
  projectId: "codex-harness-engineering",
  workId: "odi",
  taskId: "common-core",
};
const sourceHash = (source: SourceReference): string =>
  source.kind === "native" ? sourceHash(source.receipt) : source.sha256!;

function fixture(): WorkMethodAdmissionInput {
  const standing: WorkMethodStanding = {
    schema: "common.agent-work-method.standing.v1",
    protocol: structuredClone(protocol),
    purpose: "Whole approved ODI Data/Logic/Action/Security delivery through existing owners.",
    acceptedWhy:
      "Keep current purpose, actual decisions and original WHY available to every admitted actor.",
    authority: ref("authority.json"),
    owners: [
      { role: "source", id: "agent-platform" },
      { role: "native", id: "codex-owner" },
    ],
    routes: [
      {
        id: "host-task",
        kind: "task",
        adapter: structuredClone(adapter),
        supported: true,
        maxPacketBytes: 32000,
      },
    ],
  };
  const packet: WorkMethodPacket = {
    schema: "common.agent-work-method.scoped.v1",
    packetId: "packet:core",
    epochId: "adoption:1",
    protocol: structuredClone(protocol),
    adapter: structuredClone(adapter),
    logical: structuredClone(task),
    semantic: { issue: null, spec: null, plan: null },
    task: {
      text: "Implement only the standing/scoped/receipt common core.",
      sha256: sha("Implement only the standing/scoped/receipt common core."),
      source: ref("core.task.packet.json"),
    },
    lens: {
      currentAuthority: {
        text: "Approved bounded bootstrap through the existing source owner.",
        sourceRefs: ["authority.json"],
      },
      wholeObjective: standing.purpose,
      taskGoal: "Implement the minimal common work-method core.",
      acceptedWhy: [
        {
          text: standing.acceptedWhy,
          attribution: "current-user",
          sourceRefs: ["user-answer.json"],
        },
      ],
      acceptedDecisions: [
        { id: "common-first", revision: "008", sourceRefs: ["user-answer.json"] },
      ],
      owners: [
        ...standing.owners,
        { role: "domain", id: "retained-domain-owner" },
        { role: "data", id: "retained-memory-owner" },
      ],
      dependencies: [],
      completedEffectRefs: ["completed:no-replay"],
      uncertainEffectRefs: ["usage:unknown"],
      remainingAcceptance: [
        "Actual operator use and distinct domain Acceptance remain owner decisions.",
      ],
    },
    authority: standing.authority,
    phase: "bootstrap",
    role: "bounded source implementation owner",
    ownerTransitions: [],
    native: native(),
    references: {
      purpose: [ref("purpose.json")],
      decisions: [ref("user-answer.json")],
      context: [ref("checkpoint.json")],
      memory: {
        status: "none-applicable",
        sources: [],
        reason:
          "The bounded source Task uses current source facts; no additional Memory is selected.",
      },
    },
    source: {
      access: "write",
      repositoryId: "agent-platform",
      baseCommit: "a".repeat(40),
      headCommit: "b".repeat(40),
      branch: "work/core",
      worktree: "/isolated/core",
      ownedPaths: ["packages/work-continuity/src/work-method.ts"],
      resultPath: "/assigned/core.result.json",
    },
    fences: {
      completed: ["completed:no-replay"],
      consumed: ["historical:consumed"],
      uncertain: ["usage:unknown"],
      source: ref("effect-fences.json"),
      budget: ref("same-usd14-unknown-usage.json"),
      permissions: ref("native-controls-retained.json"),
    },
    prerequisites: [],
  };
  const actor = {
    actorId: "owned:core",
    environmentId: "host",
    runtime: "native-codex" as const,
    profileId: null,
    paneId: null,
    terminalId: null,
    native: native(),
  };
  const receipt: WorkMethodReceipt = {
    schema: "common.agent-work-method.receipt.v1",
    id: "receipt:core",
    observedAt: "2026-10-20T01:00:00Z",
    actor,
    routeId: "host-task",
    logical: structuredClone(task),
    packetId: packet.packetId,
    packetSha256: workMethodPacketSha256(packet),
    taskSha256: packet.task.sha256,
    protocol: structuredClone(protocol),
    adapter: structuredClone(adapter),
    currentness: "current",
    received: true,
    used: true,
    useEvidence: [ref("first-use.json")],
    authorityReloaded: false,
    admission: {
      ownerId: "codex-owner",
      point: "native-owner:before-task-effects",
      mechanism: "owner-admission",
      consumedBeforeEffects: true,
      evidence: [ref("owner-admission.json")],
    },
    transitionRefs: [],
  };
  const input: WorkMethodAdmissionInput = {
    standing,
    packet,
    receipt,
    expectedActor: structuredClone(actor),
    routeId: "host-task",
    currentness: {
      protocol: structuredClone(protocol),
      adapter: structuredClone(adapter),
      status: "current",
    },
    sources: [],
    prerequisites: [],
    concurrent: [],
    advice: {
      status: "held",
      reason:
        "Exact paid event correlation and cumulative usage remain unknown; no paid inference.",
      actorId: actor.actorId,
      eventId: null,
      packetSha256: receipt.packetSha256,
      taskSha256: packet.task.sha256,
      sourceSha256: null,
      budgetSource: packet.fences.budget,
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
  rebind(input);
  return input;
}

function rebind(input: WorkMethodAdmissionInput): void {
  input.receipt.packetSha256 = workMethodPacketSha256(input.packet);
  input.receipt.taskSha256 = input.packet.task.sha256;
  input.advice.packetSha256 = input.receipt.packetSha256;
  input.advice.taskSha256 = input.packet.task.sha256;
  input.sources = workMethodRequiredSources(
    input.packet,
    input.standing,
    input.receipt,
    input.release,
  ).map((source) => ({ source, actualSha256: sourceHash(source), currentness: "current" }));
  if (input.currentEvent) {
    input.currentEvent.packetSha256 = input.receipt.packetSha256;
    input.currentEvent.taskSha256 = input.packet.task.sha256;
  }
  for (const source of workMethodAdviceRequiredSources(input.advice, input.currentEvent))
    if (!input.sources.some((row) => JSON.stringify(row.source) === JSON.stringify(source)))
      input.sources.push({ source, actualSha256: sourceHash(source), currentness: "current" });
}

function readyJudgmentInput(): WorkMethodAdmissionInput {
  const input = fixture();
  input.requiresJudgment = true;
  input.currentEvent = {
    eventId: "native-event:observed",
    actorId: input.expectedActor.actorId,
    packetSha256: input.receipt.packetSha256,
    taskSha256: input.packet.task.sha256,
    source: ref("native-current-event-source.json"),
    native: structuredClone(input.receipt.actor.native),
  };
  Object.assign(input.advice, {
    status: "consumed",
    eventId: input.currentEvent.eventId,
    sourceSha256: sourceHash(input.currentEvent.source),
    cumulativeUsageUsd: 3.2,
    delivered: true,
    consumed: true,
    evidence: [ref("native-advice-consumption.json")],
  });
  rebind(input);
  return input;
}

function coverage(input: WorkMethodAdmissionInput): WorkMethodCoverageEpoch {
  return {
    schema: "common.agent-work-method.coverage.v1",
    epochId: input.packet.epochId,
    ownerId: "root",
    attestation: ref("finite-owner-inventory.json"),
    closedAt: "2026-10-20T01:01:00Z",
    changedBeforeRelease: false,
    rows: [
      {
        id: "host-core",
        environmentId: "host",
        ownerId: "codex-owner",
        routeId: input.routeId,
        actorId: input.expectedActor.actorId,
        required: true,
        state: "active",
        protocol: structuredClone(protocol),
        adapter: structuredClone(adapter),
        classificationEvidence: [ref("active-owner.json")],
        wake: null,
        receiptId: input.receipt.id,
        packetSha256: input.receipt.packetSha256,
      },
    ],
  };
}

function coverageSources(
  epoch: WorkMethodCoverageEpoch,
  observations: Parameters<typeof workMethodCoverageRequiredSources>[1] = [],
): WorkMethodSourceObservation[] {
  return workMethodCoverageRequiredSources(epoch, observations).map((source) => ({
    source,
    actualSha256: sourceHash(source),
    currentness: "current",
  }));
}
function coverageEvaluation(
  epoch: WorkMethodCoverageEpoch,
  observations: Parameters<typeof evaluateWorkMethodCoverage>[1],
) {
  return evaluateWorkMethodCoverage(epoch, observations, coverageSources(epoch, observations));
}

describe("exact scoped packet and explicit native unknowns", () => {
  test("standing instructions can be validated while idle without inventing a Work or Goal", () => {
    const input = fixture();
    expect(validateWorkMethodStanding(input.standing)).toEqual({ valid: true, findings: [] });
    expect(validateWorkMethodPacket(input.packet).valid).toBe(true);
    expect(validateWorkMethodReceipt(input.receipt).valid).toBe(true);
    expect(input.packet.native).toEqual(native());
  });

  test("exact task bytes, accepted WHY, completed fences and unknown native IDs survive validation", () => {
    const input = fixture();
    input.packet.task.text += " Also change a provider.";
    expect(validateWorkMethodPacket(input.packet).findings).toContain("EXACT_TASK_HASH_MISMATCH");
    const missingWhy = fixture().packet;
    missingWhy.lens.acceptedWhy[0]!.attribution = "inference";
    expect(validateWorkMethodPacket(missingWhy).findings).toContain("ACCEPTED_WHY_REQUIRED");
    const lostFence = fixture().packet;
    lostFence.fences.uncertain = [];
    expect(validateWorkMethodPacket(lostFence).findings).toContain("UNCERTAIN_FENCE_OMITTED");
    const invented = { ...fixture().packet, native: { threadId: "guessed-parent" } };
    expect(validateWorkMethodPacket(invented).valid).toBe(false);
  });

  test("semantic proposals and missing contracts remain explicit, not admitted by validation", () => {
    const packet = fixture().packet;
    packet.semantic.spec = {
      id: "spec:proposed",
      revision: null,
      status: "proposed",
      source: ref("spec-proposal.json"),
    };
    expect(validateWorkMethodPacket(packet).valid).toBe(true);
    packet.semantic.spec.status = "admitted";
    expect(validateWorkMethodPacket(packet).findings).toContain("ADMITTED_REVISION_REQUIRED");
    packet.semantic.spec.revision = 2;
    expect(validateWorkMethodPacket(packet).valid).toBe(true);
  });

  test("canonical hash ignores object key order but binds changed scope and source bytes", () => {
    const packet = fixture().packet;
    const reversed = Object.fromEntries(
      Object.entries(packet).reverse(),
    ) as unknown as WorkMethodPacket;
    expect(workMethodPacketSha256(reversed)).toBe(workMethodPacketSha256(packet));
    const original = workMethodPacketSha256(packet);
    packet.source.ownedPaths.push("another.ts");
    expect(workMethodPacketSha256(packet)).not.toBe(original);
    packet.source.ownedPaths = ["../outside.ts"];
    expect(validateWorkMethodPacket(packet).findings).toContain("INVALID_OWNED_PATH");
  });
});

describe("owner admission rather than generic native permission", () => {
  test("deterministic bootstrap is eligible with truthful held paid advice and no new authority", () => {
    const result = decideWorkMethodAdmission(fixture());
    expect(result.status).toBe("owner-eligible");
    expect(result.adviceMode).toBe("deterministic-held-advice");
    expect(result.nativePermissionGranted).toBe(false);
    expect(result.executionAuthorityGranted).toBe(false);
    expect(result.retryAllowed).toBe(false);
    expect(result.completionGranted).toBe(false);
  });

  test.each([
    [
      "advisory",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.admission.mechanism = "instruction-advisory";
      },
      "ADVISORY_IS_NOT_ADMISSION",
    ],
    [
      "delivery only",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.used = false;
      },
      "RECEIVE_USE_REQUIRED",
    ],
    [
      "unconsumed guard",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.admission.consumedBeforeEffects = false;
      },
      "ADMISSION_NOT_CONSUMED",
    ],
    [
      "stale receiver",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.currentness = "stale";
      },
      "RECEIPT_NOT_CURRENT",
    ],
    [
      "unknown live projection",
      (x: WorkMethodAdmissionInput) => {
        x.currentness.status = "unknown";
      },
      "PROJECTION_NOT_CURRENT",
    ],
    [
      "unsupported direct route",
      (x: WorkMethodAdmissionInput) => {
        x.standing.routes[0]!.supported = false;
      },
      "UNSUPPORTED_ROUTE",
    ],
    [
      "oversize packet",
      (x: WorkMethodAdmissionInput) => {
        x.standing.routes[0]!.maxPacketBytes = 20;
      },
      "PACKET_EXCEEDS_ROUTE_BOUND",
    ],
    [
      "other packet receipt",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.packetSha256 = sha("other packet");
      },
      "RECEIPT_PACKET_BINDING_MISMATCH",
    ],
    [
      "other native actor",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.actor.native.threadId = "unrelated-thread";
      },
      "RECEIPT_ACTOR_MISMATCH",
    ],
    [
      "adapter drift",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.adapter.sha256 = sha("changed adapter");
      },
      "ADAPTER_MISMATCH",
    ],
    [
      "protocol version drift",
      (x: WorkMethodAdmissionInput) => {
        x.receipt.protocol.version = "2.0.0";
      },
      "PROTOCOL_MISMATCH",
    ],
  ])("%s remains held", (_name, mutate, reason) => {
    const input = fixture();
    mutate(input);
    expect(decideWorkMethodAdmission(input).reasons).toContain(reason);
    expect(decideWorkMethodAdmission(input).status).toBe("held");
  });

  test("source drift, missing observations and conflicting duplicate observations hold", () => {
    for (const kind of ["drift", "missing", "duplicate"] as const) {
      const input = fixture();
      if (kind === "drift") input.sources[0]!.actualSha256 = sha("changed source");
      if (kind === "missing") input.sources.shift();
      if (kind === "duplicate") input.sources.push(structuredClone(input.sources[0]!));
      expect(decideWorkMethodAdmission(input).reasons).toContain("REQUIRED_SOURCE_NOT_CURRENT");
    }
  });

  test("qualified producer result alone cannot satisfy receiver consumption or another Work", () => {
    const input = fixture();
    const dependency = {
      producer: {
        ...task,
        projectId: "producer-project",
        workId: "producer-work",
        taskId: "producer",
      },
      receiver: structuredClone(task),
      milestone: "shared-audit-delivery" as const,
      producerReceipt: ref("producer.result.json"),
      receiverReceipt: ref("consumer.use.json"),
    };
    input.packet.prerequisites.push(dependency);
    rebind(input);
    input.prerequisites = [
      { ...dependency, currentness: "current", producerSatisfied: true, receiverUsed: false },
    ];
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "QUALIFIED_PREREQUISITE_NOT_CONSUMED",
    );
    input.prerequisites[0]!.receiverUsed = true;
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    input.prerequisites[0]!.producer = { ...dependency.producer, workId: "unrelated-work" };
    expect(decideWorkMethodAdmission(input).status).toBe("held");
  });

  test.each(["followup", "resume", "refresh", "post-compact"] as const)(
    "%s requires current authority and actual transition evidence",
    (kind) => {
      const input = fixture();
      input.standing.routes[0]!.kind = kind;
      expect(decideWorkMethodAdmission(input).reasons).toContain(
        "RECOVERY_AUTHORITY_RELOAD_REQUIRED",
      );
      input.receipt.authorityReloaded = true;
      input.receipt.transitionRefs = [ref("actual-recovery-first-use.json")];
      rebind(input);
      expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    },
  );

  test("concurrent actors cannot borrow a packet or share a writable branch/worktree", () => {
    const input = fixture();
    const other = {
      actorId: "owned:other",
      packetId: "packet:other",
      packetSha256: sha("other"),
      logical: { ...task, taskId: "other" },
      source: structuredClone(input.packet.source),
    };
    input.concurrent = [other];
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "CONCURRENT_SOURCE_ISOLATION_REQUIRED",
    );
    other.source.branch = "work/other";
    other.source.worktree = "/isolated/other";
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    other.actorId = input.expectedActor.actorId;
    expect(decideWorkMethodAdmission(input).reasons).toContain("CONCURRENT_ACTOR_PACKET_CONFLICT");
  });

  test("paid judgment requires exact current event, packet, source, known cumulative allowance and consumed result", () => {
    const input = fixture();
    input.requiresJudgment = true;
    expect(decideWorkMethodAdmission(input).reasons).toContain("REQUIRED_JUDGMENT_HELD");
    input.currentEvent = {
      eventId: "native-event:observed",
      native: structuredClone(input.receipt.actor.native),
      actorId: input.expectedActor.actorId,
      packetSha256: input.receipt.packetSha256,
      taskSha256: input.packet.task.sha256,
      source: ref("native-current-event-source.json"),
    };
    rebind(input);
    Object.assign(input.advice, {
      status: "consumed",
      eventId: "native-event:observed",
      sourceSha256: sourceHash(input.currentEvent.source),
      cumulativeUsageUsd: 3.2,
      delivered: true,
      consumed: true,
      evidence: [ref("native-advice-consumption.json")],
    });
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    expect(decideWorkMethodAdmission(input).adviceMode).toBe("consumed");
    for (const change of [
      { cumulativeUsageUsd: null },
      { cumulativeLimitUsd: null },
      { cumulativeUsageUsd: 14.01 },
      { eventId: null },
      { packetSha256: sha("concurrent packet") },
      { sourceSha256: sha("old source") },
      { consumed: false },
      { budgetSource: ref("other-work-budget.json") },
    ]) {
      const changed = structuredClone(input);
      Object.assign(changed.advice, change);
      expect(decideWorkMethodAdmission(changed).status).toBe("held");
    }
    const unrelatedEvent = structuredClone(input);
    unrelatedEvent.currentEvent!.eventId = "previous-native-event";
    expect(decideWorkMethodAdmission(unrelatedEvent).status).toBe("held");
    unrelatedEvent.currentEvent = null;
    expect(decideWorkMethodAdmission(unrelatedEvent).status).toBe("held");
    input.advice.cumulativeLimitUsd = null;
    input.requiresJudgment = false;
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    expect(decideWorkMethodAdmission(input).adviceHeldReasons).toContain(
      "ADVICE_CONSUMPTION_BINDING_INVALID",
    );
    const separateWork = structuredClone(input);
    separateWork.packet.logical.workId = "separate-common-contract-fixture";
    separateWork.receipt.logical.workId = separateWork.packet.logical.workId;
    separateWork.packet.fences.budget = ref("separate-owner-budget-cap25.json");
    separateWork.advice.budgetSource = separateWork.packet.fences.budget;
    separateWork.advice.cumulativeLimitUsd = 25;
    rebind(separateWork);
    expect(decideWorkMethodAdmission(separateWork).adviceMode).toBe("consumed");
    expect(decideWorkMethodAdmission(separateWork).executionAuthorityGranted).toBe(false);
  });

  test("malformed external input fails closed without throwing a caller exception", () => {
    for (const invalid of [null, {}, { packet: null }, { ...fixture(), sources: null }])
      expect(decideWorkMethodAdmission(invalid as unknown as WorkMethodAdmissionInput).status).toBe(
        "held",
      );
  });
});

describe("finite coverage epoch and wake gates", () => {
  test("bootstrap works before epoch closure; general work needs its current source-bound release", () => {
    const input = fixture();
    const bootstrap = decideWorkMethodAdmission(input);
    expect(bootstrap.status).toBe("owner-eligible");
    const ready = coverageEvaluation(coverage(input), [
      { receipt: structuredClone(input.receipt), admission: bootstrap },
    ]);
    expect(ready.status).toBe("owner-eligible");
    input.packet.phase = "general";
    rebind(input);
    expect(decideWorkMethodAdmission(input).reasons).toContain("GENERAL_WORK_RELEASE_REQUIRED");
    input.release = {
      schema: "common.agent-work-method.release.v1",
      epochId: input.packet.epochId,
      ownerId: "codex-owner",
      source: ref("root-release.json"),
      coverage: ready,
      bootstrapClosed: true,
      currentness: "current",
    };
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    for (const mutation of [
      { epochId: "other-epoch" },
      { bootstrapClosed: false },
      { currentness: "unknown" as const },
    ]) {
      const changed = structuredClone(input);
      Object.assign(changed.release!, mutation);
      expect(decideWorkMethodAdmission(changed).reasons).toContain(
        "GENERAL_WORK_RELEASE_NOT_CURRENT",
      );
    }
    input.sources.find(
      (row) => row.source.kind === "snapshot" && row.source.path === "root-release.json",
    )!.actualSha256 = sha("stale release");
    expect(decideWorkMethodAdmission(input).reasons).toContain("REQUIRED_SOURCE_NOT_CURRENT");
  });

  test("a held coverage result or changed receive-use receipt cannot be promoted to release", () => {
    const input = fixture(),
      epoch = coverage(input);
    const observed = { receipt: input.receipt, admission: decideWorkMethodAdmission(input) };
    observed.receipt.useEvidence.push(ref("later-unreviewed-use.json"));
    expect(coverageEvaluation(epoch, [observed]).status).toBe("held");
    input.packet.phase = "general";
    input.release = {
      schema: "common.agent-work-method.release.v1",
      epochId: epoch.epochId,
      ownerId: "codex-owner",
      source: ref("held-release.json"),
      coverage: coverageEvaluation(epoch, [observed]),
      bootstrapClosed: true,
      currentness: "current",
    };
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("held");
  });
  test("closed current owned denominator joins exact actor, route, packet and receipt", () => {
    const input = fixture(),
      epoch = coverage(input);
    const observations = [{ receipt: input.receipt, admission: decideWorkMethodAdmission(input) }];
    const result = coverageEvaluation(epoch, observations);
    expect(result.status).toBe("owner-eligible");
    expect(result.arbitraryAgentCoverage).toBe(false);
    expect(result.nativePermissionGranted).toBe(false);
    epoch.rows[0]!.packetSha256 = sha("other task");
    expect(coverageEvaluation(epoch, observations).heldRowIds).toEqual(["host-core"]);
  });

  test("required unknown actor, open epoch and changed denominator prevent initial release", () => {
    const input = fixture(),
      epoch = coverage(input);
    const observations = [{ receipt: input.receipt, admission: decideWorkMethodAdmission(input) }];
    epoch.rows.push({
      ...structuredClone(epoch.rows[0]!),
      id: "unknown-profile",
      actorId: null,
      state: "unknown",
      receiptId: null,
      packetSha256: null,
    });
    expect(coverageEvaluation(epoch, observations).heldRowIds).toEqual(["unknown-profile"]);
    epoch.closedAt = null;
    expect(coverageEvaluation(epoch, observations).reasons).toContain("COVERAGE_EPOCH_OPEN");
    epoch.changedBeforeRelease = true;
    expect(coverageEvaluation(epoch, observations).reasons).toContain(
      "COVERAGE_CHANGED_BEFORE_RELEASE",
    );
  });

  test("dormant actors need evidenced wake holds; non-agent classification needs evidence", () => {
    const input = fixture(),
      epoch = coverage(input);
    const row = epoch.rows[0]!;
    row.state = "dormant";
    row.actorId = null;
    row.receiptId = null;
    row.packetSha256 = null;
    expect(coverageEvaluation(epoch, []).status).toBe("held");
    row.wake = {
      mechanism: "owner-admission",
      holdBeforeGeneralEffects: true,
      evidence: [ref("registered-wake-boundary.json")],
    };
    expect(coverageEvaluation(epoch, []).status).toBe("owner-eligible");
    row.wake.mechanism = "instruction-advisory";
    expect(coverageEvaluation(epoch, []).status).toBe("held");
    row.state = "non-agent";
    row.classificationEvidence = [];
    expect(coverageEvaluation(epoch, []).status).toBe("held");
    row.classificationEvidence = [ref("owner-observed-non-agent-service.json")];
    expect(coverageEvaluation(epoch, []).status).toBe("owner-eligible");
  });

  test("wake actor must be reclassified and pass admission; drift reopens only affected coverage", () => {
    const input = fixture(),
      epoch = coverage(input);
    const initial = decideWorkMethodAdmission(input);
    input.receipt.currentness = "stale";
    const stale = decideWorkMethodAdmission(input);
    expect(stale.status).toBe("held");
    expect(initial.status).toBe("owner-eligible");
    expect(
      coverageEvaluation(epoch, [{ receipt: input.receipt, admission: stale }]).heldRowIds,
    ).toEqual(["host-core"]);
    const unrelated = fixture();
    unrelated.packet.logical.taskId = "other-slice";
    unrelated.receipt.logical.taskId = "other-slice";
    rebind(unrelated);
    expect(decideWorkMethodAdmission(unrelated).status).toBe("owner-eligible");
  });
});

describe("independent review regression cases", () => {
  test("RR-01 a duplicate writable actor cannot borrow the same packet's self-observation exemption", () => {
    const input = fixture();
    const binding = {
      actorId: input.receipt.actor.actorId,
      packetId: input.packet.packetId,
      packetSha256: workMethodPacketSha256(input.packet),
      logical: structuredClone(input.packet.logical),
      source: structuredClone(input.packet.source),
    };
    input.concurrent = [binding];
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    binding.actorId = "second-live-writer";
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "CONCURRENT_SOURCE_ISOLATION_REQUIRED",
    );
    expect(decideWorkMethodAdmission(input).reasons).toContain("CONCURRENT_SOURCE_PATH_CONFLICT");
    binding.source.access = "read-only";
    binding.source.ownedPaths = [];
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    binding.actorId = input.receipt.actor.actorId;
    expect(decideWorkMethodAdmission(input).reasons).toContain("CONCURRENT_ACTOR_PACKET_CONFLICT");
  });

  test("RR-02 current coverage rechecks each active receipt evidence and holds only its affected row", () => {
    const first = fixture();
    first.receipt.transitionRefs = [ref("first-authority-reload.json")];
    rebind(first);
    const second = fixture();
    second.packet.packetId = "second-packet";
    second.packet.logical.taskId = "second-task";
    second.receipt.logical = structuredClone(second.packet.logical);
    second.receipt.packetId = second.packet.packetId;
    second.receipt.id = "second-receipt";
    second.receipt.actor.actorId = "second-actor";
    second.expectedActor = structuredClone(second.receipt.actor);
    second.receipt.useEvidence = [ref("second-use.json")];
    second.receipt.admission.evidence = [ref("second-admission.json")];
    rebind(second);
    const epoch = coverage(first);
    epoch.rows.push({ ...coverage(second).rows[0]!, id: "second-row" });
    const observations = [first, second].map((input) => ({
      receipt: input.receipt,
      admission: decideWorkMethodAdmission(input),
    }));
    const current = coverageSources(epoch, observations);
    expect(evaluateWorkMethodCoverage(epoch, observations, current).status).toBe("owner-eligible");
    for (const required of [
      ...first.receipt.useEvidence,
      ...first.receipt.admission.evidence,
      ...first.receipt.transitionRefs,
    ]) {
      for (const failure of ["missing", "stale", "changed", "duplicate"] as const) {
        const changed = structuredClone(current);
        const index = changed.findIndex(
          (entry) => JSON.stringify(entry.source) === JSON.stringify(required),
        );
        expect(index).toBeGreaterThanOrEqual(0);
        if (failure === "missing") changed.splice(index, 1);
        if (failure === "stale") changed[index]!.currentness = "stale";
        if (failure === "changed") changed[index]!.actualSha256 = sha("changed active proof");
        if (failure === "duplicate") changed.push(structuredClone(changed[index]!));
        const result = evaluateWorkMethodCoverage(epoch, observations, changed);
        expect(result.status).toBe("held");
        expect(result.heldRowIds).toEqual(["host-core"]);
        const general = structuredClone(second);
        general.packet.phase = "general";
        general.release = {
          schema: "common.agent-work-method.release.v1",
          epochId: epoch.epochId,
          ownerId: "codex-owner",
          source: ref("release-with-stale-bootstrap-proof.json"),
          coverage: result,
          bootstrapClosed: true,
          currentness: "current",
        };
        rebind(general);
        expect(decideWorkMethodAdmission(general).reasons).toContain(
          "GENERAL_WORK_RELEASE_NOT_CURRENT",
        );
      }
    }
  });

  test.each(["dormant", "non-agent"] as const)(
    "CR-01 %s classification and wake references require independent current evidence",
    (state) => {
      const epoch = coverage(fixture()),
        row = epoch.rows[0]!;
      row.state = state;
      row.actorId = null;
      row.receiptId = null;
      row.packetSha256 = null;
      epoch.attestation = ref("independently-observed-epoch.json");
      row.classificationEvidence = [ref("independently-observed-classification.json")];
      row.wake =
        state === "dormant"
          ? {
              mechanism: "owner-admission",
              holdBeforeGeneralEffects: true,
              evidence: [ref("independently-observed-wake.json")],
            }
          : null;
      const observed = coverageSources(epoch);
      expect(evaluateWorkMethodCoverage(epoch, [], observed).status).toBe("owner-eligible");
      expect(evaluateWorkMethodCoverage(epoch, [], []).status).toBe("held");
      for (const required of workMethodCoverageRequiredSources(epoch)) {
        for (const failure of ["missing", "stale", "duplicate", "changed"] as const) {
          const changed = structuredClone(observed);
          const index = changed.findIndex(
            (item) => JSON.stringify(item.source) === JSON.stringify(required),
          );
          if (failure === "missing") changed.splice(index, 1);
          if (failure === "stale") changed[index]!.currentness = "stale";
          if (failure === "changed")
            changed[index]!.actualSha256 = sha("changed exclusion or wake bytes");
          if (failure === "duplicate")
            changed.push({ ...changed[index]!, actualSha256: sha("conflicting observation") });
          const result = evaluateWorkMethodCoverage(epoch, [], changed);
          expect(result.status).toBe("held");
          if (required === epoch.attestation)
            expect(result.reasons).toContain("COVERAGE_ATTESTATION_NOT_CURRENT");
          else expect(result.heldRowIds).toEqual([row.id]);
        }
      }
    },
  );

  test("CR-02 consumption and current-event sources each require one current exact observation", () => {
    const input = readyJudgmentInput();
    expect(decideWorkMethodAdmission(input).adviceMode).toBe("consumed");
    for (const required of workMethodAdviceRequiredSources(input.advice, input.currentEvent)) {
      for (const failure of ["missing", "stale", "duplicate", "changed"] as const) {
        const changed = structuredClone(input);
        const index = changed.sources.findIndex(
          (item) => JSON.stringify(item.source) === JSON.stringify(required),
        );
        if (failure === "missing") changed.sources.splice(index, 1);
        if (failure === "stale") changed.sources[index]!.currentness = "stale";
        if (failure === "changed")
          changed.sources[index]!.actualSha256 = sha("stale consumption proof");
        if (failure === "duplicate")
          changed.sources.push({
            ...changed.sources[index]!,
            actualSha256: sha("conflicting event proof"),
          });
        expect(decideWorkMethodAdmission(changed).status).toBe("held");
        expect(decideWorkMethodAdmission(changed).adviceMode).toBe("deterministic-held-advice");
        changed.requiresJudgment = false;
        expect(decideWorkMethodAdmission(changed).status).toBe("owner-eligible");
        expect(decideWorkMethodAdmission(changed).adviceHeldReasons).toContain(
          "ADVICE_CONSUMPTION_BINDING_INVALID",
        );
      }
    }
  });

  test("CR-03 disjoint concurrent writers still need separate branches and worktrees", () => {
    const input = fixture();
    const other = {
      actorId: "owned:other",
      packetId: "packet:other",
      packetSha256: sha("other packet"),
      logical: { ...task, taskId: "other" },
      source: { ...input.packet.source, ownedPaths: ["packages/other-writable-slice.ts"] },
    };
    input.concurrent = [other];
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "CONCURRENT_SOURCE_ISOLATION_REQUIRED",
    );
    other.source.worktree = "/isolated/other";
    expect(decideWorkMethodAdmission(input).status).toBe("held");
    other.source.branch = "work/other";
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    other.source.worktree = input.packet.source.worktree;
    expect(decideWorkMethodAdmission(input).status).toBe("held");
    other.source.access = "read-only";
    other.source.ownedPaths = [];
    other.source.branch = input.packet.source.branch;
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    other.source.ownedPaths = ["not-read-only.ts"];
    expect(decideWorkMethodAdmission(input).reasons).toContain("READ_ONLY_OWNED_PATHS");
  });

  test("CR-04 scoped owners cannot replace standing boundaries without qualified current authority", () => {
    const input = fixture();
    input.packet.lens.owners = input.packet.lens.owners.filter(
      (row) => !["source", "native"].includes(row.role),
    );
    input.packet.lens.owners.push(
      { role: "source", id: "replacement-source" },
      { role: "native", id: "replacement-native" },
    );
    input.receipt.admission.ownerId = "replacement-native";
    rebind(input);
    expect(decideWorkMethodAdmission(input).reasons).toContain("STANDING_OWNER_NOT_PRESERVED");
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "ADMISSION_OWNER_TRANSITION_REQUIRED",
    );
    input.packet.ownerTransitions = input.standing.owners.map((from) => ({
      from,
      to: input.packet.lens.owners.find((row) => row.role === from.role)!,
      logical: structuredClone(input.packet.logical),
      authority: ref("qualified-owner-transition.json"),
    }));
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    for (const failure of ["missing", "stale", "duplicate"] as const) {
      const changed = structuredClone(input);
      const index = changed.sources.findIndex(
        (row) =>
          row.source.kind === "snapshot" && row.source.path === "qualified-owner-transition.json",
      );
      if (failure === "missing") changed.sources.splice(index, 1);
      if (failure === "stale") changed.sources[index]!.currentness = "stale";
      if (failure === "duplicate") changed.sources.push(structuredClone(changed.sources[index]!));
      expect(decideWorkMethodAdmission(changed).reasons).toContain("REQUIRED_SOURCE_NOT_CURRENT");
    }
    input.packet.ownerTransitions[0]!.logical.taskId = "another-task";
    expect(validateWorkMethodPacket(input.packet).findings).toContain(
      "WRONG_OWNER_TRANSITION_TASK",
    );
  });

  test("CR-04 Work-specific narrowing preserves standing owners and requires admission delegation", () => {
    const input = fixture();
    input.packet.lens.owners.push({ role: "source", id: "narrow-source-owner" });
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    input.receipt.admission.ownerId = "narrow-source-owner";
    expect(decideWorkMethodAdmission(input).reasons).toContain(
      "ADMISSION_OWNER_TRANSITION_REQUIRED",
    );
    input.packet.ownerTransitions.push({
      from: input.standing.owners.find((row) => row.role === "source")!,
      to: { role: "source", id: "narrow-source-owner" },
      logical: structuredClone(input.packet.logical),
      authority: ref("exact-source-owner-delegation.json"),
    });
    rebind(input);
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
  });

  test("Hermes session identity stays distinct from Codex thread identity in packet, actor and event", () => {
    const input = readyJudgmentInput();
    input.packet.native.sessionId = "hermes-session:observed";
    input.receipt.actor.runtime = "native-hermes";
    input.receipt.actor.native.sessionId = input.packet.native.sessionId;
    input.expectedActor = structuredClone(input.receipt.actor);
    input.currentEvent!.native = structuredClone(input.receipt.actor.native);
    rebind(input);
    expect(input.packet.native.threadId).toBeNull();
    expect(decideWorkMethodAdmission(input).status).toBe("owner-eligible");
    input.currentEvent!.native.sessionId = "unrelated-session";
    expect(decideWorkMethodAdmission(input).status).toBe("held");
    input.currentEvent!.native = structuredClone(input.receipt.actor.native);
    input.packet.native.sessionId = "packet-from-another-session";
    rebind(input);
    expect(decideWorkMethodAdmission(input).reasons).toContain("OBSERVED_NATIVE_ID_MISMATCH");
  });
});
