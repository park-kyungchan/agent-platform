import { test, expect } from "bun:test";
import {
  validateRuntimeIndex,
  selectRuntimeReference,
  type NativeReferenceScope,
} from "./runtime-index.ts";
const scope: NativeReferenceScope = {
  runtime: "native-hermes",
  profileId: "fixture-profile",
  sessionId: "fixture-session",
  agentId: null,
  nativeProjectId: null,
  nativeTaskId: null,
  nativeRunId: null,
  logical: {
    domainId: "native-harness",
    projectId: "fixture-project",
    workId: "fixture-work",
    issueId: "fixture-issue",
    specId: "fixture-spec",
    specRevision: 6,
    planId: "fixture-plan",
    planRevision: 6,
    taskId: "fixture-task",
  },
};
const entry = {
  ...scope,
  observation: {
    class: "declared" as const,
    observedAtUtc: null,
    evidenceRefs: [],
    currentness: "current" as const,
  },
  sourceOnly: true as const,
};
const index = { schema: "native-harness.runtime-index.v1", entries: [entry] };
test("runtime reference index preserves explicit unknown native IDs without inventing authentication", () => {
  const validated = validateRuntimeIndex(index);
  expect(validated.entries[0]!.nativeTaskId).toBeNull();
  const selected = selectRuntimeReference(index, scope);
  expect(selected.status).toBe("matched");
  expect(selected.nativeAuthorshipVerified).toBe(false);
  expect(selected.authority).toBe("none");
  expect(selected.nativePermissionGranted).toBe(false);
  validated.entries[0]!.logical.taskId = "changed";
  expect(entry.logical.taskId).toBe("fixture-task");
});
test("missing, foreign, stale, duplicate and unknown native selections stay held", () => {
  for (const foreign of [
    { ...scope, sessionId: null },
    { ...scope, profileId: null },
    { ...scope, agentId: "child" },
    { ...scope, nativeTaskId: "foreign" },
    { ...scope, logical: { ...scope.logical, workId: "foreign" } },
  ])
    expect(selectRuntimeReference(index, foreign).status).toBe("held");
  for (const currentness of ["stale", "unknown"])
    expect(
      selectRuntimeReference(
        { ...index, entries: [{ ...entry, observation: { ...entry.observation, currentness } }] },
        scope,
      ).status,
    ).toBe("held");
  expect(selectRuntimeReference({ ...index, entries: [entry, entry] }, scope).status).toBe("held");
});
test("observed/authenticated/behavioral evidence classes remain declarations requiring exact evidence locators", () => {
  for (const classification of ["observed", "authenticated", "behaviorally-verified"]) {
    expect(() =>
      validateRuntimeIndex({
        ...index,
        entries: [{ ...entry, observation: { ...entry.observation, class: classification } }],
      }),
    ).toThrow("EVIDENCE_REQUIRED");
    const result = selectRuntimeReference(
      {
        ...index,
        entries: [
          {
            ...entry,
            observation: {
              ...entry.observation,
              class: classification,
              observedAtUtc: "2026-10-15T00:00:00.000Z",
              evidenceRefs: ["fixture://native-observation"],
            },
          },
        ],
      },
      scope,
    );
    expect(result.status).toBe("matched");
    expect(result.nativeAuthorshipVerified).toBe(false);
  }
});
test("index refuses mirrored native payloads, oversize lists and prototype-like extra fields", () => {
  expect(() => validateRuntimeIndex({ ...index, transcript: [] })).toThrow();
  expect(() => validateRuntimeIndex({ ...index, entries: [{ ...entry, messages: [] }] })).toThrow();
  expect(() =>
    validateRuntimeIndex({ ...index, entries: Array.from({ length: 257 }, () => entry) }),
  ).toThrow();
  expect(() =>
    validateRuntimeIndex({
      ...index,
      entries: [{ ...entry, logical: { ...scope.logical, workId: "bad\n" } }],
    }),
  ).toThrow();
});
