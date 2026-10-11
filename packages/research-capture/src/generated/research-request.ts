/* Generated from the canonical research-request schema. Do not edit. */

export interface ResearchRequest {
  projectId: string;
  url: string;
  /**
   * Authorized request headers; credential values are redacted from persisted metadata and request records.
   */
  headers?: {
    [k: string]: string;
  };
  mode?: "auto" | "static" | "dynamic";
  selector?: string;
  waitSelector?: string;
  expectedText?: string;
  timeoutMs?: number;
  maxAttempts?: number;
}
