import { createHash } from "node:crypto";
import { canonicalV3 } from "../git-work/v3-codec.ts";
import * as validators from "./generated/memory-v1-validators.js";
import type { MemoryRecord, MemoryDocument, MemoryQuery, GoverningDocument, LegacyBinding } from "./generated/memory-v1.ts";

export type MemoryTypes = { MemoryRecord: MemoryRecord; MemoryDocument: MemoryDocument; MemoryQuery: MemoryQuery; GoverningDocument: GoverningDocument; LegacyBinding: LegacyBinding };
export const canonicalMemory = canonicalV3;
export const memoryDigest = (bytes: Uint8Array | string): string => createHash("sha256").update(bytes).digest("hex");
export function assertMemory<K extends keyof MemoryTypes>(kind: K, value: unknown): asserts value is MemoryTypes[K] {
  canonicalMemory(value);
  if (!validators[`validate${kind}`](value)) throw Error("INVALID_MEMORY_" + kind.toUpperCase());
}
export function encodeMemory<K extends keyof MemoryTypes>(kind: K, value: MemoryTypes[K]): string {
  assertMemory(kind, value);
  return canonicalMemory(value);
}
export function decodeMemory<K extends keyof MemoryTypes>(kind: K, bytes: Uint8Array | string): MemoryTypes[K] {
  const text = typeof bytes === "string" ? bytes : new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  if (Buffer.byteLength(text) > 1_048_576) throw Error("MEMORY_DOCUMENT_BOUND");
  const value: unknown = JSON.parse(text);
  assertMemory(kind, value);
  if (canonicalMemory(value) !== text) throw Error("MEMORY_CANONICAL_TRANSPORT_REQUIRED");
  return value;
}
