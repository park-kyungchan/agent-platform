/** P-owned fixed fixture adapters within the existing probe worker. No expected answers here. */
const fixedScenarioIds = new Set([
  "git-source-positive",
  "git-source-hash",
  "git-source-missing",
  "git-source-foreign",
  "git-source-currentness",
  "git-allocation-positive",
  "git-allocation-conflict",
  "git-allocation-stale",
  "git-allocation-denied",
  "git-legacy-positive",
  "git-legacy-fence",
  "git-legacy-migration",
  "git-legacy-migration-conflict",
  "git-artifact-positive",
  "git-artifact-receiver-negative",
  "git-artifact-source-negative",
  "owner-v1",
  "owner-v2",
  "owner-v3",
  "owner-v4",
  "owner-v5",
  "owner-v6",
  "owner-before-drift",
  "owner-after-read-drift",
  "owner-tool-drift",
  "owner-dependency-drift",
  "owner-package-export-negative",
  "owner-first-git",
  "owner-first-memory",
  "owner-first-work-method",
  "owner-public-exports",
  "owner-native-positive",
  "owner-native-after-drift",
  "owner-hermes-positive",
  "owner-hermes-first-import",
  "owner-hermes-after-drift",
  "memory-local-positive",
  "memory-local-excluded",
  "memory-local-original-denied",
  "memory-local-pin-negative",
  "memory-local-scope-negative",
  "memory-git-positive",
  "memory-git-reference-only",
  "memory-git-stale-generation",
  "memory-git-head-drift",
  "memory-git-pin-negative",
  "memory-v1-positive",
  "memory-v1-denied",
  "memory-v1-expired",
  "memory-v1-policy-drift",
  "memory-v1-original-hash",
  "memory-v1-view-reauthorize",
  "service-policy-positive",
  "service-policy-excluded",
  "service-policy-missing",
  "handoff-custody-positive",
  "handoff-cas-positive",
  "handoff-stale-writer",
  "handoff-completed-effect-loss",
  "handoff-source-symlink",
  "handoff-partial-publication",
  "projection-native-positive",
  "projection-foreign-root",
  "operational-intent-positive",
  "operational-intent-source-negative",
  "operational-intent-duplicate-json",
  "operational-review-positive",
  "operational-review-forged-page",
  "operational-artifact-v1",
  "operational-artifact-v2",
  "operational-artifact-v3",
  "operational-artifact-v4",
  "operational-artifact-duplicate",
  "operational-governance-positive",
  "operational-governance-unadmitted",
  "operational-evaluation-positive",
  "operational-evaluation-wrong-source",
  "operational-context-positive",
  "operational-context-forbidden-effect",
  "operational-wiki-positive",
  "operational-wiki-forged-quote",
  "operational-inception-positive",
  "operational-inception-effect-negative",
  "operational-inception-missing-interview",
  "operational-runtime-reference-positive",
  "operational-runtime-reference-stale",
  "operational-runtime-reference-ambiguous",
  "operational-docs-positive",
  "operational-docs-hash-negative",
]);
export const scenarioModule = "ci/quality/trusted/scenario-adapters.ts";
export async function loadBehaviorModule(
  root: string,
  module: string,
): Promise<Record<string, unknown>> {
  if (module !== scenarioModule) return import(root + "/" + module);
  return {
    run: async (id: unknown) => {
      if (typeof id !== "string" || !fixedScenarioIds.has(id))
        throw Error("TRUSTED_CI_FIXED_SCENARIO_REQUIRED");
      if (id.startsWith("git-")) return (await import("./scenarios/git")).gitScenario(root, id);
      if (id.startsWith("owner-"))
        return (await import("./scenarios/owner")).ownerScenario(root, id);
      if (id.startsWith("memory-"))
        return (await import("./scenarios/memory")).memoryScenario(root, id);
      if (
        id.startsWith("service-") ||
        id.startsWith("handoff-") ||
        id.startsWith("operational-") ||
        id.startsWith("projection-")
      )
        return (await import("./scenarios/domain")).domainScenario(root, id);
      throw Error("TRUSTED_CI_UNKNOWN_FINITE_SCENARIO");
    },
  };
}
