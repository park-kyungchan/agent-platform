/** Minimal neutral successor of the existing common operational invariant. */
export interface TaskOperationalState {
  execution: "planned" | "running" | "complete" | "failed" | "interrupted" | "uncertain";
  verification: "pending" | "passed" | "failed" | "unverified";
  localAudit:
    "intent-durable" | "result-durable" | "uncertainty-durable" | "unavailable" | "unknown";
  sharedAudit: "pending" | "unsynchronized" | "delivery-confirmed";
  formalCompletion: "pending" | "audit-synchronization-pending" | "complete";
}

export function validateTaskOperationalState(state: TaskOperationalState): void {
  if (
    !state ||
    !["planned", "running", "complete", "failed", "interrupted", "uncertain"].includes(
      state.execution,
    ) ||
    !["pending", "passed", "failed", "unverified"].includes(state.verification) ||
    !["intent-durable", "result-durable", "uncertainty-durable", "unavailable", "unknown"].includes(
      state.localAudit,
    ) ||
    !["pending", "unsynchronized", "delivery-confirmed"].includes(state.sharedAudit) ||
    !["pending", "audit-synchronization-pending", "complete"].includes(state.formalCompletion)
  ) {
    throw Error("INVALID_TASK_OPERATIONAL_STATE");
  }
  const successful =
    state.execution === "complete" &&
    state.verification === "passed" &&
    state.localAudit === "result-durable";
  if (
    state.formalCompletion === "complete" &&
    !(successful && state.sharedAudit === "delivery-confirmed")
  ) {
    throw Error("FORMAL_COMPLETION_PREREQUISITES_MISSING");
  }
  if (
    state.formalCompletion === "audit-synchronization-pending" &&
    !(successful && state.sharedAudit !== "delivery-confirmed")
  ) {
    throw Error("INVALID_AUDIT_PENDING_COMPLETION");
  }
  if (
    state.execution === "uncertain" &&
    (state.formalCompletion !== "pending" || state.verification === "passed")
  ) {
    throw Error("UNCERTAIN_EXECUTION_CANNOT_COMPLETE");
  }
}
