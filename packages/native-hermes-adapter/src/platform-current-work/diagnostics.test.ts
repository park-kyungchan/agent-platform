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
