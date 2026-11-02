import { readQualityScope, testFile } from "../scope";
import { readFile, readdir, lstat, readlink } from "node:fs/promises";
import { join, relative } from "node:path";
import {
  insist,
  object,
  sha256,
  trustedDigest,
  type TrustedPolicy,
  requiredSemanticCases,
} from "./policy";
import { dependencyInputs } from "./source";

/** Hash data only. No package manager, candidate module, preload or configuration is evaluated. */
export async function fileInventory(root: string): Promise<Record<string, string>> {
  const pins: Record<string, string> = {};
  async function walk(dir: string): Promise<void> {
    for (const e of await readdir(dir, { withFileTypes: true })) {
      if ([".git", "node_modules"].includes(e.name)) continue;
      const f = join(dir, e.name),
        p = relative(root, f);
      if (e.isDirectory()) await walk(f);
      else if (e.isSymbolicLink()) pins[p] = sha256("symlink\0" + (await readlink(f)));
      else {
        const info = await lstat(f);
        insist(
          info.isFile() && info.size <= 100_000_000 && Object.keys(pins).length < 20000,
          "POLICY_SOURCE_BUDGET",
        );
        pins[p] = sha256(await readFile(f));
      }
    }
  }
  await walk(root);
  return pins;
}
/** Read P's actual complete minimum. Candidate/profile-supplied lists cannot reduce it. */
export async function readPolicyMinimum(root: string): Promise<TrustedPolicy["legacyMinimum"]> {
  const scope = await readQualityScope(root, "architecture/quality-policy.json", []);
  const coverage = object(
    JSON.parse(await readFile(join(root, "ci/quality/trusted/coverage.json"), "utf8")),
  );
  const legacyMinimum = {
    packageTests: scope.testFiles,
    qualityTests: scope.lintFiles.filter((path) => path.startsWith("ci/") && testFile(path)),
    supportFiles: scope.testSupportFiles,
  };
  const rows = coverage.legacyTests as {
    path: string;
    semanticCases: string[];
    limitation: string;
  }[];
  insist(Array.isArray(rows), "P_COVERAGE_MAP_REQUIRED");
  for (const path of [...legacyMinimum.packageTests, ...legacyMinimum.qualityTests]) {
    const mapped = rows.filter((row) => row.path === path);
    insist(
      mapped.length === 1 &&
        mapped[0]!.limitation.length > 0 &&
        mapped[0]!.semanticCases.every((id) => requiredSemanticCases.some((c) => c.id === id)),
      "EVERY_P_MINIMUM_REQUIRES_EXPLICIT_COVERAGE",
    );
  }
  return legacyMinimum;
}
export async function verifyInstalledPolicyInputs(root: string, p: TrustedPolicy): Promise<void> {
  const pins = await fileInventory(root);
  const def = object(
    JSON.parse(await readFile(join(root, "ci/quality/trusted/policy-definition.json"), "utf8")),
  );
  insist(
    trustedDigest(await readPolicyMinimum(root)) === trustedDigest(p.legacyMinimum) &&
      p.coverageSha256 === pins["ci/quality/trusted/coverage.json"] &&
      p.workflowSha256 === pins[".github/workflows/trusted-source.yml"] &&
      p.workerRecipeSha256 === pins["ci/quality/trusted/worker.Dockerfile"] &&
      p.toolPinsSha256 === pins["ci/quality/tool-pins.json"] &&
      trustedDigest(p.dependencyInputs) === trustedDigest(await dependencyInputs(root)) &&
      p.revision === def.revision &&
      p.protectedTag === def.protectedTag &&
      p.reviewPublisher.login === def.reviewPublisherLogin &&
      p.maxAuditAgeSeconds === def.maxAuditAgeSeconds,
    "INSTALLED_POLICY_INPUTS_MISMATCH",
  );
}
