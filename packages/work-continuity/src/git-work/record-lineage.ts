import type { StoredRecord } from "./generated/v3.ts";
import { assertV3, canonicalV3 } from "./v3-codec.ts";
import { gitOwnerRead, gitOwnerPathHistory, sourceBlobOid } from "./git-owner-transaction.ts";

function checkSuccessor(before: StoredRecord, after: StoredRecord): void {
  if (before.kind !== after.kind) throw Error("V3_RECORD_SUCCESSOR_KIND_MISMATCH");
  if (before.kind === "run" && after.kind === "run") {
    const x = before.value,
      y = after.value;
    for (const field of [
      "runId",
      "task",
      "attempt",
      "effectiveContractDigest",
      "sourceSnapshotId",
      "parentAuthorityRef",
    ] as const)
      if (canonicalV3(x[field]) !== canonicalV3(y[field]))
        throw Error("V3_RUN_IDENTITY_OR_ADMISSION_REWRITTEN");
    if (
      x.effectRefs.some((id) => !y.effectRefs.includes(id)) ||
      x.nativeBindings.some(
        (binding) => !y.nativeBindings.some((next) => canonicalV3(next) === canonicalV3(binding)),
      )
    )
      throw Error("V3_RUN_OBSERVATION_HISTORY_REWRITTEN");
    if (
      (["succeeded", "failed", "cancelled"].includes(x.status) && y.status !== x.status) ||
      (x.status !== "reserved" && y.status === "reserved") ||
      (["succeeded", "failed", "cancelled", "uncertain"].includes(x.status) &&
        y.status === "started")
    )
      throw Error("V3_RUN_STATE_REGRESSION");
  } else if (before.kind === "work" && after.kind === "work") {
    const x = before.value,
      y = after.value;
    if (x.projectId !== y.projectId || x.workId !== y.workId || y.revision < x.revision)
      throw Error("V3_WORK_SUCCESSOR_IDENTITY_OR_REVISION");
    const order = ["proposed", "admitted", "active", "verified", "accepted"];
    if (
      (x.status === "cancelled" && y.status !== "cancelled") ||
      (x.status !== "cancelled" &&
        y.status !== "cancelled" &&
        order.indexOf(y.status) < order.indexOf(x.status))
    )
      throw Error("V3_WORK_STATE_REGRESSION");
    const { status: _, ...oldBody } = x,
      { status: __, ...newBody } = y;
    if (y.revision === x.revision && canonicalV3(oldBody) !== canonicalV3(newBody))
      throw Error("V3_WORK_CHANGE_REQUIRES_REVISION");
  } else if (before.kind === "task" && after.kind === "task") {
    const x = before.value,
      y = after.value;
    if (
      x.projectId !== y.projectId ||
      x.workId !== y.workId ||
      x.taskId !== y.taskId ||
      y.revision < x.revision
    )
      throw Error("V3_TASK_SUCCESSOR_IDENTITY_OR_REVISION");
    if (
      (["accepted", "cancelled"].includes(x.status) && y.status !== x.status) ||
      (x.status === "verified" && !["verified", "accepted", "cancelled"].includes(y.status))
    )
      throw Error("V3_TASK_STATE_REGRESSION");
    const { status: _, ...oldBody } = x,
      { status: __, ...newBody } = y;
    if (y.revision === x.revision && canonicalV3(oldBody) !== canonicalV3(newBody))
      throw Error("V3_TASK_CHANGE_REQUIRES_REVISION");
  } else throw Error("V3_RECORD_SUCCESSOR_KIND_MISMATCH");
}

/** Exact bounded owner-branch ancestry, distinct from native authorship/permission. */
export async function verifyRecordSuccessorLineage(
  root: string,
  creationCommit: string,
  head: string,
  path: string,
  creationBytes: string,
): Promise<void> {
  await gitOwnerRead(root, ["merge-base", "--is-ancestor", creationCommit, head]);
  const initial = JSON.parse(creationBytes) as unknown;
  assertV3("StoredRecord", initial);
  const { nodes, blobs } = await gitOwnerPathHistory(root, head, path);
  const decoded = new Map<string, StoredRecord>();
  const descendants = new Set<string>();
  const records = new Map<string, StoredRecord>();
  for (const node of nodes) {
    const inherits =
      node.commit === creationCommit || node.parents.some((parent) => descendants.has(parent));
    const blob = blobs.get(node.commit)!;
    if (!inherits) {
      if (blob !== null) throw Error("V3_RECORD_OUTSIDE_INTRODUCING_TRANSACTION_LINEAGE");
      continue;
    }
    descendants.add(node.commit);
    if (blob === null) throw Error("V3_ALLOCATED_RECORD_REMOVED");
    if (!decoded.has(blob)) {
      const bytes = await gitOwnerRead(root, ["cat-file", "blob", blob]);
      if (sourceBlobOid(bytes) !== blob) throw Error("V3_RECORD_HISTORY_BLOB_MISMATCH");
      const row: unknown = JSON.parse(bytes);
      assertV3("StoredRecord", row);
      decoded.set(blob, row);
    }
    const current = decoded.get(blob)!;
    if (node.commit === creationCommit && blob !== sourceBlobOid(creationBytes))
      throw Error("V3_RECORD_CREATION_BYTES_MISMATCH");
    for (const parent of node.parents) {
      const prior = records.get(parent);
      if (prior) checkSuccessor(prior, current);
    }
    records.set(node.commit, current);
  }
  if (!descendants.has(head)) throw Error("V3_RECORD_CREATION_NOT_IN_OWNER_ANCESTRY");
}
