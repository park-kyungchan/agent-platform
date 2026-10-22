import { test, expect } from "bun:test";
import {
  withMemoryQuery, createMemoryQueryView, memoryClaimKey, MemoryAcquisitionGap,
  type MemoryDocument, type GoverningDocument, type MemoryQuery, type Scope,
  type NativeMemoryReader, type NativeMemoryReadLease,
  type NativeMemoryUse, type MemoryAddress, type NativeMemoryPermit, type SourceRef,
} from "./v1.ts";
import { memory, governing, nativeFixture } from "./fixtures/native-reader.ts";
import { memoryDigest } from "./codec.ts";

const ownerA: Scope = { kind: "work", projectId: "synthetic-one", workId: "style" };
const ownerB: Scope = { kind: "work", projectId: "synthetic-two", workId: "style" };
const localKey = "rendering";
const request = (extra: Partial<MemoryQuery> = {}): MemoryQuery => ({
  schema: "common.memory-query.v1", purpose: "synthetic authority regression",
  query: "visible-evidence", mode: "current", maxChars: 64000, limit: 20, ...extra,
});
const decision = (id: string, value: string, owner: Scope = ownerA, text = "표기 규약") =>
  governing(id, value, { owner, text, claim: { key: localKey, value } });
const adopted = (id: string, value: string, owner: Scope = ownerA, text = "visible-evidence") =>
  memory(id, text, { owner, kind: "decision", authority: "owner-adopted" }, { key: localKey, value });
type Fixture = ReturnType<typeof nativeFixture>;
// Explicit synthetic owner proof from the supplied acquired originals. Common
// code must still join owner/key, exact sources and current applicability.
function qualifiedOwner(f: Fixture, decisions: MemoryDocument[], change?: (
  lease: NativeMemoryReadLease,
) => Partial<NativeMemoryReadLease>): NativeMemoryReader {
  return {
    async withCurrentRead(q, consume) {
      return f.owner.withCurrentRead(q, (lease) => consume({
        ...lease,
        async qualifyMemoryUse(_address: MemoryAddress, document: MemoryDocument): Promise<NativeMemoryUse | null> {
          const fixture = decisions.find((d) => d.record.id === document.record.id &&
            memoryClaimKey({ owner: d.record.owner, key: localKey }) ===
            memoryClaimKey({ owner: document.record.owner, key: localKey }));
          return fixture ? {
            kind: "governing-decision", source: fixture.record.sourceRefs[0]!,
            decision: { ...decision("actual-" + fixture.record.id, fixture.claim!.value, fixture.record.owner),
              sourceRefs: fixture.record.sourceRefs },
          } : null;
        },
        ...change?.(lease),
      }));
    },
  };
}

test("canonical claim tuples preserve raw local keys and exact owner boundaries", () => {
  const rawKey = "legacy/local:key";
  const identity = { owner: ownerA, key: rawKey };
  expect(memoryClaimKey(identity)).toBe(memoryClaimKey({ key: rawKey,
    owner: { workId: "style", kind: "work", projectId: "synthetic-one" } }));
  expect(memoryClaimKey(identity)).not.toBe(memoryClaimKey({ owner: ownerB, key: rawKey }));
  expect(memoryClaimKey(identity)).not.toBe(memoryClaimKey({
    owner: { kind: "project", projectId: "synthetic-one" }, key: rawKey,
  }));
  expect(identity.key).toBe(rawKey);
});

test("fresh paraphrase lexical miss retains exact applicable conflicting current originals", async () => {
  const docs = [decision("one", "한국어"), decision("two", "영어")];
  const f = nativeFixture(docs);
  const out = await withMemoryQuery(request({ query: "How should a reply be phrased for readers?" }),
    f.owner, (r) => r);
  expect(out.counts.matched).toBe(0);
  expect(out.hits).toEqual([]);
  expect(out.governing.map((g) => g.source)).toEqual(f.addresses.map((a) => a.source));
  expect(out.governing.every((g) => g.governs)).toBe(true);
  expect(out.conflicts).toHaveLength(1);
  expect(out.conflicts[0]).toMatchObject({
    claimIdentity: { owner: ownerA, key: localKey }, governingConflict: true,
    resolution: "conflict", selectedGoverningValue: null, incompleteSources: [],
  });
  expect(out.sourceCoverage).toBe("complete");
});

test("same-owner incompatible adopted Decisions never become eligible, including a lexical miss", async () => {
  const docs = [adopted("first", "one"), adopted("second", "two")];
  const f = nativeFixture(docs), owner = qualifiedOwner(f, docs);
  for (const query of ["visible-evidence", "How should a reply be phrased for readers?"]) {
    const out = await withMemoryQuery(request({ query }), owner, (r) => r);
    expect(out.governing).toHaveLength(2);
    expect(out.conflicts[0]!.governingConflict).toBe(true);
    expect(out.conflicts[0]!.selectedGoverningValue).toBeNull();
    expect(out.hits.every((h) => !h.currentInstructionEligible)).toBe(true);
  }
});

test.each(["different", "same"])("equal local keys and Memory IDs in independent owners preserve %s values", async (values) => {
  const docs = [adopted("same-old-id", "one", ownerA),
    adopted("same-old-id", values === "same" ? "one" : "two", ownerB, "별도 준칙")];
  const f = nativeFixture(docs), owner = qualifiedOwner(f, docs);
  const out = await withMemoryQuery(request(), owner, (r) => r);
  expect(out.hits).toHaveLength(1);
  expect(out.hits[0]!.document.record.owner).toEqual(ownerA);
  expect(out.hits[0]!.currentInstructionEligible).toBe(true);
  expect(out.counts.governingClosure).toBe(0);
  expect(out.conflicts).toHaveLength(2);
  expect(out.conflicts.every((c) => !c.governingConflict && c.resolution === "resolved")).toBe(true);
  expect(new Set(out.conflicts.map((c) => memoryClaimKey(c.claimIdentity))).size).toBe(2);
  expect(out.conflicts.every((c) => c.claimKey === localKey)).toBe(true);
  const both = await withMemoryQuery(request({ query: "visible-evidence 준칙" }), owner, (r) => r);
  expect(both.hits).toHaveLength(2);
  expect(new Set(both.hits.map((h) => h.key)).size).toBe(2);
});

test("overlapping subject discovery and selected Work do not confer cross-owner override", async () => {
  const summary = memory("summary", "visible-evidence", { owner: ownerA, subjects: [ownerB] },
    { key: localKey, value: "one" });
  const other = adopted("other", "two", ownerB, "별도 준칙");
  const f = nativeFixture([summary, other]);
  const out = await withMemoryQuery(request({ scopes: [ownerB],
    work: { projectId: "synthetic-two", workId: "style" } }), qualifiedOwner(f, [other]), (r) => r);
  expect(out.hits.map((h) => h.document.record.id)).toEqual(["summary"]);
  expect(out.counts.governingClosure).toBe(0);
  expect(out.conflicts.find((c) => c.claimIdentity.owner.kind === "work" &&
    c.claimIdentity.owner.projectId === "synthetic-one")!.selectedGoverningValue).toBeNull();
  const forgeryDoc = adopted("forgery-control", "one"), forgeryFixture = nativeFixture([forgeryDoc]);
  const forged = qualifiedOwner(forgeryFixture, [forgeryDoc], () => ({
    async qualifyMemoryUse(_a, document) {
      return { kind: "governing-decision", source: document.record.sourceRefs[0]!,
        decision: { ...decision("cross-scope", "two", ownerB), sourceRefs: document.record.sourceRefs } };
    },
  }));
  await expect(withMemoryQuery(request(), forged, (r) => r)).rejects.toThrow("SOURCE_SCOPE_MISMATCH");
});

test.each(["original", "envelope"])("known same-claim unavailable %s holds only its claim and retains source provenance", async (phase) => {
  const winner = adopted("winner", "one"), missing = decision("missing", "two");
  const f = nativeFixture([winner, missing]), missingAddress = f.addresses[1]!;
  const owner = qualifiedOwner(f, [winner], (lease) => ({
    ...(phase === "original" ? {
      async readOriginal(source: SourceRef, permit: NativeMemoryPermit) {
        if (source.uri === missing.sourceRefs[0]!.uri) throw new MemoryAcquisitionGap("unreachable");
        return lease.readOriginal(source, permit);
      },
    } : {
      async acquire(address: MemoryAddress, permit: NativeMemoryPermit) {
        if (address.source.uri === missingAddress.source.uri) throw new MemoryAcquisitionGap("missing");
        return lease.acquire(address, permit);
      },
    }),
  }));
  const out = await withMemoryQuery(request(), owner, (r) => r);
  expect(out.hits).toHaveLength(1);
  expect(out.hits[0]!.currentInstructionEligible).toBe(false);
  expect(out.conflicts[0]).toMatchObject({ resolution: "incomplete", selectedGoverningValue: null,
    governingConflict: false, claimIdentity: { owner: ownerA, key: localKey },
    incompleteSources: [{ code: phase === "original" ? "unreachable" : "missing",
      source: missingAddress.source, kind: "governing", recordClass: "decision" }] });
  expect(out.acquisitionGaps[0]).toMatchObject({ source: missingAddress.source,
    owner: ownerA, claimIdentity: { owner: ownerA, key: localKey } });
  if (phase === "original") expect(out.acquisitionGaps[0]!.original).toEqual(missing.sourceRefs[0]);
  expect(out.sourceCoverage).toBe("partial");
  expect(out.nativePermissionGranted).toBe(false);
});

test.each(["other-owner", "other-claim", "historical", "advisory", "unknown-inventory", "denied"])(
  "unrelated %s gaps do not hold the complete qualified claim", async (control) => {
    const winner = adopted("winner", "one");
    const unavailable = control === "advisory"
      ? memory("unavailable", "別文", { owner: ownerA }, { key: localKey, value: "two" })
      : decision("unavailable", "two", control === "other-owner" ? ownerB : ownerA);
    if (control === "other-claim" && unavailable.schema === "common.memory-governing.v1")
      unavailable.claim.key = "unrelated-key";
    const f = nativeFixture(control === "unknown-inventory" ? [winner] : [winner, unavailable]);
    if (control === "historical") f.addresses[1]!.currentness = "historical";
    if (control === "unknown-inventory") f.state.gaps = [{ code: "unreachable" }];
    if (control === "denied") f.denied.add(f.addresses[1]!.source.uri);
    const owner = qualifiedOwner(f, [winner], (lease) => ({
      async acquire(address, permit) {
        if (address.source.uri === f.addresses[1]?.source.uri) throw new MemoryAcquisitionGap("unreachable");
        return lease.acquire(address, permit);
      },
    }));
    const out = await withMemoryQuery(request({ mode: "history" }), owner, (r) => r);
    const group = out.conflicts.find((c) => memoryClaimKey(c.claimIdentity) ===
      memoryClaimKey({ owner: ownerA, key: localKey }))!;
    expect(group.resolution).toBe("resolved");
    expect(group.selectedGoverningValue).toBe("one");
    expect(group.incompleteSources).toEqual([]);
    expect(out.hits.find((h) => h.document.record.id === "winner")!.currentInstructionEligible).toBe(true);
  },
);

test.each(["count", "original", "original-precheck", "envelope", "envelope-precheck"])(
  "known %s budget omission cannot qualify a surviving unique Decision", async (variant) => {
    const winner = adopted("winner", "one");
    let missing: MemoryDocument | GoverningDocument = decision("missing", "two");
    let docs: Array<MemoryDocument | GoverningDocument>;
    let originalBytes: Uint8Array | undefined;
    if (variant === "count") {
      docs = [winner, ...Array.from({ length: 999 }, (_, i) => memory("filler-" + i, "별도 자료")), missing];
    } else if (variant.startsWith("original")) {
      originalBytes = new TextEncoder().encode("x".repeat(variant === "original" ? 6_000_000 : 8_388_608));
      const ref = (id: string) => ({ owner: "synthetic-owner", uri: "fixture://large/" + id,
        revision: "1", digest: memoryDigest(originalBytes!) });
      missing.sourceRefs = [ref("missing")];
      docs = [winner, ...[0, 1].map((i) => memory("filler-" + i, "별도 자료", { sourceRefs: [ref(String(i))] })), missing];
    } else {
      const large = (doc: MemoryDocument) => ({ ...doc, legacy: {
        protocol: "synthetic-legacy", identityKey: doc.record.id, original: doc.record.sourceRefs[0]!,
        metadata: Array.from({ length: 15 }, () => "x".repeat(60_000)),
      } });
      if (variant === "envelope") missing = large(adopted("missing", "two", ownerA, "別文"));
      docs = [winner, large(memory("filler-0", "別文")), large(memory("filler-1", "別文")),
        ...(variant === "envelope-precheck" ? [large(memory("filler-2", "別文"))] : []), missing];
    }
    const f = nativeFixture(docs), address = f.addresses.at(-1)!;
    const owner = qualifiedOwner(f, [winner], (lease) => ({
      async readOriginal(source, permit) {
        return source.uri.startsWith("fixture://large/") ? originalBytes! : lease.readOriginal(source, permit);
      },
    }));
    const out = await withMemoryQuery(request(), owner, (r) => r);
    expect(out.sourceCoverage).toBe("partial");
    expect(out.omissions.sourceLimit).toBeGreaterThan(0);
    expect(out.hits.find((h) => h.document.record.id === "winner")!.currentInstructionEligible).toBe(false);
    const group = out.conflicts.find((c) => c.claimKey === localKey)!;
    expect(group.resolution).toBe("incomplete");
    expect(group.selectedGoverningValue).toBeNull();
    expect(group.incompleteSources.find((gap) => gap.source.uri === address.source.uri)).toMatchObject({ source: address.source, kind: address.kind,
      recordClass: "decision", code: variant === "count" ? "source-count-limit" :
        variant.startsWith("original") ? "original-byte-limit" : "envelope-byte-limit" });
    if (variant !== "envelope") expect(f.reads.has(address.source.uri)).toBe(false);
  },
);

test("governing output omissions retain exact affected provenance and no unique selection", async () => {
  const docs = Array.from({ length: 21 }, (_, i) => decision("same-" + i, "one"));
  const f = nativeFixture(docs);
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.governing).toHaveLength(20);
  expect(out.omissions.governing).toBe(1);
  expect(out.conflicts[0]!.resolution).toBe("incomplete");
  expect(out.conflicts[0]!.selectedGoverningValue).toBeNull();
  expect(out.conflicts[0]!.incompleteSources[0]).toMatchObject({
    source: f.addresses[20]!.source, code: "governing-output-limit",
  });
});

test("a standing preference is held only by its exact incomplete claim", async () => {
  const preference = memory("standing", "visible-evidence", {
    owner: ownerA, kind: "preference", authority: "owner-adopted",
  }, { key: localKey, value: "one" });
  const unrelated = memory("unrelated-standing", "visible-evidence", {
    owner: ownerB, kind: "preference", authority: "owner-adopted",
  }, { key: localKey, value: "two" });
  const f = nativeFixture([preference, unrelated, decision("unavailable", "two")]);
  const owner = qualifiedOwner(f, [], (lease) => ({
    async qualifyMemoryUse(_address, document) {
      return { kind: "standing-preference", source: document.record.sourceRefs[0]! };
    },
    async acquire(address, permit) {
      if (address.source.uri === f.addresses[2]!.source.uri) throw new MemoryAcquisitionGap("missing");
      return lease.acquire(address, permit);
    },
  }));
  const out = await withMemoryQuery(request(), owner, (r) => r);
  expect(out.hits.find((h) => h.document.record.id === "standing")!.currentInstructionEligible).toBe(false);
  expect(out.hits.find((h) => h.document.record.id === "unrelated-standing")!.currentInstructionEligible).toBe(true);
});

test("current/history authority and repeat views reauthorize originals through consumer completion", async () => {
  const docs = [decision("current", "one"), decision("past", "two")];
  docs[1]!.status = "superseded";
  const f = nativeFixture(docs), view = createMemoryQueryView(request({ mode: "history", query: "표기" }), f.owner);
  const history = await view.withCurrent((r) => r);
  expect(history.governing).toHaveLength(2);
  expect(history.governing.filter((g) => g.governs)).toHaveLength(1);
  expect(history.conflicts[0]).toMatchObject({ conflictingEvidence: true,
    governingConflict: false, selectedGoverningValue: "one" });
  f.denied.add(f.addresses[0]!.header.sourceRefs[0]!.uri);
  f.state.generation++;
  const revoked = await view.withCurrent((r) => r);
  expect(revoked.governing.every((g) => g.document.id !== "current")).toBe(true);
  expect(revoked.conflicts[0]!.selectedGoverningValue).toBeNull();
  f.denied.clear();
  await expect(view.withCurrent(() => { f.state.generation++; })).rejects.toThrow("POLICY_REVOKED");
  view.close();
});
