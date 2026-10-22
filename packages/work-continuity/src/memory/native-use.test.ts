import { test, expect } from "bun:test";
import {
  withMemoryQuery,
  canonicalMemory,
  memoryDigest,
  type MemoryQuery,
  type MemoryDocument,
  type MemoryRecord,
  type GoverningDocument,
  type SourceRef,
  type NativeMemoryUse,
  type NativeMemoryReader,
  type Scope,
} from "@palantirkc/work-continuity/memory/v1";
import { memory, nativeFixture, syntheticReaderId } from "./fixtures/native-reader.ts";

const ownerA: Scope = { kind: "work", projectId: "synthetic-use", workId: "current" };
const ownerB: Scope = { kind: "work", projectId: "synthetic-other", workId: "current" };
const localKey = "presentation";
const request = (extra: Partial<MemoryQuery> = {}): MemoryQuery => ({
  schema: "common.memory-query.v1",
  purpose: "generic native-use public contract regression",
  query: "합성 방침",
  mode: "current",
  maxChars: 64000,
  limit: 20,
  ...extra,
});
function row(
  id: string,
  options: {
    kind?: MemoryRecord["kind"];
    useKind?: NativeMemoryUse["kind"];
    owner?: Scope;
    summaryClaim?: { key: string; value: string };
    actual?: { value: string; overrides?: Partial<Omit<GoverningDocument, "sourceRefs">> };
  },
) {
  const owner = options.owner ?? ownerA,
    useKind = options.useKind ?? "governing-decision";
  const originalDecision: Omit<GoverningDocument, "sourceRefs"> | undefined = options.actual
    ? {
        schema: "common.memory-governing.v1",
        id: "original-decision-" + id,
        revision: 1,
        owner,
        recordClass: "decision",
        status: "active",
        allowedReaders: [syntheticReaderId],
        claim: { key: localKey, value: options.actual.value },
        text: "합성 방침 원문 " + id,
        ...options.actual.overrides,
      }
    : undefined;
  // Native original bytes carry adoption and actual Decision fields independently
  // of the Memory summary. No expected value comes from a candidate result.
  const original = {
    adoption: {
      id,
      owner,
      useKind,
      ...(options.summaryClaim ? { claim: options.summaryClaim } : {}),
    },
    ...(originalDecision ? { decision: originalDecision } : {}),
  };
  const bytes = new TextEncoder().encode(canonicalMemory(original));
  const source: SourceRef = {
    owner: "synthetic-owner",
    uri: "fixture://exact-native-original/" + id,
    revision: "native-original1",
    digest: memoryDigest(bytes),
  };
  const actual: GoverningDocument | undefined = originalDecision
    ? { ...originalDecision, sourceRefs: [source] }
    : undefined;
  const document = memory(
    id,
    "합성 방침 요약 " + id,
    { owner, kind: options.kind ?? "decision", authority: "owner-adopted", sourceRefs: [source] },
    options.summaryClaim,
  );
  const proposed: unknown = { kind: useKind, source, ...(actual ? { decision: actual } : {}) };
  return { document, actual, proposed, source, bytes, useKind };
}
type Row = ReturnType<typeof row>;
function fixture(
  rows: Row[],
  options: {
    representation?: "memory" | "direct";
    governing?: "qualified" | "missing" | "false";
    mutateGoverning?: boolean;
    beforeAdoption?: (index: number, document: MemoryDocument) => void;
  } = {},
) {
  const originals = new Map(rows.map((r) => [r.source.uri, r.bytes]));
  const documents =
    options.representation === "direct" ? rows.map((r) => r.actual!) : rows.map((r) => r.document);
  const f = nativeFixture(documents, undefined, originals),
    state = { governingCalls: 0, adoptionCalls: 0 };
  const owner: NativeMemoryReader = {
    async withCurrentRead(q, consume) {
      return f.owner.withCurrentRead(q, (lease) =>
        consume({
          ...lease,
          async qualifyMemoryUse(address, document) {
            const index = rows.findIndex((r) => r.document.record.id === document.record.id);
            const selected = rows[index]!;
            expect(canonicalMemory(document)).toBe(canonicalMemory(selected.document));
            expect(f.events).toContain("original:" + selected.source.uri);
            expect(f.reads.get(address.source.uri)).toBe(1);
            const original = JSON.parse(
              new TextDecoder().decode(originals.get(selected.source.uri)!),
            ) as {
              adoption: {
                id: string;
                owner: Scope;
                useKind: NativeMemoryUse["kind"];
                claim?: { key: string; value: string };
              };
            };
            expect(original.adoption).toEqual({
              id: document.record.id,
              owner: document.record.owner,
              useKind: selected.useKind,
              ...(document.claim ? { claim: document.claim } : {}),
            });
            state.adoptionCalls++;
            options.beforeAdoption?.(index, document);
            // Deliberately includes runtime-invalid native adapter shapes in negative
            // cases. The public producer must enforce the disjoint contract itself.
            return selected.proposed as NativeMemoryUse | null;
          },
          qualifyGoverning:
            options.governing === "missing"
              ? undefined
              : async (_address, document) => {
                  state.governingCalls++;
                  const selected = rows.find((r) => r.actual?.id === document.id)!;
                  expect(f.events).toContain("original:" + selected.source.uri);
                  const original = JSON.parse(
                    new TextDecoder().decode(originals.get(selected.source.uri)!),
                  ) as {
                    decision: Omit<GoverningDocument, "sourceRefs">;
                  };
                  // Actual governing qualification joins the acquired original, not a
                  // Memory kind/authority label or a mirrored summary claim.
                  expect(canonicalMemory(document)).toBe(
                    canonicalMemory({ ...original.decision, sourceRefs: [selected.source] }),
                  );
                  if (options.mutateGoverning) document.allowedReaders = ["unrelated-reader"];
                  return options.governing !== "false";
                },
        }),
      );
    },
  };
  return { ...f, owner, nativeState: state };
}

const standingCases = (
  [
    ["preference", "standing-preference"],
    ["procedure", "standing-procedure"],
  ] as const
).flatMap(([kind, useKind]) =>
  (["historical", "foreign", "inapplicable"] as const).flatMap((variant) =>
    (["missing", "false"] as const).map((governing) => ({ kind, useKind, variant, governing })),
  ),
);
test.each(standingCases)(
  "standing $kind cannot promote attached $variant Decision with $governing governing callback",
  async (spec) => {
    const overrides: Partial<GoverningDocument> =
      spec.variant === "historical"
        ? { status: "superseded" }
        : spec.variant === "foreign"
          ? { owner: ownerB }
          : {};
    const selected = row("standing-attachment", {
      kind: spec.kind,
      useKind: spec.useKind,
      actual: { value: "unadmitted", overrides },
    });
    const f = fixture([selected], { governing: spec.governing });
    let consumed = false;
    await expect(
      withMemoryQuery(request(), f.owner, () => {
        consumed = true;
      }),
    ).rejects.toThrow("NATIVE_USE_SHAPE_MISMATCH");
    expect(consumed).toBe(false);
    expect(f.nativeState.governingCalls).toBe(0);
    expect(f.nativeState.adoptionCalls).toBe(1);
    expect(f.reads.size).toBe(1);
    expect(f.events).toContain("original:" + selected.source.uri);
  },
);

test.each([
  ["preference", "standing-preference"],
  ["procedure", "standing-procedure"],
  ["knowledge", "standing-procedure"],
] as const)("legitimate %s/%s standing adoption needs no Decision", async (kind, useKind) => {
  const selected = row("legitimate-standing", { kind, useKind });
  const f = fixture([selected], { governing: "missing" });
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.hits).toHaveLength(1);
  expect(out.hits[0]!.currentInstructionEligible).toBe(true);
  expect(out.hits[0]!.adoptionSource).toEqual(selected.source);
  expect(out.governing).toEqual([]);
  expect(out.conflicts).toEqual([]);
  expect(out.nativePermissionGranted).toBe(false);
  expect(f.nativeState.governingCalls).toBe(0);
});

const standingKinds = [
  ["preference", "standing-preference"],
  ["procedure", "standing-procedure"],
] as const;

test.each(standingKinds)(
  "incompatible current %s originals hold both honest %s uses without Decision promotion",
  async (kind, useKind) => {
    const rows = ["alpha", "beta"].map((value) =>
      row("standing-" + value, { kind, useKind, summaryClaim: { key: localKey, value } }),
    );
    const f = fixture(rows, { governing: "missing" });
    const out = await withMemoryQuery(request(), f.owner, (r) => r);
    expect(out.hits).toHaveLength(2);
    expect(out.hits.every((h) => !h.currentInstructionEligible)).toBe(true);
    expect(out.hits.map((h) => h.document)).toEqual(rows.map((r) => r.document));
    expect(out.hits.map((h) => h.adoptionSource)).toEqual(rows.map((r) => r.source));
    expect(out.governing).toEqual([]);
    expect(out.conflicts).toHaveLength(1);
    expect(out.conflicts[0]).toMatchObject({
      claimIdentity: { owner: ownerA, key: localKey },
      conflictingEvidence: true,
      governingConflict: false,
      standingConflict: true,
      resolution: "conflict",
      selectedGoverningValue: null,
      incompleteSources: [],
      standingSources: rows.map((r, i) => ({
        source: f.addresses[i]!.source,
        adoptionSource: r.source,
        kind: useKind,
        value: r.document.claim!.value,
      })),
    });
    expect(out.acquisitionGaps).toEqual([]);
    expect(out.sourceCoverage).toBe("complete");
    expect(Object.values(out.omissions).every((n) => n === 0)).toBe(true);
    expect(f.nativeState.adoptionCalls).toBe(2);
    expect(f.nativeState.governingCalls).toBe(0);
    for (const r of rows) expect(f.events).toContain("original:" + r.source.uri);
  },
);

test.each(standingKinds)(
  "single and compatible %s/%s claims stay useful without fabricated authority",
  async (kind, useKind) => {
    for (const count of [1, 2]) {
      const rows = Array.from({ length: count }, (_, i) =>
        row("compatible-" + i, {
          kind,
          useKind,
          summaryClaim: { key: localKey, value: "alpha" },
        }),
      );
      const f = fixture(rows, { governing: "missing" });
      const out = await withMemoryQuery(request(), f.owner, (r) => r);
      expect(out.hits).toHaveLength(count);
      expect(out.hits.every((h) => h.currentInstructionEligible)).toBe(true);
      expect(out.governing).toEqual([]);
      expect(out.conflicts[0]).toMatchObject({
        conflictingEvidence: false,
        governingConflict: false,
        resolution: "unqualified",
        selectedGoverningValue: null,
      });
      expect(out.conflicts[0]!.standingConflict).toBeUndefined();
      expect(f.nativeState.governingCalls).toBe(0);
    }
  },
);

test.each(["owner", "local-key"] as const)(
  "standing contradictions require the same exact %s as well as an incompatible value",
  async (separation) => {
    const rows = [
      row("separate-one", {
        kind: "preference",
        useKind: "standing-preference",
        summaryClaim: { key: localKey, value: "alpha" },
      }),
      row("separate-two", {
        kind: "procedure",
        useKind: "standing-procedure",
        owner: separation === "owner" ? ownerB : ownerA,
        summaryClaim: { key: separation === "local-key" ? "other-key" : localKey, value: "beta" },
      }),
    ];
    const f = fixture(rows);
    const out = await withMemoryQuery(request(), f.owner, (r) => r);
    expect(out.hits).toHaveLength(2);
    expect(out.hits.every((h) => h.currentInstructionEligible)).toBe(true);
    expect(out.conflicts).toHaveLength(2);
    expect(out.conflicts.every((c) => !c.standingConflict && !c.conflictingEvidence)).toBe(true);
    expect(out.governing).toEqual([]);
  },
);

test("a known standing contradiction holds only its owner/key and a foreign winner cannot resolve it", async () => {
  const rows = ["alpha", "beta"].map((value) =>
    row("scoped-" + value, {
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value },
    }),
  );
  rows.push(
    row("foreign-resolution", { owner: ownerB, actual: { value: "alpha" } }),
    row("independent-standing", {
      owner: ownerB,
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value: "alpha" },
    }),
    row("independent-key", {
      kind: "procedure",
      useKind: "standing-procedure",
      summaryClaim: { key: "other-key", value: "beta" },
    }),
  );
  const f = fixture(rows);
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.hits).toHaveLength(5);
  expect(
    out.hits
      .filter((h) => h.currentInstructionEligible)
      .map((h) => h.document.record.id)
      .sort(),
  ).toEqual(["foreign-resolution", "independent-key", "independent-standing"]);
  const unresolved = out.conflicts.find(
    (c) =>
      c.claimIdentity.owner.kind === "work" &&
      c.claimIdentity.owner.projectId === "synthetic-use" &&
      c.claimKey === localKey,
  );
  expect(unresolved).toMatchObject({
    standingConflict: true,
    governingConflict: false,
    selectedGoverningValue: null,
    resolution: "conflict",
  });
  expect(out.governing).toHaveLength(1);
  expect(out.governing[0]!.document.owner).toEqual(ownerB);
});

test.each(["superseded", "old-revision", "expired", "future", "unadopted", "unqualified"] as const)(
  "%s summary disagreement stays evidence without holding an honest current standing use",
  async (noise) => {
    const rows = ["alpha", "beta"].map((value) =>
      row("noise-" + value, {
        kind: "preference",
        useKind: "standing-preference",
        summaryClaim: { key: localKey, value },
      }),
    );
    const record = rows[1]!.document.record;
    if (noise === "superseded") record.status = "superseded";
    if (noise === "expired") record.validUntil = "2000-01-01T00:00:00Z";
    if (noise === "future") record.validFrom = "2999-01-01T00:00:00Z";
    if (noise === "unadopted") record.authority = "source-observation";
    if (noise === "unqualified") rows[1]!.proposed = null;
    const f = fixture(rows, { governing: "missing" });
    if (noise === "old-revision") f.addresses[1]!.currentness = "historical";
    const out = await withMemoryQuery(request({ mode: "history" }), f.owner, (r) => r);
    expect(out.hits).toHaveLength(2);
    expect(
      out.hits.find((h) => h.document.record.id === "noise-alpha")!.currentInstructionEligible,
    ).toBe(true);
    expect(
      out.hits.find((h) => h.document.record.id === "noise-beta")!.currentInstructionEligible,
    ).toBe(false);
    expect(out.conflicts[0]).toMatchObject({
      conflictingEvidence: true,
      governingConflict: false,
      resolution: "unqualified",
      selectedGoverningValue: null,
    });
    expect(out.conflicts[0]!.standingConflict).toBeUndefined();
    expect(out.governing).toEqual([]);
    expect(f.nativeState.adoptionCalls).toBe(noise === "unqualified" ? 2 : 1);
    expect(f.nativeState.governingCalls).toBe(0);
  },
);

test.each(standingKinds)(
  "actual current governing original resolves %s/%s conflict only for its compatible use",
  async (kind, useKind) => {
    const rows = ["alpha", "beta"].map((value) =>
      row("resolved-" + value, { kind, useKind, summaryClaim: { key: localKey, value } }),
    );
    rows.push(row("actual-resolution", { actual: { value: "alpha" } }));
    const f = fixture(rows);
    const out = await withMemoryQuery(request(), f.owner, (r) => r);
    expect(out.hits).toHaveLength(3);
    expect(
      out.hits
        .filter((h) => h.currentInstructionEligible)
        .map((h) => h.document.record.id)
        .sort(),
    ).toEqual(["actual-resolution", "resolved-alpha"]);
    expect(out.governing).toHaveLength(1);
    expect(out.governing[0]!.document).toEqual(rows[2]!.actual!);
    expect(out.conflicts[0]).toMatchObject({
      conflictingEvidence: true,
      governingConflict: false,
      standingConflict: true,
      resolution: "resolved",
      selectedGoverningValue: "alpha",
    });
    expect(out.conflicts[0]!.standingSources).toHaveLength(2);
    expect(f.nativeState.governingCalls).toBe(1);
  },
);

test("an unqualified actual Decision cannot choose a winner between current standing uses", async () => {
  const rows = ["alpha", "beta"].map((value) =>
    row("unresolved-" + value, {
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value },
    }),
  );
  rows.push(row("unqualified-resolution", { actual: { value: "alpha" } }));
  const f = fixture(rows, { governing: "false" });
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.hits).toHaveLength(3);
  expect(out.hits.every((h) => !h.currentInstructionEligible)).toBe(true);
  expect(out.governing).toEqual([]);
  expect(out.conflicts[0]).toMatchObject({
    standingConflict: true,
    resolution: "conflict",
    selectedGoverningValue: null,
  });
});

test("known standing conflict retains exact originals and values despite a lexical miss", async () => {
  const rows = ["alpha", "beta"].map((value) =>
    row("no-anchor-" + value, {
      kind: "procedure",
      useKind: "standing-procedure",
      summaryClaim: { key: localKey, value },
    }),
  );
  const f = fixture(rows);
  const out = await withMemoryQuery(
    request({ query: "How should a reply be phrased for readers?" }),
    f.owner,
    (r) => r,
  );
  expect(out.counts.matched).toBe(0);
  expect(out.hits).toEqual([]);
  expect(out.governing).toEqual([]);
  expect(out.conflicts).toHaveLength(1);
  expect(out.conflicts[0]).toMatchObject({ standingConflict: true, resolution: "conflict" });
  expect(out.conflicts[0]!.standingSources!.map((s) => s.adoptionSource)).toEqual(
    rows.map((r) => r.source),
  );
  expect(out.conflicts[0]!.standingSources!.map((s) => s.value)).toEqual(["alpha", "beta"]);
});

test("denied original never enters the current standing conflict or triggers its body read", async () => {
  const rows = ["alpha", "beta"].map((value) =>
    row("denied-" + value, {
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value },
    }),
  );
  const f = fixture(rows);
  f.denied.add(rows[1]!.source.uri);
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.hits).toHaveLength(1);
  expect(out.hits[0]!.currentInstructionEligible).toBe(true);
  expect(out.conflicts[0]!.standingConflict).toBeUndefined();
  expect(f.events).not.toContain("original:" + rows[1]!.source.uri);
  expect(f.reads.has(f.addresses[1]!.source.uri)).toBe(false);
});

test("standing-conflict output remains inside the current native lease", async () => {
  const rows = ["alpha", "beta"].map((value) =>
    row("revoke-" + value, {
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value },
    }),
  );
  const f = fixture(rows);
  await expect(
    withMemoryQuery(request(), f.owner, (out) => {
      expect(out.conflicts[0]!.standingConflict).toBe(true);
      f.state.generation++;
    }),
  ).rejects.toThrow("SYNTHETIC_POLICY_REVOKED");
});

test("omitting a standing conflict for the output budget cannot restore eligibility", async () => {
  const rows = Array.from({ length: 6 }, (_, i) =>
    row("bounded-" + i, {
      kind: "preference",
      useKind: "standing-preference",
      summaryClaim: { key: localKey, value: "different-" + i + "x".repeat(1000) },
    }),
  );
  const f = fixture(rows);
  const out = await withMemoryQuery(request({ maxChars: 4096 }), f.owner, (r) => r);
  expect(out.omissions.conflicts).toBe(1);
  expect(out.hits.length).toBeGreaterThan(0);
  expect(out.hits.every((h) => !h.currentInstructionEligible)).toBe(true);
  expect(JSON.stringify(out).length).toBeLessThanOrEqual(4096);
});

test("fully qualified claim-less actual Decision uses original fields and remains eligible", async () => {
  const selected = row("genuine", { actual: { value: "original-value" } }),
    f = fixture([selected]);
  expect(Object.hasOwn(selected.document, "claim")).toBe(false);
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.governing).toHaveLength(1);
  expect(out.governing[0]!.document).toEqual(selected.actual!);
  expect(out.governing[0]!.source).toEqual(selected.source);
  expect(out.governing[0]!.governs).toBe(true);
  expect(out.conflicts[0]).toMatchObject({
    conflictingEvidence: false,
    governingConflict: false,
    resolution: "resolved",
    selectedGoverningValue: "original-value",
  });
  expect(out.hits[0]!.currentInstructionEligible).toBe(true);
  expect(Object.hasOwn(out.hits[0]!.document, "claim")).toBe(false);
  expect(f.nativeState.governingCalls).toBe(1);
});

test("missing or false governing qualification never becomes actual Decision authority", async () => {
  for (const governing of ["missing", "false"] as const) {
    const selected = row("unqualified", { actual: { value: "value" } }),
      f = fixture([selected], { governing });
    if (governing === "missing") {
      await expect(withMemoryQuery(request(), f.owner, (r) => r)).rejects.toThrow(
        "ACTUAL_GOVERNING_DECISION_REQUIRED",
      );
    } else {
      const out = await withMemoryQuery(request(), f.owner, (r) => r);
      expect(out.governing).toEqual([]);
      expect(out.conflicts).toEqual([]);
      expect(out.hits[0]!.currentInstructionEligible).toBe(false);
    }
  }
});

test("required Decision and extra standing properties are also checked for untyped native adapters", async () => {
  const cases: unknown[] = [
    { kind: "governing-decision" },
    { kind: "standing-preference", decision: undefined },
    { kind: "standing-preference", decision: null },
    { kind: "standing-preference", unchecked: true },
  ];
  for (const extra of cases) {
    const selected = row("invalid-shape", { kind: "preference", useKind: "standing-preference" });
    selected.proposed = { ...(extra as object), source: selected.source };
    const f = fixture([selected]);
    await expect(withMemoryQuery(request(), f.owner, (r) => r)).rejects.toThrow();
  }
});

test("two claim-less summaries report the same actual conflicts as direct governing documents", async () => {
  const rows = [
    row("one", { actual: { value: "alpha" } }),
    row("two", { actual: { value: "beta" } }),
  ];
  const adopted = fixture(rows),
    direct = fixture(rows, { representation: "direct" });
  const out = await withMemoryQuery(request(), adopted.owner, (r) => r);
  const equivalent = await withMemoryQuery(request(), direct.owner, (r) => r);
  expect(rows.every((r) => !Object.hasOwn(r.document, "claim"))).toBe(true);
  const expectedOriginals = rows.map((r) => {
    if (!r.actual) throw Error("Synthetic actual Decision required");
    return r.actual;
  });
  expect(out.governing.map((g) => g.document)).toEqual(expectedOriginals);
  expect(out.governing.map((g) => g.document)).toEqual(equivalent.governing.map((g) => g.document));
  expect(out.conflicts).toEqual(equivalent.conflicts);
  expect(out.conflicts[0]).toMatchObject({
    claimIdentity: { owner: ownerA, key: localKey },
    conflictingEvidence: true,
    governingConflict: true,
    resolution: "conflict",
    selectedGoverningValue: null,
  });
  expect(out.hits).toHaveLength(2);
  expect(
    out.hits.every((h) => !h.currentInstructionEligible && !Object.hasOwn(h.document, "claim")),
  ).toBe(true);
});

test("summary-versus-actual contradiction remains evidence while only the actual Decision governs", async () => {
  const selected = row("mismatch", {
    summaryClaim: { key: localKey, value: "summary-value" },
    actual: { value: "original-value" },
  });
  const f = fixture([selected]),
    out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.conflicts[0]).toMatchObject({
    conflictingEvidence: true,
    governingConflict: false,
    resolution: "resolved",
    selectedGoverningValue: "original-value",
  });
  expect(out.hits[0]!.document.claim!.value).toBe("summary-value");
  expect(out.hits[0]!.currentInstructionEligible).toBe(false);
  expect(out.governing[0]!.document.claim.value).toBe("original-value");
});

test("historical-only disagreement remains evidence without a current governing conflict", async () => {
  const rows = [
    row("current", { actual: { value: "alpha" } }),
    row("historical", { actual: { value: "beta", overrides: { status: "superseded" } } }),
  ];
  const f = fixture(rows, { representation: "direct" });
  f.addresses[1]!.currentness = "historical";
  const out = await withMemoryQuery(request({ mode: "history" }), f.owner, (r) => r);
  expect(out.conflicts[0]).toMatchObject({
    conflictingEvidence: true,
    governingConflict: false,
    selectedGoverningValue: "alpha",
  });
  expect(out.governing.map((g) => g.governs)).toEqual([true, false]);
  expect(out.governing[1]!.document.status).toBe("superseded");
  const current = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(current.conflicts[0]!.conflictingEvidence).toBe(false);
  expect(current.governing).toHaveLength(1);
});

test("claim-less actual Decisions still preserve distinct owner scopes", async () => {
  const rows = [
    row("scope-one", { actual: { value: "alpha" } }),
    row("scope-two", { owner: ownerB, actual: { value: "beta" } }),
  ];
  const f = fixture(rows),
    out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.conflicts).toHaveLength(2);
  expect(
    out.conflicts.every(
      (c) => !c.conflictingEvidence && !c.governingConflict && c.resolution === "resolved",
    ),
  ).toBe(true);
  expect(out.hits.every((h) => h.currentInstructionEligible)).toBe(true);
});

test("native callback aliases cannot change an already validated actual Decision", async () => {
  const rows = [
    row("one", { actual: { value: "alpha" } }),
    row("two", { actual: { value: "alpha" } }),
  ];
  const original = structuredClone(rows[0]!.actual!);
  const f = fixture(rows, {
    beforeAdoption(index) {
      if (index === 1)
        (
          rows[0]!.proposed as Extract<NativeMemoryUse, { kind: "governing-decision" }>
        ).decision.owner = ownerB;
    },
  });
  const out = await withMemoryQuery(request(), f.owner, (r) => r);
  expect(out.governing[0]!.document).toEqual(original);
  expect(out.conflicts).toHaveLength(1);
  expect(out.conflicts[0]!.selectedGoverningValue).toBe("alpha");
});

test("native qualification cannot mutate the document whose exact source it qualifies", async () => {
  const selected = row("mutation", { actual: { value: "alpha" } });
  const f = fixture([selected], { mutateGoverning: true });
  let consumed = false;
  await expect(
    withMemoryQuery(request(), f.owner, () => {
      consumed = true;
    }),
  ).rejects.toThrow("GOVERNING_QUALIFICATION_MUTATED_DOCUMENT");
  expect(consumed).toBe(false);
});
