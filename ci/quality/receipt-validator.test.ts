import { createHash } from "node:crypto";
import { describe, expect, test } from "bun:test";
import {
  deriveReviewedSourceManifest,
  validateIndependentReviewReceipt,
  type ReviewReceiptV2,
  type ReviewResultV1,
} from "./receipt-validator";
import { validateReceiptWithEvidence, type ReviewEvidenceReader } from "./receipt-validation-gate";

const head = "a".repeat(40),
  tree = "b".repeat(40),
  resultCommit = "c".repeat(40);
const repository = "owner/platform";
const treeResponse = {
  sha: tree,
  truncated: false,
  tree: [
    { path: "ci/source-check.ts", mode: "100644", type: "blob", sha: "d".repeat(40) },
    { path: "ci", mode: "040000", type: "tree", sha: "e".repeat(40) },
    { path: "package.json", mode: "100644", type: "blob", sha: "f".repeat(40) },
  ],
};
const manifest = deriveReviewedSourceManifest(treeResponse, tree);

function fixture() {
  const result: ReviewResultV1 = {
    schema: "agent-platform.independent-review-result.v1",
    recordClass: "verification",
    binding: {
      codeOwner: "agent-platform",
      evidenceOwner: "codex-harness-engineering",
      scope: "canonical-source-ci-quality",
    },
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer: { runtime: "native-codex", taskId: "native-reviewer-thread" },
    author: { runtime: "native-codex", taskId: "native-author-thread" },
    conclusion: "pass",
    unresolvedFindings: 0,
  };
  const resultBytes = Buffer.from(JSON.stringify(result));
  const receipt: ReviewReceiptV2 = {
    ...result,
    schema: "native-independent-review.v2",
    provenance: {
      owner: "codex-harness-engineering",
      resultRef: `https://github.com/${repository}/blob/${resultCommit}/native-review-results/${head}.json`,
      sha256: createHash("sha256").update(resultBytes).digest("hex"),
    },
  };
  const reads: string[] = [];
  const reader: ReviewEvidenceReader = {
    readTree: async (sha) => {
      reads.push("tree:" + sha);
      return treeResponse;
    },
    readFile: async (path, commit) => {
      reads.push("file:" + commit + ":" + path);
      return {
        type: "file",
        path,
        encoding: "base64",
        content: resultBytes.toString("base64"),
        size: resultBytes.length,
      };
    },
  };
  return { result, resultBytes, receipt, reader, reads };
}

const legacy = {
  schema: "native-independent-review.v1",
  headSha: head,
  reviewedTreeSha: tree,
  reviewer: { runtime: "native-codex", taskId: "reviewer" },
  author: { taskId: "author" },
  conclusion: "pass",
  unresolvedFindings: 0,
  provenance: {
    owner: "hermes-harness-engineering",
    resultRef: "historical-native-result",
    sha256: "1".repeat(64),
  },
};

describe("versioned exact-head independent review receipt", () => {
  test("preserves the Hermes v1 receipt without entering v2 evidence reads", async () => {
    const { reader, reads } = fixture();
    expect(await validateReceiptWithEvidence(legacy, head, tree, repository, reader)).toEqual({
      version: 1,
    });
    expect(reads).toEqual([]);
    expect(() =>
      validateIndependentReviewReceipt(
        { ...legacy, provenance: { ...legacy.provenance, owner: "codex-harness-engineering" } },
        head,
        tree,
      ),
    ).toThrow();
  });
  test("derives a stable manifest from all actual tree entries independent of API order", () => {
    expect(
      deriveReviewedSourceManifest(
        { ...treeResponse, tree: [...treeResponse.tree].reverse() },
        tree,
      ),
    ).toEqual(manifest);
    expect(manifest.entryCount).toBe(3);
    expect(manifest.blobCount).toBe(2);
    expect(
      deriveReviewedSourceManifest({ ...treeResponse, tree: treeResponse.tree.slice(0, 2) }, tree)
        .sha256,
    ).not.toBe(manifest.sha256);
  });
  test("rejects truncated, duplicate, unsafe, unsupported and excessive manifests", () => {
    for (const response of [
      { ...treeResponse, truncated: true },
      { ...treeResponse, sha: "0".repeat(40) },
      { ...treeResponse, tree: [] },
      { ...treeResponse, tree: [...treeResponse.tree, treeResponse.tree[0]] },
      { ...treeResponse, tree: [{ ...treeResponse.tree[0], path: "../escape" }] },
      { ...treeResponse, tree: [{ ...treeResponse.tree[0], type: "commit", mode: "160000" }] },
      { ...treeResponse, tree: Array.from({ length: 4097 }, () => treeResponse.tree[0]) },
    ])
      expect(() => deriveReviewedSourceManifest(response, tree)).toThrow();
  });
  test("verifies v2 against the actual immutable head tree and a digest-verified result", async () => {
    const { receipt, reader, reads } = fixture();
    expect(await validateReceiptWithEvidence(receipt, head, tree, repository, reader)).toEqual({
      version: 2,
      manifest,
    });
    expect(reads).toEqual([
      "tree:" + tree,
      `file:${resultCommit}:native-review-results/${head}.json`,
    ]);
  });
  test("rejects unknown owner, evidence owner, scope and a mixed Hermes-v2 contract", () => {
    const { receipt } = fixture();
    for (const binding of [
      { ...receipt.binding, codeOwner: "unknown" },
      { ...receipt.binding, evidenceOwner: "hermes-harness-engineering" },
      { ...receipt.binding, scope: "arbitrary-runtime-work" },
      { ...receipt.binding, other: "extra-boundary" },
    ])
      expect(() =>
        validateIndependentReviewReceipt({ ...receipt, binding }, head, tree, {
          repository,
          sourceManifestSha256: manifest.sha256,
        }),
      ).toThrow();
    expect(() =>
      validateIndependentReviewReceipt(
        { ...receipt, provenance: { ...receipt.provenance, owner: "hermes-harness-engineering" } },
        head,
        tree,
        { repository, sourceManifestSha256: manifest.sha256 },
      ),
    ).toThrow();
  });
  test("rejects stale head, tree, manifest, self-review and non-native identities", () => {
    const { receipt } = fixture();
    for (const changed of [
      { ...receipt, headSha: "0".repeat(40) },
      { ...receipt, reviewedTreeSha: "0".repeat(40) },
      { ...receipt, reviewedSourceManifestSha256: "0".repeat(64) },
      { ...receipt, reviewer: receipt.author },
      { ...receipt, reviewer: { ...receipt.author, taskId: " " + receipt.author.taskId } },
      { ...receipt, reviewer: { ...receipt.reviewer, taskId: "reviewer\nidentity" } },
      { ...receipt, author: { ...receipt.author, taskId: "author\u0000identity" } },
      { ...receipt, author: { ...receipt.author, runtime: "other" } },
      { ...receipt, reviewer: { ...receipt.reviewer, runtime: "other" } },
      { ...receipt, conclusion: "fail" },
      { ...receipt, unresolvedFindings: 1 },
      { ...receipt, schema: "native-independent-review.v3" },
    ])
      expect(() =>
        validateIndependentReviewReceipt(changed, head, tree, {
          repository,
          sourceManifestSha256: manifest.sha256,
        }),
      ).toThrow();
    expect(() => validateIndependentReviewReceipt(receipt, head, tree)).toThrow();
  });
  test("rejects mutable, external, cross-repository and wrong-result provenance", () => {
    const { receipt } = fixture();
    for (const resultRef of [
      receipt.provenance.resultRef.replace(resultCommit, "main"),
      receipt.provenance.resultRef.replace("github.com", "evil.example"),
      receipt.provenance.resultRef.replace("owner/platform", "other/platform"),
      receipt.provenance.resultRef + "?raw=1",
      receipt.provenance.resultRef.replace("native-review-results", "unreviewed-data"),
      receipt.provenance.resultRef.replace(resultCommit, head),
    ])
      expect(() =>
        validateIndependentReviewReceipt(
          { ...receipt, provenance: { ...receipt.provenance, resultRef } },
          head,
          tree,
          { repository, sourceManifestSha256: manifest.sha256 },
        ),
      ).toThrow();
  });
  test("rejects actual result corruption and stale result bindings even with a matching result digest", async () => {
    const { receipt, reader, result } = fixture();
    await expect(
      validateReceiptWithEvidence(
        { ...receipt, provenance: { ...receipt.provenance, sha256: "0".repeat(64) } },
        head,
        tree,
        repository,
        reader,
      ),
    ).rejects.toThrow("digest mismatch");
    for (const changed of [
      { ...result, headSha: "0".repeat(40) },
      { ...result, reviewer: { ...result.reviewer, taskId: "different-reviewer" } },
      { ...result, recordClass: "AcceptanceDecision" },
    ]) {
      const bytes = Buffer.from(JSON.stringify(changed));
      const updated = {
        ...receipt,
        provenance: {
          ...receipt.provenance,
          sha256: createHash("sha256").update(bytes).digest("hex"),
        },
      };
      await expect(
        validateReceiptWithEvidence(updated, head, tree, repository, {
          ...reader,
          readFile: async (path) => ({
            type: "file",
            path,
            encoding: "base64",
            content: bytes.toString("base64"),
            size: bytes.length,
          }),
        }),
      ).rejects.toThrow();
    }
  });
  test("rejects invalid encoded bytes and unexpected result files before trusting content", async () => {
    const { receipt, reader, resultBytes } = fixture();
    for (const changed of [
      { path: "other.json" },
      { size: resultBytes.length + 1 },
      { content: "!invalid" },
      { size: 65537 },
    ])
      await expect(
        validateReceiptWithEvidence(receipt, head, tree, repository, {
          ...reader,
          readFile: async (path) => ({
            type: "file",
            path,
            encoding: "base64",
            content: resultBytes.toString("base64"),
            size: resultBytes.length,
            ...changed,
          }),
        }),
      ).rejects.toThrow();
  });
});
