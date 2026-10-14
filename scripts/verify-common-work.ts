import { resolve } from "node:path";
import {
  openGitWork,
  readGitWorkContext,
  validateGitWorkRecords,
  validateGitWorkTransition,
  type EffectFenceRecord,
  type GitWorkRecord,
} from "../packages/work-continuity/src/git-work/index.ts";
import { createDocumentationReader } from "../packages/work-continuity/src/runtime-docs/index.ts";
import { gitReadEnvironment, resolveGitWorkSelection } from "./harness.ts";

/** Source-only verification from the original Git checkout, including when
 * finite-owner QA runs this script from its separate dependency-installed tree. */
export async function verifyCommonWork(repositoryRoot: string, commit = "HEAD") {
  const selection = await resolveGitWorkSelection({
    repositoryRoot: resolve(repositoryRoot),
    repositoryId: "park-kyungchan/agent-platform",
    commit,
    projectId: "codex-harness-engineering",
    workId: "ontology-driven-harness-operation",
    maxRecords: 512,
  });
  const reader = await openGitWork(selection);
  const validation = await validateGitWorkRecords(reader);
  if (!validation.valid)
    throw Error("Delivered common Work failed: " + validation.findings.join("; "));
  const references = await reader.listRecords();
  const records: GitWorkRecord[] = await Promise.all(
    references.map((ref) => reader.readRecord(ref)),
  );
  const decisions = records.filter((record) => record.kind === "decision");
  const fences = records.filter(
    (record): record is EffectFenceRecord => record.kind === "effect-fence",
  );
  const historical = fences.find((record) => record.id === "checkpoint-56-historical-effect-fence");
  if (
    decisions.length < 37 ||
    !historical ||
    historical.completedEffectIds.length !== 63 ||
    historical.uncertainEffectIds.length !== 2
  )
    throw Error(
      "Original actual-decision/effect-fence baseline changed without its required source successor",
    );
  const context = await readGitWorkContext(reader, {
    taskId: "GHA-22",
    latestRelevant: "current",
    maxChars: 24_000,
    maxRecords: 128,
  });
  if (
    context.status !== "source-qualified" ||
    context.authority !== "none" ||
    context.nativePermissionGranted !== false
  )
    throw Error("Delivered common Work context failed: " + context.holdReasons.join("; "));
  const registry = await reader.readSource({
    kind: "snapshot",
    path: "architecture/runtime-documentation-routing/registry.json",
  });
  const documentation = createDocumentationReader(registry.value);
  const route = documentation.select({
    surface: "codex.cli",
    topic: "permissions",
    freshness: "latest",
  });
  if (
    documentation.registry.routes.length < 84 ||
    route.status !== "SELECTED" ||
    route.executionAuthority !== "none" ||
    route.nativePermissionGranted !== false
  )
    throw Error("Docs-only shared Runtime Index failed");
  const binding = (
    await reader.readSource({ kind: "snapshot", path: "architecture/common-git-harness.json" })
  ).value;
  if (
    !binding ||
    typeof binding !== "object" ||
    Array.isArray(binding) ||
    !Array.isArray(binding.sharedDefinitionInputs)
  )
    throw Error("Common shared source definition binding absent");
  for (const pin of binding.sharedDefinitionInputs) {
    if (
      !pin ||
      typeof pin !== "object" ||
      Array.isArray(pin) ||
      typeof pin.path !== "string" ||
      typeof pin.sha256 !== "string"
    )
      throw Error("Invalid shared definition pin");
    if ((await reader.readSource({ kind: "snapshot", path: pin.path })).sha256 !== pin.sha256)
      throw Error("Shared docs definition drift");
  }
  let beforeCommit = process.env.COMMON_BASE_SHA;
  if (beforeCommit && !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(beforeCommit))
    throw Error("Exact native CI base commit required");
  if (!beforeCommit) {
    // Current repository always has a parent. An explicitly selected first commit
    // can still use the initial source-migration baseline without inventing one.
    try {
      beforeCommit = (await resolveGitWorkSelection({ ...selection, commit: reader.commit + "^" }))
        .commit;
    } catch {
      beforeCommit = undefined;
    }
  }
  let transitionValidated = false;
  if (beforeCommit) {
    const baseSelection = { ...selection, commit: beforeCommit };
    let before: Awaited<ReturnType<typeof openGitWork>> | undefined;
    try {
      before = await openGitWork(baseSelection);
    } catch (error) {
      if (!(error instanceof Error) || error.message !== "REGULAR_GIT_BLOB_REQUIRED") throw error;
      const child = Bun.spawn(
        [
          "git",
          "--no-replace-objects",
          "-C",
          selection.repositoryRoot,
          "ls-tree",
          "-z",
          beforeCommit,
          "--",
          reader.workPath + "/work.json",
        ],
        { stdout: "pipe", stderr: "ignore", env: gitReadEnvironment() },
      );
      const entry = await new Response(child.stdout).text();
      if ((await child.exited) !== 0) throw Error("Comparison base source unavailable");
      if (entry !== "") throw error;
    }
    if (before) {
      const transition = await validateGitWorkTransition(before, reader);
      if (!transition.valid)
        throw Error("Common Work semantic transition failed: " + transition.findings.join("; "));
      transitionValidated = true;
    }
  }
  return {
    schema: "agent-platform.common-work-verification.v1",
    status: "pass",
    verificationClass: "committed-common-source-only",
    repositoryId: selection.repositoryId,
    commit: reader.commit,
    records: records.length,
    actualDecisions: decisions.length,
    completedHistoricalEffects: 63,
    uncertainHistoricalEffects: 2,
    contextRecords: context.records.length,
    contextSources: context.sources.length,
    runtimeDocumentationRoutes: documentation.registry.routes.length,
    comparisonBaseCommit: beforeCommit ?? null,
    transitionValidated,
    runtimeActivationEstablished: false,
    wholeWorkAcceptanceEstablished: false,
    nativePermissionGranted: false,
  };
}
if (import.meta.main) {
  try {
    if (process.argv.length > 4)
      throw Error(
        "Usage: bun scripts/verify-common-work.ts [repository-root] [commit-or-local-ref]",
      );
    console.log(
      JSON.stringify(
        await verifyCommonWork(
          process.argv[2] ?? resolve(import.meta.dir, ".."),
          process.argv[3] ?? "HEAD",
        ),
        null,
        2,
      ),
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Common Work verification failed");
    process.exitCode = 1;
  }
}
