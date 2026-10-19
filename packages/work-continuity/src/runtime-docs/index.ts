import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  normalizeDocumentationAlias,
  parseDocumentationRequest,
  validateDocumentationRegistry,
} from "./validation.ts";
import type {
  DocumentationExtractRead,
  DocumentationExtractReadOptions,
  DocumentationPlan,
  DocumentationReader,
  DocumentationRoute,
  DocumentationSurface,
} from "./types.ts";
export * from "./types.ts";
export { parseDocumentationRequest, safeDocumentationUrl } from "./validation.ts";

const MAX_REGISTRY_BYTES = 262_144;
const SENSITIVE_TOPICS = new Set([
  "auth",
  "authentication",
  "permissions",
  "rules",
  "billing",
  "security",
  "docker-security",
  "firewall",
  "pull-request-target",
  "ssh-keys",
  "policy",
]);
function freeze(value: unknown): void {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return;
  for (const child of Object.values(value)) freeze(child);
  Object.freeze(value);
}
/** Clone, validate and freeze once. Selection never reparses or revalidates the
 * registry, reads host state, fetches documents or decides execution admission. */
export function createDocumentationReader(input: unknown): DocumentationReader {
  const registry = structuredClone(input);
  validateDocumentationRegistry(registry);
  const serialized = JSON.stringify(registry);
  if (Buffer.byteLength(serialized) > MAX_REGISTRY_BYTES) throw Error("REGISTRY_BYTE_BUDGET");
  freeze(registry);
  const routeByAlias = new Map<string, DocumentationRoute>(),
    routeById = new Map<string, DocumentationRoute>();
  for (const route of registry.routes) {
    routeById.set(route.id, route);
    for (const alias of [route.topic, ...route.aliases])
      routeByAlias.set(`${route.surface}:${normalizeDocumentationAlias(alias)}`, route);
  }
  const reader: DocumentationReader = {
    registry,
    registrySha256: createHash("sha256").update(serialized).digest("hex"),
    readSelectedExtract(input, options) {
      return readSelectedExtract(reader, input, options);
    },
    select(input, observations = {}) {
      const request = parseDocumentationRequest(input);
      const result: DocumentationPlan = {
        status: "NEEDS_DISCOVERY",
        request,
        primary: null,
        secondary: [],
        discovery: [],
        evidenceGaps: [],
        retrieval: "DISCOVER",
        limits: { maxPages: 3, maxExcerptChars: 6000, maxDiscoveryRequests: 2 },
        executionAuthority: "none",
        nativePermissionGranted: false,
      };
      if (!Object.hasOwn(registry.surfaces, request.surface)) {
        result.status = "NEEDS_DISAMBIGUATION";
        result.discovery = Object.keys(registry.surfaces).filter((surface) =>
          request.surface === "github"
            ? surface.startsWith("github.")
            : request.surface === "hostinger"
              ? ["hostinger.control", "vps.host", "vps.container", "hermes.agent"].includes(surface)
              : true,
        );
        result.evidenceGaps.push(
          "subject-surface-unresolved; do-not-infer-subject-from-execution-host",
        );
        return result;
      }
      const primary = routeByAlias.get(
        `${request.surface}:${normalizeDocumentationAlias(request.topic)}`,
      );
      if (!primary) {
        result.discovery = [registry.surfaces[request.surface as DocumentationSurface]!.index];
        result.evidenceGaps.push("topic-not-covered; bounded-official-discovery-only");
        return result;
      }
      result.status = "SELECTED";
      result.primary = primary;
      const secondaryIds: string[] = [];
      if (
        request.executionHost?.startsWith("github.actions.") &&
        request.surface !== "github.actions"
      )
        secondaryIds.push("github.actions.workflow", "github.actions.security");
      if (request.executionHost === "github.codespaces" && request.surface !== "github.codespaces")
        secondaryIds.push("github.codespaces.environment");
      result.secondary = [...new Set(secondaryIds)]
        .map((id) => routeById.get(id))
        .filter((route): route is DocumentationRoute => !!route)
        .slice(0, 2);
      const installed = observations.installedVersion;
      if (
        installed !== undefined &&
        (typeof installed !== "string" ||
          !installed.trim() ||
          installed.length > 120 ||
          /[\u0000-\u001f]/.test(installed))
      )
        throw Error("INVALID_INSTALLED_VERSION_OBSERVATION");
      const versionConflict =
        !!request.targetVersion && !!installed && request.targetVersion !== installed;
      const version = request.freshness === "installed" ? installed : request.targetVersion;
      const documentationConflict =
        !!version && !!primary.documentationVersion && version !== primary.documentationVersion;
      const cache = observations.cache;
      const matchingCache =
        !!cache &&
        cache.canonicalUrl === primary.canonicalUrl &&
        /^[a-f0-9]{64}$/.test(cache.bodySha256) &&
        cache.bodySha256 === cache.independentlyConfirmedBodySha256 &&
        (cache.targetVersion ?? undefined) === (version ?? undefined) &&
        !versionConflict &&
        !documentationConflict;
      if (primary.evidenceStatus !== "content-read")
        result.evidenceGaps.push("route-body-not-fully-reviewed");
      if (request.freshness === "installed" && (!installed || versionConflict)) {
        result.retrieval = "VERIFY_INSTALLED_VERSION_AND_READ";
        result.evidenceGaps.push(
          "installed-version-unverified-or-conflicting; documentation-is-not-deployed-availability-proof",
        );
      } else if (documentationConflict) {
        result.retrieval = "READ_VERSION_MATCHED_SOURCE";
        result.evidenceGaps.push("documentation-version-differs-from-applicable-version");
      } else if (
        request.freshness === "latest" ||
        primary.freshnessPolicy === "revalidate" ||
        SENSITIVE_TOPICS.has(primary.topic)
      ) {
        result.retrieval =
          matchingCache && cache?.revalidatedInCurrentTask === true
            ? "REUSE_MATCHING_PAGE"
            : "REVALIDATE_SELECTED_PAGE";
      } else result.retrieval = matchingCache ? "REUSE_MATCHING_PAGE" : "READ_SELECTED_PAGE";
      if (versionConflict)
        result.evidenceGaps.push("requested-version-differs-from-installed-version");
      if (request.freshness === "installed" && installed && !matchingCache)
        result.evidenceGaps.push(
          "confirm-version-matched-release-or-source-if-current-page-cannot-substantiate-this-version",
        );
      return result;
    },
  };
  return Object.freeze(reader);
}
/** Read one bounded registry file and construct its reader exactly once. A custom
 * path is an explicit caller-selected data input, never a deployment pointer. */
export async function readDocumentationRegistry(
  path: string | URL = new URL(
    "../../../../architecture/runtime-documentation-routing/registry.json",
    import.meta.url,
  ),
): Promise<DocumentationReader> {
  const file = await open(path, "r");
  try {
    const stat = await file.stat();
    if (!stat.isFile() || stat.size > MAX_REGISTRY_BYTES) throw Error("REGISTRY_FILE_BUDGET");
    const bytes = Buffer.alloc(MAX_REGISTRY_BYTES + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await file.read(bytes, offset, bytes.length - offset, offset);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    if (offset > MAX_REGISTRY_BYTES) throw Error("REGISTRY_FILE_BUDGET");
    return createDocumentationReader(
      JSON.parse(bytes.subarray(0, offset).toString("utf8")) as unknown,
    );
  } finally {
    await file.close();
  }
}

/** Read only the selected source-owned extract. This does not retrieve, store or
 * authenticate documents, and an extract digest never stands for an HTTP body. */
async function readSelectedExtract(
  reader: DocumentationReader,
  input: unknown,
  options: DocumentationExtractReadOptions,
): Promise<DocumentationExtractRead> {
  const plan = reader.select(input, options.observations);
  const result: DocumentationExtractRead = { plan, excerpt: null };
  const gap = (message: string): DocumentationExtractRead => {
    plan.evidenceGaps.push(message);
    return result;
  };
  const route = plan.primary;
  if (!route) return result;
  if (
    plan.retrieval === "VERIFY_INSTALLED_VERSION_AND_READ" ||
    plan.retrieval === "READ_VERSION_MATCHED_SOURCE" ||
    plan.evidenceGaps.includes("requested-version-differs-from-installed-version")
  )
    return gap("retained-extract-applicable-version-unverified");
  const version =
    plan.request.freshness === "installed"
      ? options.observations?.installedVersion
      : plan.request.targetVersion;
  const needsCurrentness =
    plan.request.freshness === "latest" ||
    route.freshnessPolicy === "revalidate" ||
    SENSITIVE_TOPICS.has(route.topic);
  // A caller's raw-body cache observation cannot validate this Markdown extract.
  plan.retrieval = needsCurrentness ? "REVALIDATE_SELECTED_PAGE" : "READ_SELECTED_PAGE";
  const extract = route.retainedExtract;
  if (!extract) return gap("retained-extract-unavailable; bounded-official-lookup-required");
  const observed = options.revalidation;
  const currentExtractObserved =
    observed?.observedInCurrentTask === true &&
    observed.canonicalUrl === route.canonicalUrl &&
    observed.extractSha256 === extract.extractSha256 &&
    (observed.targetVersion ?? undefined) === (version ?? undefined);
  if (version && !route.documentationVersion && !currentExtractObserved) {
    plan.retrieval = "READ_VERSION_MATCHED_SOURCE";
    return gap("retained-extract-applicable-version-unverified");
  }
  if (needsCurrentness && !currentExtractObserved)
    return gap("retained-extract-needs-targeted-current-task-revalidation");
  let file;
  try {
    const directory =
      options.directory instanceof URL ? fileURLToPath(options.directory) : options.directory;
    file = await open(
      join(directory, extract.fileName),
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    const stat = await file.stat();
    if (!stat.isFile() || stat.size !== extract.extractBytes)
      return gap("retained-extract-file-or-byte-budget-mismatch");
    const bytes = Buffer.alloc(extract.extractBytes + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await file.read(bytes, offset, bytes.length - offset, offset);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    if (offset !== extract.extractBytes)
      return gap("retained-extract-file-or-byte-budget-mismatch");
    const content = bytes.subarray(0, offset);
    if (createHash("sha256").update(content).digest("hex") !== extract.extractSha256)
      return gap("retained-extract-hash-mismatch");
    const lines = new TextDecoder("utf-8", { fatal: true }).decode(content).split("\n");
    if (extract.lineRanges.some(([, end]) => end > lines.length))
      return gap("retained-extract-section-missing");
    const text = extract.lineRanges
      .map(([start, end]) => lines.slice(start - 1, end).join("\n"))
      .join("\n\n");
    if (Array.from(text).length > plan.limits.maxExcerptChars)
      return gap("retained-extract-excerpt-budget");
    plan.retrieval = "REUSE_MATCHING_PAGE";
    result.excerpt = {
      kind: "retained-markdown-extract",
      canonicalUrl: route.canonicalUrl,
      extractSha256: extract.extractSha256,
      excerptSha256: createHash("sha256").update(text).digest("hex"),
      text,
      retrievedAt: extract.retrievedAt,
      etag: extract.etag,
      lastModified: extract.lastModified,
    };
    return result;
  } catch {
    return gap("retained-extract-unreadable; bounded-official-lookup-required");
  } finally {
    await file?.close();
  }
}
