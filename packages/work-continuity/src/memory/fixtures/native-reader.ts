/** Private source-only synthetic owner. Never install as a native authority. */
import type { MemoryDocument, GoverningDocument, MemoryRecord } from "../generated/memory-v1.ts";
import type { MemoryAddress, NativeMemoryReader, NativeMemoryReadLease } from "../reader.ts";
import type { Graph } from "../../git-work/generated/v3.ts";
import { canonicalMemory, memoryDigest } from "../codec.ts";
export const syntheticReaderId = "synthetic-reader";
export function memory(
  id: string,
  text: string,
  overrides: Partial<MemoryRecord> = {},
  claim?: { key: string; value: string },
): MemoryDocument {
  return {
    schema: "common.memory-document.v1",
    record: {
      id,
      revision: 1,
      owner: { kind: "user", userId: "synthetic-user" },
      subjects: [],
      kind: "knowledge",
      status: "active",
      authority: "source-observation",
      sourceRefs: [
        {
          owner: "synthetic-owner",
          uri: "fixture://original/" + id,
          revision: "original1",
          digest: memoryDigest("original:" + id),
        },
      ],
      text,
      allowedReaders: [syntheticReaderId],
      ...overrides,
    },
    ...(claim ? { claim } : {}),
  };
}
export function governing(
  id: string,
  value: string,
  overrides: Partial<GoverningDocument> = {},
): GoverningDocument {
  return {
    schema: "common.memory-governing.v1",
    id,
    revision: 1,
    owner: { kind: "user", userId: "synthetic-user" },
    recordClass: "decision",
    status: "active",
    sourceRefs: [
      {
        owner: "synthetic-owner",
        uri: "fixture://decision/" + id,
        revision: "1",
        digest: memoryDigest("original:" + id),
      },
    ],
    allowedReaders: [syntheticReaderId],
    claim: { key: "language", value },
    text: "언어 선택 " + value,
    ...overrides,
  };
}
export function nativeFixture(documents: Array<MemoryDocument | GoverningDocument>, graph?: Graph) {
  const events: string[] = [],
    reads = new Map<string, number>(),
    denied = new Set<string>();
  const state = {
    generation: 1,
    active: true,
    historyAllowed: true,
    governingQualified: true,
    gaps: [] as Array<{ code: "missing" | "unreachable" | "unqualified-owner" }>,
    truncated: false,
    beforeAcquire: undefined as undefined | (() => void),
    afterAcquire: undefined as undefined | (() => void),
    contextReads: 0,
  };
  const bodies = new Map<string, Uint8Array>();
  const addresses: MemoryAddress[] = documents.map((doc, index) => {
    const bytes = new TextEncoder().encode(canonicalMemory(doc));
    const source = {
      owner: "synthetic-owner",
      uri: "fixture://memory/" + index,
      revision: "1",
      digest: memoryDigest(bytes),
    };
    bodies.set(source.uri, bytes);
    if (doc.schema === "common.memory-document.v1") {
      const { text: _, ...header } = doc.record;
      return {
        kind: "memory",
        currentness: "current",
        source,
        header,
        ...(doc.claim ? { claimKey: doc.claim.key } : {}),
      };
    }
    const { schema: _, text: __, claim, ...core } = doc;
    return {
      kind: "governing",
      currentness: "current",
      source,
      header: { ...core, claimKey: claim.key },
    };
  });
  const permits = new WeakMap<object, { uri: string; generation: number }>();
  const owner: NativeMemoryReader = {
    async withCurrentRead(request, consume) {
      events.push("lease");
      const generation = state.generation;
      const now = new Date();
      const assertCurrent = () => {
        if (!state.active || state.generation !== generation)
          throw Error("SYNTHETIC_POLICY_REVOKED");
      };
      assertCurrent();
      const lease: NativeMemoryReadLease = {
        readerId: syntheticReaderId,
        policyGeneration: String(generation),
        ownerPolicyGenerations: { "synthetic-owner": String(generation) },
        nowUtc: now.toISOString(),
        expiresAtUtc: new Date(now.getTime() + 60_000).toISOString(),
        assertCurrent,
        async discover() {
          assertCurrent();
          events.push("discover");
          return {
            entries: structuredClone(addresses),
            gaps: state.gaps,
            truncated: state.truncated,
          };
        },
        async authorize(address, input) {
          assertCurrent();
          events.push("authorize:" + address.source.uri);
          if (
            (input.mode === "history" && !state.historyAllowed) ||
            denied.has(address.source.uri) ||
            address.header.sourceRefs.some((ref) => denied.has(ref.uri))
          )
            return null;
          const permit = {};
          permits.set(permit, { uri: address.source.uri, generation });
          return permit;
        },
        async readOriginal(source, permit) {
          assertCurrent();
          const allowed = permits.get(permit);
          if (!allowed || allowed.generation !== state.generation)
            throw Error("SYNTHETIC_BAD_PERMIT");
          events.push("original:" + source.uri);
          const id = source.uri.split("/").at(-1)!;
          return new TextEncoder().encode("original:" + id);
        },
        async acquire(address, permit) {
          state.beforeAcquire?.();
          assertCurrent();
          const allowed = permits.get(permit);
          if (
            !allowed ||
            allowed.uri !== address.source.uri ||
            allowed.generation !== state.generation
          )
            throw Error("SYNTHETIC_BAD_PERMIT");
          events.push("acquire:" + address.source.uri);
          reads.set(address.source.uri, (reads.get(address.source.uri) ?? 0) + 1);
          const bytes = bodies.get(address.source.uri)!;
          state.afterAcquire?.();
          return bytes.slice();
        },
        async qualifyGoverning() {
          assertCurrent();
          return state.governingQualified;
        },
        async readTaskGraph() {
          assertCurrent();
          state.contextReads++;
          if (!graph) throw Error("SYNTHETIC_NO_TASK_GRAPH");
          return graph;
        },
      };
      const result = await consume(lease);
      assertCurrent();
      return result;
    },
  };
  return { owner, state, events, reads, denied, addresses, bodies };
}
