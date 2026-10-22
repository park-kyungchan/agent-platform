import { reviewGate } from "../../source-check";
import {
  insist,
  object,
  repository,
  sha256,
  same,
  type Subject,
  type TrustedPolicy,
} from "./policy";
import { repoPath, type ProviderReader } from "./provider";
import { readAuthor } from "./source";

/** Adapt authenticated reads to the existing P-owned exact receipt/result/tree verifier. */
export async function verifyReview(
  reader: ProviderReader,
  p: TrustedPolicy,
  root: string,
  subject: Subject,
) {
  const author = await readAuthor(root, subject);
  let receiptSha256: string | null = null,
    receipt: Record<string, unknown> | null = null;
  let latest: Record<string, unknown> | null = null;
  async function readApi(path: string): Promise<unknown> {
    const raw = await reader.json(repoPath(path));
    if (path === `commits/${subject.commit}/statuses?per_page=100`) {
      insist(Array.isArray(raw) && raw.length <= 100, "REVIEW_STATUS_PAGE_REQUIRED");
      latest = object(raw.find((x) => object(x).context === "native-independent-review"));
      insist(
        Number.isSafeInteger(latest.id) && latest.state === "success",
        "LATEST_REVIEW_HELD_OR_MISSING",
      );
    }
    if (path.startsWith(`contents/native-reviews/${subject.commit}.json?ref=`)) {
      const content = object(raw);
      insist(
        content.encoding === "base64" && typeof content.content === "string",
        "IMMUTABLE_REVIEW_BYTES_REQUIRED",
      );
      const bytes = Buffer.from(content.content, "base64");
      receiptSha256 = sha256(bytes);
      receipt = object(JSON.parse(bytes.toString()));
      // The historical v1 branch remains canonical, but cannot satisfy this new v2 source gate.
      insist(
        ["native-independent-review.v2", "native-independent-review-successor.v1"].includes(
          String(receipt.schema),
        ),
        "FULL_MANIFEST_AND_RESULT_REVIEW_REQUIRED",
      );
      insist(
        receipt.reviewedSourceManifestSha256 === subject.sourceManifestSha256 &&
          object(receipt.author).runtime === author.metadata.author.runtime &&
          object(receipt.author).taskId === author.metadata.author.threadId,
        "OBSERVED_SOURCE_AUTHOR_REVIEW_JOIN_REQUIRED",
      );
      // The canonical P gate validates a direct, explicit full-proof successor; no string-only reuse.
      insist(!Object.hasOwn(receipt, "successor"), "UNQUALIFIED_SUCCESSOR_REUSE_REFUSED");
    }
    return raw;
  }
  await reviewGate({
    env: {
      GITHUB_REPOSITORY: repository,
      REVIEW_HEAD_SHA: subject.commit,
      GITHUB_TOKEN: "authenticated-read-callback-only",
      NATIVE_REVIEW_STATUS_CREATOR_ID: String(p.reviewPublisher.id),
      NATIVE_REVIEW_STATUS_CREATOR_LOGIN: p.reviewPublisher.login,
    },
    readApi,
  });
  insist(receiptSha256 && receipt && latest, "FULL_REVIEW_CHAIN_REQUIRED");
  const current = await reader.json(repoPath(`commits/${subject.commit}/statuses?per_page=100`));
  insist(
    Array.isArray(current) &&
      same(object(current.find((x) => object(x).context === "native-independent-review")), latest),
    "REVIEW_STATUS_CHANGED",
  );
  return {
    receiptSha256,
    latestStatus: latest,
    sourceAuthor: author,
    nativeAuthenticationEstablished: false,
    nativePermissionGranted: false,
  };
}
