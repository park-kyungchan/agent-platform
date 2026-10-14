import { count, exact, record, same, text, utc } from "./contracts.ts";
import { contentDigest, stableJson } from "./bot-correlation.ts";

export type CustodyBoundaryKind =
  "decision" | "delegation" | "verification" | "interruption" | "handoff";
export interface CustodyBoundary {
  boundaryId: string;
  kind: CustodyBoundaryKind;
  atUtc: string;
  sourceRefs: string[];
  completedEffectRefs: string[];
  pendingWorkRefs: string[];
  /** Opaque semantic records remain with their owners. */
  decisionRefs: string[];
  nativeTaskRunRefs: string[];
}
export interface NativeCheckpointSnapshot {
  ownerId: string;
  workId: string;
  checkpointId: string;
  revision: number;
  /** Owner's actual persisted revision digest, used as an opaque CAS token. */
  digest: string;
  boundaries: CustodyBoundary[];
  missedHookBoundaryIds: string[];
}
export interface CheckpointCasRequest {
  ownerId: string;
  workId: string;
  checkpointId: string;
  expectedRevision: number;
  expectedDigest: string;
  idempotencyKey: string;
  boundary: CustodyBoundary;
}
export type CustodyPlan =
  | { status: "already-recorded"; checkpoint: NativeCheckpointSnapshot }
  | { status: "cas-required"; request: CheckpointCasRequest };
function validateBoundary(value: unknown): asserts value is CustodyBoundary {
  const b = record(value, "boundary");
  exact(
    b,
    [
      "boundaryId",
      "kind",
      "atUtc",
      "sourceRefs",
      "completedEffectRefs",
      "pendingWorkRefs",
      "decisionRefs",
      "nativeTaskRunRefs",
    ],
    "boundary",
  );
  text(b.boundaryId, "boundaryId");
  utc(b.atUtc, "atUtc");
  if (
    !["decision", "delegation", "verification", "interruption", "handoff"].includes(
      b.kind as string,
    )
  )
    throw new Error("Unknown custody boundary");
  for (const key of [
    "sourceRefs",
    "completedEffectRefs",
    "pendingWorkRefs",
    "decisionRefs",
    "nativeTaskRunRefs",
  ]) {
    const values = b[key];
    if (!Array.isArray(values)) throw new Error(`${key}: array required`);
    for (const ref of values) text(ref, key);
    if (new Set(values).size !== values.length) throw new Error(`${key}: duplicate reference`);
  }
  if (!(b.sourceRefs as string[]).length)
    throw new Error("Boundary requires source-owned provenance");
}
export function validateNativeCheckpoint(snapshot: NativeCheckpointSnapshot): void {
  const s = record(snapshot, "checkpoint");
  exact(
    s,
    [
      "ownerId",
      "workId",
      "checkpointId",
      "revision",
      "digest",
      "boundaries",
      "missedHookBoundaryIds",
    ],
    "checkpoint",
  );
  for (const key of ["ownerId", "workId", "checkpointId", "digest"]) text(s[key], key);
  count(s.revision, "revision");
  if (!Array.isArray(s.boundaries) || !Array.isArray(s.missedHookBoundaryIds))
    throw new Error("Checkpoint history and missed hook IDs required");
  const ids = new Set<string>();
  for (const b of snapshot.boundaries) {
    validateBoundary(b);
    if (ids.has(b.boundaryId)) throw new Error("Duplicate boundary ID");
    ids.add(b.boundaryId);
  }
  for (const id of snapshot.missedHookBoundaryIds) text(id, "missedHookBoundaryId");
  if (new Set(snapshot.missedHookBoundaryIds).size !== snapshot.missedHookBoundaryIds.length)
    throw new Error("Duplicate missed hook ID");
}
/** Request only: the existing native owner performs atomic CAS and idempotency persistence. */
export function planBoundaryCustody(
  snapshot: NativeCheckpointSnapshot,
  boundary: CustodyBoundary,
): CustodyPlan {
  validateNativeCheckpoint(snapshot);
  validateBoundary(boundary);
  const previous = snapshot.boundaries.find((b) => b.boundaryId === boundary.boundaryId);
  if (previous) {
    if (stableJson(previous) !== stableJson(boundary))
      throw new Error("Boundary ID reused with conflicting content");
    return { status: "already-recorded", checkpoint: structuredClone(snapshot) };
  }
  return {
    status: "cas-required",
    request: {
      ownerId: snapshot.ownerId,
      workId: snapshot.workId,
      checkpointId: snapshot.checkpointId,
      expectedRevision: snapshot.revision,
      expectedDigest: snapshot.digest,
      idempotencyKey: contentDigest([
        snapshot.ownerId,
        snapshot.workId,
        snapshot.checkpointId,
        boundary.boundaryId,
      ]),
      boundary: structuredClone(boundary),
    },
  };
}
export function verifyBoundaryCustodyReadback(
  before: NativeCheckpointSnapshot,
  request: CheckpointCasRequest,
  after: NativeCheckpointSnapshot,
): void {
  validateNativeCheckpoint(before);
  validateNativeCheckpoint(after);
  const expected = planBoundaryCustody(before, request.boundary);
  if (expected.status !== "cas-required" || stableJson(expected.request) !== stableJson(request))
    throw new Error("CAS request differs from inspected predecessor");
  for (const key of ["ownerId", "workId", "checkpointId"] as const)
    same(after[key], before[key], key);
  if (after.revision <= before.revision || after.digest === before.digest)
    throw new Error("Owner revision/digest did not advance");
  for (const b of [...before.boundaries, request.boundary]) {
    const actual = after.boundaries.find((candidate) => candidate.boundaryId === b.boundaryId);
    if (!actual || stableJson(actual) !== stableJson(b))
      throw new Error("Readback lost or rewrote custody boundary");
  }
  for (const id of before.missedHookBoundaryIds)
    if (!after.missedHookBoundaryIds.includes(id))
      throw new Error("Missed hook evidence must be preserved");
}
/** Hooks fail open. Their failure is observable custody debt, never a fabricated checkpoint. */
export function exposeCustodyGaps(
  snapshot: NativeCheckpointSnapshot,
  expectedBoundaryIds: string[],
  failedOpenHookBoundaryIds: string[],
): {
  missedBoundaries: string[];
  failOpenHookFailures: string[];
  reconciledFailedHookBoundaries: string[];
  custodyComplete: boolean;
} {
  validateNativeCheckpoint(snapshot);
  for (const id of [...expectedBoundaryIds, ...failedOpenHookBoundaryIds]) text(id, "boundaryId");
  const present = new Set(snapshot.boundaries.map((b) => b.boundaryId));
  const failures = [...new Set([...snapshot.missedHookBoundaryIds, ...failedOpenHookBoundaryIds])];
  const missed = [...new Set([...expectedBoundaryIds, ...failures])].filter(
    (id) => !present.has(id),
  );
  return {
    missedBoundaries: missed,
    failOpenHookFailures: failures,
    reconciledFailedHookBoundaries: failures.filter((id) => present.has(id)),
    custodyComplete: missed.length === 0,
  };
}
