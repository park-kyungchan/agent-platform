import type {
  InfrastructureObservation, ContainerInventoryEntry, ProcessIdentity, OperationRequestProperties,
  DomainProperties,
} from "../src/domain";

export const NOW = "2026-10-06T12:00:00Z";
export const TARGET = "a".repeat(64);
export const SIBLING = "b".repeat(64);
export const HOST = "vps-operations:object:VpsHost:test-host";
export const PROFILES = ["default", "dc-context", "dc-contract-audit", "dc-gang-a-dev", "dc-review", "ontology-platform"];
export const processIdentity: ProcessIdentity = {
  hostPid: 5000, localPid: 206, startTicks: 8000, pidNamespace: "pid:[4026532639]", containerId: TARGET, uid: 10000,
};
export const inventoryEntry: ContainerInventoryEntry = {
  containerId: TARGET, name: "hermes-project-hermes-serve-1", service: "hermes-serve",
  imageId: "sha256:" + "c".repeat(64), imageRevision: "d".repeat(40),
  state: "running", dockerHealth: "healthy", initPid: 4900, pidNamespace: processIdentity.pidNamespace,
  role: "gateway-owner", mounts: [{ type: "bind", source: "/docker/test/data", destination: "/opt/data", readWrite: true }],
  publishedPorts: [], restartCount: 0, restartPolicy: "unless-stopped",
};

export function snapshot(): InfrastructureObservation {
  return {
    schemaVersion: 1, observedAtUtc: NOW, targetService: "hermes-serve", profile: "dc-gang-a-dev", expectedProfiles: [...PROFILES],
    lock: {
      device: 2049, inode: 811313, held: true,
      holders: [{ pid: 5000, localPid: 206, startTicks: 8000, namespace: processIdentity.pidNamespace, containerId: TARGET, command: "hermes gateway run", matchingFdCount: 1 }],
    },
    runtimes: [
      {
        service: "hermes-agent", containerId: SIBLING, initPid: 4800, namespace: "pid:[4026532299]",
        dataSource: "/docker/test/data", dockerHealth: "healthy",
        rootSlot: { up: false, localPid: null, raw: "down (exitcode 0) 30 seconds", downFileExists: true },
        native: {
          identify: { kind: "hermes-gateway", pid: 206, start_time: 8000, served_profiles: [...PROFILES] },
          nativeServing: false, nativeOwnerPid: null, profileOn: false,
          status: { gateway_state: "running", desired_state: "running", answering_pid: 206, answered_at: Date.parse(NOW) / 1000 },
          adapters: [], http: { code: 0 },
        },
      },
      {
        service: "hermes-serve", containerId: TARGET, initPid: 4900, namespace: processIdentity.pidNamespace,
        dataSource: "/docker/test/data", dockerHealth: "healthy",
        rootSlot: { up: true, localPid: 206, raw: "up (pid 206 pgid 206) 30 seconds", downFileExists: false },
        native: {
          identify: { protocol: 1, kind: "hermes-gateway", pid: 206, start_time: 8000, hermes_home: "/opt/data", profile: "default", supervisor: "manual", code_sha: "d".repeat(40), code_version: "unknown", served_profiles: [...PROFILES] },
          nativeServing: true, nativeOwnerPid: 206, profileOn: true,
          profiles: PROFILES.map(profile => ({ profile, nativeOn: true, liveServed: true, nativeOwnerPid: 206 })),
          status: { gateway_state: "running", desired_state: "running", answering_pid: 206, answered_at: Date.parse(NOW) / 1000 },
          adapters: [{ name: "ontology-platform:telegram", state: "connected", errorCode: null, credentialRejected: false }],
          http: { code: 200, gateway_running: true, gateway_state: "running", gateway_mode: "multiplex" },
        },
      },
    ],
    inventory: { hostId: HOST, observedAtUtc: NOW, containers: [structuredClone(inventoryEntry)] },
    ports: [{
      protocol: "tcp", address: "127.0.0.1", port: 9120, pidNamespace: processIdentity.pidNamespace,
      owner: structuredClone(processIdentity), containerId: TARGET, purpose: "native-backend", exposure: "loopback",
    }],
    preservedSession: { pid: 6000, startTicks: 9000, namespace: processIdentity.pidNamespace, alive: true },
  };
}

export function operation(): OperationRequestProperties {
  return {
    hostId: HOST, kind: "native-gateway-restart",
    target: { containerId: TARGET, service: "hermes-serve", processIdentity: structuredClone(processIdentity), lockIdentity: snapshot().lock },
    preserveProcesses: [{ hostPid: 6000, localPid: 1409, startTicks: 9000, pidNamespace: processIdentity.pidNamespace, containerId: TARGET, uid: 10000 }],
    scope: "infrastructure", reason: "Recover the shared native gateway while preserving the conversation.",
    status: "authorized", authority: { actorType: "user", actorId: "operator", scope: "gateway-only native restart", sourceEvidenceRef: "user-instruction-evidence" },
    requestedAtUtc: NOW, idempotencyKey: "restart-exact-incarnation-8000",
    constraints: {
      revalidateProcessIdentity: true, exactLockInodeRequired: true, installedNativeLifecycle: true,
      broadContainerRestart: false, destructiveDeletion: false, exposeCredentials: false,
    },
  };
}

export function objectProperties(): DomainProperties {
  return {
    VpsHost: { hostname: "test-host", sshEndpoint: { address: "100.64.0.10", port: 22, user: "root", hostKeyAlias: "test-host" }, platform: "linux", excludedEffects: ["academy-business", "ministry-business", "credential-mutation"] },
    ContainerRuntime: { hostId: HOST, inventory: structuredClone(inventoryEntry), observedAtUtc: NOW },
    GatewayRuntime: { hostId: HOST, containerId: TARGET, sharedVolumeId: "vps-operations:object:SharedVolume:data", topology: "shared", observedOwner: structuredClone(processIdentity), lock: snapshot().lock, state: "running", profiles: PROFILES.map(profile => ({ profile, nativeOn: true, liveServed: true, nativeOwnerPid: 206 })), adapterErrors: [], observedAtUtc: NOW },
    SharedVolume: { hostId: HOST, backingPath: "/docker/test/data", mountDestinations: [{ containerId: TARGET, destination: "/opt/data", readWrite: true }], device: 2049, inode: 4000 },
    Observation: { hostId: HOST, source: "live-ssh", observedAtUtc: NOW, evidence: { locator: "/evidence/snapshot.json", sha256: "e".repeat(64), byteLength: 1000, mediaType: "application/json" }, observation: snapshot() },
    VerificationResult: { hostId: HOST, observationRefs: ["observed-snapshot"], verificationClass: "current-runtime", verifiedAtUtc: NOW, status: "passed", criteria: [{ id: "native-serving", status: "pass", description: "Fresh native owner serves the requested profiles.", observationRefs: ["observed-snapshot"] }], adapterHealth: "healthy", limitations: [] },
    AcceptanceDecision: { hostId: HOST, verificationRefs: ["verification-result"], status: "accepted", decidedAtUtc: NOW, authority: { actorType: "user", actorId: "operator", scope: "gateway restoration", sourceEvidenceRef: "user-instruction-evidence" }, rationale: "The explicitly requested restoration criteria passed.", conditions: [] },
    OperationRequest: operation(),
  };
}

