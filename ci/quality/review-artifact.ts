import { createHash } from "node:crypto";
import { readArtifactMember } from "./artifact-archive";
import {
  deriveReviewedSourceManifest,
  validateExactNativeReview,
  validateNativeReviewSource,
  type NativeReviewIdentity,
} from "./receipt-validator";

export const reviewArtifactBranch = "main";
export const reviewArtifactWorkflow = ".github/workflows/native-review-evidence.yml";
export const reviewArtifactMember = "native-review.json";
export const reviewArtifactMaximum = 45_000;
export interface ReviewArtifactReader {
  json(path: string): Promise<unknown>;
  archive(id: number): Promise<Uint8Array>;
}
export interface ReviewArtifactPolicy {
  repository: string;
  publisher: { id: string; login: string; type: "User" | "Bot" };
  binding: Record<string, string>;
}
const object = (v: unknown): Record<string, unknown> =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
const sha256 = (v: Uint8Array) => createHash("sha256").update(v).digest("hex");
function need(v: unknown, code: string): asserts v {
  if (!v) throw Error("NATIVE_REVIEW_ARTIFACT_" + code);
}
function same(a: unknown, b: unknown) {
  return JSON.stringify(a) === JSON.stringify(b);
}
function publisher(raw: unknown, expected: ReviewArtifactPolicy["publisher"]) {
  const p = object(raw);
  return String(p.id) === expected.id && p.login === expected.login && p.type === expected.type;
}
export function reviewArtifactTarget(target: unknown, repository: string) {
  need(
    typeof target === "string" && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository),
    "TARGET_REQUIRED",
  );
  const prefix = `https://github.com/${repository}/actions/runs/`;
  need(target.startsWith(prefix), "SAME_REPOSITORY_TARGET_REQUIRED");
  const match = /^([1-9][0-9]*)\/artifacts\/([1-9][0-9]*)#sha256=([a-f0-9]{64})$/.exec(
    target.slice(prefix.length),
  );
  need(
    match && Number.isSafeInteger(Number(match[1])) && Number.isSafeInteger(Number(match[2])),
    "EXACT_RUN_ARTIFACT_DIGEST_REQUIRED",
  );
  return { runId: Number(match[1]), artifactId: Number(match[2]), digest: match[3]! };
}
export function validateCompletedReviewObservation(
  bytes: Uint8Array,
  policy: ReviewArtifactPolicy,
  head: string,
  tree: string,
  manifest: string,
) {
  need(bytes.length > 0 && bytes.length <= reviewArtifactMaximum, "RAW_SIZE_REQUIRED");
  const raw = object(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
  need(
    ["native-independent-review-output.v1", "owner-scoped-native-review-output.v1"].includes(
      String(raw.schema),
    ) && raw.reviewStatus === "complete",
    "NATIVE_RAW_OUTPUT_REQUIRED",
  );
  const binding = object(raw.binding);
  need(
    Object.keys(binding).sort().join("|") === Object.keys(policy.binding).sort().join("|") &&
      Object.keys(policy.binding).every((key) => binding[key] === policy.binding[key]),
    "EXACT_OWNER_BINDING_REQUIRED",
  );
  validateNativeReviewSource(raw, head, tree, manifest);
  need(
    Array.isArray(raw.findings) &&
      Number.isSafeInteger(raw.unresolvedFindings) &&
      Number(raw.unresolvedFindings) === raw.findings.length &&
      ((raw.conclusion === "pass" && raw.findings.length === 0) ||
        (raw.conclusion === "fail" && raw.findings.length > 0)),
    "COMPLETE_FINDING_COUNT_REQUIRED",
  );
  return raw as Record<string, unknown> & {
    reviewer: NativeReviewIdentity;
    author: NativeReviewIdentity;
  };
}
export function validateRawReviewArtifact(
  bytes: Uint8Array,
  policy: ReviewArtifactPolicy,
  head: string,
  tree: string,
  manifest: string,
) {
  const raw = validateCompletedReviewObservation(bytes, policy, head, tree, manifest);
  validateExactNativeReview(raw, head, tree, manifest);
  return raw;
}
/** One existing native status context per head; the actual pending status ID fences completion. */
export function reviewAttemptTarget(repository: string, head: string): string {
  need(
    /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository) && /^[a-f0-9]{40}$/.test(head),
    "EXACT_REVIEW_HEAD_REQUIRED",
  );
  return `https://github.com/${repository}/commit/${head}#native-independent-review`;
}
export function validateArtifactAttempt(
  history: unknown[],
  status: unknown,
  raw: Record<string, unknown>,
  policy: ReviewArtifactPolicy,
  head: string,
): void {
  const states = history.map(object).filter((s) => s.context === "native-independent-review"),
    pending = states[1];
  need(
    same(states[0], status) &&
      pending &&
      pending.state === "pending" &&
      Number.isSafeInteger(pending.id) &&
      Number(pending.id) > 0 &&
      pending.id === raw.nativeReviewPendingStatusId &&
      pending.id !== object(status).id &&
      publisher(pending.creator, policy.publisher) &&
      pending.target_url === reviewAttemptTarget(policy.repository, head),
    "REVIEW_ATTEMPT_SUPERSEDED",
  );
}
/** Historical legacy stays admissible until a later formal hold supersedes that proof. */
export function validateLegacyReviewAttempt(
  policy: { repository: string; publisher: { id: string; login?: string; type?: string } },
  head: string,
  history: unknown,
  expected: { id: unknown; target_url: unknown },
  receipt: unknown,
): void {
  need(Array.isArray(history) && history.length <= 100, "LEGACY_ATTEMPT_HISTORY_REQUIRED");
  const states = history.map(object).filter((s) => s.context === "native-independent-review"),
    latest = states[0];
  const owns = (s: Record<string, unknown>) =>
    String(object(s.creator).id) === policy.publisher.id &&
    (!policy.publisher.login || object(s.creator).login === policy.publisher.login) &&
    (!policy.publisher.type || object(s.creator).type === policy.publisher.type);
  need(
    latest &&
      latest.state === "success" &&
      latest.id === expected.id &&
      latest.target_url === expected.target_url &&
      owns(latest),
    "LEGACY_LATEST_STATUS_CHANGED",
  );
  const held = (s: Record<string, unknown>) =>
    ["pending", "failure", "error"].includes(String(s.state));
  const legacyMarker = `https://github.com/${policy.repository}/commit/${head}#native-review-attempt-`;
  const formal = states
    .slice(1)
    .some(
      (s) =>
        held(s) &&
        (s.target_url === reviewAttemptTarget(policy.repository, head) ||
          (typeof s.target_url === "string" && s.target_url.startsWith(legacyMarker))),
    );
  const prior = states.findIndex(
    (s, i) => i > 0 && s.state === "success" && s.target_url === latest.target_url && owns(s),
  );
  const restoredAfterHold = prior > 0 && states.slice(1, prior).some(held),
    proof = object(receipt),
    id = proof.nativeReviewPendingStatusId;
  if (id === undefined && !formal && !restoredAfterHold) return;
  need(
    ["native-independent-review.v2", "owner-scoped-independent-review.v1"].includes(
      String(proof.schema),
    ) &&
      Number.isSafeInteger(id) &&
      Number(id) > 0,
    "FRESH_LEGACY_ATTEMPT_REQUIRED",
  );
  const pending = states.findIndex(
    (s, i) =>
      i > 0 &&
      s.id === id &&
      s.state === "pending" &&
      s.target_url === reviewAttemptTarget(policy.repository, head) &&
      owns(s),
  );
  need(
    pending > 0 &&
      states
        .slice(1, pending)
        .every((s) => s.state === "success" && s.target_url === latest.target_url && owns(s)),
    "LEGACY_ATTEMPT_SUPERSEDED",
  );
}
/** Read-only transport. Only the separately authenticated native publisher can attest these raw bytes. */
export async function validateReviewArtifact(
  input: { policy: ReviewArtifactPolicy; head: string; status: unknown },
  reader: ReviewArtifactReader,
) {
  // Snapshot every policy/reader before an untrusted callback can yield.
  const policy = structuredClone(input.policy),
    head = input.head,
    status = structuredClone(object(input.status));
  const json = reader.json.bind(reader),
    archive = reader.archive.bind(reader);
  need(
    /^[a-f0-9]{40}$/.test(head) &&
      /^[1-9][0-9]*$/.test(policy.publisher.id) &&
      policy.publisher.type === "User",
    "EXTERNAL_NATIVE_PUBLISHER_REQUIRED",
  );
  need(
    status.context === "native-independent-review" &&
      status.state === "success" &&
      Number.isSafeInteger(status.id) &&
      Number(status.id) > 0 &&
      publisher(status.creator, policy.publisher),
    "AUTHENTICATED_STATUS_REQUIRED",
  );
  const target = reviewArtifactTarget(status.target_url, policy.repository);
  const run = object(await json(`actions/runs/${target.runId}`));
  const repo = object(run.repository),
    headRepo = object(run.head_repository);
  need(
    run.id === target.runId &&
      run.run_attempt === 1 &&
      run.event === "workflow_dispatch" &&
      run.status === "completed" &&
      run.conclusion === "success" &&
      publisher(run.actor, policy.publisher) &&
      publisher(run.triggering_actor, policy.publisher),
    "EXTERNAL_DISPATCH_RUN_REQUIRED",
  );
  need(
    repo.full_name === policy.repository &&
      Number.isSafeInteger(repo.id) &&
      Number(repo.id) > 0 &&
      headRepo.id === repo.id &&
      /^[a-f0-9]{40}$/.test(String(run.head_sha)) &&
      run.head_branch === reviewArtifactBranch &&
      (run.path === reviewArtifactWorkflow ||
        run.path === `${reviewArtifactWorkflow}@refs/heads/${reviewArtifactBranch}`),
    "EXACT_REPOSITORY_WORKFLOW_REQUIRED",
  );
  const artifact = object(await json(`actions/artifacts/${target.artifactId}`)),
    artifactRun = object(artifact.workflow_run);
  need(
    artifact.id === target.artifactId &&
      artifact.name === `native-review-${head}` &&
      artifact.expired === false &&
      Number.isSafeInteger(artifact.size_in_bytes) &&
      Number(artifact.size_in_bytes) > 0 &&
      Number(artifact.size_in_bytes) <= 100_000 &&
      artifact.digest === `sha256:${target.digest}`,
    "EXACT_IMMUTABLE_ARTIFACT_REQUIRED",
  );
  need(
    artifactRun.id === target.runId &&
      artifactRun.repository_id === repo.id &&
      artifactRun.head_repository_id === repo.id &&
      artifactRun.head_sha === run.head_sha,
    "ARTIFACT_RUN_JOIN_REQUIRED",
  );
  const bytes = readArtifactMember(
    await archive(target.artifactId),
    target.digest,
    reviewArtifactMember,
    reviewArtifactMaximum,
  );
  const commit = object(await json(`git/commits/${head}`)),
    tree = object(commit.tree).sha;
  need(commit.sha === head && typeof tree === "string", "EXACT_SOURCE_COMMIT_REQUIRED");
  const manifest = deriveReviewedSourceManifest(await json(`git/trees/${tree}?recursive=1`), tree);
  const raw = validateRawReviewArtifact(bytes, policy, head, tree, manifest.sha256);
  const latest = await json(`commits/${head}/statuses?per_page=100`);
  need(
    Array.isArray(latest) &&
      latest.length <= 100 &&
      same(
        latest.find((s) => object(s).context === "native-independent-review"),
        status,
      ),
    "LATEST_STATUS_CHANGED",
  );
  validateArtifactAttempt(latest as unknown[], status, raw, policy, head);
  return {
    schema: "native-independent-review-artifact-ci.v1",
    headSha: head,
    reviewedTreeSha: tree,
    manifest,
    artifact: target,
    rawSha256: sha256(bytes),
    reviewer: raw.reviewer,
    author: raw.author,
    publisherAuthenticated: true,
    nativeAuthenticationEstablished: false,
    nativePermissionGranted: false,
    acceptanceEstablished: false,
  };
}

/** The bearer goes only to api.github.com; signed storage redirects are fetched without it. */
export function authenticatedReviewReader(repository: string, token: string): ReviewArtifactReader {
  need(
    /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository) && token.length > 0,
    "AUTHENTICATED_READER_REQUIRED",
  );
  const fetchReview = globalThis.fetch.bind(globalThis);
  async function body(response: Response, maximum: number) {
    need(response.body, "BODY_REQUIRED");
    const reader = response.body.getReader(),
      chunks: Uint8Array[] = [];
    let size = 0;
    try {
      for (;;) {
        const next = await reader.read();
        if (next.done) break;
        size += next.value.length;
        need(size <= maximum, "BODY_LIMIT");
        chunks.push(next.value);
      }
      return Buffer.concat(chunks);
    } finally {
      await reader.cancel();
      reader.releaseLock();
    }
  }
  async function api(path: string) {
    need(
      (path === "." || /^[A-Za-z0-9_.?=&/-]+$/.test(path)) && !path.split(/[/?]/).includes(".."),
      "FIXED_API_PATH_REQUIRED",
    );
    return fetchReview(
      `https://api.github.com/repos/${repository}${path === "." ? "" : "/" + path}`,
      {
        redirect: "manual",
        signal: AbortSignal.timeout(30_000),
        headers: {
          Authorization: "Bearer " + token,
          Accept: "application/vnd.github+json",
          "X-GitHub-Api-Version": "2026-03-10",
        },
      },
    );
  }
  return {
    async json(path) {
      const response = await api(path);
      need(response.status === 200, "API_READ_FAILED");
      return JSON.parse((await body(response, 8_000_000)).toString()) as unknown;
    },
    async archive(id) {
      need(Number.isSafeInteger(id) && id > 0, "ARTIFACT_ID_REQUIRED");
      const response = await api(`actions/artifacts/${id}/zip`);
      need(response.status === 302, "STORAGE_REDIRECT_REQUIRED");
      const location = response.headers.get("location");
      await response.body?.cancel();
      need(location, "STORAGE_LOCATION_REQUIRED");
      const url = new URL(location);
      need(
        url.protocol === "https:" &&
          !url.username &&
          !url.password &&
          !url.port &&
          !url.hash &&
          (url.hostname.endsWith(".blob.core.windows.net") ||
            url.hostname.endsWith(".actions.githubusercontent.com")),
        "STORAGE_ORIGIN_REFUSED",
      );
      const download = await fetchReview(url, {
        redirect: "error",
        credentials: "omit",
        signal: AbortSignal.timeout(30_000),
      });
      need(download.status === 200, "ARCHIVE_READ_FAILED");
      return body(download, 100_000);
    },
  };
}

/** The native publisher must compare transport bytes to its actual independent observation. */
export async function validatePublishedReviewArtifact(
  input: { policy: ReviewArtifactPolicy; head: string; status: unknown; rawBytes: Uint8Array },
  reader: ReviewArtifactReader,
) {
  const nativeRawSha256 = sha256(input.rawBytes);
  const result = await validateReviewArtifact(input, reader);
  need(result.rawSha256 === nativeRawSha256, "ORIGINAL_NATIVE_BYTES_REQUIRED");
  return result;
}
