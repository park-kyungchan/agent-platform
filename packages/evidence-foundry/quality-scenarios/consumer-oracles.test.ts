import { expect, test } from "bun:test";
import {
  decodeCompactGitWorkContext,
  readGitWorkContext,
  validateGitWorkRecords,
} from "@palantirkc/work-continuity/git-work";
import {
  fixture,
  objective,
  nextAction,
  clause,
  reference,
  task,
  put,
  commit,
} from "./fixtures/work.ts";
import { codex, hermes } from "./fixtures/consumers.ts";

function codexEnvelope(stdout: string) {
  const text = JSON.parse(stdout).hookSpecificOutput.additionalContext as string;
  return { text, selected: JSON.parse(text.slice(text.indexOf("\n") + 1)).selectedWork };
}
const expectedDecisions = ["decision-0", "decision-1", "decision-2", "decision-3", "decision-4"];
function assertMeaning(compact: Parameters<typeof decodeCompactGitWorkContext>[0]) {
  const decoded = decodeCompactGitWorkContext(compact);
  expect(decoded.normative.find((r) => r.kind === "work")?.purpose).toMatchObject({ objective });
  expect(
    decoded.normative
      .filter((r) => r.kind === "spec")
      .map((r) => r.revision)
      .sort(),
  ).toEqual([1, 2]);
  expect(
    decoded.normative
      .filter((r) => r.kind === "plan")
      .map((r) => r.revision)
      .sort(),
  ).toEqual([1, 2]);
  for (const plan of decoded.normative.filter((r) => r.kind === "plan"))
    expect(plan.clauses).toEqual([clause]);
  expect(decoded.task?.logical).toMatchObject({
    specRevision: 1,
    planRevision: 1,
    taskId: "review",
  });
  expect(decoded.task?.nextAction?.value).toBe(nextAction);
  expect(decoded.decisions.map((r) => r.id)).toEqual(expectedDecisions);
  return decoded;
}

test("PT-QC-01/02: exact historical meaning and deduplicated selection survive both Codex events and Hermes", async () => {
  const f = await fixture();
  try {
    expect((await validateGitWorkRecords(await f.open())).findings).toEqual([]);
    const c = await codex(f.root, f.oid);
    for (const event of ["SessionStart", "UserPromptSubmit"] as const) {
      const result = await c.read(event);
      expect(result.status).toBe("context-injected");
      const envelope = codexEnvelope(result.stdout);
      expect(envelope.text.length).toBeLessThanOrEqual(12000);
      expect(envelope.selected.status).toBe("source-qualified");
      const decoded = assertMeaning(envelope.selected);
      const reader = await f.open();
      for (let i = 0; i < 5; i++) {
        const original = await reader.readSource(reference(`original-${i}`));
        expect(original.value).toEqual({
          answer: `Exact answer ${i}`,
          question: `Exact question ${i}`,
        });
        expect(
          decoded.sources.some(
            (s) => s.path.endsWith(`/original-${i}.json`) && s.blobOid === original.blobOid,
          ),
        ).toBe(true);
      }
      expect(result.stdout).not.toContain("PRIVATE-UNRELATED-PROMPT");
      // A SELECTED/fit-only oracle would miss these contained payload corruptions.
      const damaged = structuredClone(envelope.selected);
      damaged.strings[damaged.strings.indexOf(nextAction)] = "Inspect patch";
      expect(() => assertMeaning(damaged)).toThrow();
      const missingOriginal = structuredClone(envelope.selected);
      // Canonical decoder still validates metadata; removing a selected decision violates our independently fixed order.
      missingOriginal.decisions.pop();
      expect(() => assertMeaning(missingOriginal)).toThrow();
    }
    expect(c.checks()).toBeGreaterThanOrEqual(4);
    const h = hermes(f.root, f.oid),
      result = await h.read();
    expect(result.status).toBe("source-qualified");
    expect(JSON.stringify(result).length).toBeLessThanOrEqual(12000);
    if (!result.context) throw Error("Hermes complete consumer context required");
    assertMeaning(result.context.work);
    expect(h.checks()).toBeGreaterThanOrEqual(2);
    expect(result).toMatchObject({
      authority: "none",
      nativePermissionGranted: false,
      nativeAuthorshipVerified: false,
      semanticAcceptanceEstablished: false,
    });
    const tiny = await h.read(1000);
    expect(tiny.status).toBe("held");
    expect(tiny.context).toBeNull();
  } finally {
    await f.cleanup();
  }
}, 30000);

test("PT-QC-02: unsupported explicit Decision still holds rather than losing the valid-input control", async () => {
  const f = await fixture();
  try {
    const c = await codex(f.root, f.oid, "review", "missing-decision");
    for (const event of ["SessionStart", "UserPromptSubmit"] as const) {
      const result = await c.read(event);
      expect(result.status).toBe("context-held");
      expect(result.stdout).not.toContain(nextAction);
    }
  } finally {
    await f.cleanup();
  }
}, 30000);

test("PT-QC-03: absent prior pane does not prevent useful independent recovery, but consumed Task IDs remain held", async () => {
  const f = await fixture();
  try {
    const reader = await f.open();
    const recovered = await readGitWorkContext(reader, {
      taskId: "review",
      latestRelevant: "current",
    });
    expect(recovered.status).toBe("source-qualified");
    expect(recovered.historicalEffects).toMatchObject({ completedCount: 68, uncertainCount: 3 });
    const delegated = await reader.readSource(reference("delegated-result"));
    expect(delegated.sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(delegated.value).toMatchObject({
      oldPane: null,
      summary: "Independent review identified a source-boundary issue",
    });
    for (const id of ["published", "uncertain"]) {
      await put(f.root, task(id));
      const consumedOid = await commit(f.root);
      const context = await readGitWorkContext(await f.open(consumedOid), {
        taskId: id,
        latestRelevant: "current",
      });
      expect(context.status).toBe("held");
      const c = await codex(f.root, consumedOid, id);
      expect((await c.read("SessionStart")).status).toBe("context-held");
      expect((await hermes(f.root, consumedOid, id).read()).status).toBe("held");
    }
    // The useful source action actually runs: recover and verify all five exact original Q/A bindings.
    for (let i = 0; i < 5; i++)
      expect((await reader.readSource(reference(`original-${i}`, "#/content/answer"))).value).toBe(
        `Exact answer ${i}`,
      );
  } finally {
    await f.cleanup();
  }
}, 30000);

test("PT-QC-02: distinct explicit Decision joins Task originals in authored order on both Codex events", async () => {
  const f = await fixture();
  try {
    const source = f.rows.find((r) => r.kind === "decision" && r.id === "decision-3");
    if (!source || source.kind !== "decision") throw Error("Independent Decision fixture required");
    await put(f.root, {
      ...source,
      id: "extra-decision",
      meaning: "An independent explicit decision retains its separate identity",
    });
    const oid = await commit(f.root);
    const c = await codex(f.root, oid, "review", "extra-decision");
    for (const event of ["SessionStart", "UserPromptSubmit"] as const) {
      const result = await c.read(event);
      expect(result.status).toBe("context-injected");
      const decoded = decodeCompactGitWorkContext(codexEnvelope(result.stdout).selected);
      expect(decoded.decisions.map((d) => d.id)).toEqual([
        "decision-0",
        "decision-1",
        "decision-2",
        "decision-3",
        "extra-decision",
        "decision-4",
      ]);
      expect(decoded.task?.logical).toMatchObject({ specRevision: 1, planRevision: 1 });
      expect(decoded.task?.nextAction?.value).toBe(nextAction);
    }
  } finally {
    await f.cleanup();
  }
}, 30000);
