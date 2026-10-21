/** Generated from git-work-v3.schema.json; do not edit. SHA-256: 85626ea6e2a057dabd00dd2db711fad4b0b9ef5f6e5991b00eb0394850f11553 */

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
  leadActorId: Id;
  revision: Revision;
  contracts: ContractSet;
  status: "proposed" | "admitted" | "active" | "verified" | "accepted" | "cancelled";
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
 * via the `definition` "Task".
 */
export interface Task {
  projectId: Id;
  workId: Id;
  taskId: Id;
  title: Id;
  parentTaskId?: Id;
  assigneeActorId: Id;
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
