// Domain value types ported from archive/agent-platform-pre-strip-20261104.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationLockHoldersEntry {
    #[serde(rename = "pid")]
    pub pid: u64,
    #[serde(rename = "localPid")]
    pub local_pid: u64,
    #[serde(rename = "startTicks")]
    pub start_ticks: u64,
    #[serde(rename = "namespace")]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "containerId")]
    pub container_id: Option<String>,
    #[serde(rename = "command")]
    pub command: String,
    #[serde(rename = "matchingFdCount")]
    pub matching_fd_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationLock {
    #[serde(rename = "device")]
    pub device: u64,
    #[serde(rename = "inode")]
    pub inode: u64,
    #[serde(rename = "held")]
    pub held: bool,
    #[serde(rename = "holders")]
    pub holders: Vec<InfrastructureObservationLockHoldersEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryRootSlot {
    #[serde(rename = "up")]
    pub up: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "localPid")]
    pub local_pid: Option<u64>,
    #[serde(rename = "raw")]
    pub raw: String,
    #[serde(rename = "downFileExists")]
    pub down_file_exists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeIdentify {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "protocol")]
    pub protocol: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "kind")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "pid")]
    pub pid: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "start_time")]
    pub start_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "hermes_home")]
    pub hermes_home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "profile")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "supervisor")]
    pub supervisor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "code_sha")]
    pub code_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "code_version")]
    pub code_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "served_profiles")]
    pub served_profiles: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeProfilesEntry {
    #[serde(rename = "profile")]
    pub profile: String,
    #[serde(rename = "nativeOn")]
    pub native_on: bool,
    #[serde(rename = "liveServed")]
    pub live_served: bool,
    #[serde(rename = "nativeOwnerPid")]
    pub native_owner_pid: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gateway_state")]
    pub gateway_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "desired_state")]
    pub desired_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "answering_pid")]
    pub answering_pid: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "answered_at")]
    pub answered_at: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeAdaptersEntry {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "state")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "errorCode")]
    pub error_code: Option<String>,
    #[serde(rename = "credentialRejected")]
    pub credential_rejected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "configured")]
    pub configured: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "writerPid")]
    pub writer_pid: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeAdapterErrorsEntry {
    #[serde(rename = "adapter")]
    pub adapter: String,
    #[serde(rename = "profile")]
    pub profile: Option<String>,
    #[serde(rename = "category")]
    pub category: String,
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "message")]
    pub message: String,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
    #[serde(rename = "credentialsRedacted")]
    pub credentials_redacted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNativeHttp {
    #[serde(rename = "code")]
    pub code: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gateway_running")]
    pub gateway_running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gateway_state")]
    pub gateway_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gateway_mode")]
    pub gateway_mode: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntryNative {
    #[serde(rename = "identify")]
    pub identify: Option<InfrastructureObservationRuntimesEntryNativeIdentify>,
    #[serde(rename = "nativeServing")]
    pub native_serving: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "nativeOwnerPid")]
    pub native_owner_pid: Option<u64>,
    #[serde(rename = "profileOn")]
    pub profile_on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "profiles")]
    pub profiles: Option<Vec<InfrastructureObservationRuntimesEntryNativeProfilesEntry>>,
    #[serde(rename = "status")]
    pub status: InfrastructureObservationRuntimesEntryNativeStatus,
    #[serde(rename = "adapters")]
    pub adapters: Vec<InfrastructureObservationRuntimesEntryNativeAdaptersEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "adapterErrors")]
    pub adapter_errors: Option<Vec<InfrastructureObservationRuntimesEntryNativeAdapterErrorsEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "http")]
    pub http: Option<InfrastructureObservationRuntimesEntryNativeHttp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "error")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationRuntimesEntry {
    #[serde(rename = "service")]
    pub service: String,
    #[serde(rename = "containerId")]
    pub container_id: String,
    #[serde(rename = "initPid")]
    pub init_pid: u64,
    #[serde(rename = "namespace")]
    pub namespace: String,
    #[serde(rename = "dataSource")]
    pub data_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "dockerHealth")]
    pub docker_health: Option<String>,
    #[serde(rename = "rootSlot")]
    pub root_slot: InfrastructureObservationRuntimesEntryRootSlot,
    #[serde(rename = "native")]
    pub native: InfrastructureObservationRuntimesEntryNative,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationInventoryContainersEntryMountsEntry {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "source")]
    pub source: String,
    #[serde(rename = "destination")]
    pub destination: String,
    #[serde(rename = "readWrite")]
    pub read_write: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationInventoryContainersEntryPublishedPortsEntry {
    #[serde(rename = "containerPort")]
    pub container_port: u64,
    #[serde(rename = "protocol")]
    pub protocol: String,
    #[serde(rename = "hostAddress")]
    pub host_address: String,
    #[serde(rename = "hostPort")]
    pub host_port: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationInventoryContainersEntry {
    #[serde(rename = "containerId")]
    pub container_id: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "service")]
    pub service: String,
    #[serde(rename = "imageId")]
    pub image_id: String,
    #[serde(rename = "imageRevision")]
    pub image_revision: Option<String>,
    #[serde(rename = "state")]
    pub state: String,
    #[serde(rename = "dockerHealth")]
    pub docker_health: String,
    #[serde(rename = "initPid")]
    pub init_pid: Option<u64>,
    #[serde(rename = "pidNamespace")]
    pub pid_namespace: Option<String>,
    #[serde(rename = "role")]
    pub role: String,
    #[serde(rename = "mounts")]
    pub mounts: Vec<InfrastructureObservationInventoryContainersEntryMountsEntry>,
    #[serde(rename = "publishedPorts")]
    pub published_ports: Vec<InfrastructureObservationInventoryContainersEntryPublishedPortsEntry>,
    #[serde(rename = "restartCount")]
    pub restart_count: u64,
    #[serde(rename = "restartPolicy")]
    pub restart_policy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationInventory {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
    #[serde(rename = "containers")]
    pub containers: Vec<InfrastructureObservationInventoryContainersEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationPortsEntryOwner {
    #[serde(rename = "hostPid")]
    pub host_pid: u64,
    #[serde(rename = "localPid")]
    pub local_pid: u64,
    #[serde(rename = "startTicks")]
    pub start_ticks: u64,
    #[serde(rename = "pidNamespace")]
    pub pid_namespace: String,
    #[serde(rename = "containerId")]
    pub container_id: Option<String>,
    #[serde(rename = "uid")]
    pub uid: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationPortsEntry {
    #[serde(rename = "protocol")]
    pub protocol: String,
    #[serde(rename = "address")]
    pub address: String,
    #[serde(rename = "port")]
    pub port: u64,
    #[serde(rename = "pidNamespace")]
    pub pid_namespace: String,
    #[serde(rename = "owner")]
    pub owner: Option<InfrastructureObservationPortsEntryOwner>,
    #[serde(rename = "containerId")]
    pub container_id: Option<String>,
    #[serde(rename = "purpose")]
    pub purpose: String,
    #[serde(rename = "exposure")]
    pub exposure: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservationPreservedSession {
    #[serde(rename = "pid")]
    pub pid: u64,
    #[serde(rename = "startTicks")]
    pub start_ticks: u64,
    #[serde(rename = "namespace")]
    pub namespace: String,
    #[serde(rename = "alive")]
    pub alive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfrastructureObservation {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
    #[serde(rename = "targetService")]
    pub target_service: String,
    #[serde(rename = "profile")]
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "expectedProfiles")]
    pub expected_profiles: Option<Vec<String>>,
    #[serde(rename = "lock")]
    pub lock: InfrastructureObservationLock,
    #[serde(rename = "runtimes")]
    pub runtimes: Vec<InfrastructureObservationRuntimesEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "inventory")]
    pub inventory: Option<InfrastructureObservationInventory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ports")]
    pub ports: Option<Vec<InfrastructureObservationPortsEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "preservedSession")]
    pub preserved_session: Option<InfrastructureObservationPreservedSession>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VpsHostPropertiesSshEndpoint {
    #[serde(rename = "address")]
    pub address: String,
    #[serde(rename = "port")]
    pub port: u64,
    #[serde(rename = "user")]
    pub user: String,
    #[serde(rename = "hostKeyAlias")]
    pub host_key_alias: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VpsHostProperties {
    #[serde(rename = "hostname")]
    pub hostname: String,
    #[serde(rename = "sshEndpoint")]
    pub ssh_endpoint: VpsHostPropertiesSshEndpoint,
    #[serde(rename = "platform")]
    pub platform: String,
    #[serde(rename = "excludedEffects")]
    pub excluded_effects: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerRuntimeProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "inventory")]
    pub inventory: InfrastructureObservationInventoryContainersEntry,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayRuntimeProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "containerId")]
    pub container_id: String,
    #[serde(rename = "sharedVolumeId")]
    pub shared_volume_id: String,
    #[serde(rename = "topology")]
    pub topology: String,
    #[serde(rename = "observedOwner")]
    pub observed_owner: Option<InfrastructureObservationPortsEntryOwner>,
    #[serde(rename = "lock")]
    pub lock: InfrastructureObservationLock,
    #[serde(rename = "state")]
    pub state: String,
    #[serde(rename = "profiles")]
    pub profiles: Vec<InfrastructureObservationRuntimesEntryNativeProfilesEntry>,
    #[serde(rename = "adapterErrors")]
    pub adapter_errors: Vec<InfrastructureObservationRuntimesEntryNativeAdapterErrorsEntry>,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedVolumePropertiesMountDestinationsEntry {
    #[serde(rename = "containerId")]
    pub container_id: String,
    #[serde(rename = "destination")]
    pub destination: String,
    #[serde(rename = "readWrite")]
    pub read_write: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedVolumeProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "backingPath")]
    pub backing_path: String,
    #[serde(rename = "mountDestinations")]
    pub mount_destinations: Vec<SharedVolumePropertiesMountDestinationsEntry>,
    #[serde(rename = "device")]
    pub device: u64,
    #[serde(rename = "inode")]
    pub inode: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationPropertiesEvidence {
    #[serde(rename = "locator")]
    pub locator: String,
    #[serde(rename = "sha256")]
    pub sha256: String,
    #[serde(rename = "byteLength")]
    pub byte_length: u64,
    #[serde(rename = "mediaType")]
    pub media_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "source")]
    pub source: String,
    #[serde(rename = "observedAtUtc")]
    pub observed_at_utc: String,
    #[serde(rename = "evidence")]
    pub evidence: ObservationPropertiesEvidence,
    #[serde(rename = "observation")]
    pub observation: InfrastructureObservation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationResultPropertiesCriteriaEntry {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "description")]
    pub description: String,
    #[serde(rename = "observationRefs")]
    pub observation_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationResultProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "observationRefs")]
    pub observation_refs: Vec<String>,
    #[serde(rename = "verificationClass")]
    pub verification_class: String,
    #[serde(rename = "verifiedAtUtc")]
    pub verified_at_utc: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "criteria")]
    pub criteria: Vec<VerificationResultPropertiesCriteriaEntry>,
    #[serde(rename = "adapterHealth")]
    pub adapter_health: String,
    #[serde(rename = "limitations")]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceDecisionPropertiesAuthority {
    #[serde(rename = "actorType")]
    pub actor_type: String,
    #[serde(rename = "actorId")]
    pub actor_id: String,
    #[serde(rename = "scope")]
    pub scope: String,
    #[serde(rename = "sourceEvidenceRef")]
    pub source_evidence_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceDecisionProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "verificationRefs")]
    pub verification_refs: Vec<String>,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "decidedAtUtc")]
    pub decided_at_utc: String,
    #[serde(rename = "authority")]
    pub authority: Option<AcceptanceDecisionPropertiesAuthority>,
    #[serde(rename = "rationale")]
    pub rationale: String,
    #[serde(rename = "conditions")]
    pub conditions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequestPropertiesTarget {
    #[serde(rename = "containerId")]
    pub container_id: String,
    #[serde(rename = "service")]
    pub service: String,
    #[serde(rename = "processIdentity")]
    pub process_identity: Option<InfrastructureObservationPortsEntryOwner>,
    #[serde(rename = "lockIdentity")]
    pub lock_identity: Option<InfrastructureObservationLock>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequestPropertiesConstraints {
    #[serde(rename = "revalidateProcessIdentity")]
    pub revalidate_process_identity: bool,
    #[serde(rename = "exactLockInodeRequired")]
    pub exact_lock_inode_required: bool,
    #[serde(rename = "installedNativeLifecycle")]
    pub installed_native_lifecycle: bool,
    #[serde(rename = "broadContainerRestart")]
    pub broad_container_restart: bool,
    #[serde(rename = "destructiveDeletion")]
    pub destructive_deletion: bool,
    #[serde(rename = "exposeCredentials")]
    pub expose_credentials: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequestProperties {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(rename = "target")]
    pub target: OperationRequestPropertiesTarget,
    #[serde(rename = "preserveProcesses")]
    pub preserve_processes: Vec<InfrastructureObservationPortsEntryOwner>,
    #[serde(rename = "scope")]
    pub scope: String,
    #[serde(rename = "reason")]
    pub reason: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "authority")]
    pub authority: Option<AcceptanceDecisionPropertiesAuthority>,
    #[serde(rename = "requestedAtUtc")]
    pub requested_at_utc: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(rename = "constraints")]
    pub constraints: OperationRequestPropertiesConstraints,
}
