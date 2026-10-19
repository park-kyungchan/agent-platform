import {
  ABSTAIN_ID,
  type PreparedDecision,
  type SemanticDecision,
  type CurrentReadBinding,
  type ReadonlyCandidate,
  type ReadonlyHandler,
  type ReadConsumptionReceipt,
  type ReadObservation,
} from "./contract.ts";
import { semanticDigest, validateReadonlyCandidates } from "./policy.ts";

/** One finite injected read, never a dispatcher, approval system, store, retry loop or task completer. */
export async function consumeReadonlySelection(input: {
  prepared: PreparedDecision;
  decision: SemanticDecision;
  handlers: ReadonlyMap<string, ReadonlyHandler>;
  /** Existing native owner authenticates actual session ownership, authorization and material source/resource preconditions. */
  observeCurrent(candidate: ReadonlyCandidate): Promise<CurrentReadBinding>;
}): Promise<ReadConsumptionReceipt> {
  const { prepared, decision } = input;
  const receipt = (
    status: ReadConsumptionReceipt["status"],
    reason: string,
    dispatched = false,
    observation: ReadObservation | null = null,
    preconditionEvidenceRefs: string[] = [],
  ): ReadConsumptionReceipt => ({
    requestId: prepared.request.requestId,
    selectedCandidateId: decision.selectedCandidateId,
    status,
    reason,
    dispatched,
    observation,
    preconditionEvidenceRefs,
    executionAuthorityGranted: false,
    taskCompletionGranted: false,
    retryAllowed: false,
  });
  if (
    decision.status !== "decision" ||
    semanticDigest(prepared.request) !== prepared.requestDigest ||
    decision.requestId !== prepared.request.requestId ||
    decision.stateDigest !== prepared.request.stateDigest ||
    decision.reuseKey !== prepared.reuseKey ||
    decision.executionAuthorityGranted !== false
  )
    return receipt("held", "decision-binding-mismatch");
  if (decision.selectedCandidateId === ABSTAIN_ID)
    return receipt("abstained", "needs-evidence-or-valid-candidate");
  const candidate = prepared.candidates.find(
    (offered) => offered.candidateId === decision.selectedCandidateId,
  );
  if (!candidate) return receipt("held", "candidate-not-offered");
  try {
    validateReadonlyCandidates([candidate]);
  } catch {
    return receipt("held", "candidate-invalid");
  }
  if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
    return receipt("held", "candidate-drift");
  if (candidate.kind === "wait")
    return receipt("selected-no-effect", "wait-for-material-native-event");
  const handler = candidate.handlerKey ? input.handlers.get(candidate.handlerKey) : undefined;
  if (!handler || !candidate.authorizationRef)
    return receipt("held", "fixed-authorized-read-handler-missing");
  let current: CurrentReadBinding;
  try {
    current = await input.observeCurrent(candidate);
  } catch {
    return receipt("held", "current-native-precondition-unavailable");
  }
  if (
    !current ||
    current.authorized !== true ||
    current.bindingDigest !== prepared.bindingDigest ||
    current.sourceRevision !== prepared.sourceRevision ||
    current.candidateDigest !== prepared.candidateDigests[candidate.candidateId] ||
    current.authorizationRef !== candidate.authorizationRef ||
    !Array.isArray(current.evidenceRefs) ||
    !current.evidenceRefs.length ||
    !current.evidenceRefs.every((ref) => typeof ref === "string" && ref.length > 0)
  )
    return receipt("held", "current-native-precondition-mismatch");
  if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
    return receipt("held", "candidate-changed-during-precondition");
  let observation: ReadObservation;
  try {
    observation = await handler(candidate);
  } catch {
    return receipt(
      "read-unknown",
      "read-outcome-unknown-no-retry",
      true,
      null,
      current.evidenceRefs,
    );
  }
  if (
    !observation ||
    !["observed", "failed", "unknown"].includes(observation.status) ||
    typeof observation.observationId !== "string" ||
    !observation.observationId ||
    !Array.isArray(observation.evidenceRefs) ||
    !observation.evidenceRefs.every((ref) => typeof ref === "string" && ref.length > 0) ||
    typeof observation.postconditionSatisfied !== "boolean"
  )
    return receipt("read-unknown", "read-postcondition-invalid", true, null, current.evidenceRefs);
  if (observation.status === "unknown")
    return receipt(
      "read-unknown",
      "read-postcondition-unknown",
      true,
      observation,
      current.evidenceRefs,
    );
  if (
    observation.status !== "observed" ||
    !observation.postconditionSatisfied ||
    !observation.evidenceRefs.length
  )
    return receipt(
      "read-failed",
      "read-postcondition-not-satisfied",
      true,
      observation,
      current.evidenceRefs,
    );
  if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
    return receipt(
      "read-unknown",
      "candidate-changed-during-read",
      true,
      observation,
      current.evidenceRefs,
    );
  let after: CurrentReadBinding;
  try {
    after = await input.observeCurrent(candidate);
  } catch {
    return receipt(
      "read-unknown",
      "post-read-currentness-unavailable",
      true,
      observation,
      current.evidenceRefs,
    );
  }
  if (
    after.authorized !== true ||
    after.bindingDigest !== current.bindingDigest ||
    after.sourceRevision !== current.sourceRevision ||
    after.candidateDigest !== current.candidateDigest ||
    after.authorizationRef !== current.authorizationRef ||
    !Array.isArray(after.evidenceRefs) ||
    !after.evidenceRefs.length ||
    !after.evidenceRefs.every((ref) => typeof ref === "string" && ref.length > 0)
  )
    return receipt(
      "read-unknown",
      "post-read-currentness-changed",
      true,
      observation,
      current.evidenceRefs,
    );
  if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
    return receipt(
      "read-unknown",
      "candidate-changed-during-postcondition",
      true,
      observation,
      current.evidenceRefs,
    );
  return receipt("verified-read", "fresh-native-read-postcondition-observed", true, observation, [
    ...current.evidenceRefs,
    ...after.evidenceRefs,
  ]);
}
