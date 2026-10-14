import { createHash } from "node:crypto";

export type ReviewBinding = {
  codeOwner: "agent-platform";
  evidenceOwner: "codex-harness-engineering";
  scope: "canonical-source-ci-quality";
};
export type NativeReviewIdentity = { runtime: "native-codex"; taskId: string };
export type ReviewReceiptV2 = {
  schema: "native-independent-review.v2";
  binding: ReviewBinding;
  headSha: string;
  reviewedTreeSha: string;
  reviewedSourceManifestSha256: string;
  reviewer: NativeReviewIdentity;
  author: NativeReviewIdentity;
  conclusion: "pass";
  unresolvedFindings: 0;
  provenance: {
    owner: "codex-harness-engineering";
    resultRef: string;
    sha256: string;
  };
};
export type ReviewResultV1 = Omit<ReviewReceiptV2, "schema" | "provenance"> & {
  schema: "agent-platform.independent-review-result.v1";
  recordClass: "verification";
};
export type ReviewedSourceEntry = {
  path: string;
  mode: string;
  type: "blob" | "tree";
  sha: string;
};
export type VerifiedManifest = {
  schema: "agent-platform.reviewed-source-manifest.v1";
  reviewedTreeSha: string;
  entryCount: number;
  blobCount: number;
  sha256: string;
};
export type ImmutableResult = { commit: string; path: string; sha256: string };
type ReviewContext = { repository: string; sourceManifestSha256: string };
type ObjectValue = Record<string, unknown>;

const hash = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const object = (value: unknown): ObjectValue =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as ObjectValue)
    : {};
const fullSha = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{40}$/.test(value);
const digest = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const taskId = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= 256 &&
  value.trim() === value &&
  !/[\s\u0000-\u001f\u007f]/.test(value);

// This retains the v1 Hermes-only predicate, including its historical author shape.
// A v1 receipt never enters the new ownership or manifest contract.
export function validateLegacyReceipt(receipt: unknown, head: string, tree: string): void {
  const value = object(receipt);
  const reviewer = object(value.reviewer);
  const author = object(value.author);
  const provenance = object(value.provenance);
  if (
    !fullSha(head) ||
    !fullSha(tree) ||
    value.schema !== "native-independent-review.v1" ||
    value.headSha !== head ||
    value.reviewedTreeSha !== tree ||
    reviewer.runtime !== "native-codex" ||
    typeof reviewer.taskId !== "string" ||
    !reviewer.taskId.trim() ||
    typeof author.taskId !== "string" ||
    !author.taskId.trim() ||
    reviewer.taskId === author.taskId ||
    value.conclusion !== "pass" ||
    value.unresolvedFindings !== 0 ||
    provenance.owner !== "hermes-harness-engineering" ||
    typeof provenance.resultRef !== "string" ||
    !provenance.resultRef.trim() ||
    !digest(provenance.sha256)
  ) {
    throw new Error(
      "Native independent review receipt does not prove the exact reviewed head/tree and independent native reviewer",
    );
  }
}

export function deriveReviewedSourceManifest(response: unknown, tree: string): VerifiedManifest {
  const value = object(response);
  if (
    !fullSha(tree) ||
    value.sha !== tree ||
    value.truncated !== false ||
    !Array.isArray(value.tree) ||
    value.tree.length < 1 ||
    value.tree.length > 4096
  ) {
    throw new Error("Reviewed source tree is unavailable, truncated or outside the manifest bound");
  }
  const seen = new Set<string>();
  const entries: ReviewedSourceEntry[] = value.tree.map((entry: unknown) => {
    const item = object(entry);
    if (
      typeof item.path !== "string" ||
      !item.path ||
      item.path.length > 512 ||
      /[\\\u0000-\u001f\u007f]/.test(item.path) ||
      item.path
        .split("/")
        .some((segment) => !segment || segment === "." || segment === ".." || segment === ".git") ||
      seen.has(item.path) ||
      !fullSha(item.sha) ||
      !(
        (item.type === "tree" && item.mode === "040000") ||
        (item.type === "blob" && ["100644", "100755", "120000"].includes(String(item.mode)))
      )
    ) {
      throw new Error("Reviewed source tree contains duplicate, unsafe or unsupported entries");
    }
    seen.add(item.path);
    return {
      path: item.path,
      mode: String(item.mode),
      type: item.type as "tree" | "blob",
      sha: item.sha,
    };
  });
  entries.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
  const blobCount = entries.filter((entry) => entry.type === "blob").length;
  if (!blobCount) throw new Error("Reviewed source manifest contains no source blobs");
  return {
    schema: "agent-platform.reviewed-source-manifest.v1",
    reviewedTreeSha: tree,
    entryCount: entries.length,
    blobCount,
    sha256: hash("agent-platform.reviewed-source-manifest.v1\n" + JSON.stringify(entries)),
  };
}

function validateV2Binding(
  receipt: ObjectValue,
  head: string,
  tree: string,
  manifestSha256: string,
): void {
  const binding = object(receipt.binding);
  const reviewer = object(receipt.reviewer);
  const author = object(receipt.author);
  if (
    !fullSha(head) ||
    !fullSha(tree) ||
    !digest(manifestSha256) ||
    binding.codeOwner !== "agent-platform" ||
    binding.evidenceOwner !== "codex-harness-engineering" ||
    binding.scope !== "canonical-source-ci-quality" ||
    Object.keys(binding).sort().join("|") !== "codeOwner|evidenceOwner|scope" ||
    receipt.headSha !== head ||
    receipt.reviewedTreeSha !== tree ||
    receipt.reviewedSourceManifestSha256 !== manifestSha256 ||
    reviewer.runtime !== "native-codex" ||
    author.runtime !== "native-codex" ||
    !taskId(reviewer.taskId) ||
    !taskId(author.taskId) ||
    reviewer.taskId === author.taskId ||
    receipt.conclusion !== "pass" ||
    receipt.unresolvedFindings !== 0
  ) {
    throw new Error(
      "Receipt does not prove the exact v2 ownership, scope, head, tree, manifest and independent native identities",
    );
  }
}

export function validateIndependentReviewReceipt(
  receipt: unknown,
  head: string,
  tree: string,
  context?: ReviewContext,
): ImmutableResult | undefined {
  const value = object(receipt);
  if (value.schema === "native-independent-review.v1") {
    validateLegacyReceipt(value, head, tree);
    return undefined;
  }
  if (
    value.schema !== "native-independent-review.v2" ||
    !context ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(context.repository)
  ) {
    throw new Error("Unsupported native independent review receipt schema or missing v2 context");
  }
  validateV2Binding(value, head, tree, context.sourceManifestSha256);
  const provenance = object(value.provenance);
  if (
    provenance.owner !== "codex-harness-engineering" ||
    typeof provenance.resultRef !== "string" ||
    !digest(provenance.sha256)
  ) {
    throw new Error("v2 independent review provenance owner or digest mismatch");
  }
  const target = new URL(provenance.resultRef);
  const prefix = `/${context.repository}/blob/`;
  const path = `native-review-results/${head}.json`;
  const suffix = "/" + path;
  if (
    target.protocol !== "https:" ||
    target.hostname !== "github.com" ||
    target.port ||
    target.username ||
    target.password ||
    target.search ||
    target.hash ||
    !target.pathname.startsWith(prefix) ||
    !target.pathname.endsWith(suffix)
  ) {
    throw new Error("v2 result provenance must be an immutable same-repository result file");
  }
  const commit = target.pathname.slice(prefix.length, -suffix.length);
  if (!fullSha(commit) || commit === head) {
    throw new Error("v2 result provenance requires a separate immutable commit");
  }
  return { commit, path, sha256: provenance.sha256 };
}

export function decodeBoundedReviewFile(
  content: unknown,
  path: string,
  maximumBytes: number,
): Uint8Array {
  const value = object(content);
  if (
    value.type !== "file" ||
    value.path !== path ||
    value.encoding !== "base64" ||
    typeof value.content !== "string" ||
    !Number.isSafeInteger(value.size) ||
    typeof value.size !== "number" ||
    value.size <= 0 ||
    value.size > maximumBytes
  ) {
    throw new Error("Independent review file is not a bounded repository file");
  }
  const base64 = value.content.replace(/\n/g, "");
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(base64)) {
    throw new Error("Independent review file is not valid base64");
  }
  const bytes = Buffer.from(base64, "base64");
  if (bytes.length !== value.size) throw new Error("Independent review file byte size mismatch");
  return bytes;
}

export function verifyIndependentReviewResult(
  receipt: unknown,
  bytes: Uint8Array,
  expected: ImmutableResult,
  head: string,
  tree: string,
  manifestSha256: string,
): void {
  if (hash(bytes) !== expected.sha256)
    throw new Error("Immutable independent review result digest mismatch");
  const result = object(JSON.parse(Buffer.from(bytes).toString("utf8")));
  if (
    result.schema !== "agent-platform.independent-review-result.v1" ||
    result.recordClass !== "verification"
  ) {
    throw new Error("Immutable independent review result schema mismatch");
  }
  validateV2Binding(result, head, tree, manifestSha256);
  const value = object(receipt);
  if (
    object(result.reviewer).taskId !== object(value.reviewer).taskId ||
    object(result.author).taskId !== object(value.author).taskId
  ) {
    throw new Error("Immutable independent review result identities differ from its receipt");
  }
}
