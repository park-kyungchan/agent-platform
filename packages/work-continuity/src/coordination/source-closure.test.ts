import { test, expect } from "bun:test";
import { sourceHash } from "./source-contract.ts";

test.each([
  "loader-alias",
  "loader-namespace",
  "loader-prefix",
  "loader-precache",
  "loader-comment-cr",
  "loader-comment-cr-precache",
  "loader-comment-ls",
  "loader-comment-ls-precache",
  "loader-comment-ps",
  "loader-comment-ps-precache",
  "positive-static-comment-lf",
  "positive-static-comment-crlf",
  "reflection",
  "vm-alias",
  "free-global",
  "positive-static",
  "positive-dependency",
  "positive-tool",
  "dirty-orphan",
  "dirty-declared",
  "foreign-root",
  "overlap-root",
  "wrong-installed",
  "orphan-tool",
  "dirty-tool",
])(
  "fresh real Git/private source occurrence: %s",
  async (occurrence) => {
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--no-macros",
        "--no-addons",
        new URL("./fixtures/source-correction-worker.ts", import.meta.url).pathname,
        occurrence,
      ],
      {
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
        env: { PATH: "/usr/local/bin:/usr/bin:/bin", HOME: "/tmp" },
      },
    );
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(code, sourceHash(stderr)).toBe(0);
    expect(JSON.parse(stdout)).toMatchObject({ occurrence, status: "pass" });
  },
  30000,
);
