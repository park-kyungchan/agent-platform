import type { Graph, TaskRef, Scope, SourceRef } from "../git-work/generated/v3.ts";
import type {
  MemoryRecord,
  MemoryQuery,
  GoverningDocument,
  MemoryDocument,
} from "./generated/memory-v1.ts";

export type MemoryHeader = Omit<MemoryRecord, "text">;
/** Local/legacy key bytes have meaning only within their exact semantic owner.
 * Equal keys, parent scopes and discovery overlap confer no override relation. */
export interface MemoryClaimIdentity {
  owner: Scope;
  key: string;
}
export type GoverningHeader = Omit<GoverningDocument, "schema" | "text" | "claim"> & {
  claimKey: string;
};
export type MemoryAddress = {
  source: SourceRef;
  currentness: "current" | "historical" | "unknown";
  claimKey?: string;
} & ({ kind: "memory"; header: MemoryHeader } | { kind: "governing"; header: GoverningHeader });
/** Opaque native object: never accepted from a query, JSON grant or source descriptor. */
export type NativeMemoryPermit = object;
export interface MemoryInventory {
  entries: MemoryAddress[];
  gaps: Array<{ code: "missing" | "unreachable" | "unqualified-owner"; scope?: Scope }>;
  truncated: boolean;
}
/** Standing adoption and actual Decision authority are disjoint. The runtime
 * also checks exact shapes; TypeScript alone cannot qualify native input. */
export type NativeMemoryUse =
  | {
      kind: "standing-preference" | "standing-procedure";
      source: SourceRef;
      decision?: never;
    }
  | {
      kind: "governing-decision";
      source: SourceRef;
      /** Required actual record, separately qualified from acquired originals. */
      decision: GoverningDocument;
    };
/** The existing native owner authenticates the current caller and retains every
 * selected owner's read/policy and current-source-head lease until the callback and consumer finish.
 * authorize MUST cover originals and all source/derived exclusions before read.
 * acquire MUST independently enforce its opaque permit and immutable revision.
 * Implementations may aggregate owners; common code creates no store or client.
 */
export interface NativeMemoryReadLease {
  readerId: string;
  ownerPolicyGenerations: Readonly<Record<string, string>>;
  policyGeneration: string;
  nowUtc: string;
  expiresAtUtc: string;
  assertCurrent(): void | Promise<void>;
  discover(input: Pick<MemoryQuery, "scopes" | "mode" | "purpose">): Promise<MemoryInventory>;
  authorize(
    address: MemoryAddress,
    input: Pick<MemoryQuery, "mode" | "purpose">,
  ): Promise<NativeMemoryPermit | null>;
  /** Return the exact permitted immutable original bytes, not a summary/cache claim. */
  readOriginal(source: SourceRef, permit: NativeMemoryPermit): Promise<Uint8Array>;
  acquire(address: MemoryAddress, permit: NativeMemoryPermit): Promise<Uint8Array>;
  /** Authenticate adoption evidence/applicability from already acquired originals. */
  qualifyMemoryUse?(
    address: MemoryAddress,
    document: MemoryDocument,
    permit: NativeMemoryPermit,
  ): Promise<NativeMemoryUse | null>;
  /** Actual governing Decision/Spec verification and applicability, not a Memory label. */
  qualifyGoverning?(
    address: MemoryAddress,
    document: GoverningDocument,
    permit: NativeMemoryPermit,
  ): Promise<boolean>;
  /** Exact current/historical source-qualified graph, never an inferred latest graph. */
  readTaskGraph?(task: TaskRef): Promise<Graph>;
}
export interface NativeMemoryReader {
  withCurrentRead<T>(
    request: Readonly<MemoryQuery>,
    consume: (lease: NativeMemoryReadLease) => Promise<T>,
  ): Promise<T>;
}
export class MemoryAcquisitionGap extends Error {
  constructor(readonly code: "missing" | "unreachable" | "denied") {
    super("MEMORY_SOURCE_" + code.toUpperCase());
  }
}
