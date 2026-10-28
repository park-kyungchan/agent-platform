import { reviewArtifactTarget } from "../quality/review-artifact";
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

test("the actual author CLI boundary reports its refusal location", async () => {
  const child = Bun.spawn(
    [process.execPath, "--no-env-file", `${import.meta.dir}/source-author-cli.ts`],
    { env: { PATH: process.env.PATH }, stdout: "pipe", stderr: "pipe" },
  );
  const [exitCode, stderr, stdout] = await Promise.all([
    child.exited,
    new Response(child.stderr).text(),
    new Response(child.stdout).text(),
  ]);
  expect(exitCode).toBe(1);
  expect(stdout).toBe("");
  expect(stderr.trim()).toMatch(/^source_author_refused@ci\/reuse\/source-author-cli\.ts:\d+$/);
});

test("native publisher diagnostics retain actual admitted quality throw sites", () => {
  try {
    reviewArtifactTarget("https://foreign.invalid/", "fixture/owner");
    throw Error("expected refusal");
  } catch (error) {
    expect(locatedError(error, "review_artifact_refused")).toMatch(
      /^review_artifact_refused@ci\/quality\/review-artifact\.ts:[1-9][0-9]*$/,
    );
  }
});
