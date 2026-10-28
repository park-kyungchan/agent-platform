import { codexReviewThread } from "./review-runtimes";
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

test("consumer policy discovery reads fixed JSON in both current layouts and never imports a consumer gate", async () => {
  const { mkdtemp, mkdir, writeFile, rm } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const { nativeReviewPolicy } = await import("./publish-review-artifact");
  for (const path of ["architecture/common-ci-provider.json", "ci/provider.json"]) {
    const root = await mkdtemp(join(tmpdir(), "native-review-policy-"));
    try {
      const owner = {
        repository: "fixture/owner",
        projectId: "fixture-owner",
        domainId: "fixture-domain",
        semanticOwner: "fixture-owner",
        scope: "source-check",
        statusContext: "native-independent-review",
        publisher: { id: "42", login: "native-publisher", type: "User" },
      };
      await mkdir(join(root, "architecture"));
      await mkdir(join(root, "ci"));
      await writeFile(
        join(root, "project.json"),
        JSON.stringify({ projectId: owner.projectId, domainId: owner.domainId }),
      );
      await writeFile(
        join(root, path),
        JSON.stringify({
          codeOwner: "agent-platform",
          generatedOnly: true,
          sourceOnly: true,
          runtimeAuthority: false,
          nativePermissionGranted: false,
          policy: owner,
        }),
      );
      await writeFile(
        join(root, "ci/native-review-gate.js"),
        'throw Error("Consumer code must not execute during policy discovery");',
      );
      expect(await nativeReviewPolicy(root)).toMatchObject({
        repository: owner.repository,
        publisher: owner.publisher,
        binding: { semanticOwner: owner.semanticOwner },
      });
      await writeFile(join(root, path), JSON.stringify({ codeOwner: "foreign", policy: owner }));
      await expect(nativeReviewPolicy(root)).rejects.toThrow(
        "Fixed source-only consumer projection required",
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
});

test("native Codex review identity must come from exactly one real thread event", () => {
  const id = "00000000-0000-4000-8000-000000000001";
  const event = JSON.stringify({ type: "thread.started", thread_id: id }) + "\n";
  expect(
    codexReviewThread(Buffer.from(event + JSON.stringify({ type: "turn.completed" }) + "\n")),
  ).toBe(id);
  for (const text of [
    "",
    JSON.stringify({ type: "thread.started", thread_id: "logical-task" }) + "\n",
    event + event,
    JSON.stringify({ type: "turn.completed" }) + "\n",
    event + "not-json\n",
  ])
    expect(() => codexReviewThread(Buffer.from(text))).toThrow();
});
