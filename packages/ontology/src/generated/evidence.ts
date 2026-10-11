/**
 * Generated from schemas/evidence.schema.json; do not edit.
 * json-schema-to-typescript 16.0.0
 * Input SHA-256: c5b3115398c1df5e64f4834569988036003fcdd5c3e74be0a27799056800894c
 */

/**
 * Persisted evidence-domain property contracts. Domain entity properties remain separate from observed truth, evaluator judgment, and acceptance authority.
 */
export type EvidenceProperties =
  | ProjectProperties
  | SourceCaptureProperties
  | ClaimProperties
  | VerificationUnitProperties
  | EvalDatasetProperties
  | EvalSuiteProperties
  | EvaluatorProperties
  | EvalRunProperties
  | CaseResultProperties
  | DecisionProperties;
export type EvidenceJsonValue = null | boolean | number | string | EvidenceJsonValue[] | EvidenceJsonObject;

export interface ProjectProperties {
  name: string;
  domainId: string;
  [k: string]: EvidenceJsonValue | string;
}
export interface EvidenceJsonObject {
  [k: string]: EvidenceJsonValue;
}
export interface SourceCaptureProperties {
  url: string;
  retrievedAt: string;
  outcome: "success" | "failure" | "partial";
  artifacts: CaptureArtifact[];
  metadata: EvidenceJsonObject;
  originalPath?: string;
  originalRetrievedAt?: string;
  diagnostics?: EvidenceJsonObject[];
  sourceFingerprint?: string;
  [k: string]:
    | EvidenceJsonValue
    | string
    | "success"
    | "failure"
    | "partial"
    | CaptureArtifact[]
    | EvidenceJsonObject
    | EvidenceJsonObject[]
    | undefined;
}
export interface CaptureArtifact {
  kind: string;
  sha256: string;
  byteLength: number;
  mediaType?: string;
  encoding?: string;
  blobRef?: string;
  [k: string]: EvidenceJsonValue | string | number | undefined;
}
export interface ClaimProperties {
  statement: string;
  verificationUnitIds: string[];
  temporal: "fixed-revision" | "current-state";
  conflictKey?: string;
  sourceRefs?: string[];
  fingerprints?: EvidenceFingerprints;
  freshnessHours?: number;
  [k: string]:
    | EvidenceJsonValue
    | string
    | string[]
    | "fixed-revision"
    | "current-state"
    | EvidenceFingerprints
    | number
    | undefined;
}
export interface EvidenceFingerprints {
  [k: string]: string;
}
export interface VerificationUnitProperties {
  objective: string;
  scope: string;
  inputSnapshot: EvidenceJsonObject;
  expectedPredicate: EvidenceJsonObject;
  method: string;
  fingerprints?: EvidenceFingerprints;
  temporal?: "fixed-revision" | "current-state";
  freshnessHours?: number;
  sourceRefs?: string[];
  objectiveVersion?: string;
  thresholdVersion?: string;
  [k: string]:
    | EvidenceJsonValue
    | string
    | EvidenceJsonObject
    | EvidenceFingerprints
    | "fixed-revision"
    | "current-state"
    | number
    | string[]
    | undefined;
}
export interface EvalDatasetProperties {
  caseRefs: string[];
  name?: string;
  datasetVersion?: string;
  fingerprints?: EvidenceFingerprints;
  [k: string]: EvidenceJsonValue | string[] | string | EvidenceFingerprints | undefined;
}
export interface EvalSuiteProperties {
  datasetRef: string;
  target: EvidenceJsonObject;
  evaluatorRefs: string[];
  name?: string;
  suiteVersion?: string;
  iterations?: number;
  budget?: EvalBudget;
  objectiveVersion?: string;
  thresholdVersion?: string;
  [k: string]: EvidenceJsonValue | string | EvidenceJsonObject | string[] | number | EvalBudget | undefined;
}
export interface EvalBudget {
  maxRuns?: number;
  maxDurationMs?: number;
  maxTotalTokens?: number;
  [k: string]: EvidenceJsonValue | number | undefined;
}
export interface EvaluatorProperties {
  method: string;
  name?: string;
  evaluatorVersion?: string;
  objectiveVersion?: string;
  thresholdVersion?: string;
  configuration?: EvidenceJsonObject;
  [k: string]: EvidenceJsonValue | string | EvidenceJsonObject | undefined;
}
export interface EvalRunProperties {
  suiteRef: string;
  status: "running" | "completed" | "failed" | "budget-exceeded";
  results?: string[];
  metrics?: EvidenceJsonObject;
  startedAt?: string;
  finishedAt?: string | null;
  diagnostics?: EvidenceJsonValue[];
  fingerprints?: EvidenceFingerprints;
  datasetVersion?: string;
  suiteVersion?: string;
  evaluatorVersions?: EvidenceJsonObject;
  targetFingerprint?: string;
  [k: string]:
    | EvidenceJsonValue
    | string
    | "running"
    | "completed"
    | "failed"
    | "budget-exceeded"
    | string[]
    | EvidenceJsonObject
    | null
    | EvidenceJsonValue[]
    | EvidenceFingerprints
    | undefined;
}
export interface CaseResultProperties {
  runRef: string;
  unitRef: string;
  verdict: "supported" | "refuted" | "insufficient";
  verificationClass: "deterministic-verified" | "source-supported" | "model-judged" | "human-reviewed";
  status?: "completed" | "error" | "budget-exceeded";
  passed?: boolean | null;
  metrics?: EvalMetric[];
  fingerprints?: EvidenceFingerprints;
  verifiedAt?: string;
  diagnostics?: EvidenceJsonValue[];
  iteration?: number;
  evaluatorRef?: string;
  evaluatorVersion?: string;
  objectiveVersion?: string;
  thresholdVersion?: string;
  evidenceRefs?: string[];
  [k: string]:
    | EvidenceJsonValue
    | string
    | "supported"
    | "refuted"
    | "insufficient"
    | "deterministic-verified"
    | "source-supported"
    | "model-judged"
    | "human-reviewed"
    | "completed"
    | "error"
    | "budget-exceeded"
    | boolean
    | null
    | EvalMetric[]
    | EvidenceFingerprints
    | EvidenceJsonValue[]
    | number
    | string[]
    | undefined;
}
export interface EvalMetric {
  name: string;
  value: EvidenceJsonValue;
  objectiveVersion: string;
  thresholdVersion: string;
  threshold: EvidenceJsonValue;
  passed: boolean | null;
}
export interface DecisionProperties {
  subjectRef: string;
  status: "accepted" | "rejected" | "superseded";
  rationale: string;
  verificationRefs: string[];
  authority: DecisionAuthority;
  choice: EvidenceJsonValue;
  [k: string]: EvidenceJsonValue | string | "accepted" | "rejected" | "superseded" | string[] | DecisionAuthority;
}
export interface DecisionAuthority {
  actorId: string;
  scope: string;
  [k: string]: EvidenceJsonValue | string;
}

export type DomainObjectType =
  | "Project"
  | "SourceCapture"
  | "Claim"
  | "VerificationUnit"
  | "EvalDataset"
  | "EvalSuite"
  | "Evaluator"
  | "EvalRun"
  | "CaseResult"
  | "Decision";
