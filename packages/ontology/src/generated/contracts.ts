/**
 * Generated from schemas/artifact.schema.json; do not edit.
 * json-schema-to-typescript 16.0.0
 * Input SHA-256: 614f7deb0b9d7717ab8a992f144955e05875c012716001d4aa0a96edbce92ba8
 */

/**
 * Canonical shared artifact contract. Domain-specific observations belong in details. Schema shape version is independent of package release version.
 */
export type Artifact =
  | ProjectArtifact
  | TaskArtifact
  | ProposalArtifact
  | DecisionArtifact
  | EvidenceArtifact
  | VerificationArtifact
  | RunArtifact;
export type References = {
  artifactId: string;
  relation:
    | "parent"
    | "supports"
    | "authorizes"
    | "implements"
    | "verifies"
    | "supersedes"
    | "derivedFrom"
    | "attemptOf"
    | "accepts";
  sha256?: string;
}[];
export type JsonValue = null | boolean | number | string | JsonValue[] | JsonObject;

export interface ProjectArtifact {
  schemaVersion: "1.0.0";
  kind: "project";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: ProjectPayload;
}
export interface ProjectPayload {
  name: string;
  status: "active" | "archived";
  scope: string;
  ontologyPackage: {
    name: "@palantirkc/ontology";
    version: string;
  };
  commands: {
    [k: string]: string;
  };
  details?: JsonObject;
}
export interface JsonObject {
  [k: string]: JsonValue;
}
export interface TaskArtifact {
  schemaVersion: "1.0.0";
  kind: "task";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: TaskPayload;
}
export interface TaskPayload {
  title: string;
  objective: string;
  status: "planned" | "running" | "complete" | "blocked" | "cancelled";
  /**
   * @minItems 1
   */
  acceptanceCriteria: [string, ...string[]];
  verificationRefs?: string[];
  details?: JsonObject;
}
export interface ProposalArtifact {
  schemaVersion: "1.0.0";
  kind: "proposal";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: ProposalPayload;
}
export interface ProposalPayload {
  title: string;
  status: "draft" | "proposed" | "withdrawn";
  recommendation: string;
  alternatives: string[];
  evidenceRefs: string[];
  details?: JsonObject;
}
export interface DecisionArtifact {
  schemaVersion: "1.0.0";
  kind: "decision";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: DecisionPayload;
}
export interface DecisionPayload {
  title: string;
  status: "accepted" | "rejected" | "superseded";
  choice: string;
  rationale: string;
  evidenceRefs: string[];
  proposalRef?: string;
  authority?: {
    actorType: "user" | "project-owner";
    actorId: string;
    sourceEvidenceRef: string;
    scope: string;
  };
  details?: JsonObject;
}
export interface EvidenceArtifact {
  schemaVersion: "1.0.0";
  kind: "evidence";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: EvidencePayload;
}
export interface EvidencePayload {
  claim: string;
  method: string;
  sourceUrl: string;
  fetchedAt: string;
  upstreamRevision?: string;
  result: {
    outcome: "success" | "failure" | "partial";
    processExitCode: number | null;
    contentMatched: boolean | null;
    details?: JsonObject;
  };
  attempts: {
    attempt: number;
    outcome: "success" | "failure";
    error?: string;
    details?: JsonObject;
  }[];
  limitations: string[];
  details?: JsonObject;
}
export interface VerificationArtifact {
  schemaVersion: "1.0.0";
  kind: "verification";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: VerificationPayload;
}
export interface VerificationPayload {
  /**
   * @minItems 1
   */
  targetRefs: [string, ...string[]];
  /**
   * @minItems 1
   */
  criteria: [
    {
      id: string;
      status: "pass" | "fail" | "unverified";
      message: string;
      evidenceRefs: string[];
    },
    ...{
      id: string;
      status: "pass" | "fail" | "unverified";
      message: string;
      evidenceRefs: string[];
    }[]
  ];
  inputHashes: {
    path: string;
    sha256: string;
  }[];
  runtime: {
    name: string;
    version: string;
  };
  details?: JsonObject;
}
export interface RunArtifact {
  schemaVersion: "1.0.0";
  kind: "run";
  id: string;
  projectId: string;
  domainId: string;
  createdAt: string;
  references?: References;
  payload: RunPayload;
}
export interface RunPayload {
  taskRef: string;
  state: "running" | "completed" | "failed" | "interrupted";
  startedAt: string;
  finishedAt: string | null;
  inputHashes: {
    path: string;
    sha256: string;
  }[];
  runtime: {
    name: string;
    version: string;
  };
  evidenceRefs: string[];
  verificationRefs: string[];
  details?: JsonObject;
}
