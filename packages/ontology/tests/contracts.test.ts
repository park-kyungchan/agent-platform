import { describe, expect, test } from "bun:test";
import { readFile, readdir } from "node:fs/promises";
import { validateArtifact, validateReferences } from "../src/index.js";
import type { Artifact } from "../src/contracts.js";

const fixturePath = new URL("./fixtures/valid-set.json", import.meta.url);
const rawFixtures: unknown[] = JSON.parse(await readFile(fixturePath, "utf8"));
const invalidFixtureNames = await readdir(new URL("./fixtures/invalid", import.meta.url));
const fixtures = rawFixtures.map(raw => {
  const result = validateArtifact(raw);
  if (!result.valid) throw new Error(JSON.stringify(result.issues));
  return result.value;
});
const fixture = <K extends Artifact["kind"]>(kind: K): Extract<Artifact, { kind: K }> => {
  const found = fixtures.find(record => record.kind === kind);
  if (!found) throw new Error(`Missing fixture ${kind}.`);
  // Fixtures are already runtime-validated above; the predicate proves the literal kind.
  return structuredClone(found) as Extract<Artifact, { kind: K }>;
};
const codes = (records: unknown[]) => validateReferences(records).issues.map(issue => issue.code);

describe("canonical JSON shape", () => {
  for (const record of fixtures) test(`valid ${record.kind}`, () => expect(validateArtifact(record).valid).toBe(true));
  test("valid closed fixture references", () => expect(validateReferences(fixtures)).toEqual({ valid: true, issues: [] }));
  for (const filename of invalidFixtureNames) {
    test(`rejects ${filename}`, async () => {
      const invalid: unknown = JSON.parse(await readFile(new URL(`./fixtures/invalid/${filename}`, import.meta.url), "utf8"));
      expect(validateArtifact(invalid).valid).toBe(false);
    });
  }
  test("retains domain details without accepting undeclared payload fields", () => {
    const record = fixture("evidence");
    record.payload.details = { selector: "#content", matches: [1, true, null, { text: "measured" }] };
    expect(validateArtifact(record).valid).toBe(true);
    expect(validateArtifact({ ...record, payload: { ...record.payload, unexpected: true } }).valid).toBe(false);
  });
  test("rejects non-JSON input before schema recursion", () => {
    const record = fixture("evidence");
    expect(validateArtifact({ ...record, payload: { ...record.payload, details: { count: NaN } } }).valid).toBe(false);
    expect(validateArtifact({ ...record, payload: { ...record.payload, details: { action: () => true } } }).valid).toBe(false);
    const cycle: Record<string, unknown> = {};
    cycle.self = cycle;
    expect(validateArtifact({ ...record, payload: { ...record.payload, details: cycle } }).valid).toBe(false);
  });
  test("does not mutate input", () => {
    const record = fixture("evidence");
    const before = JSON.stringify(record);
    validateArtifact(record);
    expect(JSON.stringify(record)).toBe(before);
  });
});

describe("reference and lifecycle checks", () => {
  test("rejects dangling evidence", () => expect(codes(fixtures.filter(record => record.kind !== "evidence"))).toContain("dangling-reference"));
  test("requires authority evidence to match actor and scope", () => {
    const decision = fixture("decision");
    if (!decision.payload.authority) throw new Error("Fixture authority missing.");
    decision.payload.authority.scope = "different-project";
    expect(codes(fixtures.map(record => record.kind === "decision" ? decision : record))).toContain("authority-scope-mismatch");
  });
  test("does not treat process success with content mismatch as a passing criterion", () => {
    const evidence = fixture("evidence");
    evidence.payload.result.contentMatched = false;
    expect(validateArtifact(evidence).valid).toBe(true);
    expect(codes(fixtures.map(record => record.kind === "evidence" ? evidence : record))).toContain("inapplicable-pass-evidence");
  });
  test("retains failures followed by a successful retry", () => {
    const evidence = fixture("evidence");
    evidence.payload.attempts = [{ attempt: 1, outcome: "failure", error: "Connection timeout." }, { attempt: 2, outcome: "success" }];
    expect(validateArtifact(evidence).valid).toBe(true);
    evidence.payload.attempts.pop();
    expect(validateArtifact(evidence).valid).toBe(false);
  });
  test("an unverified criterion cannot complete a task", () => {
    const verification = fixture("verification");
    verification.payload.criteria[0]!.status = "unverified";
    expect(codes(fixtures.map(record => record.kind === "verification" ? verification : record))).toContain("incomplete-task-verification");
  });
  test("rejects duplicate artifact IDs", () => expect(codes([...fixtures, fixture("evidence")])).toContain("duplicate-id"));
  test("rejects cycles only for declared supersedes relations", () => {
    const first = fixture("evidence");
    const second = fixture("evidence");
    second.id = "ontology-foundation:evidence:second";
    first.references = [{ artifactId: second.id, relation: "supersedes" }];
    second.references = [{ artifactId: first.id, relation: "supersedes" }];
    expect(codes([first, second])).toContain("supersedes-cycle");
    first.references[0]!.relation = "supports";
    second.references[0]!.relation = "supports";
    expect(validateReferences([first, second]).valid).toBe(true);
  });
});
