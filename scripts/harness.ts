import {
  preflightQualityImprovement,
  compareQualityImprovement,
} from "../packages/evidence-foundry/src/modules/evals/quality-improvement.ts";
import { readFile } from "node:fs/promises";
import {
  preflightImprovement,
  compareImprovement,
} from "../packages/evidence-foundry/src/modules/evals/improvement.ts";
import {
  openGitWork,
  resolveGitWorkSelection,
  qualifyGitWorkStart,
  readGitWorkContext,
  readCompactGitWorkContext,
  validateGitWorkRecords,
  validateGitWorkTransition,
  type ContextRequest,
  type StartRequest,
} from "../packages/work-continuity/src/git-work/index.ts";
import {
  readGitWorkV3Context,
  previewGitWorkV3Allocation,
  allocateGitWorkIdentity,
  type NativeGitIdentityOwner,
  type NativeLegacyMigrationOwner,
  readVersionedLegacyGitWork,
  readMigratedGitWorkV3,
  previewLegacyGitWorkMigration,
  admitLegacyGitWork,
  recoverLegacyCreateRequest,
  openExactV3SourceReader,
  readSourceQualifiedGitWorkV3Context,
  type GitAllocationTarget,
  type TaskRef,
} from "../packages/work-continuity/src/git-work/v3.ts";
import {
  readDocumentationRegistry,
  type DocumentationObservations,
} from "../packages/work-continuity/src/runtime-docs/index.ts";

const usage =
  "bun run harness context|validate|start|docs|improvement-preflight|improvement-compare|quality-preflight|quality-compare <request.json>; transition <before.json> <after.json>; source <snapshot.json> <relative-path>";
async function readRequest(path: string | undefined): Promise<unknown> {
  if (!path) throw Error(usage);
  const bytes = await readFile(path);
  if (bytes.length > 1_048_576) throw Error("Request exceeds the bounded input size");
  return JSON.parse(bytes.toString("utf8")) as unknown;
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("Request must be a JSON object");
  return value as Record<string, unknown>;
}
function documentationFields(value: Record<string, unknown>, allowed: string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key)))
    throw Error("UNTRUSTED_OR_UNKNOWN_FIELD");
}
function documentationText(value: unknown, max: number): void {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > max ||
    /[\u0000-\u001f\u007f]/.test(value)
  )
    throw Error("INVALID_DOCUMENTATION_OBSERVATION");
}
/** Validate transport shape only; observations remain unchanged caller claims. */
function documentationObservations(value: unknown): DocumentationObservations {
  const observations = object(value);
  documentationFields(observations, ["installedVersion", "cache"]);
  if (Object.hasOwn(observations, "installedVersion"))
    documentationText(observations.installedVersion, 120);
  if (Object.hasOwn(observations, "cache")) {
    const cache = object(observations.cache);
    documentationFields(cache, [
      "canonicalUrl",
      "bodySha256",
      "independentlyConfirmedBodySha256",
      "revalidatedInCurrentTask",
      "targetVersion",
    ]);
    documentationText(cache.canonicalUrl, 2000);
    documentationText(cache.bodySha256, 64);
    documentationText(cache.independentlyConfirmedBodySha256, 64);
    if (typeof cache.revalidatedInCurrentTask !== "boolean")
      throw Error("INVALID_DOCUMENTATION_OBSERVATION");
    if (Object.hasOwn(cache, "targetVersion")) documentationText(cache.targetVersion, 120);
  }
  return observations as unknown as DocumentationObservations;
}
export {
  gitReadEnvironment,
  resolveGitWorkSelection,
} from "../packages/work-continuity/src/git-work/index.ts";
export async function runHarness(
  argv: string[],
  nativeOwner?: NativeGitIdentityOwner,
  nativeMigrationOwner?: NativeLegacyMigrationOwner,
): Promise<unknown> {
  const [command, path, extra] = argv;
  if (argv.length !== (command === "transition" || command === "source" ? 3 : 2))
    throw Error(usage);
  const request = object(await readRequest(path));
  switch (command) {
    case "context-v3": {
      documentationFields(request, ["graph", "task", "mode", "expectedDigest", "snapshot"]);
      if (request.snapshot !== undefined) {
        const selection = await resolveGitWorkSelection(request.snapshot);
        return readSourceQualifiedGitWorkV3Context({
          ...request,
          reader: await openExactV3SourceReader(selection),
        } as unknown as Parameters<typeof readSourceQualifiedGitWorkV3Context>[0]);
      }
      return readGitWorkV3Context(
        request as unknown as {
          graph: unknown;
          task: TaskRef;
          mode: "full" | "compact" | "dispatch" | "resume";
          expectedDigest?: string;
        },
      );
    }
    case "legacy-v3": {
      documentationFields(request, [
        "mode",
        "snapshot",
        "task",
        "contextMode",
        "expectedDigest",
        "request",
        "target",
        "recovery",
      ]);
      if (request.mode === "read") {
        const selection = await resolveGitWorkSelection(request.snapshot);
        const projected = await readVersionedLegacyGitWork(selection);
        return readGitWorkV3Context({
          graph: projected.graph,
          task: request.task as TaskRef,
          mode: request.contextMode as "full" | "compact" | "dispatch" | "resume",
          ...(request.expectedDigest === undefined
            ? {}
            : { expectedDigest: String(request.expectedDigest) }),
        });
      }
      if (request.mode === "read-migrated") {
        const selection = await resolveGitWorkSelection(request.snapshot);
        return readGitWorkV3Context({
          graph: await readMigratedGitWorkV3(selection),
          task: request.task as TaskRef,
          mode: request.contextMode as "full" | "compact" | "dispatch" | "resume",
          ...(request.expectedDigest === undefined
            ? {}
            : { expectedDigest: String(request.expectedDigest) }),
        });
      }
      if (request.mode === "recover") {
        const recovery = object(request.recovery);
        documentationFields(recovery, [
          "ownerId",
          "legacyOwner",
          "createRequestId",
          "kind",
          "meaningSha256",
        ]);
        return recoverLegacyCreateRequest({
          ...recovery,
          selection: await resolveGitWorkSelection(request.snapshot),
        } as unknown as Parameters<typeof recoverLegacyCreateRequest>[0]);
      }
      const target = object(request.target);
      documentationFields(target, [
        "repositoryRoot",
        "ownerRef",
        "expectedHead",
        "expectedAuthorityRevision",
      ]);
      if (request.mode === "dry-run")
        return previewLegacyGitWorkMigration(request.request, String(target.repositoryRoot));
      if (request.mode !== "apply") throw Error("V3_EXPLICIT_LEGACY_MIGRATION_MODE_REQUIRED");
      if (!nativeMigrationOwner) throw Error("V3_CURRENT_NATIVE_MIGRATION_OWNER_REQUIRED");
      return admitLegacyGitWork(
        request.request,
        target as unknown as GitAllocationTarget,
        nativeMigrationOwner,
      );
    }
    case "allocate-v3": {
      documentationFields(request, ["mode", "request", "target"]);
      const target = object(request.target);
      documentationFields(target, [
        "repositoryRoot",
        "ownerRef",
        "expectedHead",
        "expectedAuthorityRevision",
      ]);
      if (request.mode === "dry-run")
        return previewGitWorkV3Allocation(
          request.request,
          target as unknown as GitAllocationTarget,
        );
      if (request.mode !== "apply") throw Error("V3_EXPLICIT_ALLOCATION_MODE_REQUIRED");
      if (!nativeOwner) throw Error("V3_NATIVE_OWNER_ADAPTER_REQUIRED");
      return allocateGitWorkIdentity(
        request.request,
        target as unknown as GitAllocationTarget,
        nativeOwner,
      );
    }
    case "quality-preflight":
      return preflightQualityImprovement(request);
    case "quality-compare":
      return compareQualityImprovement(request);
    case "improvement-preflight":
      return preflightImprovement(request);
    case "improvement-compare":
      return compareImprovement(request);
    case "context": {
      const outputVersion = request.outputVersion ?? "v1";
      if (outputVersion !== "v1" && outputVersion !== "v2")
        throw Error("EXPLICIT_CONTEXT_OUTPUT_VERSION_REQUIRED");
      if (
        request.metadataEncoding !== undefined &&
        (outputVersion !== "v2" || request.metadataEncoding !== "metadata.v1")
      )
        throw Error("EXPLICIT_CONTEXT_METADATA_ENCODING_REQUIRED");
      const reader = await openGitWork(await resolveGitWorkSelection(request.snapshot));
      const context = object(request.context) as unknown as ContextRequest;
      if (outputVersion === "v2")
        return request.metadataEncoding === "metadata.v1"
          ? readCompactGitWorkContext(reader, context, { taskId: context.taskId }, "metadata.v1")
          : readCompactGitWorkContext(reader, context, { taskId: context.taskId });
      return readGitWorkContext(reader, context);
    }
    case "validate":
      return validateGitWorkRecords(await openGitWork(await resolveGitWorkSelection(request)));
    case "transition":
      return validateGitWorkTransition(
        await openGitWork(await resolveGitWorkSelection(request)),
        await openGitWork(await resolveGitWorkSelection(await readRequest(extra))),
      );
    case "source": {
      if (!extra) throw Error(usage);
      return (await openGitWork(await resolveGitWorkSelection(request))).readSource({
        kind: "snapshot",
        path: extra,
      });
    }
    case "start":
      return qualifyGitWorkStart(request as unknown as StartRequest);
    case "docs": {
      const reader = await readDocumentationRegistry();
      if (!Object.hasOwn(request, "request") && !Object.hasOwn(request, "observations"))
        return reader.select(request);
      documentationFields(request, ["request", "observations"]);
      const docsRequest = object(request.request);
      const observations = Object.hasOwn(request, "observations")
        ? documentationObservations(request.observations)
        : undefined;
      return reader.select(docsRequest, observations);
    }
    default:
      throw Error(usage);
  }
}
if (import.meta.main) {
  try {
    const result = await runHarness(process.argv.slice(2));
    const compactContext =
      result !== null &&
      typeof result === "object" &&
      (result as Record<string, unknown>).schema === "common.git-work-context.compact.v1";
    console.log(JSON.stringify(result, null, compactContext ? undefined : 2));
    if (result && typeof result === "object") {
      const value = result as Record<string, unknown>;
      if (
        value.valid === false ||
        ["held", "defer", "repair", "stop"].includes(String(value.status))
      )
        process.exitCode = 1;
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Common Harness request failed");
    process.exitCode = 1;
  }
}
