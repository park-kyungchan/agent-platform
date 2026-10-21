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
import { canonicalJson, semanticDigest, validateReadonlyCandidates } from "./policy.ts";

const safeText = (read: () => unknown): string | null => {
  try {
    const value = read();
    return typeof value === "string" ? value.slice(0, 1024) : null;
  } catch {
    return null;
  }
};
/** One finite injected read, never a dispatcher, approval system, store, retry loop or task completer. */
export async function consumeReadonlySelection(input: {
  prepared: PreparedDecision;
  decision: SemanticDecision;
  handlers: ReadonlyMap<string, ReadonlyHandler>;
  /** Existing native owner authenticates actual session ownership, authorization and material source/resource preconditions. */
  observeCurrent(candidate: ReadonlyCandidate): Promise<CurrentReadBinding>;
}): Promise<ReadConsumptionReceipt> {
  const { prepared, decision } = input;
  const requestId = prepared.request.requestId,
    selectedCandidateId = decision.selectedCandidateId;
  const receipt = (
    status: ReadConsumptionReceipt["status"],
    reason: string,
    dispatched = false,
    observation: ReadObservation | null = null,
    preconditionEvidenceRefs: string[] = [],
  ): ReadConsumptionReceipt => ({
    requestId,
    selectedCandidateId,
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
    (prepared.request.schema === "jev.semantic-request.v2" &&
      decision.consumptionPolicy !== prepared.request.consumptionPolicy) ||
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
    current = JSON.parse(
      canonicalJson(await input.observeCurrent(candidate)),
    ) as CurrentReadBinding;
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
  // From this point every failure preserves the fact that the one handler was invoked.
  const preconditionEvidenceRefs = [...current.evidenceRefs];
  let observation: ReadObservation | null = null;
  let originalStatus: string | null = null,
    originalObservationId: string | null = null;
  let boundary: "post-read" | "receipt-serialization" = "post-read";
  try {
    let rawObservation: ReadObservation;
    try {
      rawObservation = await handler(candidate);
    } catch {
      return receipt(
        "read-unknown",
        "read-outcome-unknown-no-retry",
        true,
        null,
        preconditionEvidenceRefs,
      );
    }
    boundary = "receipt-serialization";
    originalStatus = safeText(() => rawObservation?.status);
    originalObservationId = safeText(() => rawObservation?.observationId);
    observation = JSON.parse(canonicalJson(rawObservation)) as ReadObservation;
    boundary = "post-read";
    if (
      !observation ||
      !["observed", "failed", "unknown"].includes(observation.status) ||
      typeof observation.observationId !== "string" ||
      !observation.observationId ||
      !Array.isArray(observation.evidenceRefs) ||
      !observation.evidenceRefs.every((ref) => typeof ref === "string" && ref.length > 0) ||
      typeof observation.postconditionSatisfied !== "boolean"
    )
      return receipt(
        "read-unknown",
        "read-postcondition-invalid",
        true,
        null,
        preconditionEvidenceRefs,
      );
    if (observation.status === "unknown")
      return receipt(
        "read-unknown",
        "read-postcondition-unknown",
        true,
        observation,
        preconditionEvidenceRefs,
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
        preconditionEvidenceRefs,
      );
    if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
      return receipt(
        "read-unknown",
        "candidate-changed-during-read",
        true,
        observation,
        preconditionEvidenceRefs,
      );
    let after: CurrentReadBinding;
    try {
      after = JSON.parse(
        canonicalJson(await input.observeCurrent(candidate)),
      ) as CurrentReadBinding;
    } catch {
      return receipt(
        "read-unknown",
        "post-read-currentness-unavailable",
        true,
        observation,
        preconditionEvidenceRefs,
      );
    }
    if (
      !after ||
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
        preconditionEvidenceRefs,
      );
    if (semanticDigest(candidate) !== prepared.candidateDigests[candidate.candidateId])
      return receipt(
        "read-unknown",
        "candidate-changed-during-postcondition",
        true,
        observation,
        preconditionEvidenceRefs,
      );
    return receipt("verified-read", "fresh-native-read-postcondition-observed", true, observation, [
      ...preconditionEvidenceRefs,
      ...after.evidenceRefs,
    ]);
  } catch {
    return {
      ...receipt(
        "read-unknown",
        "post-dispatch-failure-no-retry",
        true,
        observation,
        preconditionEvidenceRefs,
      ),
      failure: {
        schema: "jev.read-failure.v1",
        boundary,
        originalStatus,
        originalObservationId,
      },
    };
  }
}

/** Existing owners persist results. A thrown write has an unknown outcome and is never retried here. */
export async function persistReadConsumptionReceipt(
  original: ReadConsumptionReceipt,
  persist: (serialized: string) => Promise<void>,
): Promise<{
  status: "stored" | "storage-unknown";
  receipt: ReadConsumptionReceipt;
  retryAllowed: false;
}> {
  const originalFacts = {
    requestId: safeText(() => original.requestId) ?? "unknown",
    selectedCandidateId: safeText(() => original.selectedCandidateId) ?? "unknown",
    dispatched: original.dispatched === true,
    status: safeText(() => original.status),
    observationId: safeText(() => original.observation?.observationId),
  };
  let boundary: "receipt-serialization" | "owner-persistence" = "receipt-serialization";
  try {
    const serialized = canonicalJson(original);
    boundary = "owner-persistence";
    await persist(serialized);
    return {
      status: "stored",
      receipt: JSON.parse(serialized) as ReadConsumptionReceipt,
      retryAllowed: false,
    };
  } catch {
    // This independent envelope contains only bounded primitives; no source/error payload is copied.
    return {
      status: "storage-unknown",
      receipt: {
        requestId: originalFacts.requestId,
        selectedCandidateId: originalFacts.selectedCandidateId,
        status: originalFacts.dispatched ? "read-unknown" : "held",
        reason: "owner-result-outcome-unknown-no-retry",
        dispatched: originalFacts.dispatched,
        observation: null,
        preconditionEvidenceRefs: [],
        executionAuthorityGranted: false,
        taskCompletionGranted: false,
        retryAllowed: false,
        failure: {
          schema: "jev.read-failure.v1",
          boundary,
          originalStatus: originalFacts.status,
          originalObservationId: originalFacts.observationId,
        },
      },
      retryAllowed: false,
    };
  }
}
