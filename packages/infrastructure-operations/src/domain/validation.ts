import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import type { ValidateFunction } from "ajv";
import { validateObjectRecord, validateObjectProperties } from "@palantirkc/ontology/operational";
import type { ValidationResult, ValidationIssue } from "@palantirkc/ontology";
import { domainRegistry, domainObjectType, objectTypeId } from "./registry";
import { domainPropertySchemas, observationSchema } from "./schemas";
import { DOMAIN_VERSION, PROJECT_ID } from "./types";
import type {
  DomainObjectType, DomainProperties, DomainObjectRecord, InfrastructureObservation,
  OperationRequestProperties, AcceptanceDecisionProperties, VerificationResultProperties,
  SanitizedAdapterError,
} from "./types";

const ajv = new Ajv2020({ strict: true, allErrors: true, coerceTypes: false, useDefaults: false, removeAdditional: false });
addFormats(ajv);
const validators = new Map<DomainObjectType, ValidateFunction>();
const observationValidator = ajv.compile<InfrastructureObservation>(observationSchema);
for (const [name, schema] of Object.entries(domainPropertySchemas)) validators.set(name as DomainObjectType, ajv.compile(schema));

function shape<T>(value: unknown, validator: ValidateFunction<T>): ValidationResult<T> {
  const invalidJson = jsonIssue(value);
  if (invalidJson) return { valid: false, issues: [invalidJson] };
  if (!validator(value)) return {
    valid: false, issues: (validator.errors ?? []).map(error => ({
      code: `schema:${error.keyword}`, path: error.instancePath, message: error.message ?? "Domain schema validation failed.",
    })),
  };
  return { valid: true, value, issues: [] };
}
function jsonIssue(value: unknown, path = "", seen = new WeakSet<object>()): ValidationIssue | null {
  if (value === null || typeof value === "string" || typeof value === "boolean") return null;
  if (typeof value === "number") return Number.isFinite(value) ? null : issue("non-json-number", path, "Domain numbers must be finite JSON values.");
  if (typeof value !== "object") return issue("non-json-value", path, "Domain records contain JSON values only.");
  if (seen.has(value)) return issue("cyclic-json-value", path, "Domain records cannot contain cycles.");
  if (Object.getOwnPropertySymbols(value).length) return issue("non-json-key", path, "Domain records cannot contain symbol keys.");
  if (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) return issue("non-json-object", path, "Domain objects must be plain JSON objects.");
  seen.add(value);
  for (const [key, nested] of Object.entries(value)) {
    const invalid = jsonIssue(nested, `${path}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`, seen);
    if (invalid) return invalid;
  }
  if (Array.isArray(value) && Object.keys(value).length !== value.length) return issue("sparse-json-array", path, "Domain arrays cannot contain holes or custom properties.");
  seen.delete(value);
  return null;
}
function result<T>(value: T, issues: ValidationIssue[]): ValidationResult<T> {
  return issues.length ? { valid: false, issues } : { valid: true, value, issues: [] };
}
function requireValid<T>(checked: ValidationResult<T>): T {
  if (!checked.valid) throw new Error("Invalid infrastructure record: " + checked.issues.map(issue => `${issue.path} ${issue.code}: ${issue.message}`).join("; "));
  return checked.value;
}
const issue = (code: string, path: string, message: string): ValidationIssue => ({ code, path, message });

function duplicateIssues(values: readonly string[], path: string): ValidationIssue[] {
  return new Set(values).size === values.length ? [] : [issue("duplicate-identity", path, "Domain identities in this collection must be unique.")];
}
const credentialPattern = /\bBearer\s+[A-Za-z0-9._~+/=-]{8,}|\bsk-[A-Za-z0-9_-]{10,}|\b(?:password|token|secret|api[_-]?key)\s*[:=]\s*(?!<redacted>|\[redacted\])[A-Za-z0-9._~+/=-]{8,}/i;
function errorIssues(errors: readonly SanitizedAdapterError[], path: string): ValidationIssue[] {
  return errors.flatMap((error, index) => credentialPattern.test(error.message)
    ? [issue("credential-shaped-content", `${path}/${index}/message`, "Adapter messages must be sanitized before persistence.")] : []);
}

export function validateObservation(value: unknown): ValidationResult<InfrastructureObservation> {
  const checked = shape(value, observationValidator);
  if (!checked.valid) return checked;
  const data = checked.value;
  const issues = duplicateIssues(data.runtimes.map(runtime => runtime.containerId), "/runtimes");
  data.runtimes.forEach((runtime, index) => {
    issues.push(...duplicateIssues((runtime.native.profiles ?? []).map(profile => profile.profile), `/runtimes/${index}/native/profiles`));
    issues.push(...errorIssues(runtime.native.adapterErrors ?? [], `/runtimes/${index}/native/adapterErrors`));
    if (runtime.native.error && credentialPattern.test(runtime.native.error)) issues.push(issue("credential-shaped-content", `/runtimes/${index}/native/error`, "Probe errors must be sanitized before persistence."));
  });
  if (data.inventory) issues.push(...duplicateIssues(data.inventory.containers.map(container => container.containerId), "/inventory/containers"));
  // Observed disagreements (for example On without matching identity) remain evidence.
  // They are evaluated separately instead of being erased by shape validation.
  return result(data, issues);
}
export function assertObservation(value: unknown): asserts value is InfrastructureObservation { requireValid(validateObservation(value)); }

export function validateOperationRequest(value: unknown): ValidationResult<OperationRequestProperties> {
  const checked = shape<OperationRequestProperties>(value, validators.get("OperationRequest") as ValidateFunction<OperationRequestProperties>);
  if (!checked.valid) return checked;
  const request = checked.value;
  const issues: ValidationIssue[] = [];
  if (["authorized", "executed"].includes(request.status) && !request.authority) issues.push(issue("missing-authority", "/authority", "Authorized or executed requests require a scoped authority declaration; its presence does not prove actual permission."));
  if (request.kind === "native-gateway-stop" || request.kind === "native-gateway-restart") {
    const process = request.target.processIdentity;
    const lock = request.target.lockIdentity;
    if (!process) issues.push(issue("missing-process-incarnation", "/target/processIdentity", "Stop/restart requests require the exact process incarnation."));
    if (!lock || !request.constraints.exactLockInodeRequired) issues.push(issue("missing-exact-lock", "/target/lockIdentity", "Stop/restart requests require exact lock device/inode evidence."));
    if (process && process.containerId !== request.target.containerId) issues.push(issue("target-process-container", "/target/processIdentity/containerId", "Process incarnation must belong to the targeted container."));
    if (process && lock) {
      const holder = lock.holders.find(holder => holder.pid === process.hostPid && holder.localPid === process.localPid && holder.startTicks === process.startTicks && holder.namespace === process.pidNamespace && holder.containerId === process.containerId && holder.matchingFdCount > 0);
      if (!lock.held || lock.holders.length !== 1 || !holder) issues.push(issue("unproven-exact-holder", "/target/lockIdentity", "A singleton held lock must identify this exact process incarnation."));
    }
    if (process && request.preserveProcesses.some(preserved => preserved.hostPid === process.hostPid && preserved.startTicks === process.startTicks && preserved.pidNamespace === process.pidNamespace)) issues.push(issue("preserved-process-targeted", "/preserveProcesses", "A preserved conversation process cannot be the stop/restart target."));
  }
  return result(request, issues);
}
export function assertOperationRequest(value: unknown): asserts value is OperationRequestProperties { requireValid(validateOperationRequest(value)); }

export function validateDomainProperties<K extends DomainObjectType>(name: K, value: unknown): ValidationResult<DomainProperties[K]> {
  if (name === "OperationRequest") return validateOperationRequest(value) as ValidationResult<DomainProperties[K]>;
  const checked = shape<DomainProperties[K]>(value, validators.get(name) as ValidateFunction<DomainProperties[K]>);
  if (!checked.valid) return checked;
  const issues: ValidationIssue[] = [];
  if (name === "Observation") {
    const props = checked.value as DomainProperties["Observation"];
    const nested = validateObservation(props.observation);
    if (!nested.valid) issues.push(...nested.issues.map(item => ({ ...item, path: "/observation" + item.path })));
    if (props.observedAtUtc !== props.observation.observedAtUtc) issues.push(issue("observation-time-mismatch", "/observedAtUtc", "Observation wrapper time must match the preserved raw snapshot."));
  }
  if (name === "GatewayRuntime") {
    const props = checked.value as DomainProperties["GatewayRuntime"];
    issues.push(...duplicateIssues(props.profiles.map(profile => profile.profile), "/profiles"));
    issues.push(...errorIssues(props.adapterErrors, "/adapterErrors"));
  }
  if (name === "VerificationResult") {
    const props = checked.value as VerificationResultProperties;
    issues.push(...duplicateIssues(props.criteria.map(criterion => criterion.id), "/criteria"));
    for (const [index, criterion] of props.criteria.entries()) {
      if (criterion.status !== "unverified" && !criterion.observationRefs.length) issues.push(issue("unsupported-criterion", `/criteria/${index}/observationRefs`, "Pass/fail criteria require observation references."));
      if (criterion.observationRefs.some(ref => !props.observationRefs.includes(ref))) issues.push(issue("undeclared-observation", `/criteria/${index}/observationRefs`, "Criterion observations must be declared by the verification result."));
    }
    if (props.status === "passed" && props.criteria.some(criterion => criterion.status !== "pass")) issues.push(issue("contradictory-verification", "/status", "A passed verification requires every declared criterion to pass."));
    if (props.status === "failed" && !props.criteria.some(criterion => criterion.status === "fail")) issues.push(issue("missing-failure-criterion", "/status", "A failed verification requires an observed failed criterion."));
  }
  if (name === "AcceptanceDecision") {
    const props = checked.value as AcceptanceDecisionProperties;
    if (props.status === "accepted" && !props.authority) issues.push(issue("missing-authority", "/authority", "Accepted decisions require scoped authority; validation does not grant it."));
  }
  return result(checked.value, issues);
}
export function assertDomainProperties<K extends DomainObjectType>(name: K, value: unknown): asserts value is DomainProperties[K] { requireValid(validateDomainProperties(name, value)); }

export function validateDomainObject(value: unknown): ValidationResult<DomainObjectRecord> {
  const envelope = validateObjectRecord(value);
  if (!envelope.valid) return envelope as ValidationResult<DomainObjectRecord>;
  const name = domainObjectType(envelope.value.objectType);
  if (!name) return { valid: false, issues: [issue("unknown-infrastructure-object", "/objectType", "Object type is not registered in this infrastructure domain.")] };
  const issues: ValidationIssue[] = [];
  if (envelope.value.objectTypeVersion !== DOMAIN_VERSION) issues.push(issue("domain-version-mismatch", "/objectTypeVersion", "Infrastructure object records pin the domain contract version."));
  const definition = domainRegistry.objectTypes.find(definition => definition.id === envelope.value.objectType)!;
  const upperProperties = validateObjectProperties(envelope.value.properties, definition);
  if (!upperProperties.valid) issues.push(...upperProperties.issues.map(item => ({ ...item, path: "/properties" + item.path })));
  const properties = validateDomainProperties(name, envelope.value.properties);
  if (!properties.valid) issues.push(...properties.issues.map(item => ({ ...item, path: "/properties" + item.path })));
  return result(envelope.value as DomainObjectRecord, issues);
}
export function assertDomainObject(value: unknown): asserts value is DomainObjectRecord { requireValid(validateDomainObject(value)); }

export interface CreateDomainObjectOptions {
  projectId?: string;
  scope?: "project" | "shared";
  createdAt?: string;
  updatedAt?: string;
  version?: number;
}
export function createDomainObject<K extends DomainObjectType>(name: K, id: string, properties: DomainProperties[K], options: CreateDomainObjectOptions = {}): DomainObjectRecord<K> {
  assertDomainProperties(name, properties);
  const createdAt = options.createdAt ?? new Date().toISOString();
  const record = {
    schemaVersion: "1.0.0" as const, kind: "object" as const, id,
    objectType: objectTypeId(name), objectTypeVersion: DOMAIN_VERSION,
    version: options.version ?? 1, projectId: options.projectId ?? PROJECT_ID,
    scope: options.scope ?? "project", createdAt, updatedAt: options.updatedAt ?? createdAt,
    properties: structuredClone(properties),
  };
  assertDomainObject(record);
  return record as DomainObjectRecord<K>;
}
