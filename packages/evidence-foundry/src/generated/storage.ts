/** Generated from schemas/storage.schema.json. Do not edit. */

export type StorageRecord = CommitBatch | AuditRecord | BackupManifest;

export interface CommitBatch {
  schemaVersion: "1.0.0";
  kind: "commit";
  id: string;
  sequence: number;
  previousHash: string | null;
  createdAt: string;
  /**
   * @minItems 1
   */
  records: [RecordReference, ...RecordReference[]];
}
export interface RecordReference {
  sha256: string;
  kind: "object" | "link" | "audit";
  id: string;
}
export interface AuditRecord {
  schemaVersion: "1.0.0";
  kind: "audit";
  id: string;
  createdAt: string;
  operation: string;
  status: "success" | "failure" | "simulated" | "interrupted";
  idempotencyKey?: string;
  requestHash?: string;
  details: {
    [k: string]: unknown;
  };
}
export interface BackupManifest {
  schemaVersion: "1.0.0";
  kind: "backup";
  createdAt: string;
  head: string | null;
  files: {
    path: string;
    sha256: string;
  }[];
}
