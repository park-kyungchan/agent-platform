/** Neutral identity successor; native runtime and Git versions are separate. */
export interface LogicalIdentity {
  domainId: string;
  projectId: string;
  workId: string;
  issueId: string;
  specId: string;
  specRevision: number;
  planId: string;
  planRevision: number;
  taskId: string;
}

export function assertLogicalIdentity(value: LogicalIdentity): void {
  if (
    !value ||
    ![
      value.domainId,
      value.projectId,
      value.workId,
      value.issueId,
      value.specId,
      value.planId,
      value.taskId,
    ].every(
      (item) =>
        typeof item === "string" &&
        item.trim().length > 0 &&
        item.length <= 1024 &&
        !/[\u0000-\u001f]/.test(item),
    )
  ) {
    throw Error("INCOMPLETE_LOGICAL_IDENTITY");
  }
  if (
    ![value.specRevision, value.planRevision].every(
      (item) => Number.isSafeInteger(item) && item > 0,
    )
  ) {
    throw Error("INVALID_LOGICAL_REVISION");
  }
}
