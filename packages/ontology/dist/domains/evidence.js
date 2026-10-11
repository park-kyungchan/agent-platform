import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import schema from "../../schemas/evidence.schema.json" with { type: "json" };
import registryData from "../../ontology/domains/evidence/registry.json" with { type: "json" };
import vocabulary from "../../ontology/domains/evidence/vocabulary.json" with { type: "json" };
import { assertOntologyRegistry, validateObjectRecord, validateOntologyGraph } from "../operational-validation.js";
export const evidenceSchema = schema;
export const evidenceVocabulary = Object.freeze(vocabulary);
assertOntologyRegistry(registryData);
function freeze(value) {
    if (value !== null && typeof value === "object") {
        Object.freeze(value);
        for (const child of Object.values(value))
            freeze(child);
    }
    return value;
}
export const evidenceRegistry = freeze(registryData);
export const evidenceObjectTypes = Object.freeze(registryData.objectTypes.map(type => type.id));
const ajv = new Ajv2020({ strict: true, allErrors: true, coerceTypes: false, useDefaults: false, removeAdditional: false });
addFormats(ajv);
ajv.addSchema(schema);
const validators = new Map(evidenceObjectTypes.map(type => [type, ajv.compile({ $ref: `${schema.$id}#/$defs/${type}Properties` })]));
/** Validate domain properties without granting truth, freshness, or actual authorization. */
export function validateEvidenceObject(value) {
    const envelope = validateObjectRecord(value);
    if (!envelope.valid)
        return envelope;
    const objectType = envelope.value.objectType;
    const validate = validators.get(objectType);
    if (!validate)
        return { valid: false, issues: [{ code: "unknown-evidence-object-type", path: "/objectType", message: `Unknown evidence domain type ${objectType}.` }] };
    if (!validate(envelope.value.properties))
        return { valid: false, issues: (validate.errors ?? []).map(error => ({ code: `schema:${error.keyword}`, path: `/properties${error.instancePath}`, message: error.message ?? "Property validation failed." })) };
    const issues = [];
    const properties = envelope.value.properties;
    if (objectType === "CaseResult" && (properties.status === "error" || properties.status === "budget-exceeded")) {
        if (properties.verdict !== "insufficient")
            issues.push({ code: "invalid-error-verdict", path: "/properties/verdict", message: "An evaluator error or exceeded budget provides insufficient verification." });
        if (properties.passed === true)
            issues.push({ code: "invalid-error-pass", path: "/properties/passed", message: "An evaluator error or exceeded budget cannot pass." });
    }
    if (objectType === "EvalRun") {
        if (properties.status === "running" && properties.finishedAt !== undefined && properties.finishedAt !== null)
            issues.push({ code: "running-finished", path: "/properties/finishedAt", message: "A running eval has no finish timestamp." });
        if (typeof properties.startedAt === "string" && typeof properties.finishedAt === "string" && Date.parse(properties.finishedAt) < Date.parse(properties.startedAt))
            issues.push({ code: "negative-eval-duration", path: "/properties/finishedAt", message: "Finish time cannot precede start time." });
    }
    return issues.length ? { valid: false, issues } : { valid: true, value: envelope.value, issues: [] };
}
export function assertEvidenceObject(value) {
    const checked = validateEvidenceObject(value);
    if (!checked.valid)
        throw new Error(checked.issues.map(issue => `${issue.path} ${issue.code}: ${issue.message}`).join("; "));
}
export function validateEvidenceGraph(objects, links) {
    const checked = validateOntologyGraph(objects, links, evidenceRegistry);
    const issues = [...checked.issues];
    const records = new Map();
    objects.forEach((value, index) => {
        const domain = validateEvidenceObject(value);
        if (!domain.valid)
            issues.push(...domain.issues.map(issue => ({ ...issue, path: `/objects/${index}${issue.path}` })));
        else
            records.set(domain.value.id, domain.value);
    });
    for (const record of records.values()) {
        if (record.objectType === "CaseResult" && typeof record.properties.evaluatorRef === "string") {
            const evaluator = records.get(record.properties.evaluatorRef);
            if (evaluator && ["model-judged", "native-codex"].includes(String(evaluator.properties.method)) && record.properties.verificationClass === "deterministic-verified") {
                issues.push({ code: "model-promotion", path: `/objects/${record.id}/properties/verificationClass`, message: "Model judgment cannot be promoted to deterministic verification." });
            }
        }
        if (record.objectType === "Decision" && typeof record.properties.subjectRef === "string" && !records.has(record.properties.subjectRef))
            issues.push({ code: "dangling-reference", path: `/objects/${record.id}/properties/subjectRef`, message: "The decision's subject must be supplied to closed-set validation." });
    }
    return { valid: issues.length === 0, issues };
}
