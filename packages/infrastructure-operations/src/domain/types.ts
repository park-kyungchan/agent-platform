import type { ObjectRecord, JsonObject } from "@palantirkc/ontology/operational";

export const PROJECT_ID = "vps-operations" as const;
export const DOMAIN_ID = "infrastructure" as const;
export const DOMAIN_VERSION = "1.0.0" as const;

/** The full process incarnation. PID alone must never authorize a signal. */
export interface ProcessIdentity {
  hostPid: number;
  localPid: number;
  startTicks: number;
  pidNamespace: string;
  containerId: string | null;
  uid: number;
}

export interface HolderIdentity {
  pid: number;
  localPid: number;
  startTicks: number;
  namespace: string;
  containerId?: string;
  command: string;
  matchingFdCount: number;
}

export interface LockIdentity {
  device: number;
  inode: number;
  held: boolean;
  holders: HolderIdentity[];
}

export interface MountBinding {
  type: "bind" | "volume" | "tmpfs";
  source: string;
  destination: string;
  readWrite: boolean;
}

export interface PublishedPort {
  containerPort: number;
  protocol: "tcp" | "udp";
  hostAddress: string;
  hostPort: number;
}

export interface PortOwnership {
  protocol: "tcp" | "udp";
  address: string;
  port: number;
  pidNamespace: string;
  owner: ProcessIdentity | null;
  containerId: string | null;
  purpose: "dashboard" | "native-backend" | "ingress" | "identity-provider" | "ssh" | "other";
  exposure: "loopback" | "tailnet" | "container-network" | "public" | "unknown";
}

export interface ContainerInventoryEntry {
  containerId: string;
  name: string;
  service: string;
  imageId: string;
  imageRevision: string | null;
  state: "created" | "running" | "paused" | "restarting" | "removing" | "exited" | "dead";
  dockerHealth: "healthy" | "unhealthy" | "starting" | "none";
  initPid: number | null;
  pidNamespace: string | null;
  role: "gateway-owner" | "backend" | "ingress" | "identity-provider" | "other";
  mounts: MountBinding[];
  publishedPorts: PublishedPort[];
  restartCount: number;
  restartPolicy: "no" | "always" | "unless-stopped" | "on-failure";
}

export interface ContainerInventory {
  hostId: string;
  observedAtUtc: string;
  containers: ContainerInventoryEntry[];
}

/** Message is already sanitized by the adapter; credentials never belong in domain data. */
export interface SanitizedAdapterError {
  adapter: string;
  profile: string | null;
  category: "authentication" | "configuration" | "connectivity" | "runtime" | "unknown";
  code: string;
  message: string;
  observedAtUtc: string;
  credentialsRedacted: true;
}

export interface GatewayProfileServing {
  profile: string;
  nativeOn: boolean;
  liveServed: boolean;
  nativeOwnerPid: number | null;
}

export interface GatewayControlIdentity {
  protocol?: number;
  kind?: string;
  pid?: number;
  start_time?: number;
  hermes_home?: string;
  profile?: string;
  supervisor?: string;
  code_sha?: string | null;
  code_version?: string | null;
  served_profiles?: string[];
}

export interface RuntimeObservation {
  service: string;
  containerId: string;
  initPid: number;
  namespace: string;
  dataSource: string;
  dockerHealth?: string;
  rootSlot: { up: boolean; localPid?: number | null; raw: string; downFileExists: boolean };
  native: {
    identify: GatewayControlIdentity | null;
    nativeServing: boolean;
    nativeOwnerPid?: number | null;
    profileOn: boolean;
    profiles?: GatewayProfileServing[];
    status: { gateway_state?: string; desired_state?: string; answering_pid?: number; answered_at?: number };
    adapters: {
      name: string;
      state?: string;
      errorCode?: string | null;
      credentialRejected: boolean;
      /** Explicit native configuration; omitted means configuration remains unknown. */
      configured?: boolean;
      /** Namespace-local PID of the process that wrote this observed adapter status. */
      writerPid?: number | null;
    }[];
    /** Sanitized errors from active configured adapters only. */
    adapterErrors?: SanitizedAdapterError[];
    http?: { code: number; gateway_running?: boolean; gateway_state?: string | null; gateway_mode?: string };
    error?: string;
  };
}

/** Raw measured snapshot; a verification and an acceptance decision remain separate. */
export interface InfrastructureObservation {
  schemaVersion: 1;
  observedAtUtc: string;
  targetService: string;
  profile: string;
  expectedProfiles?: string[];
  lock: LockIdentity;
  runtimes: RuntimeObservation[];
  inventory?: ContainerInventory;
  ports?: PortOwnership[];
  preservedSession?: { pid: number; startTicks: number; namespace: string; alive: boolean };
}
export type GatewayObservation = InfrastructureObservation;

export interface VpsHostProperties {
  hostname: string;
  sshEndpoint: { address: string; port: number; user: string; hostKeyAlias: string };
  platform: "linux";
  excludedEffects: string[];
}

export interface ContainerRuntimeProperties {
  hostId: string;
  inventory: ContainerInventoryEntry;
  observedAtUtc: string;
}

export interface GatewayRuntimeProperties {
  hostId: string;
  containerId: string;
  sharedVolumeId: string;
  topology: "shared" | "standalone";
  observedOwner: ProcessIdentity | null;
  lock: LockIdentity;
  state: "unknown" | "stopped" | "starting" | "running" | "degraded" | "draining" | "failed";
  profiles: GatewayProfileServing[];
  adapterErrors: SanitizedAdapterError[];
  observedAtUtc: string;
}

export interface SharedVolumeProperties {
  hostId: string;
  backingPath: string;
  mountDestinations: { containerId: string; destination: string; readWrite: boolean }[];
  device: number;
  inode: number;
}

export interface EvidenceProvenance {
  locator: string;
  sha256: string;
  byteLength: number;
  mediaType: string;
}

export interface ObservationProperties {
  hostId: string;
  source: "live-ssh" | "imported-receipt";
  observedAtUtc: string;
  evidence: EvidenceProvenance;
  observation: InfrastructureObservation;
}

export interface VerificationCriterion {
  id: string;
  status: "pass" | "fail" | "unverified";
  description: string;
  observationRefs: string[];
}

export interface VerificationResultProperties {
  hostId: string;
  observationRefs: string[];
  verificationClass: "current-runtime" | "deterministic-contract" | "historical-receipt";
  verifiedAtUtc: string;
  status: "passed" | "failed" | "unverified";
  criteria: VerificationCriterion[];
  adapterHealth: "healthy" | "degraded" | "unverified";
  limitations: string[];
}

export interface AuthorityDeclaration {
  actorType: "user" | "policy";
  actorId: string;
  scope: string;
  sourceEvidenceRef: string;
}

export interface AcceptanceDecisionProperties {
  hostId: string;
  verificationRefs: string[];
  status: "accepted" | "rejected" | "deferred" | "superseded";
  decidedAtUtc: string;
  authority: AuthorityDeclaration | null;
  rationale: string;
  conditions: string[];
}

export interface OperationRequestProperties {
  hostId: string;
  kind: "inspect" | "native-gateway-start" | "native-gateway-stop" | "native-gateway-restart";
  target: {
    containerId: string;
    service: string;
    processIdentity: ProcessIdentity | null;
    lockIdentity: LockIdentity | null;
  };
  preserveProcesses: ProcessIdentity[];
  scope: "infrastructure";
  reason: string;
  status: "proposed" | "authorized" | "executed" | "cancelled";
  authority: AuthorityDeclaration | null;
  requestedAtUtc: string;
  idempotencyKey: string;
  constraints: {
    revalidateProcessIdentity: true;
    exactLockInodeRequired: boolean;
    installedNativeLifecycle: true;
    broadContainerRestart: false;
    destructiveDeletion: false;
    exposeCredentials: false;
  };
}

export interface DomainProperties {
  VpsHost: VpsHostProperties;
  ContainerRuntime: ContainerRuntimeProperties;
  GatewayRuntime: GatewayRuntimeProperties;
  SharedVolume: SharedVolumeProperties;
  Observation: ObservationProperties;
  VerificationResult: VerificationResultProperties;
  AcceptanceDecision: AcceptanceDecisionProperties;
  OperationRequest: OperationRequestProperties;
}
export type DomainObjectType = keyof DomainProperties;
export type DomainObjectRecord<K extends DomainObjectType = DomainObjectType> =
  Omit<ObjectRecord, "objectType" | "properties"> & {
    objectType: `infrastructure:${K}`;
    properties: DomainProperties[K] & JsonObject;
  };
