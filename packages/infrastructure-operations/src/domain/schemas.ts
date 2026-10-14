import type { DomainObjectType } from "./types";

type Schema = Record<string, unknown>;
const text: Schema = { type: "string", minLength: 1 };
const bool: Schema = { type: "boolean" };
const timestamp: Schema = { type: "string", format: "date-time" };
const natural: Schema = { type: "integer", minimum: 0, maximum: Number.MAX_SAFE_INTEGER };
const positive: Schema = { type: "integer", minimum: 1, maximum: Number.MAX_SAFE_INTEGER };
const namespace: Schema = { type: "string", pattern: "^pid:\\[\\d+\\]$" };
const containerId: Schema = { type: "string", pattern: "^[a-f0-9]{64}$" };
const imageId: Schema = { type: "string", pattern: "^sha256:[a-f0-9]{64}$" };
const port: Schema = { type: "integer", minimum: 1, maximum: 65535 };
const array = (items: Schema, minItems = 0): Schema => ({ type: "array", items, minItems });
const strings = (minItems = 0): Schema => ({
  type: "array",
  items: text,
  minItems,
  uniqueItems: true,
});
const choice = (...values: string[]): Schema => ({ type: "string", enum: values });
const nullable = (value: Schema): Schema => ({ anyOf: [value, { type: "null" }] });
const object = (properties: Record<string, Schema>, optional: readonly string[] = []): Schema => ({
  type: "object",
  properties,
  required: Object.keys(properties).filter((key) => !optional.includes(key)),
  additionalProperties: false,
});

export const processIdentitySchema = object({
  hostPid: positive,
  localPid: positive,
  startTicks: positive,
  pidNamespace: namespace,
  containerId: nullable(containerId),
  uid: natural,
});
const holderSchema = object(
  {
    pid: positive,
    localPid: positive,
    startTicks: positive,
    namespace,
    containerId,
    command: text,
    matchingFdCount: natural,
  },
  ["containerId"],
);
export const lockIdentitySchema = object({
  device: natural,
  inode: positive,
  held: bool,
  holders: array(holderSchema),
});
const mountSchema = object({
  type: choice("bind", "volume", "tmpfs"),
  source: text,
  destination: text,
  readWrite: bool,
});
const publishedPortSchema = object({
  containerPort: port,
  protocol: choice("tcp", "udp"),
  hostAddress: text,
  hostPort: port,
});
export const portOwnershipSchema = object({
  protocol: choice("tcp", "udp"),
  address: text,
  port,
  pidNamespace: namespace,
  owner: nullable(processIdentitySchema),
  containerId: nullable(containerId),
  purpose: choice("dashboard", "native-backend", "ingress", "identity-provider", "ssh", "other"),
  exposure: choice("loopback", "tailnet", "container-network", "public", "unknown"),
});
export const containerInventoryEntrySchema = object({
  containerId,
  name: text,
  service: text,
  imageId,
  imageRevision: nullable(text),
  state: choice("created", "running", "paused", "restarting", "removing", "exited", "dead"),
  dockerHealth: choice("healthy", "unhealthy", "starting", "none"),
  initPid: nullable(positive),
  pidNamespace: nullable(namespace),
  role: choice("gateway-owner", "backend", "ingress", "identity-provider", "other"),
  mounts: array(mountSchema),
  publishedPorts: array(publishedPortSchema),
  restartCount: natural,
  restartPolicy: choice("no", "always", "unless-stopped", "on-failure"),
});
export const containerInventorySchema = object({
  hostId: text,
  observedAtUtc: timestamp,
  containers: array(containerInventoryEntrySchema),
});
export const adapterErrorSchema = object({
  adapter: text,
  profile: nullable(text),
  category: choice("authentication", "configuration", "connectivity", "runtime", "unknown"),
  code: text,
  message: text,
  observedAtUtc: timestamp,
  credentialsRedacted: { const: true },
});
export const gatewayProfileServingSchema = object({
  profile: text,
  nativeOn: bool,
  liveServed: bool,
  nativeOwnerPid: nullable(positive),
});
const controlIdentitySchema = object(
  {
    protocol: positive,
    kind: text,
    pid: positive,
    start_time: positive,
    hermes_home: text,
    profile: text,
    supervisor: text,
    code_sha: nullable(text),
    code_version: nullable(text),
    served_profiles: strings(),
  },
  [
    "protocol",
    "kind",
    "pid",
    "start_time",
    "hermes_home",
    "profile",
    "supervisor",
    "code_sha",
    "code_version",
    "served_profiles",
  ],
);
const runtimeSchema = object(
  {
    service: text,
    containerId,
    initPid: positive,
    namespace,
    dataSource: text,
    dockerHealth: text,
    rootSlot: object(
      { up: bool, localPid: nullable(positive), raw: { type: "string" }, downFileExists: bool },
      ["localPid"],
    ),
    native: object(
      {
        identify: nullable(controlIdentitySchema),
        nativeServing: bool,
        nativeOwnerPid: nullable(positive),
        profileOn: bool,
        profiles: array(gatewayProfileServingSchema),
        status: object(
          {
            gateway_state: text,
            desired_state: text,
            answering_pid: positive,
            answered_at: { type: "number", minimum: 0 },
          },
          ["gateway_state", "desired_state", "answering_pid", "answered_at"],
        ),
        adapters: array(
          object(
            {
              name: text,
              state: text,
              errorCode: nullable(text),
              credentialRejected: bool,
              configured: bool,
              writerPid: nullable(positive),
            },
            ["state", "errorCode", "configured", "writerPid"],
          ),
        ),
        adapterErrors: array(adapterErrorSchema),
        http: object(
          {
            code: { type: "integer", minimum: 0, maximum: 599 },
            gateway_running: bool,
            gateway_state: nullable(text),
            gateway_mode: text,
          },
          ["gateway_running", "gateway_state", "gateway_mode"],
        ),
        error: text,
      },
      ["nativeOwnerPid", "profiles", "adapterErrors", "http", "error"],
    ),
  },
  ["dockerHealth"],
);
export const observationSchema = object(
  {
    schemaVersion: { const: 1 },
    observedAtUtc: timestamp,
    targetService: text,
    profile: text,
    expectedProfiles: strings(1),
    lock: lockIdentitySchema,
    runtimes: array(runtimeSchema, 1),
    inventory: containerInventorySchema,
    ports: array(portOwnershipSchema),
    preservedSession: object({ pid: positive, startTicks: positive, namespace, alive: bool }),
  },
  ["expectedProfiles", "inventory", "ports", "preservedSession"],
);

const authoritySchema = object({
  actorType: choice("user", "policy"),
  actorId: text,
  scope: text,
  sourceEvidenceRef: text,
});
const verificationCriterionSchema = object({
  id: text,
  status: choice("pass", "fail", "unverified"),
  description: text,
  observationRefs: strings(),
});
const evidenceSchema = object({
  locator: text,
  sha256: { type: "string", pattern: "^[a-f0-9]{64}$" },
  byteLength: natural,
  mediaType: text,
});

export const domainPropertySchemas: Record<DomainObjectType, Schema> = {
  VpsHost: object({
    hostname: text,
    sshEndpoint: object({ address: text, port, user: text, hostKeyAlias: text }),
    platform: { const: "linux" },
    excludedEffects: strings(),
  }),
  ContainerRuntime: object({
    hostId: text,
    inventory: containerInventoryEntrySchema,
    observedAtUtc: timestamp,
  }),
  GatewayRuntime: object({
    hostId: text,
    containerId,
    sharedVolumeId: text,
    topology: choice("shared", "standalone"),
    observedOwner: nullable(processIdentitySchema),
    lock: lockIdentitySchema,
    state: choice("unknown", "stopped", "starting", "running", "degraded", "draining", "failed"),
    profiles: array(gatewayProfileServingSchema),
    adapterErrors: array(adapterErrorSchema),
    observedAtUtc: timestamp,
  }),
  SharedVolume: object({
    hostId: text,
    backingPath: text,
    mountDestinations: array(object({ containerId, destination: text, readWrite: bool })),
    device: natural,
    inode: positive,
  }),
  Observation: object({
    hostId: text,
    source: choice("live-ssh", "imported-receipt"),
    observedAtUtc: timestamp,
    evidence: evidenceSchema,
    observation: observationSchema,
  }),
  VerificationResult: object({
    hostId: text,
    observationRefs: strings(1),
    verificationClass: choice("current-runtime", "deterministic-contract", "historical-receipt"),
    verifiedAtUtc: timestamp,
    status: choice("passed", "failed", "unverified"),
    criteria: array(verificationCriterionSchema, 1),
    adapterHealth: choice("healthy", "degraded", "unverified"),
    limitations: strings(),
  }),
  AcceptanceDecision: object({
    hostId: text,
    verificationRefs: strings(1),
    status: choice("accepted", "rejected", "deferred", "superseded"),
    decidedAtUtc: timestamp,
    authority: nullable(authoritySchema),
    rationale: text,
    conditions: strings(),
  }),
  OperationRequest: object({
    hostId: text,
    kind: choice(
      "inspect",
      "native-gateway-start",
      "native-gateway-stop",
      "native-gateway-restart",
    ),
    target: object({
      containerId,
      service: text,
      processIdentity: nullable(processIdentitySchema),
      lockIdentity: nullable(lockIdentitySchema),
    }),
    preserveProcesses: array(processIdentitySchema),
    scope: { const: "infrastructure" },
    reason: text,
    status: choice("proposed", "authorized", "executed", "cancelled"),
    authority: nullable(authoritySchema),
    requestedAtUtc: timestamp,
    idempotencyKey: text,
    constraints: object({
      revalidateProcessIdentity: { const: true },
      exactLockInodeRequired: bool,
      installedNativeLifecycle: { const: true },
      broadContainerRestart: { const: false },
      destructiveDeletion: { const: false },
      exposeCredentials: { const: false },
    }),
  }),
};
