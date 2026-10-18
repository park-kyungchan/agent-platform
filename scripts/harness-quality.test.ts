import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { qualityFixture } from "../packages/evidence-foundry/tests/fixtures/quality";

const directories: string[] = [];
afterEach(async () => {
  await Promise.all(
    directories.splice(0).map((path) => rm(path, { recursive: true, force: true })),
  );
});
async function invoke(command: string, input: unknown) {
  const directory = await mkdtemp(join(tmpdir(), "harness-quality-"));
  directories.push(directory);
  const request = join(directory, "request.json");
  await writeFile(request, JSON.stringify(input));
  const child = Bun.spawn(
    [Bun.which("bun")!, "--no-env-file", join(import.meta.dir, "harness.ts"), command, request],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  return { stdout, stderr, exitCode };
}
test("quality CLI preflights, compares producer-contract results, and never accepts or grants authority", async () => {
  const request = qualityFixture();
  const preflight = await invoke("quality-preflight", request.protocol);
  expect(preflight.exitCode).toBe(0);
  expect(preflight.stderr).toBe("");
  expect(JSON.parse(preflight.stdout).status).toBe("ready-for-comparison");
  const result = await invoke("quality-compare", request);
  expect(result.exitCode).toBe(0);
  expect(JSON.parse(result.stdout)).toMatchObject({
    status: "ready-for-review",
    outcome: "behavior-gain",
    acceptanceEstablished: false,
    nativePermissionGranted: false,
    actualExposureAuthenticated: false,
  });
});
test.each(["defer", "repair", "stop"])(
  "CLI emits explicit %s JSON and nonzero exit",
  async (status) => {
    const r = qualityFixture();
    if (status === "defer") r.usage.totalTokens = null;
    if (status === "repair")
      Object.assign(r.candidate.results[0]!.properties, { passed: false, verdict: "refuted" });
    if (status === "stop") r.boundaries.effects = "breached";
    const result = await invoke("quality-compare", r);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toBe("");
    expect(JSON.parse(result.stdout).status).toBe(status);
  },
);
test("quality CLI preserves STOP for explicit breach with malformed observations", async () => {
  const r = qualityFixture();
  r.boundaries.authority = "breached";
  const result = await invoke("quality-compare", { ...r, candidate: null, extra: true });
  expect(result.exitCode).toBe(1);
  expect(JSON.parse(result.stdout).status).toBe("stop");
});
test("quality CLI bounds request bytes and rejects unknown protocol fields", async () => {
  const large = await invoke("quality-preflight", { x: "x".repeat(1_048_576) });
  expect(large.exitCode).toBe(1);
  expect(large.stderr).toContain("Request exceeds the bounded input size");
  const r = await invoke("quality-preflight", {
    ...qualityFixture().protocol,
    overridePermission: true,
  });
  expect(r.exitCode).toBe(1);
  expect(JSON.parse(r.stdout).status).toBe("defer");
});
