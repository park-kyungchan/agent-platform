import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import type { ValidateFunction } from "ajv";
import schema from "../schemas/operational.schema.json" with { type: "json" };
import type { ActionRequest, ActionTypeDefinition, JsonObject, LinkRecord, ObjectRecord, ObjectTypeDefinition, OntologyRegistry, OperationalRecord } from "./generated/operational.js";
import type { ReferenceValidationResult, ValidationIssue, ValidationResult } from "./validation.js";
import { jsonIssue } from "./json-validation.js";

const ajv = new Ajv2020({ strict: true, allErrors: true, coerceTypes: false, useDefaults: false, removeAdditional: false });
addFormats(ajv);
ajv.addSchema(schema);
const validator = <T>(definition: string) => ajv.compile<T>({ $ref: `${schema.$id}#/$defs/${definition}` });
const shape = ajv.compile<OperationalRecord>(schema);
const objectShape = validator<ObjectRecord>("ObjectRecord");
const linkShape = validator<LinkRecord>("LinkRecord");
const registryShape = validator<OntologyRegistry>("OntologyRegistry");
const requestShape = validator<ActionRequest>("ActionRequest");

function check<T>(value: unknown, validate: ValidateFunction<T>): ValidationResult<T> {
  const invalidJson = jsonIssue(value);
  if (invalidJson) return { valid: false, issues: [invalidJson] };
  if (!validate(value)) return { valid: false, issues: (validate.errors ?? []).map(error => ({ code: `schema:${error.keyword}`, path: error.instancePath, message: error.message ?? "Schema validation failed." })) };
  return { valid: true, value, issues: [] };
}
function result<T>(value: T, issues: ValidationIssue[]): ValidationResult<T> {
  return issues.length ? { valid: false, issues } : { valid: true, value, issues: [] };
}
function assertResult<T>(result: ValidationResult<T>): asserts result is Extract<ValidationResult<T>, { valid: true }> {
  if (!result.valid) throw new Error(result.issues.map(issue => `${issue.path} ${issue.code}: ${issue.message}`).join("; "));
}
export function validateOperationalRecord(value: unknown): ValidationResult<OperationalRecord> {
  const checked = check(value, shape);
  if (!checked.valid) return checked;
  const record = checked.value;
  if ("objectTypes" in record) return validateOntologyRegistry(record);
  if (record.kind === "object") return validateObjectRecord(record);
  if (record.kind === "action-request") return validateActionRequest(record);
  if (record.kind === "action-execution") {
    const issues: ValidationIssue[] = [];
    if (Date.parse(record.finishedAt) < Date.parse(record.startedAt)) issues.push({ code: "negative-action-duration", path: "/finishedAt", message: "Finish time cannot precede start time." });
    if (record.outcome !== "success" && record.committedBatchId) issues.push({ code: "invalid-action-commit", path: "/committedBatchId", message: "Failed or simulated Actions cannot declare a mutation commit." });
    if (record.outcome === "failure" && !record.diagnostics.length) issues.push({ code: "missing-action-failure", path: "/diagnostics", message: "Failed Actions require observed diagnostics." });
    return result(record, issues);
  }
  return checked;
}
export function assertOperationalRecord(value: unknown): asserts value is OperationalRecord { assertResult(validateOperationalRecord(value)); }
export function validateObjectRecord(value: unknown): ValidationResult<ObjectRecord> {
  const checked = check(value, objectShape);
  if (!checked.valid) return checked;
  const issues: ValidationIssue[] = [];
  if (Date.parse(checked.value.updatedAt) < Date.parse(checked.value.createdAt)) issues.push({ code: "negative-object-duration", path: "/updatedAt", message: "Updated time cannot precede creation time." });
  return result(checked.value, issues);
}
export function assertObjectRecord(value: unknown): asserts value is ObjectRecord { assertResult(validateObjectRecord(value)); }
export function validateLinkRecord(value: unknown): ValidationResult<LinkRecord> { return check(value, linkShape); }
export function assertLinkRecord(value: unknown): asserts value is LinkRecord { assertResult(validateLinkRecord(value)); }

export function validateOntologyRegistry(value: unknown): ValidationResult<OntologyRegistry> {
  const checked = check(value, registryShape);
  if (!checked.valid) return checked;
  const registry = checked.value;
  const issues: ValidationIssue[] = [];
  const issue = (code: string, path: string, message: string) => issues.push({ code, path, message });
  const objectIds = new Set(registry.objectTypes.map(type => type.id));
  const linkIds = new Set(registry.linkTypes.map(type => type.id));
  for (const [field, definitions] of [["objectTypes", registry.objectTypes], ["linkTypes", registry.linkTypes], ["actionTypes", registry.actionTypes]] as const) {
    const keys = new Set<string>();
    definitions.forEach((definition, index) => {
      const key = `${definition.id}@${definition.version}`;
      if (keys.has(key)) issue("duplicate-definition", `/${field}/${index}`, `Duplicate definition ${key}.`);
      keys.add(key);
    });
  }
  registry.objectTypes.forEach((type, index) => {
    const keys = new Set<string>();
    type.properties.forEach((property, propertyIndex) => {
      const path = `/objectTypes/${index}/properties/${propertyIndex}`;
      if (keys.has(property.name)) issue("duplicate-property", `${path}/name`, `Duplicate property ${property.name}.`);
      keys.add(property.name);
      if (property.reference && !objectIds.has(property.reference.objectType)) issue("unknown-reference-type", `${path}/reference/objectType`, "A property reference endpoint must be registered.");
      if (property.reference && property.valueType !== (property.reference.many ? "array" : "string")) issue("reference-shape", path, "Reference properties must use string identity or an array of identities.");
    });
    if (type.identity.strategy === "composite") {
      if (!type.identity.components?.length) issue("missing-identity-components", `/objectTypes/${index}/identity`, "Composite identity requires components.");
      for (const name of type.identity.components ?? []) {
        const property = type.properties.find(property => property.name === name);
        if (!property?.required || !property.immutable) issue("unstable-identity-component", `/objectTypes/${index}/identity/components`, "Composite identity components must be declared, required, and immutable.");
      }
    }
  });
  registry.linkTypes.forEach((type, index) => {
    for (const endpoint of ["from", "to"] as const) {
      const definition = type[endpoint];
      if (!objectIds.has(definition.objectType)) issue("unknown-endpoint-type", `/linkTypes/${index}/${endpoint}`, "Link endpoints must name registered object types.");
      if (definition.cardinality.max !== null && definition.cardinality.min > definition.cardinality.max) issue("invalid-cardinality", `/linkTypes/${index}/${endpoint}/cardinality`, "Minimum cardinality cannot exceed maximum cardinality.");
    }
  });
  registry.actionTypes.forEach((type, index) => {
    for (const target of type.effects.objectTypes) if (!objectIds.has(target)) issue("unknown-action-object-type", `/actionTypes/${index}/effects`, `Unknown object effect ${target}.`);
    for (const target of type.effects.linkTypes) if (!linkIds.has(target)) issue("unknown-action-link-type", `/actionTypes/${index}/effects`, `Unknown link effect ${target}.`);
    type.preconditions.forEach((precondition, position) => {
      if (precondition.kind === "custom" && !precondition.handler) issue("missing-precondition-handler", `/actionTypes/${index}/preconditions/${position}`, "Custom preconditions require a stable handler ID.");
      if (precondition.objectType && !objectIds.has(precondition.objectType)) issue("unknown-precondition-type", `/actionTypes/${index}/preconditions/${position}`, "Preconditions must name registered object types.");
    });
    try { ajv.compile(type.inputSchema); } catch (error) { issue("invalid-action-input-schema", `/actionTypes/${index}/inputSchema`, String(error)); }
  });
  return result(registry, issues);
}
export function assertOntologyRegistry(value: unknown): asserts value is OntologyRegistry { assertResult(validateOntologyRegistry(value)); }

/** Shape validation for a registered payload; registry callers still own actual authorization and observed truth. */
export function validateObjectProperties(value: unknown, definition: ObjectTypeDefinition): ValidationResult<JsonObject> {
  const properties: Record<string, Record<string, unknown>> = {};
  for (const property of definition.properties) {
    const field: Record<string, unknown> = property.valueType === "json" ? {} : { type: property.valueType };
    if (property.format) field.format = property.format;
    if (property.enum) field.enum = property.enum;
    if (property.reference?.many) field.items = { type: "string", minLength: 1 };
    if (property.reference && !property.reference.many) field.minLength = 1;
    properties[property.name] = field;
  }
  try {
    const validate = ajv.compile<JsonObject>({ type: "object", properties, required: definition.properties.filter(property => property.required).map(property => property.name), additionalProperties: definition.additionalProperties });
    return check(value, validate);
  } catch (error) { return { valid: false, issues: [{ code: "invalid-object-definition", path: "", message: String(error) }] }; }
}
export function validateActionInput(value: unknown, definition: ActionTypeDefinition): ValidationResult<JsonObject> {
  try { return check(value, ajv.compile<JsonObject>(definition.inputSchema)); }
  catch (error) { return { valid: false, issues: [{ code: "invalid-action-input-schema", path: "", message: String(error) }] }; }
}
export function validateActionRequest(value: unknown, definition?: ActionTypeDefinition): ValidationResult<ActionRequest> {
  const checked = check(value, requestShape);
  if (!checked.valid) return checked;
  const issues: ValidationIssue[] = [];
  const keys = new Set<string>();
  checked.value.expectedVersions.forEach((expected, index) => {
    if (keys.has(expected.objectId)) issues.push({ code: "duplicate-version-precondition", path: `/expectedVersions/${index}`, message: "Each object has one expected version per Action." });
    keys.add(expected.objectId);
  });
  if (definition) {
    if (definition.id !== checked.value.actionType || definition.version !== checked.value.actionTypeVersion) issues.push({ code: "action-definition-version", path: "/actionTypeVersion", message: "Request must target the supplied Action type version." });
    const input = validateActionInput(checked.value.input, definition);
    if (!input.valid) issues.push(...input.issues.map(issue => ({ ...issue, path: `/input${issue.path}` })));
  }
  return result(checked.value, issues);
}
export function assertActionRequest(value: unknown, definition?: ActionTypeDefinition): asserts value is ActionRequest { assertResult(validateActionRequest(value, definition)); }

/** Validate one complete current graph. History revisions belong in a separate immutable store. */
export function validateOntologyGraph(objects: readonly unknown[], links: readonly unknown[], registry: OntologyRegistry): ReferenceValidationResult {
  const registryValidation = validateOntologyRegistry(registry);
  if (!registryValidation.valid) return { valid: false, issues: registryValidation.issues };
  const issues: ValidationIssue[] = [];
  const issue = (code: string, path: string, message: string) => issues.push({ code, path, message });
  const objectMap = new Map<string, ObjectRecord>();
  objects.forEach((unknownObject, index) => {
    const checked = validateObjectRecord(unknownObject);
    const path = `/objects/${index}`;
    if (!checked.valid) { issues.push(...checked.issues.map(item => ({ ...item, path: `${path}${item.path}` }))); return; }
    const value = checked.value;
    if (objectMap.has(value.id)) issue("duplicate-object-id", `${path}/id`, `Duplicate object ${value.id}.`);
    else objectMap.set(value.id, value);
    const type = registry.objectTypes.find(type => type.id === value.objectType && type.version === value.objectTypeVersion);
    if (!type) issue("unknown-object-type-version", `${path}/objectTypeVersion`, `No registered definition for ${value.objectType}@${value.objectTypeVersion}.`);
    else {
      const properties = validateObjectProperties(value.properties, type);
      if (!properties.valid) issues.push(...properties.issues.map(item => ({ ...item, path: `${path}/properties${item.path}` })));
    }
  });
  const linkKeys = new Set<string>();
  const linkIds = new Set<string>();
  const counts = new Map<string, Set<string>>();
  links.forEach((unknownLink, index) => {
    const checked = validateLinkRecord(unknownLink);
    const path = `/links/${index}`;
    if (!checked.valid) { issues.push(...checked.issues.map(item => ({ ...item, path: `${path}${item.path}` }))); return; }
    const value = checked.value;
    if (linkIds.has(value.id)) issue("duplicate-link-id", `${path}/id`, `Duplicate link ${value.id}.`);
    linkIds.add(value.id);
    const type = registry.linkTypes.find(type => type.id === value.linkType && type.version === value.linkTypeVersion);
    if (!type) { issue("unknown-link-type-version", `${path}/linkTypeVersion`, `No registered definition for ${value.linkType}@${value.linkTypeVersion}.`); return; }
    const key = `${value.linkType}@${value.linkTypeVersion}:${value.from}:${value.to}`;
    if (linkKeys.has(key)) issue("duplicate-link-endpoints", path, "One link identity represents an endpoint pair per Link type.");
    linkKeys.add(key);
    for (const endpoint of ["from", "to"] as const) {
      const target = objectMap.get(value[endpoint]);
      if (!target) issue("dangling-reference", `${path}/${endpoint}`, `Missing ${endpoint} object ${value[endpoint]}.`);
      else if (target.objectType !== type[endpoint].objectType) issue("endpoint-type", `${path}/${endpoint}`, `Expected ${type[endpoint].objectType}, found ${target.objectType}.`);
      else if (value.scope === "project" && target.scope === "project" && value.projectId !== target.projectId) issue("cross-project-link", `${path}/${endpoint}`, "A project-scoped Link cannot reach another project's private object.");
      const countKey = `${value.linkType}@${value.linkTypeVersion}:${endpoint}:${value[endpoint]}`;
      const targets = counts.get(countKey) ?? new Set<string>();
      targets.add(value[endpoint === "from" ? "to" : "from"]);
      counts.set(countKey, targets);
    }
  });
  for (const object of objectMap.values()) {
    const type = registry.objectTypes.find(type => type.id === object.objectType && type.version === object.objectTypeVersion);
    for (const property of type?.properties ?? []) {
      if (!property.reference) continue;
      const value = object.properties[property.name];
      if (value === undefined) continue;
      const targets = property.reference.many && Array.isArray(value) ? value : [value];
      for (const id of targets) {
        const target = typeof id === "string" ? objectMap.get(id) : undefined;
        const path = `/objects/${object.id}/properties/${property.name}`;
        if (!target) issue("dangling-reference", path, `Missing referenced object ${String(id)}.`);
        else if (target.objectType !== property.reference.objectType) issue("reference-type", path, `Expected ${property.reference.objectType}, found ${target.objectType}.`);
        else if (object.scope === "project" && target.scope === "project" && object.projectId !== target.projectId) issue("cross-project-reference", path, "Project-scoped references cannot reach another project's private object.");
      }
    }
    for (const linkType of registry.linkTypes) for (const endpoint of ["from", "to"] as const) {
      if (linkType[endpoint].objectType !== object.objectType) continue;
      const count = counts.get(`${linkType.id}@${linkType.version}:${endpoint}:${object.id}`)?.size ?? 0;
      const cardinality = linkType[endpoint].cardinality;
      if (count < cardinality.min || (cardinality.max !== null && count > cardinality.max)) issue("cardinality", `/objects/${object.id}/links/${linkType.id}`, `${endpoint} endpoint count ${count} violates [${cardinality.min}, ${cardinality.max ?? "unbounded"}].`);
    }
  }
  return { valid: issues.length === 0, issues };
}
