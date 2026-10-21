import { test, expect } from "bun:test";
import ts from "typescript";
import { join } from "node:path";

test("owned v3 runtime, generator and synthetic consumers satisfy strict TypeScript", () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const owned = [
    "packages/work-continuity/src/git-work/v3.ts",
    "packages/work-continuity/src/git-work/v3-public.test.ts",
    "packages/work-continuity/src/git-work/v3-codec.ts",
    "packages/work-continuity/src/git-work/effective-contract.ts",
    "packages/work-continuity/src/git-work/allocation.ts",
    "packages/work-continuity/src/git-work/v3.test.ts",
    "packages/work-continuity/src/git-work/v3-corrections.test.ts",
    "packages/work-continuity/src/git-work/v3-generated.test.ts",
    "packages/work-continuity/src/git-work/fixtures/v3-owner.ts",
    "packages/work-continuity/src/git-work/fixtures/v3-worker.ts",
    "packages/ontology/scripts/generate.ts",
    "scripts/harness.ts",
  ];
  const config = ts.readConfigFile(join(root, "ci/quality/tsconfig.json"), ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
  const program = ts.createProgram(
    owned.map((path) => join(root, path)),
    parsed.options,
  );
  const diagnostics = ts
    .getPreEmitDiagnostics(program)
    .filter((d) => d.category === ts.DiagnosticCategory.Error);
  for (const d of diagnostics.slice(0, 32)) {
    const location =
      d.file && d.start !== undefined ? d.file.getLineAndCharacterOfPosition(d.start) : null;
    console.error(
      `V3_TYPECHECK_SOURCE ${d.file?.fileName ?? "unknown"}:${(location?.line ?? 0) + 1}:${(location?.character ?? 0) + 1} TS${d.code}`,
    );
  }
  expect(diagnostics.length).toBe(0);
}, 30_000);
