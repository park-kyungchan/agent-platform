import type { ValidationIssue } from "./validation.js";
/** JSON validation is ordinary infrastructure; it has no Ontology identity. */
export declare function jsonIssue(value: unknown, path?: string, parents?: WeakSet<object>): ValidationIssue | undefined;
