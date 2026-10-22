/** Technical policy successor to Academy owner-local context-admission/1.
 * Source assessment only; native channels and actual teacher/human decisions
 * retain their owners. Broad permitted retrieval is not broad effect authority. */
export const academyMemoryPolicy = {
  schema: "academy.context-admission.b2.v1",
  predecessorSha256: "bd1f172e9b046b32fca4e479779021d6cca4975b5fdff0eeee5c0ec8a4b366fe",
  retrieval: "all-currently-permitted-scopes",
  workMeaning: "relevance-context",
  teacherAuthority: "actual-teacher-owner-decision",
  nativePermissionGranted: false,
} as const;
export const academyChannels = ["shell", "mcp", "apps", "web", "session", "delegation"] as const;
export interface AcademyContextControls {
  policyGeneration: string;
  authenticatedReaderId: string;
  current: boolean;
  unauthorizedBodyObserved: boolean;
  quarantinedDerivedArtifacts: boolean;
  originalCustodyQualified: boolean;
  channels: Record<
    (typeof academyChannels)[number],
    {
      state: "current-permission-enforced" | "disabled" | "unverified";
      policyGeneration: string;
      sourceDigest: string | null;
    }
  >;
}
export function assessAcademyMemoryAdmission(controls: AcademyContextControls) {
  const holds: string[] = [];
  if (
    !controls.authenticatedReaderId?.trim() ||
    !controls.policyGeneration?.trim() ||
    !controls.current
  )
    holds.push("CURRENT_AUTHENTICATED_READER_POLICY_REQUIRED");
  if (controls.unauthorizedBodyObserved || controls.quarantinedDerivedArtifacts)
    holds.push("INTERRUPT_QUARANTINE_AND_FRESH_CONVERSATION_REQUIRED");
  if (!controls.originalCustodyQualified) holds.push("ORIGINAL_SOURCE_CUSTODY_REQUIRED");
  for (const name of academyChannels) {
    const channel = controls.channels?.[name];
    if (
      !channel ||
      !["current-permission-enforced", "disabled"].includes(channel.state) ||
      channel.policyGeneration !== controls.policyGeneration ||
      !channel.sourceDigest ||
      !/^[a-f0-9]{64}$/.test(channel.sourceDigest)
    )
      holds.push("CHANNEL_CONTROL_REQUIRED:" + name);
  }
  return {
    ...academyMemoryPolicy,
    status: holds.length ? ("held" as const) : ("ready-for-separate-native-validation" as const),
    holds,
    policyGeneration: controls.policyGeneration,
    currentHistoryModesSeparate: true,
    sourceAcquisition: "native-permit-before-body" as const,
    standingDecisionSource: "actual-governing-decision" as const,
    humanAcceptance: false as const,
  };
}
export interface NativeAcademyContextOwner {
  withCurrentControls<T>(
    purpose: string,
    consume: (controls: AcademyContextControls) => Promise<T>,
  ): Promise<T>;
}
/** JSON/query flags alone cannot supply current native controls. */
export async function withAcademyMemoryAdmission<T>(
  purpose: string,
  owner: NativeAcademyContextOwner,
  consume: (assessment: ReturnType<typeof assessAcademyMemoryAdmission>) => T | Promise<T>,
): Promise<T> {
  if (!purpose?.trim() || !owner || typeof owner.withCurrentControls !== "function")
    throw Error("ACADEMY_CURRENT_NATIVE_OWNER_REQUIRED");
  return owner.withCurrentControls(purpose, async (controls) => {
    const assessment = assessAcademyMemoryAdmission(controls);
    if (assessment.status === "held") throw Error("ACADEMY_CONTEXT_ADMISSION_HELD");
    const result = await consume(assessment);
    if (
      controls.policyGeneration !== assessment.policyGeneration ||
      assessAcademyMemoryAdmission(controls).status === "held"
    )
      throw Error("ACADEMY_CONTEXT_POLICY_CHANGED");
    return result;
  });
}
