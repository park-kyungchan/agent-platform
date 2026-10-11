import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { assertObjectRecord, validateActionRequest, validateObjectRecord, validateOntologyGraph, validateOntologyRegistry, validateOperationalRecord } from "../src/operational.js";
import type { ActionRequest, LinkRecord, ObjectRecord, OntologyRegistry } from "../src/operational.js";
import { evidenceObjectTypes, evidenceRegistry, validateEvidenceGraph, validateEvidenceObject } from "../src/domains/evidence.js";

const time = "2026-10-06T00:00:00Z";
function object(id: string, objectType: string, properties: ObjectRecord["properties"]): ObjectRecord {
  return { schemaVersion: "1.0.0", kind: "object", id, objectType, objectTypeVersion: "1.0.0", version: 1, projectId: "pilot", scope: "project", createdAt: time, updatedAt: time, properties };
}
function link(id: string, linkType: string, from: string, to: string): LinkRecord {
  return { schemaVersion: "1.0.0", kind: "link", id, linkType, linkTypeVersion: "1.0.0", version: 1, projectId: "pilot", scope: "project", createdAt: time, from, to };
}
const project = object("pilot:project", "Project", { name: "Pilot", domainId: "research" });
const capture = object("pilot:capture", "SourceCapture", { url: "https://example.test/404", retrievedAt: time, outcome: "failure", artifacts: [], metadata: { status: 404, missing: ["wire-bytes"] } });
const unit = object("pilot:unit", "VerificationUnit", { objective: "The snapshot contains expected content", scope: "one saved snapshot", inputSnapshot: { text: "observed" }, expectedPredicate: { operator: "contains", value: "expected" }, method: "deterministic" });
const dataset = object("pilot:dataset", "EvalDataset", { caseRefs: [unit.id] });
const evaluator = object("pilot:evaluator", "Evaluator", { method: "deterministic" });
const suite = object("pilot:suite", "EvalSuite", { datasetRef: dataset.id, target: { kind: "snapshot" }, evaluatorRefs: [evaluator.id] });
const run = object("pilot:run", "EvalRun", { suiteRef: suite.id, status: "completed" });
const caseResult = object("pilot:case", "CaseResult", { runRef: run.id, unitRef: unit.id, evaluatorRef: evaluator.id, verdict: "refuted", verificationClass: "deterministic-verified", passed: false, status: "completed", metrics: [{ name: "contentMatched", value: false, objectiveVersion: "1.0.0", thresholdVersion: "1.0.0", threshold: true, passed: false }] });
const evalGraph = [unit, dataset, evaluator, suite, run, caseResult];

describe("operational upper contracts", () => {
  test("the original artifact schema and immutable 0.1.0 release retain exact bytes", async () => {
    const expected: Record<string, string> = {
      "schemas/artifact.schema.json": "614f7deb0b9d7717ab8a992f144955e05875c012716001d4aa0a96edbce92ba8",
      "releases/palantirkc-ontology-0.1.0.tgz": "650385dfebbeb0c78711ac5be0f4762ffd8a19aefc79e06be505c841d4db3add",
      "releases/palantirkc-ontology-0.1.0.json": "797d72d74663d4bb5395a016935e90cb41296768796d0316427c6e51aa93d6d5",
    };
    for (const [path, hash] of Object.entries(expected)) expect(createHash("sha256").update(await readFile(path)).digest("hex")).toBe(hash);
  });
  test("ten domain entity definitions exclude utility functions and validate immutably", () => {
    expect(evidenceObjectTypes).toHaveLength(10);
    expect(validateOntologyRegistry(evidenceRegistry).valid).toBe(true);
    expect(Object.isFrozen(evidenceRegistry.objectTypes[0]!.properties)).toBe(true);
    expect(evidenceObjectTypes).not.toContain("Parser");
  });
  test("envelopes require versioned plain JSON and chronological timestamps", () => {
    assertObjectRecord(project);
    expect(validateObjectRecord({ ...project, properties: { value: NaN } }).valid).toBe(false);
    expect(validateObjectRecord({ ...project, version: 0 }).valid).toBe(false);
    expect(validateObjectRecord({ ...project, updatedAt: "2026-10-05T00:00:00Z" }).valid).toBe(false);
    expect(validateObjectRecord({ ...project, objectTypeVersion: "current" }).valid).toBe(false);
  });
  test("registry checks duplicate declarations, identity, endpoint, and handler consistency", () => {
    const bad = structuredClone(evidenceRegistry) as OntologyRegistry;
    bad.objectTypes[0]!.properties.push({ ...bad.objectTypes[0]!.properties[0]! });
    bad.objectTypes[0]!.identity = { property: "id", strategy: "composite", immutable: true, components: ["name"] };
    bad.linkTypes[0]!.from.objectType = "Unregistered";
    bad.linkTypes[0]!.from.cardinality = { min: 2, max: 1 };
    bad.actionTypes[0]!.preconditions = [{ kind: "custom" }];
    expect(validateOntologyRegistry(bad).valid).toBe(false);
  });
  test("graph rejects dangling links, wrong endpoint types, duplicate edges and cardinality", () => {
    expect(validateOntologyGraph([project, capture], [link("p-c", "project-capture", project.id, capture.id)], evidenceRegistry).valid).toBe(true);
    expect(validateOntologyGraph([project], [link("missing", "project-capture", project.id, "missing")], evidenceRegistry).valid).toBe(false);
    expect(validateOntologyGraph([project, capture], [link("wrong", "project-capture", capture.id, project.id)], evidenceRegistry).valid).toBe(false);
    expect(validateOntologyGraph([project, capture], [link("a", "project-capture", project.id, capture.id), link("b", "project-capture", project.id, capture.id)], evidenceRegistry).valid).toBe(false);
    const secondProject = { ...project, id: "pilot:project:second" };
    const graph = validateOntologyGraph([project, secondProject, capture], [link("a", "project-capture", project.id, capture.id), link("b", "project-capture", secondProject.id, capture.id)], evidenceRegistry);
    expect(graph.issues.some(issue => issue.code === "cardinality")).toBe(true);
  });
  test("registered payload references and definition revisions resolve within a closed graph", () => {
    expect(validateOntologyGraph(evalGraph, [], evidenceRegistry).valid).toBe(true);
    expect(validateOntologyGraph(evalGraph.filter(record => record.id !== unit.id), [], evidenceRegistry).issues.some(issue => issue.code === "dangling-reference")).toBe(true);
    expect(validateOntologyGraph([{ ...project, objectTypeVersion: "2.0.0" }], [], evidenceRegistry).issues.some(issue => issue.code === "unknown-object-type-version")).toBe(true);
  });
  test("project scope permits shared references and rejects another project's private objects", () => {
    const external = { ...capture, projectId: "other" };
    expect(validateOntologyGraph([project, external], [link("cross", "project-capture", project.id, capture.id)], evidenceRegistry).valid).toBe(false);
    expect(validateOntologyGraph([project, { ...external, scope: "shared" }], [link("shared", "project-capture", project.id, capture.id)], evidenceRegistry).valid).toBe(true);
  });
  test("Action requests bind an idempotency key, input schema, and one revision per precondition", () => {
    const definition = evidenceRegistry.actionTypes.find(type => type.id === "putObject")!;
    const request: ActionRequest = { schemaVersion: "1.0.0", kind: "action-request", id: "action:a", actionType: definition.id, actionTypeVersion: definition.version, projectId: "pilot", idempotencyKey: "unique", input: { object: project as unknown as ObjectRecord["properties"] }, expectedVersions: [{ objectId: project.id, version: 1 }], simulate: true, createdAt: time };
    expect(validateActionRequest(request, definition).valid).toBe(true);
    expect(validateActionRequest({ ...request, idempotencyKey: "" }, definition).valid).toBe(false);
    expect(validateActionRequest({ ...request, input: {} }, definition).valid).toBe(false);
    expect(validateActionRequest({ ...request, expectedVersions: [request.expectedVersions[0], request.expectedVersions[0]] }, definition).valid).toBe(false);
    expect(validateActionRequest({ ...request, actionTypeVersion: "2.0.0" }, definition).valid).toBe(false);
  });
  test("simulation and failed executions cannot declare a mutation commit", () => {
    const execution = { schemaVersion: "1.0.0", kind: "action-execution", id: "execution:a", requestId: "action:a", actionType: "putObject", actionTypeVersion: "1.0.0", handlerVersion: "1.0.0", projectId: "pilot", idempotencyKey: "unique", outcome: "simulated", startedAt: time, finishedAt: time, result: null, diagnostics: [] };
    expect(validateOperationalRecord(execution).valid).toBe(true);
    expect(validateOperationalRecord({ ...execution, committedBatchId: "commit:bad" }).valid).toBe(false);
    expect(validateOperationalRecord({ ...execution, outcome: "failure" }).valid).toBe(false);
  });
});

describe("evidence domain contracts", () => {
  test("failed capture can retain no available bytes without fabricated HTTP metadata", () => {
    expect(validateEvidenceObject(capture).valid).toBe(true);
    const corrupt = { ...capture, properties: { ...capture.properties, artifacts: [{ kind: "response-content", sha256: "invalid", byteLength: 1 }] } };
    expect(validateEvidenceObject(corrupt).valid).toBe(false);
  });
  test("metric failure, refuted verdict, and verification class remain independent", () => {
    expect(validateEvidenceGraph(evalGraph, []).valid).toBe(true);
    expect(caseResult.properties.verdict).toBe("refuted");
    expect(caseResult.properties.verificationClass).toBe("deterministic-verified");
    expect(caseResult.properties.passed).toBe(false);
  });
  test("eval error and exceeded budget provide insufficient evidence and cannot pass", () => {
    for (const status of ["error", "budget-exceeded"]) {
      expect(validateEvidenceObject({ ...caseResult, properties: { ...caseResult.properties, status, verdict: "supported", passed: true } }).valid).toBe(false);
      expect(validateEvidenceObject({ ...caseResult, properties: { ...caseResult.properties, status, verdict: "insufficient", passed: null } }).valid).toBe(true);
    }
  });
  test("model judgment cannot become deterministic verification by changing a label", () => {
    const model = { ...evaluator, properties: { method: "native-codex" } };
    const checked = validateEvidenceGraph(evalGraph.map(record => record.id === evaluator.id ? model : record), []);
    expect(checked.issues.some(issue => issue.code === "model-promotion")).toBe(true);
    const judged = { ...caseResult, properties: { ...caseResult.properties, verificationClass: "model-judged" } };
    expect(validateEvidenceGraph(evalGraph.map(record => record.id === evaluator.id ? model : record.id === caseResult.id ? judged : record), []).valid).toBe(true);
  });
  test("acceptance decisions require separate subject, verification references, scoped actor, and choice", () => {
    const decision = object("pilot:decision", "Decision", { subjectRef: unit.id, status: "accepted", rationale: "Scoped pilot authorization", verificationRefs: [caseResult.id], authority: { actorId: "local-owner", scope: "pilot" }, choice: "retain observed result" });
    expect(validateEvidenceGraph([...evalGraph, decision], []).valid).toBe(true);
    expect(validateEvidenceObject({ ...decision, properties: { ...decision.properties, authority: {} } }).valid).toBe(false);
    expect(validateEvidenceGraph([...evalGraph, { ...decision, properties: { ...decision.properties, subjectRef: "missing" } }], []).valid).toBe(false);
  });
});
