import { expect, test } from "bun:test";
import { join } from "node:path";

test("generic review and publisher entrypoints refuse CI before source reads or native/authenticated effects", async () => {
  for (const entry of ["review-pr.ts", "publish-review-artifact.ts"]) {
    for (const marker of [{ CI: "true" }, { GITHUB_ACTIONS: "true" }]) {
      const child = Bun.spawn(
        [
          process.execPath,
          "--no-env-file",
          join(import.meta.dir, entry),
          "a".repeat(40),
          entry === "review-pr.ts" ? "native-claude" : "/nonexistent-native-raw",
        ],
        { cwd: "/tmp", env: { PATH: "/usr/bin:/bin", ...marker }, stdout: "pipe", stderr: "pipe" },
      );
      const [stdout, stderr, exit] = await Promise.all([
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        child.exited,
      ]);
      expect(exit).toBe(1);
      expect(stdout).toBe("");
      expect(stderr).toMatch(
        new RegExp(
          "^review_artifact_refused@ci/reuse/" + entry.replace(".", "\\.") + ":[1-9][0-9]*\\n$",
        ),
      );
    }
  }
});
