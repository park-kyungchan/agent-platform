/**
 * Generated from schemas/operational.schema.json; do not edit.
 * json-schema-to-typescript 16.0.0
 * Input SHA-256: 6bf05713aff0406ecc51ca08d7bed5c84897909600476eabf083636c1ad928f1
 */
/**
 * Versioned local Ontology contracts. Cardinality on an endpoint counts linked opposite endpoints per object of this endpoint's type. These contracts do not establish authority or transact external systems.
 */
export type OperationalRecord = ObjectTypeDefinition | LinkTypeDefinition | ActionTypeDefinition | OntologyRegistry | ObjectRecord | LinkRecord | ActionRequest | ActionExecution;
export type JsonValue = null | boolean | number | string | JsonValue[] | JsonObject;
export interface ObjectTypeDefinition {
    schemaVersion: "1.0.0";
    kind: "object-type";
    id: string;
    version: string;
    name: string;
    domainId: string;
    identity: IdentityDefinition;
    properties: PropertyDefinition[];
    additionalProperties: boolean;
    description?: string;
    propertySchemaRef?: string;
}
export interface IdentityDefinition {
    property: "id";
    strategy: "provided" | "uuid" | "content-hash" | "composite";
    immutable: true;
    prefix?: string;
    /**
     * @minItems 1
     */
    components?: [string, ...string[]];
}
export interface PropertyDefinition {
    name: string;
    valueType: "string" | "number" | "integer" | "boolean" | "object" | "array" | "null" | "json";
    required: boolean;
    immutable?: boolean;
    description?: string;
    format?: string;
    /**
     * @minItems 1
     */
    enum?: [JsonValue, ...JsonValue[]];
    reference?: PropertyReference;
}
export interface JsonObject {
    [k: string]: JsonValue;
}
export interface PropertyReference {
    objectType: string;
    many: boolean;
}
export interface LinkTypeDefinition {
    schemaVersion: "1.0.0";
    kind: "link-type";
    id: string;
    version: string;
    name: string;
    from: LinkEndpointDefinition;
    to: LinkEndpointDefinition;
    description?: string;
}
export interface LinkEndpointDefinition {
    objectType: string;
    cardinality: LinkCardinality;
}
export interface LinkCardinality {
    min: number;
    max: number | null;
}
export interface ActionTypeDefinition {
    schemaVersion: "1.0.0";
    kind: "action-type";
    id: string;
    version: string;
    name: string;
    inputSchema: JsonObject;
    preconditions: ActionPrecondition[];
    handler: ActionHandler;
    idempotency: ActionIdempotency;
    effects: ActionEffects;
    description?: string;
}
export interface ActionPrecondition {
    kind: "version-match" | "object-exists" | "custom";
    inputPath?: string;
    objectType?: string;
    handler?: string;
    description?: string;
}
export interface ActionHandler {
    id: string;
    version: string;
    scope: "local";
}
export interface ActionIdempotency {
    required: true;
    keyScope: "action-type-and-project";
}
export interface ActionEffects {
    objectTypes: string[];
    linkTypes: string[];
}
export interface OntologyRegistry {
    schemaVersion: "1.0.0";
    id: string;
    version: string;
    /**
     * @minItems 1
     */
    objectTypes: [ObjectTypeDefinition, ...ObjectTypeDefinition[]];
    linkTypes: LinkTypeDefinition[];
    actionTypes: ActionTypeDefinition[];
}
export interface ObjectRecord {
    schemaVersion: "1.0.0";
    kind: "object";
    id: string;
    objectType: string;
    objectTypeVersion: string;
    version: number;
    projectId: string;
    scope: "project" | "shared";
    createdAt: string;
    updatedAt: string;
    properties: JsonObject;
}
export interface LinkRecord {
    schemaVersion: "1.0.0";
    kind: "link";
    id: string;
    linkType: string;
    linkTypeVersion: string;
    version: number;
    from: string;
    to: string;
    projectId: string;
    scope: "project" | "shared";
    createdAt: string;
}
export interface ActionRequest {
    schemaVersion: "1.0.0";
    kind: "action-request";
    id: string;
    actionType: string;
    actionTypeVersion: string;
    projectId: string;
    idempotencyKey: string;
    input: JsonObject;
    expectedVersions: ActionExpectedVersion[];
    simulate: boolean;
    createdAt: string;
}
export interface ActionExpectedVersion {
    objectId: string;
    version: number;
}
export interface ActionExecution {
    schemaVersion: "1.0.0";
    kind: "action-execution";
    id: string;
    requestId: string;
    actionType: string;
    actionTypeVersion: string;
    handlerVersion: string;
    projectId: string;
    idempotencyKey: string;
    outcome: "success" | "failure" | "simulated";
    startedAt: string;
    finishedAt: string;
    committedBatchId?: string;
    result: JsonValue;
    diagnostics: ActionDiagnostic[];
}
export interface ActionDiagnostic {
    code: string;
    message: string;
    objectId?: string;
}
