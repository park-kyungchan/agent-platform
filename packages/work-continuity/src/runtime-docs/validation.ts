import {
  DOCUMENTATION_HOSTS,
  DOCUMENTATION_SURFACES,
  EVIDENCE_STATUSES,
  type DocumentationRegistry,
  type DocumentationRequest,
  type DocumentationSurface,
} from "./types.ts";

const SURFACE_HOSTS: Record<DocumentationSurface, readonly string[]> = {
  "muse.code": ["dev.meta.ai", "meta-models.github.io"],
  "hermes.agent": ["hermes-agent.nousresearch.com"],
  "codex.cli": ["learn.chatgpt.com", "developers.openai.com"],
  "hostinger.control": ["docs.hostinger.com", "www.hostinger.com"],
  "vps.host": ["www.hostinger.com", "docs.docker.com"],
  "vps.container": ["docs.docker.com", "hermes-agent.nousresearch.com"],
  "github.repo": ["docs.github.com", "cli.github.com"],
  "github.actions": ["docs.github.com"],
  "github.codespaces": ["docs.github.com"],
  "github.copilot": ["docs.github.com"],
  "typesafe.ai": ["docs.typesafe.ai"],
  "vercel.ai-gateway": ["vercel.com"],
};
const ALLOWED_HOSTS = new Set(Object.values(SURFACE_HOSTS).flat());
const DIGEST = /^[a-f0-9]{64}$/;
export const normalizeDocumentationAlias = (value: string): string =>
  value.normalize("NFC").trim().toLowerCase();
function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}
function text(value: unknown, name: string, max = 4000): asserts value is string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    Array.from(value).length > max ||
    /[\u0000-\u001f\u007f]/.test(value)
  )
    throw Error(`INVALID_${name}`);
}
function keys(value: Record<string, unknown>, allowed: string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key)))
    throw Error("UNTRUSTED_OR_UNKNOWN_FIELD");
}
function date(value: unknown): boolean {
  return (
    typeof value === "string" &&
    /^\d{4}-\d{2}-\d{2}$/.test(value) &&
    !Number.isNaN(Date.parse(value)) &&
    new Date(value).toISOString().slice(0, 10) === value
  );
}
function relativePath(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length <= 500 &&
    !/[\\\u0000-\u001f]/.test(value) &&
    value.split("/").every((part) => !!part && part !== "." && part !== "..") &&
    !value.includes(":")
  );
}
export function parseDocumentationRequest(input: unknown): DocumentationRequest {
  if (!object(input)) throw Error("INVALID_REQUEST");
  keys(input, ["surface", "topic", "freshness", "executionHost", "targetVersion"]);
  text(input.surface, "SURFACE", 100);
  text(input.topic, "TOPIC", 250);
  if (
    input.freshness !== undefined &&
    (typeof input.freshness !== "string" ||
      !["general", "latest", "installed"].includes(input.freshness))
  )
    throw Error("INVALID_FRESHNESS");
  if (
    input.executionHost !== undefined &&
    !DOCUMENTATION_HOSTS.includes(input.executionHost as (typeof DOCUMENTATION_HOSTS)[number])
  )
    throw Error("INVALID_EXECUTION_HOST");
  if (input.targetVersion !== undefined) text(input.targetVersion, "TARGET_VERSION", 120);
  return {
    surface: normalizeDocumentationAlias(input.surface),
    topic: input.topic,
    freshness:
      input.freshness === undefined
        ? "general"
        : (input.freshness as DocumentationRequest["freshness"]),
    ...(input.executionHost !== undefined
      ? { executionHost: input.executionHost as DocumentationRequest["executionHost"] }
      : {}),
    ...(input.targetVersion !== undefined ? { targetVersion: input.targetVersion as string } : {}),
  };
}
export function safeDocumentationUrl(value: unknown): value is string {
  if (
    typeof value !== "string" ||
    value.length > 2000 ||
    !/^https:\/\/[a-z0-9.-]+\//.test(value) ||
    /[\u0000-\u0020\\\u007f]/.test(value) ||
    /%(?:0[0-9a-f]|1[0-9a-f]|20|5c|7f)/i.test(value)
  )
    return false;
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      !url.username &&
      !url.password &&
      !url.port &&
      !url.hash &&
      ALLOWED_HOSTS.has(url.hostname)
    );
  } catch {
    return false;
  }
}
function surfaceUrl(value: unknown, surface: DocumentationSurface): value is string {
  return safeDocumentationUrl(value) && SURFACE_HOSTS[surface].includes(new URL(value).hostname);
}
function sameDocument(canonicalValue: string, representationValue: string): boolean {
  const canonical = new URL(canonicalValue),
    representation = new URL(representationValue);
  if (
    representation.hostname === "docs.github.com" &&
    representation.pathname === "/api/article/body"
  ) {
    return (
      canonical.hostname === "docs.github.com" &&
      canonical.search === "" &&
      representation.searchParams.getAll("pathname").length === 1 &&
      representation.searchParams.get("pathname") === canonical.pathname &&
      [...representation.searchParams.keys()].every((key) => key === "pathname")
    );
  }
  return (
    canonical.origin === representation.origin &&
    canonical.pathname.replace(/\.md$/, "") === representation.pathname.replace(/\.md$/, "") &&
    canonical.search === representation.search
  );
}
export function validateDocumentationRegistry(
  input: unknown,
): asserts input is DocumentationRegistry {
  if (!object(input)) throw Error("INVALID_REGISTRY");
  keys(input, [
    "schema",
    "generatedOn",
    "provenance",
    "surfaces",
    "sources",
    "captures",
    "routes",
    "limitations",
    "policy",
  ]);
  if (
    input.schema !== "work-continuity.documentation-registry.v1" ||
    !date(input.generatedOn) ||
    !object(input.policy) ||
    !object(input.provenance) ||
    !object(input.surfaces) ||
    !object(input.sources) ||
    !Array.isArray(input.routes) ||
    !Array.isArray(input.captures) ||
    !Array.isArray(input.limitations) ||
    input.limitations.length > 16 ||
    input.routes.length < 1 ||
    input.routes.length > 256 ||
    input.captures.length > 256 ||
    Object.keys(input.sources).length > 256
  )
    throw Error("INVALID_REGISTRY_CONTRACT");
  for (const limitation of input.limitations) text(limitation, "LIMITATION");
  keys(input.policy, ["executionAuthority", "nativePermissionGranted"]);
  if (input.policy.executionAuthority !== "none" || input.policy.nativePermissionGranted !== false)
    throw Error("INVALID_DOCUMENTATION_AUTHORITY");
  const p = input.provenance;
  keys(p, [
    "sourceOwner",
    "sourceDirectory",
    "originalCaptureOwner",
    "immutableBlobOwner",
    "sourceArtifacts",
  ]);
  if (
    p.sourceOwner !== "codex-harness-engineering" ||
    p.originalCaptureOwner !== "scrapling-research" ||
    p.immutableBlobOwner !== "evidence-foundry" ||
    !relativePath(p.sourceDirectory) ||
    !Array.isArray(p.sourceArtifacts) ||
    !p.sourceArtifacts.length ||
    p.sourceArtifacts.length > 16
  )
    throw Error("INVALID_PROVENANCE");
  for (const artifact of p.sourceArtifacts) {
    if (!object(artifact)) throw Error("INVALID_PROVENANCE_ARTIFACT");
    keys(artifact, ["path", "sha256"]);
    if (
      !relativePath(artifact.path) ||
      typeof artifact.sha256 !== "string" ||
      !DIGEST.test(artifact.sha256)
    )
      throw Error("INVALID_PROVENANCE_ARTIFACT");
  }
  if (!Object.keys(input.surfaces).length) throw Error("INVALID_SURFACES");
  for (const [id, surface] of Object.entries(input.surfaces)) {
    if (!DOCUMENTATION_SURFACES.includes(id as DocumentationSurface) || !object(surface))
      throw Error("INVALID_SURFACE");
    keys(surface, ["kind", "index"]);
    text(surface.kind, "SURFACE_KIND", 100);
    if (!surfaceUrl(surface.index, id as DocumentationSurface)) throw Error("UNSAFE_SURFACE_INDEX");
  }
  for (const [id, source] of Object.entries(input.sources)) {
    text(id, "SOURCE_ID", 100);
    if (!object(source)) throw Error("INVALID_SOURCE");
    keys(source, [
      "url",
      "status",
      "checkedOn",
      "rawBytesVerified",
      "bodySha256",
      "legacyUrl",
      "finalReviewCanonicalUrl",
    ]);
    if (
      !safeDocumentationUrl(source.url) ||
      !EVIDENCE_STATUSES.includes(source.status as (typeof EVIDENCE_STATUSES)[number]) ||
      !date(source.checkedOn) ||
      typeof source.rawBytesVerified !== "boolean" ||
      !(
        source.bodySha256 === null ||
        (typeof source.bodySha256 === "string" && DIGEST.test(source.bodySha256))
      ) ||
      (source.rawBytesVerified && source.bodySha256 === null)
    )
      throw Error("INVALID_SOURCE");
    for (const name of ["legacyUrl", "finalReviewCanonicalUrl"])
      if (source[name] !== undefined && !safeDocumentationUrl(source[name]))
        throw Error("UNSAFE_SOURCE_URL");
  }
  const ids = new Set<string>(),
    aliases = new Map<string, string>();
  for (const r of input.routes) {
    if (!object(r)) throw Error("INVALID_ROUTE");
    keys(r, [
      "id",
      "surface",
      "topic",
      "aliases",
      "canonicalUrl",
      "preferredUrl",
      "fallbackUrl",
      "evidenceStatus",
      "sourceId",
      "checkedOn",
      "notes",
      "sectionSelection",
      "pageId",
      "freshnessPolicy",
      "documentationVersion",
      "retainedExtract",
    ]);
    if (typeof r.surface !== "string" || !Object.hasOwn(input.surfaces, r.surface))
      throw Error("INVALID_ROUTE_SURFACE");
    text(r.topic, "TOPIC", 250);
    if (
      r.id !== `${r.surface}.${r.topic}` ||
      ids.has(r.id as string) ||
      !Array.isArray(r.aliases) ||
      r.aliases.length > 32 ||
      !EVIDENCE_STATUSES.includes(r.evidenceStatus as (typeof EVIDENCE_STATUSES)[number]) ||
      !date(r.checkedOn) ||
      !(
        r.sourceId === null ||
        (typeof r.sourceId === "string" && Object.hasOwn(input.sources, r.sourceId))
      )
    )
      throw Error("INVALID_OR_DUPLICATE_ROUTE");
    text(r.sectionSelection, "SECTION_SELECTION");
    if (
      typeof r.notes !== "string" ||
      r.notes.length > 4000 ||
      /[\u0000-\u001f\u007f]/.test(r.notes)
    )
      throw Error("INVALID_NOTES");
    if (r.pageId !== undefined) text(r.pageId, "PAGE_ID", 250);
    if (r.documentationVersion !== undefined)
      text(r.documentationVersion, "DOCUMENTATION_VERSION", 120);
    if (
      r.freshnessPolicy !== undefined &&
      (typeof r.freshnessPolicy !== "string" ||
        !["general", "revalidate"].includes(r.freshnessPolicy))
    )
      throw Error("INVALID_FRESHNESS_POLICY");
    if (r.retainedExtract !== undefined) {
      const extract = r.retainedExtract;
      if (!object(extract)) throw Error("INVALID_RETAINED_EXTRACT");
      keys(extract, [
        "fileName",
        "extractSha256",
        "extractBytes",
        "lineRanges",
        "retrievedAt",
        "etag",
        "lastModified",
      ]);
      if (
        typeof extract.fileName !== "string" ||
        !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,199}\.md$/.test(extract.fileName) ||
        typeof extract.extractSha256 !== "string" ||
        !DIGEST.test(extract.extractSha256) ||
        !Number.isSafeInteger(extract.extractBytes) ||
        Number(extract.extractBytes) < 1 ||
        Number(extract.extractBytes) > 262_144 ||
        !Array.isArray(extract.lineRanges) ||
        extract.lineRanges.length < 1 ||
        extract.lineRanges.length > 16 ||
        !(
          extract.retrievedAt === null ||
          (typeof extract.retrievedAt === "string" &&
            !Number.isNaN(Date.parse(extract.retrievedAt)))
        ) ||
        extract.etag !== null ||
        extract.lastModified !== null
      )
        throw Error("INVALID_RETAINED_EXTRACT");
      let previousEnd = 0;
      for (const range of extract.lineRanges) {
        if (
          !Array.isArray(range) ||
          range.length !== 2 ||
          !Number.isSafeInteger(range[0]) ||
          !Number.isSafeInteger(range[1]) ||
          range[0] <= previousEnd ||
          range[1] < range[0] ||
          range[1] > 262_144
        )
          throw Error("INVALID_RETAINED_EXTRACT_RANGES");
        previousEnd = range[1];
      }
    }
    for (const alias of [r.topic, ...r.aliases]) {
      text(alias, "TOPIC_ALIAS", 250);
      const key = `${r.surface}:${normalizeDocumentationAlias(alias)}`;
      if (aliases.has(key) && aliases.get(key) !== r.id) throw Error("AMBIGUOUS_TOPIC_ALIAS");
      aliases.set(key, r.id as string);
    }
    ids.add(r.id as string);
    const surface = r.surface as DocumentationSurface;
    if (
      !surfaceUrl(r.canonicalUrl, surface) ||
      !surfaceUrl(r.preferredUrl, surface) ||
      !(r.fallbackUrl === null || surfaceUrl(r.fallbackUrl, surface))
    )
      throw Error("UNSAFE_ROUTE");
    if (
      !sameDocument(r.canonicalUrl, r.preferredUrl) ||
      (r.fallbackUrl !== null && !sameDocument(r.canonicalUrl, r.fallbackUrl as string))
    )
      throw Error("DOCUMENT_REPRESENTATION_MISMATCH");
  }
  for (const capture of input.captures) {
    if (!object(capture)) throw Error("INVALID_CAPTURE");
    keys(capture, [
      "id",
      "url",
      "finalUrl",
      "retrievedAt",
      "status",
      "captureOutcome",
      "expectedPredicateMatched",
      "responseContent",
    ]);
    text(capture.id, "CAPTURE_ID", 100);
    if (
      !safeDocumentationUrl(capture.url) ||
      !safeDocumentationUrl(capture.finalUrl) ||
      typeof capture.retrievedAt !== "string" ||
      Number.isNaN(Date.parse(capture.retrievedAt)) ||
      !Number.isInteger(capture.status) ||
      Number(capture.status) < 100 ||
      Number(capture.status) > 599 ||
      typeof capture.captureOutcome !== "string" ||
      !["success", "partial"].includes(capture.captureOutcome) ||
      typeof capture.expectedPredicateMatched !== "boolean" ||
      !Array.isArray(capture.responseContent) ||
      capture.responseContent.length > 8
    )
      throw Error("INVALID_CAPTURE");
    for (const body of capture.responseContent) {
      if (!object(body)) throw Error("INVALID_CAPTURE_BODY");
      keys(body, ["sha256", "bytes", "complete"]);
      if (
        typeof body.sha256 !== "string" ||
        !DIGEST.test(body.sha256) ||
        !Number.isSafeInteger(body.bytes) ||
        Number(body.bytes) < 0 ||
        typeof body.complete !== "boolean"
      )
        throw Error("INVALID_CAPTURE_BODY");
    }
  }
}
