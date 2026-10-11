import type { ActionRequest, ActionTypeDefinition, JsonObject, LinkRecord, ObjectRecord, ObjectTypeDefinition, OntologyRegistry, OperationalRecord } from "./generated/operational.js";
import type { ReferenceValidationResult, ValidationResult } from "./validation.js";
export declare function validateOperationalRecord(value: unknown): ValidationResult<OperationalRecord>;
export declare function assertOperationalRecord(value: unknown): asserts value is OperationalRecord;
export declare function validateObjectRecord(value: unknown): ValidationResult<ObjectRecord>;
export declare function assertObjectRecord(value: unknown): asserts value is ObjectRecord;
export declare function validateLinkRecord(value: unknown): ValidationResult<LinkRecord>;
export declare function assertLinkRecord(value: unknown): asserts value is LinkRecord;
export declare function validateOntologyRegistry(value: unknown): ValidationResult<OntologyRegistry>;
export declare function assertOntologyRegistry(value: unknown): asserts value is OntologyRegistry;
/** Shape validation for a registered payload; registry callers still own actual authorization and observed truth. */
export declare function validateObjectProperties(value: unknown, definition: ObjectTypeDefinition): ValidationResult<JsonObject>;
export declare function validateActionInput(value: unknown, definition: ActionTypeDefinition): ValidationResult<JsonObject>;
export declare function validateActionRequest(value: unknown, definition?: ActionTypeDefinition): ValidationResult<ActionRequest>;
export declare function assertActionRequest(value: unknown, definition?: ActionTypeDefinition): asserts value is ActionRequest;
/** Validate one complete current graph. History revisions belong in a separate immutable store. */
export declare function validateOntologyGraph(objects: readonly unknown[], links: readonly unknown[], registry: OntologyRegistry): ReferenceValidationResult;
