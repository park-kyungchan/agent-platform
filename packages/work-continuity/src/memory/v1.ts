/** Explicit generated Memory/query API. Never re-export through historical finite facades. */
export type * from "./generated/memory-v1.ts";
export {
  assertMemory,
  encodeMemory,
  decodeMemory,
  canonicalMemory,
  memoryDigest,
} from "./codec.ts";
export {
  withMemoryQuery,
  createMemoryQueryView,
  memorySourceKey,
  type MemoryQueryResult,
} from "./query.ts";
export {
  MemoryAcquisitionGap,
  type MemoryAddress,
  type MemoryHeader,
  type GoverningHeader,
  type MemoryInventory,
  type NativeMemoryPermit,
  type NativeMemoryUse,
  type NativeMemoryReadLease,
  type NativeMemoryReader,
} from "./reader.ts";
export { projectQualifiedLegacyMemory } from "./legacy.ts";
