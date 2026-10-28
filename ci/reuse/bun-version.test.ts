import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

type Workflow = {
  permissions: Record<string, string>;
  on: Record<string, unknown>;
  jobs: Record<string, { steps: { uses?: string; run?: string; with?: Record<string, string> }[] }>;
};

test("shared action resolves one exact owner version from literal or .bun-version and rejects ambiguous inputs", async () => {
  const root = await mkdtemp(join(tmpdir(), "caller-bun-version-"));
  try {
    const output = join(root, "output"),
      file = join(root, ".bun-version");
    for (const [literal, versionFile, content, accepted] of [
      ["1.4.3", "", "", true],
      ["", ".bun-version", "1.4.3\n", true],
      ["1.4.3", ".bun-version", "1.4.3\n", false],
      ["", "../private", "", false],
      ["1.4.3\nother", "", "", false],
      ["", ".bun-version", "1.4.3\nother\n", false],
      ["latest", "", "", false],
      ["", "", "", false],
    ] as const) {
      await writeFile(file, content);
      await writeFile(output, "");
      const child = Bun.spawn(["bash", join(import.meta.dir, "resolve-bun-version.sh")], {
        env: {
          PATH: "/usr/bin:/bin",
          GITHUB_WORKSPACE: root,
          GITHUB_OUTPUT: output,
          COMMON_CI_BUN_VERSION: literal,
          COMMON_CI_BUN_VERSION_FILE: versionFile,
        },
        stdout: "pipe",
        stderr: "pipe",
      });
      expect((await child.exited) === 0).toBe(accepted);
      if (!accepted) {
        const stderr = await new Response(child.stderr).text();
        const source = (
          await readFile(join(import.meta.dir, "resolve-bun-version.sh"), "utf8")
        ).split("\n");
        const match = /^(bun_version_[a-z_]+)@ci\/reuse\/resolve-bun-version\.sh:([0-9]+)\n$/.exec(
          stderr,
        );
        expect(match).not.toBeNull();
        expect(source[Number(match![2]) - 1]).toContain(match![1] + "@");
      }
      expect(await readFile(output, "utf8")).toBe(accepted ? "bun-version=1.4.3\n" : "");
    }
    await rm(file);
    await writeFile(join(root, "target"), "1.4.3\n");
    await symlink(join(root, "target"), file);
    const child = Bun.spawn(["bash", join(import.meta.dir, "resolve-bun-version.sh")], {
      env: {
        PATH: "/usr/bin:/bin",
        GITHUB_WORKSPACE: root,
        GITHUB_OUTPUT: output,
        COMMON_CI_BUN_VERSION_FILE: ".bun-version",
      },
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(await child.exited).not.toBe(0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("all active AP source jobs consume the one version file; evidence CI cannot manufacture or publish review", async () => {
  const root = join(import.meta.dir, "../..");
  const source = Bun.YAML.parse(
    await readFile(join(root, ".github/workflows/source.yml"), "utf8"),
  ) as Workflow;
  for (const name of ["source", "quality", "merge-integration", "native-review"]) {
    const setup = source.jobs[name]!.steps.find((step) =>
      step.uses?.startsWith("oven-sh/setup-bun@"),
    );
    expect(setup!.with).toEqual({ "bun-version-file": ".bun-version" });
  }
  expect((await readFile(join(root, ".bun-version"), "utf8")).trim()).toBe(Bun.version);
  expect(source.permissions).toEqual({ contents: "read", statuses: "read", actions: "read" });
  const evidence = Bun.YAML.parse(
    await readFile(join(root, ".github/workflows/native-review-evidence.yml"), "utf8"),
  ) as Workflow;
  expect(Object.keys(evidence.on)).toEqual(["workflow_dispatch"]);
  expect(evidence.permissions).toEqual({ contents: "read" });
  const steps = evidence.jobs["retain-native-review"]!.steps;
  expect(steps).toHaveLength(2);
  expect(steps[0]!.run).toContain("write_bytes(raw)");
  expect(steps[0]!.run).not.toMatch(/claude|codex|statuses\//);
  expect(steps[1]!.uses).toMatch(/^actions\/upload-artifact@[a-f0-9]{40}$/);
});
