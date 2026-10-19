export const DOCUMENTATION_SURFACES = [
  "muse.code",
  "hermes.agent",
  "codex.cli",
  "hostinger.control",
  "vps.host",
  "vps.container",
  "github.repo",
  "github.actions",
  "github.codespaces",
  "github.copilot",
  "typesafe.ai",
  "vercel.ai-gateway",
] as const;
export type DocumentationSurface = (typeof DOCUMENTATION_SURFACES)[number];
export const DOCUMENTATION_HOSTS = [
  "local.wsl",
  "local.windows",
  "hostinger.host",
  "hostinger.container",
  "github.actions.hosted",
  "github.actions.self-hosted",
  "github.codespaces",
] as const;
export type DocumentationHost = (typeof DOCUMENTATION_HOSTS)[number];
export const EVIDENCE_STATUSES = [
  "index-listed",
  "content-read",
  "user-supplied-index-family",
  "page-opened-limited-content",
] as const;
export type DocumentationEvidenceStatus = (typeof EVIDENCE_STATUSES)[number];
export interface DocumentationRequest {
  surface: string;
  topic: string;
  freshness?: "general" | "latest" | "installed";
  executionHost?: DocumentationHost;
  targetVersion?: string;
}
export interface DocumentationRoute {
  id: string;
  surface: DocumentationSurface;
  topic: string;
  aliases: string[];
  canonicalUrl: string;
  preferredUrl: string;
  fallbackUrl: string | null;
  evidenceStatus: DocumentationEvidenceStatus;
  sourceId: string | null;
  checkedOn: string;
  notes: string;
  sectionSelection: string;
  pageId?: string;
  freshnessPolicy?: "general" | "revalidate";
  documentationVersion?: string;
  retainedExtract?: DocumentationRetainedExtract;
}
export interface DocumentationSource {
  url: string;
  status: DocumentationEvidenceStatus;
  checkedOn: string;
  rawBytesVerified: boolean;
  bodySha256: string | null;
  legacyUrl?: string;
  finalReviewCanonicalUrl?: string;
}
export interface DocumentationCapture {
  id: string;
  url: string;
  finalUrl: string;
  retrievedAt: string;
  status: number;
  captureOutcome: "success" | "partial";
  expectedPredicateMatched: boolean;
  responseContent: { sha256: string; bytes: number; complete: boolean }[];
}
export interface DocumentationRegistry {
  schema: "work-continuity.documentation-registry.v1";
  generatedOn: string;
  provenance: {
    sourceOwner: "codex-harness-engineering";
    sourceDirectory: string;
    originalCaptureOwner: "scrapling-research";
    immutableBlobOwner: "evidence-foundry";
    sourceArtifacts: { path: string; sha256: string }[];
  };
  surfaces: Partial<Record<DocumentationSurface, { kind: string; index: string }>>;
  sources: Record<string, DocumentationSource>;
  captures: DocumentationCapture[];
  routes: DocumentationRoute[];
  limitations: string[];
  policy: { executionAuthority: "none"; nativePermissionGranted: false };
}
/** Documentation evidence supplied by the caller. This does not authenticate the
 * caller, installed host, cached body or any native capability. */
export interface DocumentationObservations {
  installedVersion?: string;
  cache?: {
    canonicalUrl: string;
    bodySha256: string;
    independentlyConfirmedBodySha256: string;
    revalidatedInCurrentTask: boolean;
    targetVersion?: string;
  };
}
export type DeepReadonly<T> = T extends object
  ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
  : T;
export interface DocumentationPlan {
  status: "SELECTED" | "NEEDS_DISAMBIGUATION" | "NEEDS_DISCOVERY";
  request: DocumentationRequest;
  primary: DeepReadonly<DocumentationRoute> | null;
  secondary: DeepReadonly<DocumentationRoute>[];
  discovery: string[];
  evidenceGaps: string[];
  retrieval:
    | "REUSE_MATCHING_PAGE"
    | "READ_SELECTED_PAGE"
    | "REVALIDATE_SELECTED_PAGE"
    | "VERIFY_INSTALLED_VERSION_AND_READ"
    | "READ_VERSION_MATCHED_SOURCE"
    | "DISCOVER";
  limits: { maxPages: 3; maxExcerptChars: 6000; maxDiscoveryRequests: 2 };
  executionAuthority: "none";
  nativePermissionGranted: false;
}
export interface DocumentationReader {
  readonly registry: DeepReadonly<DocumentationRegistry>;
  readonly registrySha256: string;
  select(request: unknown, observations?: DocumentationObservations): DocumentationPlan;
  readSelectedExtract(
    request: unknown,
    options: DocumentationExtractReadOptions,
  ): Promise<DocumentationExtractRead>;
}

/** A source-owned Markdown extract, distinct from an original HTTP response body.
 * The caller supplies its directory; the registry never chooses a host path. */
export interface DocumentationRetainedExtract {
  fileName: string;
  extractSha256: string;
  extractBytes: number;
  lineRanges: [number, number][];
  retrievedAt: string | null;
  etag: null;
  lastModified: null;
}
export interface DocumentationExtractReadOptions {
  directory: string | URL;
  observations?: DocumentationObservations;
  /** Caller-supplied current-task evidence, not authenticated by this reader.
   * It must name the exact retained representation and applicable version. */
  revalidation?: {
    canonicalUrl: string;
    extractSha256: string;
    observedInCurrentTask: true;
    targetVersion?: string;
  };
}
export interface DocumentationExtractRead {
  plan: DocumentationPlan;
  excerpt: {
    kind: "retained-markdown-extract";
    canonicalUrl: string;
    extractSha256: string;
    excerptSha256: string;
    text: string;
    retrievedAt: string | null;
    etag: null;
    lastModified: null;
  } | null;
}
