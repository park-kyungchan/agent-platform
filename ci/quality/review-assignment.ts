/** Closed data contract. Authority comes only from the existing authenticated proof readers. */
export type MergeCorrespondenceAssignment = Readonly<{
  schema: "native-review-merge-correspondence.v1";
  kind: "merge-correspondence";
  orderedParents: readonly [string, string];
  parentReview: Readonly<{ statusId: number; targetUrl: string; proofSha256: string }>;
}>;

const object = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
const keys = (value: Record<string, unknown>, expected: string) =>
  Object.keys(value).sort().join("|") === expected;
const oid = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{40}$/.test(value);

export function parseMergeCorrespondenceAssignment(value: unknown): MergeCorrespondenceAssignment {
  const assignment = object(value),
    parent = object(assignment.parentReview),
    parents = assignment.orderedParents;
  if (
    !keys(assignment, "kind|orderedParents|parentReview|schema") ||
    assignment.schema !== "native-review-merge-correspondence.v1" ||
    assignment.kind !== "merge-correspondence" ||
    !Array.isArray(parents) ||
    parents.length !== 2 ||
    !parents.every(oid) ||
    parents[0] === parents[1] ||
    !keys(parent, "proofSha256|statusId|targetUrl") ||
    !Number.isSafeInteger(parent.statusId) ||
    Number(parent.statusId) <= 0 ||
    typeof parent.targetUrl !== "string" ||
    parent.targetUrl.length > 2048 ||
    typeof parent.proofSha256 !== "string" ||
    !/^[a-f0-9]{64}$/.test(parent.proofSha256)
  )
    throw Error("NATIVE_REVIEW_ASSIGNMENT_CLOSED_MERGE_PROOF_REQUIRED");
  return Object.freeze({
    schema: "native-review-merge-correspondence.v1",
    kind: "merge-correspondence",
    orderedParents: Object.freeze([parents[0], parents[1]]) as readonly [string, string],
    parentReview: Object.freeze({
      statusId: Number(parent.statusId),
      targetUrl: parent.targetUrl,
      proofSha256: parent.proofSha256,
    }),
  });
}

export function requireFullReviewAssignment(value: unknown): void {
  const proof = object(value);
  if (
    Object.hasOwn(proof, "reviewAssignment") ||
    [
      "native-independent-merge-correspondence-output.v1",
      "owner-scoped-native-merge-correspondence-output.v1",
    ].includes(String(proof.schema))
  )
    throw Error("NATIVE_REVIEW_ASSIGNMENT_FULL_PARENT_REQUIRED");
}

/** Existing FULL envelopes never carry an assignment; bounded envelopes always do. */
export function parseNativeReviewAssignment(
  value: unknown,
  platform: boolean,
): MergeCorrespondenceAssignment | undefined {
  const proof = object(value),
    boundedSchemas = [
      "native-independent-merge-correspondence-output.v1",
      "owner-scoped-native-merge-correspondence-output.v1",
    ];
  if (boundedSchemas.includes(String(proof.schema))) {
    if (
      proof.schema !== boundedSchemas[platform ? 0 : 1] ||
      !Object.hasOwn(proof, "reviewAssignment")
    )
      throw Error("NATIVE_REVIEW_ASSIGNMENT_EXACT_BOUNDED_SCHEMA_REQUIRED");
    return parseMergeCorrespondenceAssignment(proof.reviewAssignment);
  }
  if (Object.hasOwn(proof, "reviewAssignment"))
    throw Error("NATIVE_REVIEW_ASSIGNMENT_FULL_SCHEMA_CANNOT_CARRY_BOUNDED_SCOPE");
  return undefined;
}

export function sameMergeCorrespondenceAssignment(
  left: MergeCorrespondenceAssignment,
  right: MergeCorrespondenceAssignment,
): boolean {
  return (
    JSON.stringify(parseMergeCorrespondenceAssignment(left)) ===
    JSON.stringify(parseMergeCorrespondenceAssignment(right))
  );
}
