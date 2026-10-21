import { test, expect } from "bun:test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

test("committed v3 types and bundled Ajv validators reproduce from the single schema", async () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const schema = await readFile(
    new URL("../../contracts/git-work-v3.schema.json", import.meta.url),
  );
  const digest = createHash("sha256").update(schema).digest("hex");
  for (const name of ["v3.ts", "v3-validators.js", "v3-validators.d.ts"])
    expect(
      (await readFile(new URL("./generated/" + name, import.meta.url), "utf8")).includes(digest),
    ).toBe(true);
  const child = Bun.spawn(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      "--no-macros",
      "--no-addons",
      "packages/ontology/scripts/generate.ts",
      "--git-work-v3",
      "--check",
    ],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  const stdout = await new Response(child.stdout).text();
  const stderr = await new Response(child.stderr).text();
  expect(await child.exited, stderr).toBe(0);
  expect(JSON.parse(stdout).schemaSha256).toBe(digest);
}, 30_000);

test("the existing Ontology generator still reproduces its original three schema projections", async () => {
  const directory = new URL("../../../ontology/", import.meta.url).pathname;
  const child = Bun.spawn(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      "--no-macros",
      "--no-addons",
      "scripts/generate.ts",
      "--check",
    ],
    { cwd: directory, stdout: "pipe", stderr: "pipe" },
  );
  const stdout = await new Response(child.stdout).text();
  const stderr = await new Response(child.stderr).text();
  expect(await child.exited, stderr).toBe(0);
  expect(stdout.trim().split("\n").length).toBe(3);
}, 30_000);
