import { expect, test } from "bun:test";
import { qualifyItemSupplementState, semanticDigest, type ItemSupplementState } from "./index.ts";
const pin = (path: string) => ({ path, sha256: "a".repeat(64) });
const initial = (): ItemSupplementState => ({
  schema: "jev.item-supplement-state.v1",
  item: { domainId: "domain", projectId: "project", workId: "work", itemId: "field:stable" },
  revision: 1,
  previousDigest: null,
  claim: { claimId: "claim", revision: 1, actorId: "actor-one", contextRevision: "context-one" },
  supplements: 0,
  terminal: null,
  effectFences: [],
  ownerReceipt: pin("owner:1"),
  continuation: "update_plan",
});
function successor(
  previous: ItemSupplementState,
  patch: Partial<ItemSupplementState>,
): ItemSupplementState {
  return {
    ...structuredClone(previous),
    revision: previous.revision + 1,
    previousDigest: semanticDigest(previous),
    ownerReceipt: pin(`owner:${previous.revision + 1}`),
    ...patch,
  };
}
function qualify(
  previous: ItemSupplementState | null,
  observed: ItemSupplementState,
  operation: Parameters<typeof qualifyItemSupplementState>[0]["operation"],
) {
  return qualifyItemSupplementState({
    previous,
    observed,
    operation,
    current: {
      authorized: true,
      itemDigest: semanticDigest(observed.item),
      stateDigest: semanticDigest(observed),
      previousDigest: observed.previousDigest,
      ownerReceipt: observed.ownerReceipt,
      evidenceRefs: ["native:current-owner-checkpoint"],
    },
  });
}
test("two supplements survive actor, claim and context handoff and restore without a third", () => {
  const first = initial();
  expect(qualify(null, first, "initialize").status).toBe("verified-owner-state");
  const one = successor(first, { supplements: 1 });
  expect(qualify(first, one, "supplement").nextSupplementAllowed).toBe(true);
  const handoff = successor(one, {
    claim: {
      claimId: "new-claim",
      revision: 2,
      actorId: "actor-two",
      contextRevision: "compacted",
    },
  });
  expect(qualify(one, handoff, "handoff").state?.supplements).toBe(1);
  const two = successor(handoff, { supplements: 2 });
  expect(qualify(handoff, two, "supplement").nextSupplementAllowed).toBe(false);
  expect(qualify(two, structuredClone(two), "restore").status).toBe("verified-owner-state");
  expect(qualify(two, successor(two, { supplements: 3 as never }), "supplement").status).toBe(
    "held",
  );
  expect(qualify(two, successor(two, { supplements: 0 }), "handoff").status).toBe("held");
  expect(qualify(two, one, "restore").status).toBe("held");
});
test("negative terminal, result revision, stable identity and completed/unknown fences remain bound", () => {
  const first = initial();
  first.effectFences.push({
    effectId: "completed-read",
    outcome: "completed",
    receipt: pin("effect:done"),
  });
  const terminal = successor(first, {
    terminal: { outcome: "negative", resultRevision: 2, result: pin("result:negative") },
  });
  expect(qualify(first, terminal, "terminal").state?.terminal?.outcome).toBe("negative");
  const next = successor(terminal, {
    claim: { ...terminal.claim, revision: 2, actorId: "new-actor" },
  });
  expect(qualify(terminal, next, "handoff").status).toBe("verified-owner-state");
  expect(qualify(terminal, successor(terminal, { terminal: null }), "handoff").status).toBe("held");
  expect(qualify(terminal, successor(terminal, { effectFences: [] }), "handoff").status).toBe(
    "held",
  );
  expect(
    qualify(
      terminal,
      successor(terminal, { item: { ...terminal.item, itemId: "foreign" } }),
      "handoff",
    ).status,
  ).toBe("held");
  const unknown = successor(first, {
    effectFences: [
      ...first.effectFences,
      { effectId: "provider-ambiguous", outcome: "unknown", receipt: pin("effect:unknown") },
    ],
  });
  expect(qualify(first, unknown, "handoff").nextSupplementAllowed).toBe(false);
  expect(qualify(unknown, successor(unknown, { supplements: 1 }), "supplement").status).toBe(
    "held",
  );
  expect(
    qualify(unknown, successor(unknown, { terminal: terminal.terminal }), "terminal").status,
  ).toBe("held");
  expect(qualify(unknown, unknown, "restore").state?.effectFences).toEqual(unknown.effectFences);
});
test("an unknown effect introduced in the terminal checkpoint cannot become a semantic verdict", () => {
  const first = initial();
  const next = successor(first, {
    terminal: { outcome: "negative", resultRevision: 1, result: pin("verdict") },
    effectFences: [
      { effectId: "unknown-now", outcome: "unknown", receipt: pin("effect:new-unknown") },
    ],
  });
  expect(qualify(first, next, "terminal").status).toBe("held");
});
