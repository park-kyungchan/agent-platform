import {
  decodeBoundedReviewFile,
  deriveReviewedSourceManifest,
  validateIndependentReviewReceipt,
  verifyIndependentReviewResult,
  type VerifiedManifest,
} from "./receipt-validator";

export type ReviewEvidenceReader = {
  readTree: (treeSha: string) => Promise<unknown>;
  readFile: (path: string, commitSha: string) => Promise<unknown>;
};

export async function validateReceiptWithEvidence(
  receipt: unknown,
  head: string,
  tree: string,
  repository: string,
  reader: ReviewEvidenceReader,
): Promise<{ version: 1 | 2; manifest?: VerifiedManifest }> {
  if (typeof receipt !== "object" || receipt === null || !("schema" in receipt)) {
    throw new Error("Independent review receipt is unavailable");
  }
  if (receipt.schema === "native-independent-review.v1") {
    validateIndependentReviewReceipt(receipt, head, tree);
    return { version: 1 };
  }
  if (receipt.schema !== "native-independent-review.v2") {
    throw new Error("Unsupported native independent review receipt schema");
  }
  const manifest = deriveReviewedSourceManifest(await reader.readTree(tree), tree);
  const result = validateIndependentReviewReceipt(receipt, head, tree, {
    repository,
    sourceManifestSha256: manifest.sha256,
  });
  if (!result) throw new Error("v2 receipt did not resolve immutable result provenance");
  const bytes = decodeBoundedReviewFile(
    await reader.readFile(result.path, result.commit),
    result.path,
    65536,
  );
  verifyIndependentReviewResult(receipt, bytes, result, head, tree, manifest.sha256);
  return { version: 2, manifest };
}
