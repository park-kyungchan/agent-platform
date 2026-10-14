import type { Artifact } from "@palantirkc/ontology";
import type { LogicalIdentity } from "../logical-identity.ts";
import type { TaskOperationalState } from "../operational-state.ts";
export type { LogicalIdentity, TaskOperationalState };

export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export interface SnapshotSource {
  kind: "snapshot";
  path: string;
  fragment?: string;
  sha256?: string;
}
export interface ExternalGitSource {
  kind: "git";
  repositoryId: string;
  commit: string;
  path: string;
  fragment?: string;
  sha256?: string;
}
export interface NativeSource {
  kind: "native";
  ownerId: string;
  locator: string;
  receipt: SnapshotSource | ExternalGitSource;
}
export type SourceReference = SnapshotSource | ExternalGitSource | NativeSource;
export interface RecordIdentity {
  domainId: string;
  projectId: string;
  workId: string;
}
export interface RecordBase extends RecordIdentity {
  schema: "common.git-work.v1";
  id: string;
}
export interface WorkRecord extends RecordBase {
  kind: "work";
  repositoryId: string;
  issueId: string;
  purpose: {
    objective: string;
    successCriteria: string[];
    inScope: string[];
    outOfScope: string[];
  };
  spec: { id: string; revision: number };
  plan: { id: string; revision: number };
  governingDecisionIds: string[];
  historicalFenceIds: string[];
}
export interface SpecRecord extends RecordBase {
  kind: "spec";
  revision: number;
  outcome: string;
  criteria: string[];
}
export interface PlanRecord extends RecordBase {
  kind: "plan";
  revision: number;
  specId: string;
  specRevision: number;
  objective: string;
}
export interface InputRecord extends RecordBase {
  kind: "input";
  inputKind: "actual-user-answer" | "source-owner-record";
  origin: {
    ownerId: string;
    locator: string;
    kind: "native-user-message" | "source-owner-capture";
  };
  content: JsonValue;
}
export interface DecisionRecord extends RecordBase {
  kind: "decision";
  status: "accepted" | "superseded";
  answer: string;
  meaning: string;
  answerSource: SnapshotSource | ExternalGitSource;
  selectionContext?: { text: string; source: SnapshotSource | ExternalGitSource };
  supersedes: string[];
}
export interface NativeOccurrence {
  runtimeId: string;
  attemptId: string;
  occurrenceId: string;
}
export type DependencyMilestone =
  "execution-and-verification" | "shared-audit-delivery" | "formal-task-completion" | "acceptance";
export interface TaskDependency {
  taskId: string;
  milestone: DependencyMilestone;
  receiptEvidenceId?: string;
  receiverEvidenceId?: string;
}
export interface TaskRecord extends RecordBase {
  kind: "task";
  logical: LogicalIdentity;
  title: string;
  state: TaskOperationalState;
  executionClass: "source-only" | "native-effect";
  dependsOn: TaskDependency[];
  decisionIds: string[];
  effectIds: string[];
  verificationEvidenceIds: string[];
  /** Effects proposed for execution, distinct from related historical effectIds. */
  pendingEffectIds?: string[];
  runtimeId?: string;
  occurrence?: NativeOccurrence;
  executionEvidenceId?: string;
  artifact?: Artifact;
}
export interface RelationRecord extends RecordBase {
  kind: "relation";
  relation: "contains" | "purpose" | "dependsOn" | "supersedes" | "implements" | "verifiedBy";
  from: RecordReference;
  to: RecordReference;
}
export interface EffectRecord extends RecordBase {
  kind: "effect";
  taskId: string;
  status: "completed" | "uncertain";
  occurrence: NativeOccurrence;
  evidenceIds: string[];
}
export interface EffectFenceRecord extends RecordBase {
  kind: "effect-fence";
  completedEffectIds: string[];
  uncertainEffectIds: string[];
  source: SnapshotSource | ExternalGitSource;
}
export interface PrerequisiteReceipt {
  schema: "common.git-prerequisite-receipt.v1";
  logical: LogicalIdentity;
  occurrence: NativeOccurrence;
  state: TaskOperationalState;
  intent: SourceReference;
  result: SourceReference;
  verification: SourceReference;
}
export interface ReceiverReceipt {
  schema: "common.git-receiver-receipt.v1";
  receiverRuntimeId: string;
  producer: NativeOccurrence;
  prerequisiteTaskId: string;
  payload: SnapshotSource | ExternalGitSource;
}
export interface EvidenceRecord extends RecordBase {
  kind: "evidence";
  evidenceKind:
    "observation" | "verification" | "review" | "native-execution" | "receiver" | "acceptance";
  source: SourceReference;
  artifact?: Artifact;
}
export type GitWorkRecord =
  | WorkRecord
  | SpecRecord
  | PlanRecord
  | InputRecord
  | DecisionRecord
  | TaskRecord
  | RelationRecord
  | EffectRecord
  | EffectFenceRecord
  | EvidenceRecord;
export type RecordKind = GitWorkRecord["kind"];
export interface RecordReference {
  kind: RecordKind;
  id: string;
  /** Required for Spec/Plan semantic contracts; absent for other record kinds. */
  revision?: number;
}
export interface GitWorkSelection {
  repositoryRoot: string;
  repositoryId: string;
  commit: string;
  projectId: string;
  workId: string;
  maxBlobBytes?: number;
  maxRecords?: number;
  maxTotalBytes?: number;
  repositories?: Record<string, string>;
}
export interface ResolvedSource {
  /** Resolved Git receipt address; this proof does not authenticate a native owner. */
  reference: SnapshotSource | ExternalGitSource;
  /** Original native owner/locator declaration, deeply frozen by the source reader. */
  declaredNativeSource?: Readonly<NativeSource>;
  repositoryId: string;
  commit: string;
  path: string;
  blobOid: string;
  byteLength: number;
  sha256: string;
  value: JsonValue;
  document: JsonValue;
  verificationClass: "git-object-source-only";
  nativeAuthorshipVerified: false;
}
export interface GitWorkReader {
  selection: Readonly<GitWorkSelection>;
  commit: string;
  workPath: string;
  work: WorkRecord;
  readRecord(reference: RecordReference): Promise<GitWorkRecord>;
  listRecords(): Promise<RecordReference[]>;
  readSource(reference: SourceReference): Promise<ResolvedSource>;
}
export interface ValidationResult {
  valid: boolean;
  findings: string[];
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
export interface ContextRequest {
  taskId?: string;
  decisionId?: string;
  maxChars?: number;
  maxRecords?: number;
  latestRelevant: "current" | "advanced" | "unknown" | "unavailable";
}
export interface ContextRecordProjection {
  kind: RecordKind;
  id: string;
  [key: string]: JsonValue | undefined;
}
export interface ContextSourceProjection {
  id: string;
  path: string;
  blobOid: string;
  fragment?: string;
  repositoryId?: string;
  commit?: string;
  declaredNativeSource?: Readonly<NativeSource>;
  nativeAuthorshipVerified?: false;
}
export interface GitWorkContext {
  status: "source-qualified" | "held";
  repositoryId: string;
  commit: string;
  projectId: string;
  workId: string;
  records: ContextRecordProjection[];
  sources: ContextSourceProjection[];
  projection: "verified-semantic-context";
  sourcePathBase: string;
  holdReasons: string[];
  omittedRecords: number;
  historicalEffects?: {
    completedCount: number;
    uncertainCount: number;
    relevantCompletedIds: string[];
    relevantUncertainIds: string[];
    projectedFenceRecords: number;
  };
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
export type StartRequest =
  | {
      mode: "new";
      selectedCommit: string;
      repositoryId: string;
      remoteMain: {
        repositoryId: string;
        commit: string;
        observedAtUtc: string;
        checkedForStartId: string;
        synchronized: true;
        receipt: string;
      };
      startId: string;
      nowUtc: string;
      maxObservationAgeMs: number;
    }
  | {
      mode: "continue";
      repositoryId: string;
      selectedCommit: string;
      previous: RecordIdentity;
      current: RecordIdentity;
      branch: string;
    };
