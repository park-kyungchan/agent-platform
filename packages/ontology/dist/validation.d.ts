import type { Artifact } from "./generated/contracts.js";
export interface ValidationIssue {
    code: string;
    path: string;
    message: string;
}
export type ValidationResult<T = Artifact> = {
    valid: true;
    value: T;
    issues: [];
} | {
    valid: false;
    issues: ValidationIssue[];
};
export interface ReferenceValidationResult {
    valid: boolean;
    issues: ValidationIssue[];
}
/** Validate unknown JSON before narrowing it to generated schema types. Never mutates input. */
export declare function validateArtifact(value: unknown): ValidationResult;
export declare function assertArtifact(value: unknown): asserts value is Artifact;
/** Resolve a closed record set. External records must be supplied explicitly. Hashes are syntax-checked, not authenticated. */
export declare function validateReferences(records: readonly unknown[]): ReferenceValidationResult;
