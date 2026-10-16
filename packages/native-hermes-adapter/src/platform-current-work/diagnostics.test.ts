import { expect, test } from "bun:test";
import { join } from "node:path";

test("unregistered entrypoints preserve fail-closed protocol and report registration without private input", async () => {
  const marker = "PRIVATE_SYNTHETIC_UNREGISTERED_INPUT";
  for (const [entry, payload, code, stdout, error] of [
    [
      "native-hook.ts",
      { hook_event_name: "pre_llm_call", user_message: marker },
      0,
      "{}\n",
      "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD",
    ],
    [
      "read-current-work.ts",
      { event: { private: marker } },
      1,
      "",
      "CURRENT_WORK_INPUT_OR_SOURCE_FAILED",
    ],
  ] as const) {
    const child = Bun.spawn([process.execPath, "--no-env-file", join(import.meta.dir, entry)], {
      stdin: new Blob([JSON.stringify(payload)]),
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exitCode, output, diagnostic] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(exitCode).toBe(code);
    expect(output).toBe(stdout);
    expect(JSON.parse(diagnostic)).toEqual({
      error,
      stage: "registration",
      reason: "EXPLICIT_REGISTRATION_REQUIRED",
    });
    expect(output + diagnostic).not.toContain(marker);
    expect(output + diagnostic).not.toContain(import.meta.dir);
  }
});

test("output version rejects nonliteral values before registration without echoing them", async () => {
  for (const outputVersion of [["v2"], null, "PRIVATE_SYNTHETIC_VERSION"]) {
    const child = Bun.spawn(
      [process.execPath, "--no-env-file", join(import.meta.dir, "read-current-work.ts")],
      {
        stdin: new Blob([JSON.stringify({ outputVersion })]),
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const [code, out, diagnostic] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(code).toBe(1);
    expect(out).toBe("");
    expect(JSON.parse(diagnostic)).toEqual({
      error: "CURRENT_WORK_INPUT_OR_SOURCE_FAILED",
      stage: "input",
      reason: "REQUEST_UNAVAILABLE_OR_INVALID",
    });
    expect(diagnostic).not.toContain("PRIVATE_SYNTHETIC_VERSION");
  }
});
