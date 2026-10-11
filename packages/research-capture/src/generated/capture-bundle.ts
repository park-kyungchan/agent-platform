/* Generated from the canonical capture-bundle schema. Do not edit. */

export type Hash = string;
export type Headers = {
  [k: string]: string;
} | null;

export interface CaptureBundle {
  schemaVersion: 1;
  captureId: string;
  projectId: string;
  requestedUrl: string;
  startedAt: string;
  retrievedAt: string;
  adapterRevision: Hash;
  upstreamRevision: string;
  runtimeCandidate: string;
  outcome: "success" | "partial" | "failure";
  /**
   * @minItems 1
   */
  attempts: [Attempt, ...Attempt[]];
  artifacts: Artifact[];
  limitations: string[];
}
export interface Attempt {
  attempt: number;
  mode: "static" | "dynamic";
  outcome: "success" | "failure";
  exitCode: number | null;
  matched: boolean | null;
  extractionPassed: boolean;
  error?: string;
  outputPath: string;
  stdoutPath: string;
  stderrPath: string;
  startedAt: string;
  finishedAt: string;
  status: number | null;
  finalUrl: string | null;
  requestHeaders: Headers;
  responseHeaders: Headers;
  redirects: Redirect[] | null;
  unavailable: {
    field: string;
    reason: string;
  }[];
  diagnostics: Diagnostic[];
}
export interface Redirect {
  url: string;
  status: number | null;
  requestHeaders: Headers;
  responseHeaders: Headers;
}
export interface Diagnostic {
  code: string;
  message: string;
  observedAt: string;
}
export interface Artifact {
  kind: "response-content" | "rendered-dom" | "extracted-text";
  path: string;
  sha256: Hash;
  byteLength: number;
  attempt: number;
  mode: "static" | "dynamic";
  mediaType: string;
  provenance: "static-response" | "same-browser-navigation" | "rendered-page" | "selector-extraction";
  url: string;
  observedAt: string;
  complete: boolean;
}
