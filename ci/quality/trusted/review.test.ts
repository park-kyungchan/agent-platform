import { expect, test } from "bun:test";
import { deriveReviewedSourceManifest } from "../receipt-validator";
import { validatePlatformReviewWithEvidence } from "../receipt-validation-gate";
import { sha256 } from "./policy";

function fixture() {
  const oid = (x: string) => x.repeat(40),
    head = oid("a"),
    originalHead = oid("b"),
    tree = oid("c"),
    receiptCommit = oid("d"),
    resultCommit = oid("e");
  const repository = "park-kyungchan/agent-platform";
  const treeResponse = {
    sha: tree,
    truncated: false,
    tree: [{ path: "source.ts", mode: "100644", type: "blob", sha: oid("f") }],
  };
  const manifest = deriveReviewedSourceManifest(treeResponse, tree);
  const binding = {
    codeOwner: "agent-platform",
    evidenceOwner: "codex-harness-engineering",
    scope: "canonical-source-ci-quality",
  };
  const author = { runtime: "native-codex", taskId: "fixture-author-native-thread" },
    reviewer = { runtime: "native-codex", taskId: "fixture-distinct-review-native-thread" };
  const proof = {
    binding,
    headSha: originalHead,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer,
    author,
    conclusion: "pass",
    unresolvedFindings: 0,
  };
  const result = {
    schema: "agent-platform.independent-review-result.v1",
    recordClass: "verification",
    ...proof,
  };
  const resultBytes = Buffer.from(JSON.stringify(result));
  const receipt = {
    schema: "native-independent-review.v2",
    ...proof,
    provenance: {
      owner: "codex-harness-engineering",
      resultRef: `https://github.com/${repository}/blob/${resultCommit}/native-review-results/${originalHead}.json`,
      sha256: sha256(resultBytes),
    },
  };
  const receiptBytes = Buffer.from(JSON.stringify(receipt));
  const successor = {
    schema: "native-independent-review-successor.v1",
    ...proof,
    headSha: head,
    reuse: { originalHead, receiptCommit, receiptSha256: sha256(receiptBytes) },
  };
  let state = "success",
    actualOriginalTree = tree;
  const content = (path: string, bytes: Buffer) => ({
    type: "file",
    path,
    encoding: "base64",
    content: bytes.toString("base64"),
    size: bytes.length,
  });
  const reader = {
    readCommit: async (sha: string) => ({ sha, tree: { sha: actualOriginalTree } }),
    readTree: async () => treeResponse,
    readStatuses: async () => [
      {
        id: 11,
        context: "native-independent-review",
        state,
        creator: { id: 241037670, login: "park-kyungchan" },
        target_url: `https://github.com/${repository}/blob/${receiptCommit}/native-reviews/${originalHead}.json`,
      },
    ],
    readFile: async (path: string, commit: string) => {
      if (path === `native-reviews/${originalHead}.json` && commit === receiptCommit)
        return content(path, receiptBytes);
      if (path === `native-review-results/${originalHead}.json` && commit === resultCommit)
        return content(path, resultBytes);
      throw Error("unknown immutable fixture file");
    },
  };
  return {
    successor,
    head,
    tree,
    repository,
    reader,
    publisher: { id: "241037670", login: "park-kyungchan" },
    hold() {
      state = "pending";
    },
    foreignTree() {
      actualOriginalTree = oid("0");
    },
  };
}
test("explicit identical-tree successor retains the complete original immutable result and latest status", async () => {
  const f = fixture();
  await expect(
    validatePlatformReviewWithEvidence(
      f.successor,
      f.head,
      f.tree,
      f.repository,
      f.reader,
      f.publisher,
    ),
  ).resolves.toBeUndefined();
  f.hold();
  await expect(
    validatePlatformReviewWithEvidence(
      f.successor,
      f.head,
      f.tree,
      f.repository,
      f.reader,
      f.publisher,
    ),
  ).rejects.toThrow("latest review held");
});
test("another tree, forged receipt digest, reviewer substitution or recursive reuse is refused", async () => {
  const f = fixture();
  f.foreignTree();
  await expect(
    validatePlatformReviewWithEvidence(
      f.successor,
      f.head,
      f.tree,
      f.repository,
      f.reader,
      f.publisher,
    ),
  ).rejects.toThrow("identical actual");
  const g = fixture();
  g.successor.reuse.receiptSha256 = "1".repeat(64);
  await expect(
    validatePlatformReviewWithEvidence(
      g.successor,
      g.head,
      g.tree,
      g.repository,
      g.reader,
      g.publisher,
    ),
  ).rejects.toThrow("exact bytes");
  const h = fixture();
  h.successor.reviewer = { ...h.successor.reviewer, taskId: "forged-native-reviewer" };
  await expect(
    validatePlatformReviewWithEvidence(
      h.successor,
      h.head,
      h.tree,
      h.repository,
      h.reader,
      h.publisher,
    ),
  ).rejects.toThrow("retain original");
  const i = fixture();
  Object.assign(i.successor.reuse, { unqualifiedAuthority: "source-only-digest" });
  await expect(
    validatePlatformReviewWithEvidence(
      i.successor,
      i.head,
      i.tree,
      i.repository,
      i.reader,
      i.publisher,
    ),
  ).rejects.toThrow("successor binding");
});
