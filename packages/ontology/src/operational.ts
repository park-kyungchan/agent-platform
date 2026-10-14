export type * from "./generated/operational.js";
export { default as operationalSchema } from "../schemas/operational.schema.json" with { type: "json" };
export {
  validateOperationalRecord,
  assertOperationalRecord,
  validateObjectRecord,
  assertObjectRecord,
  validateLinkRecord,
  assertLinkRecord,
  validateOntologyRegistry,
  assertOntologyRegistry,
  validateObjectProperties,
  validateActionInput,
  validateActionRequest,
  assertActionRequest,
  validateOntologyGraph,
} from "./operational-validation.js";
export type { ValidationIssue, ValidationResult, ReferenceValidationResult } from "./validation.js";
