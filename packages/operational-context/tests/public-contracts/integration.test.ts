import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { parseScope, parseSource } from "../../src/backend/intent-context/contract.ts";
import { resolveContext } from "../../src/backend/intent-context/resolve.ts";
import { contextFixture } from "./context-fixture.ts";

test("canonical decoder refuses discriminant accessors without invoking caller code", () => {
  for (const parse of [parseScope, parseSource]) {
    let reads = 0;
    const input = Object.defineProperty({}, "kind", {
      get() {
        reads++;
        throw Error("caller code");
      },
    });
    expect(() => parse(input)).toThrow("INVALID_DATA");
    expect(reads).toBe(0);
  }
});

test("native clarification response slots retain distinct source identity in one retained row", () => {
  const fixture = contextFixture();
  const content = JSON.stringify({
    responses: [
      { question: "Synthetic first direction?", user_response: "Use the initial contract." },
      { question: "Synthetic corrective direction?", user_response: "Use the corrected contract." },
    ],
  });
  const source = {
    ...fixture.candidate.sources[0]!,
    kind: "clarification",
    toolCallId: "synthetic-call",
    responseIndex: 0,
    contentSha256: createHash("sha256").update(content).digest("hex"),
  };
  const candidate = {
    ...fixture.candidate,
    sources: [source, { ...source, id: "slot-two", responseIndex: 1 }],
    decisions: [
      fixture.candidate.decisions[0]!,
      { ...fixture.candidate.decisions[0]!, revision: 2, sourceIds: ["slot-two"], supersedes: [1] },
    ],
  };
  const rows = [
    {
      ...fixture.rows[0]!,
      role: "tool",
      tool_name: "clarify",
      tool_call_id: "synthetic-call",
      content,
    },
  ];
  const result = resolveContext(candidate, fixture.request, rows);
  expect(result.status).toBe("RESOLVED");
  expect(result.decisions.map((decision) => decision.revision)).toEqual([2]);
  expect(result.sources.map((item) => item.responseIndex)).toEqual([0, 1]);
  expect(result.authority).toBe("NO_EXECUTION_AUTHORITY");
});

test("copied clarification rows cannot manufacture a new response occurrence", () => {
  const fixture = contextFixture();
  const content = JSON.stringify({
    responses: [{ question: "Synthetic direction?", user_response: "Yes" }],
  });
  const source = {
    ...fixture.candidate.sources[0]!,
    kind: "clarification",
    toolCallId: "synthetic-call",
    responseIndex: 0,
    contentSha256: createHash("sha256").update(content).digest("hex"),
  };
  const candidate = {
    ...fixture.candidate,
    sources: [source, { ...source, id: "copied-row", messageId: 2 }],
    decisions: [
      fixture.candidate.decisions[0]!,
      {
        ...fixture.candidate.decisions[0]!,
        revision: 2,
        sourceIds: ["copied-row"],
        supersedes: [1],
      },
    ],
  };
  const row = {
    ...fixture.rows[0]!,
    role: "tool",
    tool_name: "clarify",
    tool_call_id: "synthetic-call",
    content,
  };
  const result = resolveContext(candidate, fixture.request, [row, { ...row, id: 2 }]);
  expect(result.status).toBe("REJECTED");
  expect(result.gaps).toContainEqual({ code: "SUPERSESSION_NEEDS_NEW_SOURCE", subject: "input" });
});
