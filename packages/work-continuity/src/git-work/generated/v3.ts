/** Generated from git-work-v3.schema.json; do not edit. SHA-256: 918dc3a6e933b50b186d7c72d15f7b21083459530e0ee9e32e42ebc81f6c7a48 */

/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Id".
 */
export type Id = string;
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Revision".
 */
export type Revision = number;
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ContractOwner".
 */
export type ContractOwner =
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
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Digest".
 */
export type Digest = string;
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Contract".
 */
export type Contract =
  | {
      id: Id;
      owner: ContractOwner;
      revision: Revision;
      kind: "issue";
      digest: Digest;
      /**
       * @maxItems 2000
       */
      sourceRefs: SourceRef[];
      body: IssueBody;
    }
  | {
      id: Id;
      owner: ContractOwner;
      revision: Revision;
      kind: "spec";
      digest: Digest;
      /**
       * @maxItems 2000
       */
      sourceRefs: SourceRef[];
      body: SpecBody;
    }
  | {
      id: Id;
      owner: ContractOwner;
      revision: Revision;
      kind: "plan";
      digest: Digest;
      /**
       * @maxItems 2000
       */
      sourceRefs: SourceRef[];
      body: PlanBody;
    };
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
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
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
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
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "StoredRecord".
 */
export type StoredRecord =
  | {
      schema: "common.git-work-record.v3";
      kind: "work";
      value: Work;
    }
  | {
      schema: "common.git-work-record.v3";
      kind: "task";
      value: Task;
    }
  | {
      schema: "common.git-work-record.v3";
      kind: "run";
      value: Run;
    };

export interface GitWorkV3Graph {
  schema: "common.git-work.v3";
  /**
   * @maxItems 2000
   */
  domains: Domain[];
  /**
   * @maxItems 2000
   */
  projects: Project[];
  /**
   * @maxItems 2000
   */
  works: Work[];
  /**
   * @maxItems 2000
   */
  tasks: Task[];
  /**
   * @maxItems 10000
   */
  contracts: Contract[];
  /**
   * @maxItems 10000
   */
  dependencies: Dependency[];
  /**
   * @maxItems 10000
   */
  relations: Related[];
  /**
   * @maxItems 10000
   */
  delegations: Delegation[];
  /**
   * @maxItems 10000
   */
  provenance: Provenance[];
  /**
   * @maxItems 10000
   */
  nativeCorrelations: NativeCorrelation[];
  /**
   * @maxItems 2048
   */
  aliases?: GitWorkAlias[];
  /**
   * @maxItems 2048
   */
  externalArtifacts?: SelectedExternalArtifact[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Domain".
 */
export interface Domain {
  id: Id;
  title: Id;
  parentDomainId?: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Project".
 */
export interface Project {
  id: Id;
  title: Id;
  primaryDomainId: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Work".
 */
export interface Work {
  projectId: Id;
  workId: Id;
  title: Id;
  leadActorId: Id | null;
  revision: Revision;
  contracts: ContractSet;
  status: "proposed" | "admitted" | "active" | "verified" | "accepted" | "cancelled";
  legacySource?: GitSourcePin;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ContractSet".
 */
export interface ContractSet {
  issue: ContractRef;
  spec: ContractRef;
  plan: ContractRef;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ContractRef".
 */
export interface ContractRef {
  id: Id;
  owner: ContractOwner;
  revision: Revision;
  kind: "issue" | "spec" | "plan";
  digest: Digest;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "GitSourcePin".
 */
export interface GitSourcePin {
  repositoryId: Id;
  commit: string;
  path: string;
  sha256: Digest;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Task".
 */
export interface Task {
  projectId: Id;
  workId: Id;
  taskId: Id;
  title: Id;
  parentTaskId?: Id;
  assigneeActorId: Id | null;
  revision: Revision;
  contractOverrides: ContractOverrides;
  /**
   * @maxItems 2000
   */
  acceptanceCriteria: Id[];
  /**
   * @maxItems 2000
   */
  resources: ResourceClaim[];
  status: "planned" | "ready" | "running" | "blocked" | "verified" | "accepted" | "cancelled";
  legacyBinding?: LegacyTaskBinding;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ContractOverrides".
 */
export interface ContractOverrides {
  issue?: ContractRef;
  spec?: ContractRef;
  plan?: ContractRef;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ResourceClaim".
 */
export interface ResourceClaim {
  resource: Id;
  mode: "read" | "write";
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "LegacyTaskBinding".
 */
export interface LegacyTaskBinding {
  protocol: "common.git-work.v1" | "common.git-work-task.v2";
  source: GitSourcePin;
  contracts: ContractSet;
  /**
   * @maxItems 2048
   */
  holdReasons: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SourceRef".
 */
export interface SourceRef {
  owner: Id;
  uri: string;
  revision: Id;
  digest: Digest;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "IssueBody".
 */
export interface IssueBody {
  parentIssue?: ContractRef;
  [k: string]: Json | ContractRef | undefined;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SpecBody".
 */
export interface SpecBody {
  /**
   * @maxItems 2000
   */
  acceptanceCriteria: AcceptanceCriterion[];
  refinesSpec?: ContractRef;
  [k: string]: Json | AcceptanceCriterion[] | ContractRef | undefined;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "AcceptanceCriterion".
 */
export interface AcceptanceCriterion {
  id: Id;
  /**
   * @maxItems 2000
   */
  covers?: Id[];
  [k: string]: Json | Id | Id[] | undefined;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "PlanBody".
 */
export interface PlanBody {
  appliesToSpec: ContractRef;
  methodRef?: ContractRef;
  [k: string]: Json | ContractRef | undefined;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Dependency".
 */
export interface Dependency {
  producer: TaskRef;
  consumer: TaskRef;
  required: "verified" | "accepted";
  artifact?: ArtifactRef;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "TaskRef".
 */
export interface TaskRef {
  projectId: Id;
  workId: Id;
  taskId: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactRef".
 */
export interface ArtifactRef {
  id: Id;
  revision: Revision;
  digest: Digest;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Related".
 */
export interface Related {
  kind: "relatedTo";
  from: Scope;
  to: Scope;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Delegation".
 */
export interface Delegation {
  from: Scope;
  to: Scope;
  authorityRef: SourceRef;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Provenance".
 */
export interface Provenance {
  subject: Scope;
  source: SourceRef;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "NativeCorrelation".
 */
export interface NativeCorrelation {
  task: TaskRef;
  binding: NativeBinding;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "NativeBinding".
 */
export interface NativeBinding {
  runtimeId: Id;
  nativeSessionId: Id | null;
  nativeTaskId: Id | null;
  nativeRunId: Id | null;
  paneId: Id | null;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "GitWorkAlias".
 */
export interface GitWorkAlias {
  namespace: Id;
  from: TypedLegacyAddress;
  to: TypedLegacyAddress;
  revision: Revision;
  provenance: GitSourcePin;
  reason: string;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "TypedLegacyAddress".
 */
export interface TypedLegacyAddress {
  kind: "work" | "task" | "run" | "issue" | "spec" | "plan";
  scope: Scope;
  key: string;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SelectedExternalArtifact".
 */
export interface SelectedExternalArtifact {
  schema: "common.git-selected-external-artifact.v3";
  producer: ProducerContractBinding;
  producerContext: GitSourcePin;
  artifact: ArtifactRef;
  artifactSource: GitSourcePin;
  producerMilestone: GitSourcePin;
  selection: ArtifactGateSelection;
  selectionReceipt: GitSourcePin;
  /**
   * @maxItems 2048
   */
  criteria: {
    criterionId: Id;
    receipt: GitSourcePin;
  }[];
  receiverOwnerId: Id;
  receiverUse: GitSourcePin;
  producerSources: TaskContractSourceClosure;
  consumerSources: TaskContractSourceClosure;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ProducerContractBinding".
 */
export interface ProducerContractBinding {
  task: TaskRef;
  workRevision: Revision;
  taskRevision: Revision;
  workContracts: ContractSet;
  contracts: ContractSet;
  effectiveDigest: Digest;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactGateSelection".
 */
export interface ArtifactGateSelection {
  instanceId: Id;
  revision: Revision;
  definition: GitSourcePin;
  consumer: ConsumerContractBinding;
  selectedAlternative: Id;
  selectionOwnerId: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ConsumerContractBinding".
 */
export interface ConsumerContractBinding {
  task: TaskRef;
  workRevision: Revision;
  taskRevision: Revision;
  workContracts: ContractSet;
  contracts: ContractSet;
  /**
   * @maxItems 2048
   */
  acceptanceCriteria: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "TaskContractSourceClosure".
 */
export interface TaskContractSourceClosure {
  work: GitSourcePin;
  task: GitSourcePin;
  /**
   * @maxItems 2048
   */
  contracts: GitSourcePin[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "WorkRef".
 */
export interface WorkRef {
  projectId: Id;
  workId: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Graph".
 */
export interface Graph {
  schema: "common.git-work.v3";
  /**
   * @maxItems 2000
   */
  domains: Domain[];
  /**
   * @maxItems 2000
   */
  projects: Project[];
  /**
   * @maxItems 2000
   */
  works: Work[];
  /**
   * @maxItems 2000
   */
  tasks: Task[];
  /**
   * @maxItems 10000
   */
  contracts: Contract[];
  /**
   * @maxItems 10000
   */
  dependencies: Dependency[];
  /**
   * @maxItems 10000
   */
  relations: Related[];
  /**
   * @maxItems 10000
   */
  delegations: Delegation[];
  /**
   * @maxItems 10000
   */
  provenance: Provenance[];
  /**
   * @maxItems 10000
   */
  nativeCorrelations: NativeCorrelation[];
  /**
   * @maxItems 2048
   */
  aliases?: GitWorkAlias[];
  /**
   * @maxItems 2048
   */
  externalArtifacts?: SelectedExternalArtifact[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Ancestor".
 */
export interface Ancestor {
  task: TaskRef;
  revision: Revision;
  contracts: ContractSet;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "DependencyTask".
 */
export interface DependencyTask {
  task: TaskRef;
  revision: Revision;
  contracts: ContractSet;
  /**
   * @maxItems 2000
   */
  resources: ResourceClaim[];
  /**
   * @maxItems 2000
   */
  acceptanceCriteria: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "EffectiveContract".
 */
export interface EffectiveContract {
  schema: "common.git-work-effective.v3";
  task: TaskRef;
  workRevision: Revision;
  taskRevision: Revision;
  workContracts: ContractSet;
  taskOverrides: ContractOverrides;
  contracts: ContractSet;
  /**
   * @maxItems 2000
   */
  parentTaskChain: Ancestor[];
  /**
   * @maxItems 2000
   */
  taskResources: ResourceClaim[];
  /**
   * @maxItems 2000
   */
  acceptanceCriteria: Id[];
  /**
   * @maxItems 10000
   */
  dependencies: Dependency[];
  /**
   * @maxItems 2000
   */
  dependencyTasks: DependencyTask[];
  digest: Digest;
  legacyBinding?: LegacyTaskBinding;
  /**
   * @maxItems 2048
   */
  holdReasons: Id[];
  /**
   * @maxItems 2048
   */
  externalArtifacts: SelectedExternalArtifact[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "Run".
 */
export interface Run {
  runId: Id;
  task: TaskRef;
  attempt: Revision;
  status: "reserved" | "started" | "succeeded" | "failed" | "cancelled" | "uncertain";
  effectiveContractDigest: Digest;
  sourceSnapshotId: Id;
  parentAuthorityRef: Id;
  /**
   * @maxItems 2000
   */
  nativeBindings: NativeBinding[];
  /**
   * @maxItems 2000
   */
  effectRefs: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "CreateRequest".
 */
export interface CreateRequest {
  schema: "common.git-work-create.v3";
  ownerId: Id;
  createRequestId: Id;
  kind: "work" | "task" | "run";
  scope: Scope;
  record: Json;
  /**
   * @maxItems 100
   */
  contracts: Json[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "AllocationBinding".
 */
export interface AllocationBinding {
  schema: "common.git-work-allocation.v3";
  ownerId: Id;
  createRequestId: Id;
  requestDigest: Digest;
  kind: "work" | "task" | "run";
  scope: Scope;
  id: string;
  recordPath: string;
  recordSha256: Digest;
  /**
   * @maxItems 100
   */
  contracts: {
    path: string;
    sha256: Digest;
  }[];
  authority: {
    actorId: Id;
    revision: Revision;
    authorityRef: Id;
    gitHookPolicyRef: Id;
  };
  creation: {
    recordBlobOid: string;
    /**
     * @maxItems 100
     */
    contractBlobs: {
      path: string;
      blobOid: string;
      sha256: Digest;
    }[];
  };
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ContextPacket".
 */
export interface ContextPacket {
  schema: "common.git-work-context.v3";
  mode: "full" | "compact" | "dispatch" | "resume";
  effective: EffectiveContract;
  /**
   * @maxItems 10000
   */
  contractSources: Contract[];
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
  status: "source-qualified" | "held";
  /**
   * @maxItems 2048
   */
  holdReasons: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "CompactContextPacket".
 */
export interface CompactContextPacket {
  schema: "common.git-work-context.compact.v3";
  encoding: "deflate-raw+base64.v1";
  effectiveDigest: Digest;
  decodedBytes: number;
  payload: string;
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SnapshotIdentity".
 */
export interface SnapshotIdentity {
  repositoryId: Id;
  commit: string;
  projectId: Id;
  workId: Id;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactAlternative".
 */
export interface ArtifactAlternative {
  id: Id;
  producer: ProducerContractBinding;
  artifact: ArtifactRef;
  /**
   * @maxItems 2048
   */
  criterionIds: Id[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SelectedArtifactGate".
 */
export interface SelectedArtifactGate {
  schema: "common.git-artifact-gate.v3";
  instanceId: Id;
  revision: Revision;
  consumer: ConsumerContractBinding;
  selectionOwnerId: Id;
  /**
   * @maxItems 2048
   */
  alternatives: ArtifactAlternative[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactSelectionReceipt".
 */
export interface ArtifactSelectionReceipt {
  schema: "common.git-artifact-selection.v3";
  selection: ArtifactGateSelection;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactCriterionReceipt".
 */
export interface ArtifactCriterionReceipt {
  schema: "common.git-artifact-criterion.v3";
  selection: ArtifactGateSelection;
  criterionId: Id;
  status: "pass" | "fail" | "unknown";
  /**
   * @maxItems 2048
   */
  evidence: GitSourcePin[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactProducerMilestone".
 */
export interface ArtifactProducerMilestone {
  schema: "common.git-artifact-producer-milestone.v3";
  producer: ProducerContractBinding;
  artifact: ArtifactRef;
  artifactSource: GitSourcePin;
  milestone: "verified" | "accepted";
  acceptanceDecision?: GitSourcePin;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactAcceptanceDecision".
 */
export interface ArtifactAcceptanceDecision {
  schema: "common.git-artifact-acceptance.v3";
  producer: ProducerContractBinding;
  artifact: ArtifactRef;
  ownerId: Id;
  decision: "accepted";
  /**
   * @maxItems 2048
   */
  evidence: GitSourcePin[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "ArtifactReceiverUse".
 */
export interface ArtifactReceiverUse {
  schema: "common.git-artifact-receiver-use.v3";
  selection: ArtifactGateSelection;
  producer: ProducerContractBinding;
  artifact: ArtifactRef;
  receiverOwnerId: Id;
  status: "used" | "rejected";
  /**
   * @maxItems 2048
   */
  evidence: GitSourcePin[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "LegacyCreateRequestBinding".
 */
export interface LegacyCreateRequestBinding {
  schema: "common.git-legacy-create-binding.v3";
  ownerId: Id;
  legacyOwner: {
    repositoryId: Id;
    projectId: Id;
    workId: Id;
  };
  createRequestId: Id;
  kind: "task";
  id: Id;
  meaningSha256: Digest;
  source: GitSourcePin;
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "LegacyMigrationRequest".
 */
export interface LegacyMigrationRequest {
  schema: "common.git-legacy-migration-request.v3";
  ownerId: Id;
  createRequestId: Id;
  snapshot: SnapshotIdentity;
  expectedInventoryDigest: Digest;
  /**
   * @maxItems 2048
   */
  bootstrapRequests: GitSourcePin[];
  /**
   * @maxItems 2048
   */
  aliases: GitWorkAlias[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "LegacyWriterFence".
 */
export interface LegacyWriterFence {
  schema: "common.git-work-writer-fence.v3";
  ownerId: Id;
  scope: WorkRef;
  authoritativeProtocol: "v3";
  legacyReadSnapshot: SnapshotIdentity;
  inventoryDigest: Digest;
  migrationRequestDigest: Digest;
  /**
   * @maxItems 10000
   */
  sources: GitSourcePin[];
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "LegacyMigrationReceipt".
 */
export interface LegacyMigrationReceipt {
  schema: "common.git-legacy-migration-receipt.v3";
  ownerId: Id;
  createRequestId: Id;
  requestDigest: Digest;
  scope: WorkRef;
  legacySnapshot: SnapshotIdentity;
  inventoryDigest: Digest;
  /**
   * @maxItems 10000
   */
  preservedSources: GitSourcePin[];
  /**
   * @maxItems 10000
   */
  projectedFiles: {
    path: string;
    blobOid: string;
    sha256: Digest;
  }[];
  writerFencePath: string;
  /**
   * @maxItems 2048
   */
  legacyRequestPaths: string[];
  /**
   * @maxItems 2048
   */
  aliases: GitWorkAlias[];
  authority: {
    actorId: Id;
    revision: Revision;
    authorityRef: Id;
    gitHookPolicyRef: Id;
  };
}
/**
 * This interface was referenced by `GitWorkV3Graph`'s JSON-Schema
 * via the `definition` "SelectedArtifactDocument".
 */
export interface SelectedArtifactDocument {
  schema: "common.git-artifact.v3";
  id: Id;
  revision: Revision;
  producer: ProducerContractBinding;
  body: Json;
}
