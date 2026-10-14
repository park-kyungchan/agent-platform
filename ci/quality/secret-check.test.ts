import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  permittedSecretSource,
  runSecretCheck,
  verifySecretCanary,
  type VerifiedSecretTool,
} from "./secret-check";

const binary = process.env.GITLEAKS_VERIFY_BINARY;
const binarySha256 = process.env.GITLEAKS_VERIFY_BINARY_SHA256;
const tool: VerifiedSecretTool = {
  path: binary ?? "/unavailable-native-test-binary",
  binarySha256: binarySha256 ?? "0".repeat(64),
  pin: {
    name: "gitleaks",
    version: "8.30.1",
    sha256: "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb",
  },
};
const temporary: string[] = [];
const fixture = async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-platform-secret-test-"));
  temporary.push(root);
  await mkdir(join(root, "src"));
  await writeFile(join(root, "src/clean.ts"), "export const value = 1;\n");
  return root;
};
afterEach(async () => {
  for (const root of temporary.splice(0)) await rm(root, { recursive: true, force: true });
});

test("only maintained source paths are admitted, never private/history/evidence/generated data", () => {
  for (const path of [
    "src/code.ts",
    "packages/core/src/main.ts",
    ".github/workflows/source.yml",
    "architecture/quality-policy.json",
    "packages/ontology/ontology/domains/evidence/registry.json",
    "packages/ontology/ontology/domains/evidence/vocabulary.json",
  ]) {
    expect(permittedSecretSource(path)).toBe(true);
  }
  for (const path of [
    "../escape.ts",
    "/tmp/escape.ts",
    ".git/config",
    ".private/secret.json",
    "sessions/current.json",
    "evidence/receipt.json",
    "packages/ontology/ontology/domains/evidence/session.json",
    "packages/ontology/ontology/domains/evidence/registry.json/secret.ts",
    "discovery/result.json",
    "generated/schema.ts",
    "node_modules/pkg/a.ts",
    "owner-fixture/native.ts",
    "releases/ontology.tgz",
    ".env",
    ".env.local",
    ".gitleaksignore",
    ".gitleaks.toml",
    "src/generated/a.ts",
    "src/types.d.ts",
    "src\\escape.ts",
  ]) {
    expect(permittedSecretSource(path)).toBe(false);
  }
});

test("an empty, duplicate or protected source denominator is rejected before scanner execution", async () => {
  const root = await fixture();
  await expect(runSecretCheck(root, [], tool)).rejects.toThrow(
    "nonempty maintained source denominator",
  );
  await expect(runSecretCheck(root, ["src/clean.ts", "src/clean.ts"], tool)).rejects.toThrow(
    "nonempty maintained source denominator",
  );
  await expect(runSecretCheck(root, ["evidence/receipt.json"], tool)).rejects.toThrow(
    "nonempty maintained source denominator",
  );
});

// Root integration should resolve the verified cached tool and call verifySecretCanary from check:quality.
// Native tests accept only an explicitly supplied digest-verified binary, never auto-install globally.
test.if(Boolean(binary && binarySha256))(
  "real pinned scanner detects a synthetic credential and discards source/secret fields",
  async () => {
    expect(await verifySecretCanary(tool)).toEqual({
      cleanPassed: true,
      syntheticCredentialDetected: true,
      sensitiveFieldsDiscarded: true,
    });
  },
);

test.if(Boolean(binary && binarySha256))(
  "unselected private and history files are never scanned",
  async () => {
    const root = await fixture();
    const synthetic =
      ["g", "h", "p", "_"].join("") +
      createHash("sha256").update("nonfunctional local gitleaks canary").digest("hex").slice(0, 36);
    for (const directory of [".git", ".private", "evidence", "sessions", "generated"]) {
      await mkdir(join(root, directory));
      await writeFile(
        join(root, directory, "unselected.ts"),
        'export const token = "' + synthetic + '";\n',
      );
    }
    const result = await runSecretCheck(root, ["src/clean.ts"], tool);
    expect(result.status).toBe("pass");
    expect(result.scannedFiles).toBe(1);
    expect(result.historyScanned).toBe(false);
  },
);

test.if(Boolean(binary && binarySha256))("symlinked selected source is rejected", async () => {
  const root = await fixture();
  await symlink(join(root, "src/clean.ts"), join(root, "src/link.ts"));
  await expect(runSecretCheck(root, ["src/link.ts"], tool)).rejects.toThrow("exact regular file");
});

test.if(Boolean(binary && binarySha256))(
  "binary hash drift is rejected before scanning",
  async () => {
    const root = await fixture();
    await expect(
      runSecretCheck(root, ["src/clean.ts"], { ...tool, binarySha256: "f".repeat(64) }),
    ).rejects.toThrow("binary changed");
  },
);
