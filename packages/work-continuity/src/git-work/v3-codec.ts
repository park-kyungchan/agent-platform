import { createHash } from "node:crypto";
import * as validators from "./generated/v3-validators.js";
import type {
  Graph,
  Work,
  Task,
  Run,
  Contract,
  CreateRequest,
  EffectiveContract,
  AllocationBinding,
  StoredRecord,
  ContextPacket,
  CompactContextPacket,
} from "./generated/v3.ts";

export type V3Types = {
  Graph: Graph;
  Work: Work;
  Task: Task;
  Run: Run;
  Contract: Contract;
  CreateRequest: CreateRequest;
  EffectiveContract: EffectiveContract;
  AllocationBinding: AllocationBinding;
  StoredRecord: StoredRecord;
  ContextPacket: ContextPacket;
  CompactContextPacket: CompactContextPacket;
};
export type V3Type = keyof V3Types;
const maximumBytes = 4_194_304;

/** Bounded, lossless JSON only. No toJSON, accessors, prototypes, sparse arrays or cycles. */
export function canonicalV3(value: unknown): string {
  let nodes = 0;
  const parents = new Set<object>();
  function visit(input: unknown, depth: number): string {
    if (++nodes > 100_000 || depth > 64) throw Error("V3_JSON_BOUND_EXCEEDED");
    if (input === null || typeof input === "boolean" || typeof input === "string")
      return JSON.stringify(input);
    if (typeof input === "number" && Number.isSafeInteger(input) && !Object.is(input, -0))
      return String(input);
    if (typeof input !== "object" || input === null) throw Error("V3_PLAIN_JSON_REQUIRED");
    if (parents.has(input)) throw Error("V3_JSON_CYCLE");
    const prototype = Object.getPrototypeOf(input);
    if (
      Array.isArray(input)
        ? prototype !== Array.prototype
        : ![Object.prototype, null].includes(prototype)
    )
      throw Error("V3_PLAIN_JSON_REQUIRED");
    if (Reflect.ownKeys(input).some((key) => typeof key !== "string"))
      throw Error("V3_PLAIN_JSON_REQUIRED");
    const descriptors = Object.getOwnPropertyDescriptors(input);
    if (
      Object.entries(descriptors).some(
        ([key, d]) =>
          !Object.hasOwn(d, "value") ||
          (!d.enumerable && !(Array.isArray(input) && key === "length")),
      )
    )
      throw Error("V3_PLAIN_JSON_REQUIRED");
    parents.add(input);
    let output: string;
    if (Array.isArray(input)) {
      if (
        Object.keys(input).length !== input.length ||
        Object.keys(input).some((key, index) => key !== String(index))
      )
        throw Error("V3_DENSE_ARRAY_REQUIRED");
      output = "[" + input.map((row) => visit(row, depth + 1)).join(",") + "]";
    } else {
      output =
        "{" +
        Object.keys(input)
          .sort()
          .map((key) => JSON.stringify(key) + ":" + visit(descriptors[key]!.value, depth + 1))
          .join(",") +
        "}";
    }
    parents.delete(input);
    if (Buffer.byteLength(output) > maximumBytes) throw Error("V3_JSON_BOUND_EXCEEDED");
    return output;
  }
  const encoded = visit(value, 0);
  if (Buffer.byteLength(encoded) > maximumBytes) throw Error("V3_JSON_BOUND_EXCEEDED");
  return encoded;
}

export const digestV3 = (value: unknown): string =>
  createHash("sha256").update(canonicalV3(value)).digest("hex");

export function assertV3<K extends V3Type>(kind: K, value: unknown): asserts value is V3Types[K] {
  canonicalV3(value);
  if (!validators[`validate${kind}`](value)) throw Error(`INVALID_V3_${kind.toUpperCase()}`);
}

export function encodeV3<K extends V3Type>(kind: K, value: V3Types[K]): string {
  assertV3(kind, value);
  return canonicalV3(value);
}

/** Canonical transport rejects duplicate keys, lossy numbers and alternate spellings. */
export function decodeV3<K extends V3Type>(kind: K, bytes: string): V3Types[K] {
  if (Buffer.byteLength(bytes) > maximumBytes) throw Error("V3_JSON_BOUND_EXCEEDED");
  const value: unknown = JSON.parse(bytes);
  assertV3(kind, value);
  if (canonicalV3(value) !== bytes) throw Error("V3_CANONICAL_TRANSPORT_REQUIRED");
  return value;
}
