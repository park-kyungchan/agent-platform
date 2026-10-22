import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { inflateRawSync } from "node:zlib";
import { canonical, hash, inputFixture } from "./support";
interface Source {
  owner: string;
  uri: string;
  revision: string;
  digest: string;
}
interface Address {
  kind: string;
  source: Source;
  currentness: string;
  header: { sourceRefs: Source[] };
  claimKey: string;
}
interface Row {
  address: Address;
  raw: string;
  use: Record<string, unknown> | null;
}
interface Scenario {
  rows: string[];
  omitAdoption?: boolean;
  omitGoverning?: boolean;
  governing?: boolean;
  mode?: string;
  asOf?: string;
  deny?: boolean;
  work?: { projectId: string; workId: string };
  transport?: string;
  staleDigest?: boolean;
  omitDigest?: boolean;
  graphDrift?: boolean;
}
interface Lease {
  readerId: string;
  policyGeneration: string;
  ownerPolicyGenerations: Record<string, string>;
  nowUtc: string;
  expiresAtUtc: string;
  assertCurrent(): void;
  discover(): Promise<{ entries: Address[]; gaps: never[]; truncated: false }>;
  authorize(address: Address): Promise<object | null>;
  readOriginal(source: Source, permit: object): Promise<Uint8Array>;
  acquire(address: Address, permit: object): Promise<Uint8Array>;
  qualifyMemoryUse?: (address: Address, document: unknown, permit: object) => Promise<unknown>;
  qualifyGoverning?: (address: Address, document: unknown, permit: object) => Promise<boolean>;
  readTaskGraph?: (task: unknown) => Promise<unknown>;
}
/** Fixed public producer/caller adaptation. This only supplies fictional native IO and returns
 * API outcomes; all required authority/history/context predicates live on the P report host. */
export async function memoryContractScenario(root: string, id: string): Promise<unknown> {
  const fixture = JSON.parse(
    await readFile(join(import.meta.dir, "../fixtures/memory-contract-inputs.json"), "utf8"),
  ) as {
    originals: Record<string, string>;
    rows: Record<string, Row>;
    scenarios: Record<string, Scenario>;
    effectiveDigest: string;
  };
  const spec = fixture.scenarios[id];
  if (!spec) throw Error("TRUSTED_CI_FIXED_MEMORY_CONTRACT_REQUIRED");
  const api = (await import(join(root, "packages/work-continuity/src/memory/v1.ts"))) as {
    withMemoryQuery: (
      query: unknown,
      owner: { withCurrentRead<T>(q: unknown, use: (lease: Lease) => Promise<T>): Promise<T> },
      consume: (value: Record<string, unknown>) => unknown,
    ) => Promise<Record<string, unknown>>;
  };
  const selected = spec.rows.map((name) => fixture.rows[name]!);
  const permits = new WeakMap<object, { row: Row; originals: Set<string>; acquired: boolean }>();
  const calls = {
    originals: [] as Source[],
    bodies: [] as Source[],
    adoption: [] as Source[],
    governing: [] as Source[],
    tasks: [] as unknown[],
    assertions: 0,
  };
  const find = (address: Address) => {
    const row = selected.find((r) => canonical(r.address) === canonical(address));
    if (!row) throw Error("FICTIONAL_EXACT_ADDRESS_REQUIRED");
    return row;
  };
  const permitFor = (permit: object) => {
    const value = permits.get(permit);
    if (!value) throw Error("FICTIONAL_OPAQUE_PERMIT_REQUIRED");
    return value;
  };
  const qualifiedBody = (address: Address, document: unknown, permit: object) => {
    const p = permitFor(permit),
      row = find(address);
    if (
      row !== p.row ||
      !p.acquired ||
      row.address.header.sourceRefs.some((r) => !p.originals.has(canonical(r)))
    )
      throw Error("FICTIONAL_ACQUIRED_ORIGINALS_REQUIRED");
    if (canonical(document) !== canonical(JSON.parse(row.raw)))
      throw Error("FICTIONAL_EXACT_DOCUMENT_REQUIRED");
    return row;
  };
  const lease: Lease = {
    readerId: "fixture-reader",
    policyGeneration: "g1",
    ownerPolicyGenerations: { "fixture-owner": "p1" },
    nowUtc: "2026-10-20T12:00:00Z",
    expiresAtUtc: "2026-10-21T12:00:00Z",
    assertCurrent() {
      calls.assertions++;
    },
    async discover() {
      return {
        entries: structuredClone(selected.map((r) => r.address)),
        gaps: [],
        truncated: false,
      };
    },
    async authorize(address) {
      const row = find(address);
      if (spec.deny) return null;
      const permit = {};
      permits.set(permit, { row, originals: new Set(), acquired: false });
      return permit;
    },
    async readOriginal(source, permit) {
      const p = permitFor(permit),
        key = canonical(source);
      if (!p.row.address.header.sourceRefs.some((r) => canonical(r) === key))
        throw Error("FICTIONAL_ORIGINAL_SCOPE_REQUIRED");
      const raw = fixture.originals[key];
      if (raw === undefined || hash(raw) !== source.digest)
        throw Error("FICTIONAL_ORIGINAL_PIN_REQUIRED");
      calls.originals.push(structuredClone(source));
      p.originals.add(key);
      return Buffer.from(raw);
    },
    async acquire(address, permit) {
      const p = permitFor(permit),
        row = find(address);
      if (
        row !== p.row ||
        row.address.header.sourceRefs.some((r) => !p.originals.has(canonical(r)))
      )
        throw Error("FICTIONAL_READ_BEFORE_BODY_REQUIRED");
      calls.bodies.push(structuredClone(address.source));
      p.acquired = true;
      return Buffer.from(row.raw);
    },
  };
  if (!spec.omitAdoption)
    lease.qualifyMemoryUse = async (address, document, permit) => {
      const row = qualifiedBody(address, document, permit);
      calls.adoption.push(structuredClone(address.source));
      return structuredClone(row.use);
    };
  if (!spec.omitGoverning)
    lease.qualifyGoverning = async (address, document, permit) => {
      const p = permitFor(permit),
        row = find(address);
      // A derived governing Decision is parsed from its actual independently read original.
      if (row.address.kind === "governing") qualifiedBody(address, document, permit);
      else {
        qualifiedBody(address, JSON.parse(row.raw), permit);
        const use = row.use;
        if (!use || use.kind !== "governing-decision" || !p.originals.has(canonical(use.source)))
          throw Error("FICTIONAL_ACTUAL_DECISION_REQUIRED");
        const original = fixture.originals[canonical(use.source)];
        if (original === undefined || canonical(JSON.parse(original)) !== canonical(document))
          throw Error("FICTIONAL_ACTUAL_DECISION_BYTES_REQUIRED");
      }
      calls.governing.push(structuredClone(address.source));
      return spec.governing !== false;
    };
  const query: Record<string, unknown> = {
    schema: "common.memory-query.v1",
    purpose: "Fictional public contract comparison",
    query: "Fixture",
    mode: spec.mode ?? "current",
    limit: 10,
    maxChars: 32000,
    ...(spec.asOf ? { asOf: spec.asOf } : {}),
    ...(spec.work ? { work: spec.work } : {}),
  };
  if (spec.transport) {
    const input = await inputFixture();
    Object.assign(query, {
      task: input.task,
      transport: spec.transport,
      ...(!spec.omitDigest
        ? { effectiveDigest: spec.staleDigest ? "0".repeat(64) : fixture.effectiveDigest }
        : {}),
    });
    lease.readTaskGraph = async (task) => {
      if (canonical(task) !== canonical(input.task)) throw Error("FICTIONAL_EXACT_TASK_REQUIRED");
      calls.tasks.push(structuredClone(task));
      if (spec.graphDrift) lease.policyGeneration = "g2";
      return structuredClone(input.graph);
    };
  }
  const owner = {
    async withCurrentRead<T>(_q: unknown, consume: (current: Lease) => Promise<T>) {
      return consume(lease);
    },
  };
  const result = await api.withMemoryQuery(query, owner, (value) => value);
  let expandedContext = result.taskContext ?? null;
  if (expandedContext && typeof expandedContext === "object" && "payload" in expandedContext) {
    const packet = expandedContext as { payload: string; decodedBytes: number };
    const bytes = inflateRawSync(Buffer.from(packet.payload, "base64"), {
      maxOutputLength: 4_194_304,
    });
    if (bytes.length !== packet.decodedBytes) throw Error("P_TRANSPORT_BYTE_LENGTH_MISMATCH");
    expandedContext = JSON.parse(bytes.toString());
  }
  const hits = result.hits as { document: { record: { id: string } } }[];
  return {
    result,
    calls,
    expandedContext,
    hitIds: hits.map((h) => h.document.record.id).sort(),
    hitsById: Object.fromEntries(hits.map((h) => [h.document.record.id, h])),
  };
}
