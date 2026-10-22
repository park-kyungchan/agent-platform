import { createHash } from "node:crypto";
import type { WorkerObservation } from "./policy";

export const sourceAssertionHoldCode = "TRUSTED_CI_SOURCE_ASSERTIONS_NOT_PROCESS_ISOLATED";

export class SourceAssertionBoundaryHold extends Error {
  constructor() {
    super(sourceAssertionHoldCode);
    this.name = "SourceAssertionBoundaryHold";
  }
}

export interface SharedProcessSourceRun {
  observation: Pick<WorkerObservation, "exitCode" | "timedOut" | "oomKilled" | "stdoutSha256">;
  stdout: Uint8Array;
}

export interface SourceAssertionEvidence {
  schema: "agent-platform.shared-process-source-observation.v1";
  state: "held";
  reason: typeof sourceAssertionHoldCode;
  assertionBoundary: "candidate-and-P-assertions-share-process";
  assertionsAuthenticated: false;
  workload: {
    state: "exit-zero-only" | "failed";
    exitCode: number;
    timedOut: boolean;
    oomKilled: boolean;
    stdoutSha256: string;
  };
}

export interface ObservedSourceWorkload<T extends SharedProcessSourceRun> {
  result: T;
  evidence: SourceAssertionEvidence;
}

/** Host-owned control flow. Worker output is retained as hashes, never parsed as completion. */
export async function observeSourceWorkload<T extends SharedProcessSourceRun>(
  run: () => Promise<T>,
): Promise<ObservedSourceWorkload<T>> {
  const result = await run();
  const { exitCode, timedOut, oomKilled } = result.observation;
  return {
    result,
    evidence: {
      schema: "agent-platform.shared-process-source-observation.v1",
      state: "held",
      reason: sourceAssertionHoldCode,
      assertionBoundary: "candidate-and-P-assertions-share-process",
      assertionsAuthenticated: false,
      workload: {
        state: exitCode === 0 && !timedOut && !oomKilled ? "exit-zero-only" : "failed",
        exitCode,
        timedOut,
        oomKilled,
        stdoutSha256: createHash("sha256").update(result.stdout).digest("hex"),
      },
    },
  };
}

/** The actual controller gate: record the original occurrence, reject failures and keep completion unknown. */
export async function superviseSourceAssertions<T extends SharedProcessSourceRun>(
  run: () => Promise<T>,
  record: (observed: ObservedSourceWorkload<T>) => void | Promise<void>,
): Promise<ObservedSourceWorkload<T>> {
  const observed = await observeSourceWorkload(run);
  await record(observed);
  if (observed.evidence.workload.state === "failed")
    throw Error("TRUSTED_CI_SOURCE_WORKLOAD_FAILED");
  return observed;
}

/** Shared workload is required, but its assertion completion stays explicitly unknown. */
export function requireObservedSourceWorkload(
  evidence: { subject: string; occurrence: string; evidence: SourceAssertionEvidence }[],
  checks: { id: string; subject: { commit: string }; observation: WorkerObservation }[],
  requiredSubjects: { commit: string }[],
): void {
  if (!Array.isArray(evidence) || evidence.length !== requiredSubjects.length)
    throw Error("TRUSTED_CI_SOURCE_WORKLOAD_EVIDENCE_REQUIRED");
  for (const subject of requiredSubjects) {
    const rows = evidence.filter((row) => row.subject === subject.commit);
    const commands = checks.filter(
      (row) => row.id === "source" && row.subject.commit === subject.commit,
    );
    if (rows.length !== 1 || commands.length !== 1)
      throw Error("TRUSTED_CI_SOURCE_WORKLOAD_EVIDENCE_REQUIRED");
    const row = rows[0]!,
      o = commands[0]!.observation,
      e = row.evidence;
    if (
      row.occurrence !== o.occurrence ||
      e.schema !== "agent-platform.shared-process-source-observation.v1" ||
      e.state !== "held" ||
      e.reason !== sourceAssertionHoldCode ||
      e.assertionBoundary !== "candidate-and-P-assertions-share-process" ||
      e.assertionsAuthenticated !== false ||
      e.workload.state !== "exit-zero-only" ||
      e.workload.exitCode !== o.exitCode ||
      e.workload.timedOut !== o.timedOut ||
      e.workload.oomKilled !== o.oomKilled ||
      e.workload.stdoutSha256 !== o.stdoutSha256
    )
      throw Error("TRUSTED_CI_SOURCE_WORKLOAD_EVIDENCE_MISMATCH");
  }
}
