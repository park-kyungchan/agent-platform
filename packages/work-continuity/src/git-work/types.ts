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
  /** Explicit source bindings; omitted on historical records, never inferred from prose. */
  continuity?: {
    schema: "common.git-work-continuity.v1";
    /** Each selected Input fragment contains this exact current typed purpose. */
    purposeSources: SourceReference[];
    /** Each selected fragment is a complete consumed-effect inventory. */
    effectInventorySources: SourceReference[];
  };
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
  /** Absent on historical objective-only Plans. New clauses require a new Plan revision. */
  clauses?: string[];
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
export interface TaskRecord extends Omit<RecordBase, "schema"> {
  /** Task-only protocol: old v1 readers must reject external-prerequisite Tasks. */
  schema: "common.git-work.v1" | "common.git-work-task.v2";
  kind: "task";
  logical: LogicalIdentity;
  title: string;
  /** Authored recovery instruction, never inferred from title or execution state. */
  nextAction?: string;
  state: TaskOperationalState;
  executionClass: "source-only" | "native-effect";
  dependsOn: TaskDependency[];
  /** Versioned external artifact route; v1 dependsOn remains strictly Work-local. */
  externalPrerequisites?: SelectedExternalArtifactPrerequisite[];
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
  /** Reverse effect-producer proof only: exact source/identity, not action instructions.
   * The complete record and its dependency graph remain privately validated. */
  contextRole?: "provenance";
  [key: string]: JsonValue | undefined;
}
export interface ContextSourceProjection {
  id: string;
  path: string;
  blobOid: string;
  /** Explicit address base; missing legacy metadata cannot establish a compact exact path. */
  pathBase: "work" | "repository";
  fragment?: string;
  repositoryId?: string;
  commit?: string;
  declaredNativeSource?: Readonly<NativeSource>;
  nativeAuthorshipVerified?: false;
}
interface NormativeContextSourceBase {
  id: string;
  /** Source ID addresses the complete canonical record; fields are exact JSON pointers. */
  source: string;
}
export type NormativeContextSource = NormativeContextSourceBase &
  (
    | {
        kind: "work";
        fields: {
          purpose: "#/purpose";
          spec: "#/spec";
          plan: "#/plan";
        };
        coverage: "complete";
      }
    | {
        kind: "spec";
        revision: number;
        fields: { outcome: "#/outcome"; criteria: "#/criteria" };
        coverage: "complete";
      }
    | {
        kind: "plan";
        revision: number;
        fields: {
          objective: "#/objective";
          clauses?: "#/clauses";
          specId: "#/specId";
          specRevision: "#/specRevision";
        };
        coverage: "complete" | "legacy-objective-only";
      }
    | {
        kind: "task";
        fields: { nextAction?: "#/nextAction" };
        coverage: "complete" | "unspecified";
      }
  );
export interface GitWorkNormativeContext {
  schema: "common.git-work-context.normative.v1";
  contracts: NormativeContextSource[];
}
export interface GitWorkContinuityBinding {
  schema: "common.git-work-context.continuity.v1";
  coverage: "bound" | "legacy-unbound" | "unavailable";
  purposeSources: string[];
  effectInventorySources: string[];
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
  latestRelevant: ContextRequest["latestRelevant"];
  /** Required values remain in records, addressed through these pooled exact Git sources. */
  normative: GitWorkNormativeContext;
  continuityBinding: GitWorkContinuityBinding;
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
/** Zero-based source address pool; optional repository/commit inherit the compact
 * context. A work path is relative to explicit sourcePathBase; repository paths
 * are already exact repository-relative paths. Neither base is guessed. */
export type CompactGitSourceAddress = [
  pathBase: "work" | "repository",
  path: string,
  blobOid: string,
  repositoryId?: string | null,
  commit?: string | null,
];
export type CompactSourcePointer = [
  source: number,
  fragment: string,
  native?: { ownerId: string; locator: string; receiptKind: "snapshot" | "git"; sha256?: string },
];
/** Every number indexes strings. Empty arrays and original order are preserved. */
export type CompactPurpose = [
  objective: number,
  successCriteria: number[],
  inScope: number[],
  outOfScope: number[],
];
/** Decode value string indices using strings. Each fields pointer selects that
 * exact corresponding value in the canonical source record. IDs/revisions are
 * record identities, independent of pooled values and source indices. */
export type CompactNormativeContract =
  | [
      kind: "work",
      id: string,
      source: number,
      fields: {
        purpose: "#/purpose";
        spec: "#/spec";
        plan: "#/plan";
        domainId: "#/domainId";
        issueId: "#/issueId";
      },
      coverage: "complete",
      purpose: CompactPurpose,
      active: { spec: [id: string, revision: number]; plan: [id: string, revision: number] },
      scope: { domainId: string; issueId: string },
    ]
  | [
      kind: "spec",
      id: string,
      revision: number,
      source: number,
      fields: { outcome: "#/outcome"; criteria: "#/criteria" },
      coverage: "complete",
      values: [outcome: number, criteria: number[]],
    ]
  | [
      kind: "plan",
      id: string,
      revision: number,
      source: number,
      fields: {
        objective: "#/objective";
        clauses?: "#/clauses";
        specId: "#/specId";
        specRevision: "#/specRevision";
      },
      coverage: "complete" | "legacy-objective-only",
      values: [objective: number, clauses?: number[]],
      spec: [id: string, revision: number],
    ];
export interface CompactTaskContext {
  id: string;
  source: CompactSourcePointer;
  /** Exact LogicalIdentity fields in their named tuple positions. */
  logical: [
    domainId: string,
    projectId: string,
    workId: string,
    issueId: string,
    specId: string,
    specRevision: number,
    planId: string,
    planRevision: number,
    taskId: string,
  ];
  /** Exact operational milestones; no formal completion/Acceptance conflation. */
  state: [
    execution: TaskOperationalState["execution"],
    verification: TaskOperationalState["verification"],
    localAudit: TaskOperationalState["localAudit"],
    sharedAudit: TaskOperationalState["sharedAudit"],
    formalCompletion: TaskOperationalState["formalCompletion"],
  ];
  executionClass: TaskRecord["executionClass"];
  effectIds: string[];
  pendingEffectIds: string[];
  nextActionCoverage: "authored" | "unspecified";
  nextAction?: { value: number; source: CompactSourcePointer };
}
/** Original-answer and question addresses only, without original text or Input.origin. */
export type CompactDecisionContext = [
  id: string,
  status: DecisionRecord["status"],
  answerSource: CompactSourcePointer,
  selectionContextSource?: CompactSourcePointer,
];
export interface CompactGitWorkContext {
  schema: "common.git-work-context.compact.v1";
  sourcePathBase: string;
  /** In-band layout makes the lossless tuples usable in a fresh native session. */
  layout: Record<string, string>;
  status: GitWorkContext["status"];
  repositoryId: string;
  commit: string;
  projectId: string;
  workId: string;
  latestRelevant: ContextRequest["latestRelevant"];
  strings: string[];
  sources: CompactGitSourceAddress[];
  normative: CompactNormativeContract[];
  task?: CompactTaskContext;
  decisions: CompactDecisionContext[];
  continuityBinding: Omit<GitWorkContinuityBinding, "purposeSources" | "effectInventorySources"> & {
    purposeSources: CompactSourcePointer[];
    effectInventorySources: CompactSourcePointer[];
  };
  historicalEffects?: GitWorkContext["historicalEffects"];
  holdReasons: string[];
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}

/** Lossless metadata packing of the same canonical compact semantics. The
 * normative string table is unchanged. The canonical decoder expands metadata
 * before applying the ordinary strict schema and semantic checks.
 */
export interface MetadataPackedGitWorkContext {
  schema: "common.git-work-context.compact.v1";
  encoding: {
    version: "metadata.v1";
    prefixes: string[];
    decoder: "@palantirkc/work-continuity/git-work#decodeCompactGitWorkContext";
  };
  metadata: JsonValue[];
  status: GitWorkContext["status"];
  strings: string[];
  sources: JsonValue[][];
  normative: JsonValue[][];
  decisions: JsonValue[][];
  continuityBinding: JsonValue[];
  historicalEffects?: JsonValue[];
  task?: JsonValue[];
  holdReasons: string[];
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
export type GitWorkCompactOutput = CompactGitWorkContext | MetadataPackedGitWorkContext;
export interface DecodedCompactGitWorkContext {
  normative: ContextRecordProjection[];
  task?: Omit<CompactTaskContext, "logical" | "state" | "nextAction"> & {
    logical: LogicalIdentity;
    state: TaskOperationalState;
    nextAction?: { value: string; source: CompactSourcePointer };
  };
  sources: { repositoryId: string; commit: string; path: string; blobOid: string }[];
  decisions: {
    id: string;
    status: DecisionRecord["status"];
    answerSource: CompactSourcePointer;
    selectionContextSource?: CompactSourcePointer;
  }[];
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

/** Every external address pins both Git snapshot and complete blob bytes. */
export interface PinnedGitSource extends ExternalGitSource {
  sha256: string;
}
export interface ArtifactSourceClosure {
  schema: "common.git-artifact-source-closure.v1";
  logical: LogicalIdentity;
  sources: PinnedGitSource[];
}
export interface SelectedArtifactAlternative {
  id: string;
  producer: LogicalIdentity;
  ownerId: string;
  sourceClosure: PinnedGitSource;
  milestone: DependencyMilestone;
  artifact: PinnedGitSource;
  criterionIds: string[];
}
export interface SelectedArtifactGateDefinition {
  schema: "common.git-artifact-gate-definition.v1";
  selectionOwnerId: string;
  alternatives: SelectedArtifactAlternative[];
}
export interface SelectedExternalArtifactPrerequisite {
  schema: "common.git-selected-external-artifact.v1";
  producer: {
    logical: LogicalIdentity;
    ownerId: string;
    /** Exact canonical Task record in the producer Work, never inserted into the consumer map. */
    record: PinnedGitSource;
    sourceClosure: PinnedGitSource;
    milestone: DependencyMilestone;
    artifact: PinnedGitSource;
    ownerMilestoneReceipt: PinnedGitSource;
  };
  consumerGate: {
    instanceId: string;
    revision: number;
    definition: PinnedGitSource;
    consumer: LogicalIdentity;
    consumerSourceClosure: PinnedGitSource;
    selectedAlternative: string;
    selectionOwnerId: string;
    ownerSelectionReceipt: PinnedGitSource;
    criterionReceipts: { criterionId: string; evidence: PinnedGitSource }[];
  };
  qualification:
    | { status: "unresolved"; receiverUse: null }
    | { status: "rejected" | "owner-qualified"; receiverUse: PinnedGitSource };
  grantsNativePermission: false;
}
/** All receipt schemas bind the entire consumer-specific gate, not a shared mutable gate status. */
export interface ArtifactSelectionReceipt {
  schema: "common.git-artifact-selection-receipt.v1";
  gate: Omit<
    SelectedExternalArtifactPrerequisite["consumerGate"],
    "ownerSelectionReceipt" | "criterionReceipts"
  >;
}
export interface ExternalArtifactMilestoneReceipt {
  schema: "common.git-external-milestone-receipt.v1";
  ownerId: string;
  producer: Omit<SelectedExternalArtifactPrerequisite["producer"], "ownerMilestoneReceipt">;
  /** Required only for acceptance; a separate owner AcceptanceDecision, never inferred from checks. */
  acceptanceDecision?: PinnedGitSource;
}
export interface ArtifactCriterionReceipt {
  schema: "common.git-artifact-criterion-receipt.v1";
  selection: ArtifactSelectionReceipt["gate"];
  producer: ExternalArtifactMilestoneReceipt["producer"];
  criterionId: string;
  status: "pass" | "fail" | "unknown";
  evidence: PinnedGitSource[];
}
export interface ExternalArtifactReceiverUse {
  schema: "common.git-external-artifact-receiver-use.v1";
  selection: ArtifactSelectionReceipt["gate"];
  producer: ExternalArtifactMilestoneReceipt["producer"];
  ownerSelectionReceipt: PinnedGitSource;
  ownerMilestoneReceipt: PinnedGitSource;
  criterionReceipts: SelectedExternalArtifactPrerequisite["consumerGate"]["criterionReceipts"];
  status: "used" | "rejected";
  receiverOwnerId: string;
  evidence: PinnedGitSource[];
}
