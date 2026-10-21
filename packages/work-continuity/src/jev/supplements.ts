import type { ContractPin } from "./contract.ts";
import { canonicalJson, semanticDigest, validateContractPin } from "./policy.ts";

/** Persisted and authenticated by existing Work/native owners, never by this module. */
export interface ItemSupplementState {
  schema: "jev.item-supplement-state.v1";
  item: { domainId: string; projectId: string; workId: string; itemId: string };
  revision: number;
  previousDigest: string | null;
  claim: { claimId: string; revision: number; actorId: string; contextRevision: string };
  supplements: 0 | 1 | 2;
  terminal: null | {
    outcome: "supported" | "negative" | "insufficient" | "rejected";
    resultRevision: number;
    result: ContractPin;
  };
  /** Append-only receipts: completed and unknown effects cannot become a fresh attempt. */
  effectFences: { effectId: string; outcome: "completed" | "unknown"; receipt: ContractPin }[];
  ownerReceipt: ContractPin;
  continuation: "update_plan";
}
export interface CurrentItemOwner {
  authorized: boolean;
  itemDigest: string;
  previousDigest: string | null;
  stateDigest: string;
  ownerReceipt: ContractPin;
  evidenceRefs: string[];
}
const id = (value: unknown): value is string =>
  typeof value === "string" &&
  value.trim().length > 0 &&
  value.length <= 1024 &&
  !/[\u0000-\u001f]/u.test(value);
function validate(state: ItemSupplementState): void {
  canonicalJson(state);
  if (
    state.schema !== "jev.item-supplement-state.v1" ||
    !state.item ||
    ![state.item.domainId, state.item.projectId, state.item.workId, state.item.itemId].every(id) ||
    !Number.isSafeInteger(state.revision) ||
    state.revision < 1 ||
    (state.previousDigest !== null && !/^[a-f0-9]{64}$/u.test(state.previousDigest)) ||
    !state.claim ||
    ![state.claim.claimId, state.claim.actorId, state.claim.contextRevision].every(id) ||
    !Number.isSafeInteger(state.claim.revision) ||
    state.claim.revision < 1 ||
    ![0, 1, 2].includes(state.supplements) ||
    state.continuation !== "update_plan" ||
    !Array.isArray(state.effectFences) ||
    state.effectFences.length > 128
  )
    throw Error("ITEM_SUPPLEMENT_STATE_INVALID");
  validateContractPin(state.ownerReceipt);
  const effects = new Set<string>();
  for (const fence of state.effectFences) {
    if (
      !id(fence.effectId) ||
      !["completed", "unknown"].includes(fence.outcome) ||
      effects.has(semanticDigest(fence))
    )
      throw Error("ITEM_EFFECT_FENCE_INVALID");
    validateContractPin(fence.receipt);
    effects.add(semanticDigest(fence));
  }
  if (state.terminal !== null) {
    if (
      !state.terminal ||
      !["supported", "negative", "insufficient", "rejected"].includes(state.terminal.outcome) ||
      !Number.isSafeInteger(state.terminal.resultRevision) ||
      state.terminal.resultRevision < 1
    )
      throw Error("ITEM_TERMINAL_RESULT_INVALID");
    validateContractPin(state.terminal.result);
  }
}
/** Verify one already-persisted transition or restore against the owner's exact current checkpoint.
 * Claim, actor and context may change. Stable item identity, supplements, terminal results and fences do not reset.
 * A provider/API failure is not a semantic terminal result; its owner retains an unknown effect fence.
 */
export function qualifyItemSupplementState(input: {
  previous: ItemSupplementState | null;
  observed: ItemSupplementState;
  current: CurrentItemOwner;
  operation: "initialize" | "supplement" | "terminal" | "handoff" | "restore";
}): {
  schema: "jev.item-supplement-qualification.v1";
  status: "verified-owner-state" | "held";
  reason: string;
  state: ItemSupplementState | null;
  nextSupplementAllowed: boolean;
  executionAuthorityGranted: false;
  retryAllowed: false;
  permitsReplay: false;
} {
  const result = (reason: string, state: ItemSupplementState | null = null) => ({
    schema: "jev.item-supplement-qualification.v1" as const,
    status: state ? ("verified-owner-state" as const) : ("held" as const),
    reason,
    state,
    nextSupplementAllowed:
      !!state &&
      state.terminal === null &&
      state.supplements < 2 &&
      !state.effectFences.some((fence) => fence.outcome === "unknown"),
    executionAuthorityGranted: false as const,
    retryAllowed: false as const,
    permitsReplay: false as const,
  });
  try {
    const { previous: p, observed: n, current: c, operation } = input;
    if (!["initialize", "supplement", "terminal", "handoff", "restore"].includes(operation))
      return result("item-operation-invalid");
    validate(n);
    if (
      !c ||
      c.authorized !== true ||
      c.itemDigest !== semanticDigest(n.item) ||
      c.previousDigest !== n.previousDigest ||
      c.stateDigest !== semanticDigest(n) ||
      canonicalJson(c.ownerReceipt) !== canonicalJson(n.ownerReceipt) ||
      !Array.isArray(c.evidenceRefs) ||
      !c.evidenceRefs.length ||
      !c.evidenceRefs.every(id)
    )
      return result("item-owner-checkpoint-mismatch");
    if (operation === "initialize") {
      if (
        p !== null ||
        n.revision !== 1 ||
        n.previousDigest !== null ||
        n.supplements !== 0 ||
        n.terminal !== null
      )
        return result("item-initialization-mismatch");
    } else {
      if (!p) return result("previous-owner-checkpoint-required");
      validate(p);
      if (semanticDigest(p.item) !== semanticDigest(n.item)) return result("stable-item-mismatch");
      if (operation === "restore") {
        if (semanticDigest(p) !== semanticDigest(n)) return result("restore-checkpoint-mismatch");
      } else {
        if (
          n.revision !== p.revision + 1 ||
          n.previousDigest !== semanticDigest(p) ||
          n.claim.revision < p.claim.revision ||
          (canonicalJson(n.claim) !== canonicalJson(p.claim) &&
            n.claim.revision <= p.claim.revision)
        )
          return result("item-transition-revision-mismatch");
        const increment = operation === "supplement" ? 1 : 0;
        if (
          n.supplements !== p.supplements + increment ||
          (operation === "supplement" &&
            (p.terminal !== null || p.effectFences.some((fence) => fence.outcome === "unknown")))
        )
          return result("supplement-limit-terminal-or-unknown-effect-fence");
        if (p.terminal !== null && canonicalJson(p.terminal) !== canonicalJson(n.terminal))
          return result("terminal-result-is-immutable");
        if (
          (operation === "terminal" && n.terminal === null) ||
          (operation !== "terminal" && canonicalJson(p.terminal) !== canonicalJson(n.terminal))
        )
          return result("explicit-terminal-transition-required");
        // An unknown native/API effect requires owner reconciliation, never a semantic verdict.
        if (
          operation === "terminal" &&
          [...p.effectFences, ...n.effectFences].some((fence) => fence.outcome === "unknown")
        )
          return result("unknown-effect-cannot-be-semantic-terminal");
        const nextFences = new Set(n.effectFences.map(semanticDigest));
        if (p.effectFences.some((fence) => !nextFences.has(semanticDigest(fence))))
          return result("effect-receipts-cannot-be-dropped");
      }
    }
    return result(
      "exact-existing-owner-state-qualified",
      JSON.parse(canonicalJson(n)) as ItemSupplementState,
    );
  } catch {
    return result("malformed-item-or-owner-state");
  }
}
