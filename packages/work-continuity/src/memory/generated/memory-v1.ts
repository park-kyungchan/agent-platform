/** Generated from memory-v1.schema.json; do not edit. SHA-256: 00b51da4a7b20abdf3ca73a927b79c53bf669b023fc9717c11d01cadef965281; S01: 1f1542021ee00b044bc5d0d7577430aeba58a0d2431aa92fcb7f05c1df7e27e0 */

/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Id".
 */
export type Id = string;
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Revision".
 */
export type Revision = number;
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Scope".
 */
export type Scope =
  | {
      kind: "user";
      userId: Id;
    }
  | {
      kind: "domain";
      domainId: Id;
    }
  | {
      kind: "project";
      projectId: Id;
    }
  | {
      kind: "work";
      projectId: Id;
      workId: Id;
    }
  | {
      kind: "task";
      projectId: Id;
      workId: Id;
      taskId: Id;
    };
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Digest".
 */
export type Digest = string;
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Timestamp".
 */
export type Timestamp = string;
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Json".
 */
export type Json =
  | null
  | boolean
  | string
  | number
  | Json[]
  | {
      [k: string]: Json;
    };

export interface MemoryV1 {
  schema: "common.memory-document.v1";
  record: MemoryRecord;
  claim?: Claim;
  legacy?: LegacyBinding;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "MemoryRecord".
 */
export interface MemoryRecord {
  id: Id;
  revision: Revision;
  owner: Scope;
  /**
   * @maxItems 100
   */
  subjects: Scope[];
  kind: "preference" | "decision" | "episode" | "procedure" | "knowledge";
  status: "candidate" | "active" | "superseded" | "revoked";
  authority: "source-observation" | "model-inference" | "owner-adopted";
  /**
   * @minItems 1
   * @maxItems 100
   */
  sourceRefs: [SourceRef, ...SourceRef[]];
  text: string;
  /**
   * @maxItems 100
   */
  allowedReaders: Id[];
  validFrom?: Timestamp;
  validUntil?: Timestamp;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "SourceRef".
 */
export interface SourceRef {
  owner: Id;
  uri: string;
  revision: Id;
  digest: Digest;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "Claim".
 */
export interface Claim {
  key: Id;
  value: string;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "LegacyBinding".
 */
export interface LegacyBinding {
  protocol: Id;
  identityKey: Id;
  generationId?: Id;
  claimKey?: Id;
  original: SourceRef;
  metadata: Json;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "TaskRef".
 */
export interface TaskRef {
  projectId: Id;
  workId: Id;
  taskId: Id;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "WorkRef".
 */
export interface WorkRef {
  projectId: Id;
  workId: Id;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "MemoryDocument".
 */
export interface MemoryDocument {
  schema: "common.memory-document.v1";
  record: MemoryRecord;
  claim?: Claim;
  legacy?: LegacyBinding;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "GoverningDocument".
 */
export interface GoverningDocument {
  schema: "common.memory-governing.v1";
  id: Id;
  revision: Revision;
  owner: Scope;
  recordClass: "decision" | "spec";
  status: "active" | "superseded" | "revoked";
  /**
   * @minItems 1
   * @maxItems 100
   */
  sourceRefs: [SourceRef, ...SourceRef[]];
  /**
   * @maxItems 100
   */
  allowedReaders: Id[];
  claim: Claim;
  text: string;
  validFrom?: Timestamp;
  validUntil?: Timestamp;
}
/**
 * This interface was referenced by `MemoryV1`'s JSON-Schema
 * via the `definition` "MemoryQuery".
 */
export interface MemoryQuery {
  schema: "common.memory-query.v1";
  purpose: string;
  query: string;
  mode: "current" | "history";
  /**
   * @maxItems 100
   */
  scopes?: Scope[];
  work?: WorkRef;
  task?: TaskRef;
  effectiveDigest?: Digest;
  transport?: "full" | "compact" | "dispatch" | "resume";
  asOf?: Timestamp;
  limit?: number;
  maxChars?: number;
}
