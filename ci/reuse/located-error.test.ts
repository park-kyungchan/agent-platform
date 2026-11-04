import { expect, test } from "bun:test";
import { locatedError } from "./located-error";

test("boundary diagnostics admit source locations without printing error messages or foreign paths", () => {
  const error = new Error("synthetic-private-payload\nwith controls");
  error.stack = `Error: synthetic-private-payload\n    at foreign (/synthetic/private-source.ts:2:4)\n    at verify (${import.meta.dir}/source-author.ts:327:11)`;
  expect(locatedError(error, "source_author_refused")).toBe(
    "source_author_refused@ci/reuse/source-author.ts:327",
  );
});

test("non-errors and absent stacks locate the boundary without serializing the thrown value", () => {
  const error = new Error("synthetic-private-payload");
  error.stack = "";
  for (const value of [
    error,
    null,
    { payload: "synthetic-private-payload" },
    "synthetic-private-payload",
  ])
    expect(locatedError(value, "caller_source_refused")).toMatch(
      /^caller_source_refused@ci\/reuse\/located-error\.ts:\d+$/,
    );
});
