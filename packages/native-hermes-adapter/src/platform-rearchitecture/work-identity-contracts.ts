/** Domain extensions over the immutable Ontology0.2.0 pin; not upper ArtifactEnvelope kinds. */
export interface IdentitySourcePin {
  path: string;
  sha256: string;
}
export interface WorkIdentityEnvelope<K extends "issue" | "spec" | "plan", S extends string, P> {
  schema: S;
  kind: K;
  id: string;
  projectId: "hermes-harness-engineering";
  ontologyVersion: "0.2.0";
  createdAt: string;
  recordClass: "current-source-projection-of-approved-scope-not-new-user-acceptance";
  payload: P;
}
export interface WorkIdentityJoin {
  workId: "hermes-platform-rearchitecture";
  issueId: string;
  specId: string;
  planId: string;
}
export type WorkIssueRecord = WorkIdentityEnvelope<
  "issue",
  "hermes.work-issue.v1",
  WorkIdentityJoin & {
    title: string;
    status: "implementation-in-progress";
    objective: string;
    authorizingDecision: IdentitySourcePin;
    sourcePins: IdentitySourcePin[];
    nativeTaskIds: string[];
    nativeSessionIds: string[];
    historicalAttributionInvented: false;
  }
>;
export type WorkSpecRecord = WorkIdentityEnvelope<
  "spec",
  "hermes.work-spec.v1",
  WorkIdentityJoin & {
    revision: 1;
    status: "derived-from-implementation-authorized-plan";
    authorizingDecision: IdentitySourcePin;
    sourcePins: IdentitySourcePin[];
    requirements: Array<{ id: string; stageId: string; outcome: string; dependsOn: string[] }>;
    acceptedPlanSections: Record<string, unknown>;
    implementationResultsAccepted: false;
    nativePermissionGranted: false;
  }
>;
export type WorkPlanBinding = WorkIdentityEnvelope<
  "plan",
  "hermes.work-plan-binding.v1",
  WorkIdentityJoin & {
    revision: 1;
    source: IdentitySourcePin;
    authorizingDecision: IdentitySourcePin;
    meaning: "New stable locator for the existing adopted plan bytes; no plan content or historical identity rewritten";
  }
>;
