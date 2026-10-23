import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { operationalTimeouts } from "./operational-timeouts";
import { fixedProcess } from "./trusted/process";
import { validateRequest } from "../../packages/research-capture/src/capture";

const root = resolve(import.meta.dir, "../..");

test("command children keep scaled defaults and exact overrides without replay", async () => {
  // Isolate the timer interception from the test runner. Only the child's clock is accelerated.
  const script = `
import { fixedProcess } from ${JSON.stringify(resolve(import.meta.dir, "trusted/process.ts"))};
import { runNativeCommand } from ${JSON.stringify(resolve(import.meta.dir, "native-tools.ts"))};
const original = globalThis.setTimeout;
let selected;
globalThis.setTimeout = (callback, milliseconds, ...args) => {
  selected = milliseconds;
  return original(callback, 50, ...args);
};
const argv = [process.execPath, "--no-env-file", "--no-install", "-e", "setInterval(() => {}, 1000)"];
const results = [];
for (const timeoutMs of [undefined, 225000, 750000]) {
  const result = await fixedProcess(argv, { cwd: "/tmp", ...(timeoutMs === undefined ? {} : { timeoutMs }) });
  results.push({ kind: "fixed", selected, timedOut: result.timedOut, failed: result.code !== 0 });
}
for (const timeoutMs of [undefined, 225000, 750000]) {
  const result = await runNativeCommand("timeout-fixture", argv, "/tmp", timeoutMs);
  results.push({ kind: "native", selected, timedOut: result.timedOut, failed: result.exitCode !== 0 });
}
globalThis.setTimeout = original;
console.log(JSON.stringify(results));
`;
  const result = await fixedProcess(
    [process.execPath, "--no-env-file", "--no-install", "-e", script],
    { cwd: "/tmp", timeoutMs: 10_000, maxBytes: 16384 },
  );
  expect(result.timedOut).toBe(false);
  expect(result.code).toBe(0);
  expect(result.stderr.toString()).toBe("");
  expect(JSON.parse(result.stdout.toString())).toEqual(
    [600_000, 225_000, 750_000, 450_000, 225_000, 750_000].map((selected, index) => ({
      kind: index < 3 ? "fixed" : "native",
      selected,
      timedOut: true,
      failed: true,
    })),
  );
});

test("enclosing operational limits outlast individual required inner checks", async () => {
  const sourceWorkflow = Bun.YAML.parse(
    await readFile(resolve(root, ".github/workflows/source.yml"), "utf8"),
  ) as { jobs: Record<string, { "timeout-minutes": number }> };
  const trustedWorkflow = Bun.YAML.parse(
    await readFile(resolve(root, ".github/workflows/trusted-source.yml"), "utf8"),
  ) as { jobs: Record<string, { "timeout-minutes": number }> };
  expect(sourceWorkflow.jobs["native-review"]!["timeout-minutes"]).toBe(15);
  expect(operationalTimeouts.reviewPublicationMs).toBe(300_000);
  expect(operationalTimeouts.qualityFixturesMs).toBeGreaterThan(3_000_000);
  expect(sourceWorkflow.jobs.quality!["timeout-minutes"] * 60_000).toBeGreaterThan(
    operationalTimeouts.qualityFixturesMs,
  );
  expect(sourceWorkflow.jobs["merge-integration"]!["timeout-minutes"] * 60_000).toBeGreaterThan(
    operationalTimeouts.qualityFixturesMs,
  );
  expect(operationalTimeouts.workerMs).toBeGreaterThan(operationalTimeouts.frozenInstallMs);
  expect(operationalTimeouts.workerMs).toBeGreaterThan(operationalTimeouts.qualityFixturesMs);
  expect(trustedWorkflow.jobs["trusted-source"]!["timeout-minutes"] * 60_000).toBeGreaterThan(
    operationalTimeouts.workerMs,
  );
  expect(operationalTimeouts.nativeCommandMs).toBeGreaterThan(
    operationalTimeouts.secretScanSeconds * 1000,
  );
});

test("research accepts the scaled range and preserves explicit request values", () => {
  for (const timeoutMs of [1000, 45_000, 180_000, 225_000, 900_000]) {
    const request = { projectId: "fixture", url: "https://example.invalid/", timeoutMs };
    expect(validateRequest(request)).toBe(request);
    expect(request.timeoutMs).toBe(timeoutMs);
  }
  for (const timeoutMs of [999, 900_001, 225_000.5])
    expect(() =>
      validateRequest({ projectId: "fixture", url: "https://example.invalid/", timeoutMs }),
    ).toThrow("Invalid research request");
});

test("the actual provider JSON and archive paths select a 300-second response bound", async () => {
  const script = `
import { githubReader } from ${JSON.stringify(resolve(import.meta.dir, "trusted/provider.ts"))};
const requested = [];
const original = AbortSignal.timeout;
AbortSignal.timeout = milliseconds => {
  requested.push(milliseconds);
  return original(5000);
};
let reads = 0;
globalThis.fetch = async (url, options) => {
  if (!options.signal || options.signal.aborted) throw Error("Missing live response bound");
  reads++;
  if (reads === 1) return Response.json({ fixture: true });
  if (reads === 2) return new Response(null, { status: 302,
    headers: { location: "https://fixture.blob.core.windows.net/artifact" } });
  if (reads === 3 && new URL(url).hostname === "fixture.blob.core.windows.net" &&
      !options.headers?.Authorization) return new Response(new Uint8Array([1, 2, 3]));
  throw Error("Unadmitted fixture request");
};
const reader = githubReader("fictional-test-token");
if (!(await reader.json("/fixture")).fixture || (await reader.archive(1)).length !== 3)
  throw Error("Required fixture consumer did not complete");
AbortSignal.timeout = original;
console.log(JSON.stringify({ reads, requested }));
`;
  const result = await fixedProcess(
    [process.execPath, "--no-env-file", "--no-install", "-e", script],
    { cwd: "/tmp", timeoutMs: 10_000, maxBytes: 16384 },
  );
  expect(result.code).toBe(0);
  expect(result.timedOut).toBe(false);
  expect(result.stderr.toString()).toBe("");
  expect(JSON.parse(result.stdout.toString())).toEqual({
    reads: 3,
    requested: [300_000, 300_000, 300_000],
  });
});

test("review API response bounds fit the remaining publication wait with a fractional clock", async () => {
  const script = `
import { reviewGate } from ${JSON.stringify(resolve(import.meta.dir, "../source-check.ts"))};
const original = AbortSignal.timeout;
const requested = [];
AbortSignal.timeout = milliseconds => {
  if (!Number.isSafeInteger(milliseconds) || milliseconds < 1) throw Error("Invalid timer bound");
  requested.push(milliseconds);
  return original(5000);
};
let clock = 0, reads = 0;
globalThis.fetch = async () => {
  reads++;
  if (reads === 1) { clock = 294999.75; return Response.json([]); }
  return Response.json([{ id: 1, context: "native-independent-review", state: "pending" }]);
};
let held = false;
try {
  await reviewGate({
    now: () => clock, delay: async ms => { clock += ms; },
    env: { GITHUB_REPOSITORY: "fixture/source", REVIEW_HEAD_SHA: "a".repeat(40),
      GITHUB_TOKEN: "fictional-test-token", NATIVE_REVIEW_STATUS_CREATOR_ID: "123",
      GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "push", GITHUB_REF: "refs/heads/main",
      GITHUB_REF_PROTECTED: "true", GITHUB_SHA: "a".repeat(40) }
  });
} catch (error) { held = String(error).includes("unsuccessful or from unexpected creator"); }
AbortSignal.timeout = original;
console.log(JSON.stringify({ reads, requested, held }));
`;
  const result = await fixedProcess(
    [process.execPath, "--no-env-file", "--no-install", "-e", script],
    { cwd: "/tmp", timeoutMs: 10_000, maxBytes: 16384 },
  );
  expect(result.code).toBe(0);
  expect(result.timedOut).toBe(false);
  expect(result.stderr.toString()).toBe("");
  const finalLine = result.stdout.toString().trim().split("\n").at(-1)!;
  expect(JSON.parse(finalLine)).toEqual({ reads: 2, requested: [300_000, 1], held: true });
});
