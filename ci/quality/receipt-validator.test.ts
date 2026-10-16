import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import type { EvidenceArtifact, VerificationArtifact, JsonObject } from "@palantirkc/ontology";
import { createHash } from "node:crypto";
import { describe, expect, test, spyOn } from "bun:test";
import {
  deriveReviewedSourceManifest,
  validateIndependentReviewReceipt,
  type ReviewReceiptV2,
  type ReviewResultV1,
  type OwnerReviewPolicy,
  type OwnerReviewProof,
  type OwnerReviewReceipt,
  type OwnerReviewResult,
  type OwnerRawNativeReview,
  type ImmutableResult,
} from "./receipt-validator";
import {
  validateReceiptWithEvidence,
  validateOwnerScopedReviewWithEvidence,
  ownerScopedReviewGate,
  type ReviewEvidenceReader,
  type OwnerReviewReader,
} from "./receipt-validation-gate";

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

function ownerFixture(
  change?: (values: {
    raw: OwnerRawNativeReview;
    result: OwnerReviewResult;
    evidence: EvidenceArtifact;
    verification: VerificationArtifact;
  }) => void,
) {
  const policy: OwnerReviewPolicy = {
    repository: "owner/math",
    projectId: "math-research-module",
    domainId: "mathematics-education",
    semanticOwner: "math-research-module",
    scope: "portable-source-ci-module-contract",
    statusContext: "native-independent-review",
    publisher: { id: "123", login: "fixture-publisher", type: "User" },
  };
  const { publisher: _publisher, statusContext: _context, ...binding } = policy;
  const proof: OwnerReviewProof = {
    binding,
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer: { runtime: "native-codex", taskId: "/fixture/independent-review" },
    author: { runtime: "native-codex", taskId: "/fixture/author" },
    conclusion: "pass",
    unresolvedFindings: 0,
  };
  const raw: OwnerRawNativeReview = {
    ...structuredClone(proof),
    schema: "owner-scoped-native-review-output.v1",
    reviewStatus: "complete",
    findings: [],
  };
  const result: OwnerReviewResult = {
    ...structuredClone(proof),
    schema: "owner-scoped-independent-review-result.v1",
    custody: {
      classification: "source-custody-native-parent-observation",
      publisher: policy.publisher,
      rawNativeResult: {
        commit: resultCommit,
        path: `native-review-raw/${head}.json`,
        sha256: "0".repeat(64),
      },
    },
  };
  const rawCommit = "2".repeat(40);
  const rawPath = `native-review-raw/${head}.json`;
  const evidence: EvidenceArtifact = {
    schemaVersion: "1.0.0",
    kind: "evidence",
    id: "math-research-module:evidence:review",
    projectId: policy.projectId,
    domainId: policy.domainId,
    createdAt: "2026-10-17T00:00:00Z",
    payload: {
      claim: "Exact independent source review observed",
      method: "native-codex-independent-source-review",
      sourceUrl: `https://github.com/${policy.repository}/blob/${rawCommit}/${rawPath}`,
      fetchedAt: "2026-10-17T00:00:00Z",
      upstreamRevision: head,
      result: { outcome: "success", processExitCode: 0, contentMatched: true },
      attempts: [{ attempt: 1, outcome: "success" }],
      limitations: ["Source custody does not authenticate a native process"],
      details: {
        semanticOwner: policy.semanticOwner,
        review: structuredClone(proof) as unknown as JsonObject,
      },
    },
  };
  const verification: VerificationArtifact = {
    schemaVersion: "1.0.0",
    kind: "verification",
    id: "math-research-module:verification:review",
    projectId: policy.projectId,
    domainId: policy.domainId,
    createdAt: "2026-10-17T00:00:00Z",
    payload: {
      targetRefs: [evidence.id],
      criteria: [
        {
          id: "independent-native-source-review",
          status: "pass",
          message: "Exact source independent pass",
          evidenceRefs: [evidence.id],
        },
      ],
      inputHashes: [],
      runtime: { name: "native-codex", version: "fixture-only" },
      details: {
        semanticOwner: policy.semanticOwner,
        review: structuredClone(proof) as unknown as JsonObject,
      },
    },
  };
  change?.({ raw, result, evidence, verification });
  const files = new Map<string, Buffer>();
  function put(role: string, value: unknown): ImmutableResult {
    const path = `${role}/${head}.json`,
      bytes = Buffer.from(JSON.stringify(value));
    files.set(path, bytes);
    return { commit: resultCommit, path, sha256: createHash("sha256").update(bytes).digest("hex") };
  }
  const rawRef = { ...put("native-review-raw", raw), commit: rawCommit };
  result.custody = {
    classification: "source-custody-native-parent-observation",
    publisher: policy.publisher,
    rawNativeResult: rawRef,
  };
  const resultRef = put("native-review-results", result),
    evidenceRef = put("native-review-evidence", evidence);
  verification.payload.inputHashes = [resultRef, evidenceRef, rawRef].map(({ path, sha256 }) => ({
    path,
    sha256,
  }));
  const verificationRef = put("native-review-verifications", verification);
  const receipt: OwnerReviewReceipt = {
    ...proof,
    schema: "owner-scoped-independent-review.v1",
    provenance: {
      owner: policy.semanticOwner,
      result: resultRef,
      evidence: evidenceRef,
      verification: verificationRef,
      rawNativeResult: rawRef,
    },
  };
  const receiptCommit = "1".repeat(40),
    receiptPath = `native-reviews/${head}.json`;
  const reads: string[] = [];
  const statuses = [
    {
      id: 42,
      context: policy.statusContext,
      state: "success",
      creator: { ...policy.publisher, id: 123 },
      target_url: `https://github.com/${policy.repository}/blob/${receiptCommit}/${receiptPath}`,
    },
  ];
  const reader: OwnerReviewReader = {
    readCommit: async (sha) => {
      reads.push("commit:" + sha);
      const parent =
        sha === rawCommit
          ? head
          : sha === resultCommit
            ? rawCommit
            : sha === receiptCommit
              ? resultCommit
              : undefined;
      return { sha, tree: { sha: tree }, parents: parent ? [{ sha: parent }] : [] };
    },
    readStatuses: async (sha) => {
      reads.push("statuses:" + sha);
      return statuses;
    },
    readTree: async (sha) => {
      reads.push("tree:" + sha);
      return treeResponse;
    },
    readFile: async (path, ref) => {
      reads.push(`file:${ref}:${path}`);
      const bytes = path === receiptPath ? Buffer.from(JSON.stringify(receipt)) : files.get(path)!;
      if (
        ref !== (path === receiptPath ? receiptCommit : path === rawPath ? rawCommit : resultCommit)
      )
        throw Error("Unexpected fixture commit");
      return {
        type: "file",
        path,
        encoding: "base64",
        content: bytes.toString("base64"),
        size: bytes.length,
      };
    },
  };
  return {
    policy,
    receipt,
    reader,
    reads,
    statuses,
    files,
    proof,
    receiptCommit,
    receiptPath,
    rawCommit,
  };
}

describe("additive owner-scoped review gate", () => {
  test("binds authenticated publisher, exact source, unchanged raw output and canonical Evidence/Verification", async () => {
    const { policy, reader, reads } = ownerFixture();
    const verified = await validateOwnerScopedReviewWithEvidence(
      policy,
      { repository: policy.repository, head },
      reader,
    );
    expect(verified.binding.semanticOwner).toBe("math-research-module");
    expect(verified.manifest).toEqual(manifest);
    expect(verified.publisherAuthenticated).toBe(true);
    expect(verified.nativeAuthenticationEstablished).toBe(false);
    expect(verified.nativePermissionGranted).toBe(false);
    expect(verified.acceptanceEstablished).toBe(false);
    expect(reads.filter((path) => path.startsWith("file:"))).toHaveLength(5);
  });
  test("constructs real acyclic Git publication commits before validating unchanged raw output", async () => {
    const root = await mkdtemp(join(tmpdir(), "owner-review-publication-"));
    const git = (...args: string[]) =>
      execFileSync("git", ["-C", root, ...args], {
        env: { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null" },
        stdio: ["ignore", "pipe", "pipe"],
      });
    const commit = (message: string) => {
      git("add", ".");
      git(
        "-c",
        "user.name=Source review fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        message,
      );
      return git("rev-parse", "HEAD").toString().trim();
    };
    const readCommit = async (sha: string) => {
      const [identity, treeSha, parents] = git("show", "-s", "--format=%H%n%T%n%P", sha)
        .toString()
        .trim()
        .split("\n");
      return {
        sha: identity,
        tree: { sha: treeSha },
        parents: (parents || "")
          .split(" ")
          .filter(Boolean)
          .map((sha) => ({ sha })),
      };
    };
    const readTree = async (treeSha: string) => ({
      sha: treeSha,
      truncated: false,
      tree: git("ls-tree", "-r", "-t", "-z", treeSha)
        .toString()
        .split("\0")
        .filter(Boolean)
        .map((row) => {
          const [metadata, path] = row.split("\t"),
            [mode, type, sha] = metadata!.split(" ");
          return { path, mode, type, sha };
        }),
    });
    const { policy, files, proof: oldProof, receipt: template } = ownerFixture();
    try {
      git("init", "-q");
      await writeFile(join(root, "source.ts"), "export const sourceOnly = true;\n");
      const sourceHead = commit("reviewed source");
      const sourceCommit = await readCommit(sourceHead),
        sourceTree = sourceCommit.tree.sha!;
      const sourceManifest = deriveReviewedSourceManifest(await readTree(sourceTree), sourceTree);
      const proof: OwnerReviewProof = {
        ...oldProof,
        headSha: sourceHead,
        reviewedTreeSha: sourceTree,
        reviewedSourceManifestSha256: sourceManifest.sha256,
      };
      async function put(role: string, value: unknown) {
        const path = `${role}/${sourceHead}.json`,
          bytes = Buffer.from(JSON.stringify(value));
        await mkdir(dirname(join(root, path)), { recursive: true });
        await writeFile(join(root, path), bytes, { flag: "wx" });
        return { path, sha256: createHash("sha256").update(bytes).digest("hex") };
      }
      const raw: OwnerRawNativeReview = {
        ...proof,
        schema: "owner-scoped-native-review-output.v1",
        reviewStatus: "complete",
        findings: [],
      };
      const rawFile = await put("native-review-raw", raw);
      const rawCommit = commit("R0 unchanged reviewer output");
      const rawRef = { commit: rawCommit, ...rawFile };
      const result: OwnerReviewResult = {
        ...proof,
        schema: "owner-scoped-independent-review-result.v1",
        custody: {
          classification: "source-custody-native-parent-observation",
          publisher: policy.publisher,
          rawNativeResult: rawRef,
        },
      };
      const evidence = JSON.parse(
        files.get(template.provenance.evidence.path)!.toString(),
      ) as EvidenceArtifact;
      evidence.payload.details = {
        semanticOwner: policy.semanticOwner,
        review: proof as unknown as JsonObject,
      };
      evidence.payload.sourceUrl = `https://github.com/${policy.repository}/blob/${rawCommit}/${rawRef.path}`;
      evidence.payload.upstreamRevision = sourceHead;
      const resultFile = await put("native-review-results", result),
        evidenceFile = await put("native-review-evidence", evidence);
      const verification = JSON.parse(
        files.get(template.provenance.verification.path)!.toString(),
      ) as VerificationArtifact;
      verification.payload.details = {
        semanticOwner: policy.semanticOwner,
        review: proof as unknown as JsonObject,
      };
      verification.payload.inputHashes = [resultFile, evidenceFile, rawFile];
      const verificationFile = await put("native-review-verifications", verification);
      const evidenceCommit = commit("E normalized custody and canonical artifacts");
      const receipt: OwnerReviewReceipt = {
        ...proof,
        schema: "owner-scoped-independent-review.v1",
        provenance: {
          owner: policy.semanticOwner,
          rawNativeResult: rawRef,
          result: { commit: evidenceCommit, ...resultFile },
          evidence: { commit: evidenceCommit, ...evidenceFile },
          verification: { commit: evidenceCommit, ...verificationFile },
        },
      };
      const receiptFile = await put("native-reviews", receipt);
      const receiptCommit = commit("R1 owner-scoped receipt");
      expect(new Set([sourceHead, rawCommit, evidenceCommit, receiptCommit]).size).toBe(4);
      const reader: OwnerReviewReader = {
        readCommit,
        readTree,
        readStatuses: async () => [
          {
            id: 1,
            context: policy.statusContext,
            state: "success",
            creator: { ...policy.publisher, id: Number(policy.publisher.id) },
            target_url: `https://github.com/${policy.repository}/blob/${receiptCommit}/${receiptFile.path}`,
          },
        ],
        readFile: async (path, sha) => {
          const bytes = git("show", `${sha}:${path}`);
          return {
            type: "file",
            path,
            encoding: "base64",
            content: bytes.toString("base64"),
            size: bytes.length,
          };
        },
      };
      const verified = await validateOwnerScopedReviewWithEvidence(
        policy,
        { repository: policy.repository, head: sourceHead },
        reader,
      );
      expect(verified.receipt.commit).toBe(receiptCommit);
      expect(verified.manifest).toEqual(sourceManifest);
      expect(git("show", `${rawCommit}:${rawRef.path}`)).toEqual(
        await readFile(join(root, rawRef.path)),
      );
      const circularReader = {
        ...reader,
        readCommit: async (sha: string) =>
          sha === evidenceCommit
            ? { ...(await readCommit(sha)), parents: [{ sha: receiptCommit }] }
            : readCommit(sha),
      };
      await expect(
        validateOwnerScopedReviewWithEvidence(
          policy,
          { repository: policy.repository, head: sourceHead },
          circularReader,
        ),
      ).rejects.toThrow("ancestry");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
  test("rejects self-referential, future and unordered publication topology", async () => {
    for (const modify of [
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.rawNativeResult.commit = resultCommit;
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.rawNativeResult.commit = fixture.receiptCommit;
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        const prior = fixture.reader.readCommit;
        fixture.reader.readCommit = async (sha) =>
          sha === resultCommit ? { sha, parents: [{ sha: fixture.receiptCommit }] } : prior(sha);
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        const prior = fixture.reader.readCommit;
        fixture.reader.readCommit = async (sha) =>
          sha === fixture.rawCommit ? { sha, parents: [{ sha: resultCommit }] } : prior(sha);
      },
    ]) {
      const fixture = ownerFixture();
      modify(fixture);
      await expect(
        validateOwnerScopedReviewWithEvidence(
          fixture.policy,
          { repository: fixture.policy.repository, head },
          fixture.reader,
        ),
      ).rejects.toThrow();
    }
  });
  test("retains the entry nested publisher policy across a delayed read mutation", async () => {
    const fixture = ownerFixture(),
      publisher = fixture.policy.publisher;
    publisher.id = "999";
    publisher.login = "required-other-publisher";
    const read = fixture.reader.readCommit;
    fixture.reader.readCommit = async (sha) => {
      await Promise.resolve();
      publisher.id = "123";
      publisher.login = "fixture-publisher";
      return read(sha);
    };
    await expect(
      validateOwnerScopedReviewWithEvidence(
        fixture.policy,
        { repository: fixture.policy.repository, head },
        fixture.reader,
      ),
    ).rejects.toThrow("publisher");
  });
  test("retains original denying callbacks and their receiver across delayed reader substitution", async () => {
    const fixture = ownerFixture(),
      validRead = fixture.reader.readFile,
      commitRead = fixture.reader.readCommit;
    let denied = 0;
    fixture.reader.readFile = async function () {
      expect(this).toBe(fixture.reader);
      denied++;
      throw Error("Original reader denied evidence");
    };
    fixture.reader.readCommit = async (sha) => {
      await Promise.resolve();
      fixture.reader.readFile = validRead;
      return commitRead(sha);
    };
    await expect(
      validateOwnerScopedReviewWithEvidence(
        fixture.policy,
        { repository: fixture.policy.repository, head },
        fixture.reader,
      ),
    ).rejects.toThrow("Original reader denied");
    expect(denied).toBe(1);
  });
  test("rejects aliased initial status ID and receipt target mutation during tree read", async () => {
    const fixture = ownerFixture(),
      treeRead = fixture.reader.readTree;
    fixture.reader.readTree = async (sha) => {
      await Promise.resolve();
      fixture.statuses[0]!.id = 43;
      fixture.statuses[0]!.target_url = fixture.statuses[0]!.target_url.replace(
        fixture.receiptCommit,
        "3".repeat(40),
      );
      return treeRead(sha);
    };
    await expect(
      validateOwnerScopedReviewWithEvidence(
        fixture.policy,
        { repository: fixture.policy.repository, head },
        fixture.reader,
      ),
    ).rejects.toThrow("status changed");
  });
  test("CI transport retains fixed policy, authenticated repository and fetch callback across awaits", async () => {
    const fixture = ownerFixture(),
      repository = fixture.policy.repository,
      fetchBefore = globalThis.fetch;
    const names = ["GITHUB_REPOSITORY", "REVIEW_HEAD_SHA", "GITHUB_TOKEN"] as const;
    const envBefore = names.map((name) => process.env[name]);
    process.env.GITHUB_REPOSITORY = repository;
    process.env.REVIEW_HEAD_SHA = head;
    process.env.GITHUB_TOKEN = "fictional-no-network-token";
    const urls: string[] = [];
    const log = spyOn(console, "log").mockImplementation(() => {});
    const read = fixture.reader;
    globalThis.fetch = (async (input: string | URL | Request) => {
      const url = new URL(
        typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
      );
      urls.push(url.href);
      expect(url.pathname.startsWith(`/repos/${repository}/`)).toBe(true);
      const path = url.pathname.slice(`/repos/${repository}/`.length);
      if (urls.length === 1) {
        await Promise.resolve();
        fixture.policy.repository = "other/unreviewed";
        fixture.policy.publisher.id = "999";
        globalThis.fetch = (() => {
          throw Error("Substituted transport must never run");
        }) as unknown as typeof fetch;
      }
      let value: unknown;
      if (path.startsWith("git/commits/"))
        value = await read.readCommit(path.slice("git/commits/".length));
      else if (path.startsWith("commits/")) value = await read.readStatuses(head);
      else if (path.startsWith("git/trees/")) value = await read.readTree(tree);
      else if (path.startsWith("contents/"))
        value = await read.readFile(path.slice("contents/".length), url.searchParams.get("ref")!);
      else throw Error("Unexpected fictional transport route");
      return new Response(JSON.stringify(value), { status: 200 });
    }) as typeof fetch;
    try {
      await ownerScopedReviewGate(fixture.policy);
      expect(urls).toHaveLength(12);
      expect(log).toHaveBeenCalledTimes(1);
    } finally {
      globalThis.fetch = fetchBefore;
      log.mockRestore();
      names.forEach((name, index) => {
        const value = envBefore[index];
        if (value === undefined) delete process.env[name];
        else process.env[name] = value;
      });
    }
  });
  test("rejects unreviewed runtime repository before any read", async () => {
    const { policy, reader, reads } = ownerFixture();
    await expect(
      validateOwnerScopedReviewWithEvidence(policy, { repository: "other/math", head }, reader),
    ).rejects.toThrow();
    expect(reads).toEqual([]);
  });
  test("rejects stale source identity, truncated manifests and wrong publisher", async () => {
    for (const modify of [
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.reader.readCommit = async () => ({ sha: "0".repeat(40), tree: { sha: tree } });
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.reader.readTree = async () => ({ ...treeResponse, truncated: true });
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.statuses[0]!.creator.id = 999;
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.statuses[0]!.creator.login = "impersonator";
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.statuses.unshift({ ...fixture.statuses[0]!, id: 43, state: "failure" });
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.statuses[0]!.target_url = fixture.statuses[0]!.target_url.replace(
          fixture.receiptCommit,
          "main",
        );
      },
    ]) {
      const fixture = ownerFixture();
      modify(fixture);
      await expect(
        validateOwnerScopedReviewWithEvidence(
          fixture.policy,
          { repository: fixture.policy.repository, head },
          fixture.reader,
        ),
      ).rejects.toThrow();
    }
  });
  test("holds a newer finding/status published during evidence verification", async () => {
    const fixture = ownerFixture();
    let reads = 0;
    fixture.reader.readStatuses = async () =>
      ++reads === 1
        ? fixture.statuses
        : [{ ...fixture.statuses[0]!, id: 43, state: "failure" }, ...fixture.statuses];
    await expect(
      validateOwnerScopedReviewWithEvidence(
        fixture.policy,
        { repository: fixture.policy.repository, head },
        fixture.reader,
      ),
    ).rejects.toThrow("latest status");
  });
  test("rejects rehashed verification that omits the raw-output input join", async () => {
    const fixture = ownerFixture();
    const ref = fixture.receipt.provenance.verification;
    const verification = JSON.parse(
      fixture.files.get(ref.path)!.toString("utf8"),
    ) as VerificationArtifact;
    verification.payload.inputHashes.pop();
    const bytes = Buffer.from(JSON.stringify(verification));
    fixture.files.set(ref.path, bytes);
    ref.sha256 = createHash("sha256").update(bytes).digest("hex");
    await expect(
      validateOwnerScopedReviewWithEvidence(
        fixture.policy,
        { repository: fixture.policy.repository, head },
        fixture.reader,
      ),
    ).rejects.toThrow("hash-bind");
  });
  test("rejects owner/schema/identity/current-finding substitution even when all changed file hashes match", async () => {
    for (const change of [
      ({ raw }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        raw.reviewer.taskId = "/fixture/other-reviewer";
      },
      ({ raw }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        raw.reviewedTreeSha = "0".repeat(40);
      },
      ({ raw }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        raw.binding.semanticOwner = "hermes-harness-engineering";
      },
      ({ raw }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        raw.findings = [{ status: "unresolved" }] as never;
      },
      ({ raw }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        raw.conclusion = "hold" as never;
      },
      ({ result }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        result.author = result.reviewer;
      },
      ({ result }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        result.schema = "agent-platform.independent-review-result.v1" as never;
      },
      ({ evidence }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        evidence.domainId = "native-harness";
      },
      ({ evidence }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        evidence.payload.details!.semanticOwner = "agent-platform";
      },
      ({ evidence }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        evidence.payload.sourceUrl += "?raw=1";
      },
      ({ evidence }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        evidence.payload.result.outcome = "failure";
      },
      ({ verification }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        verification.payload.criteria[0].status = "unverified";
      },
      ({ verification }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        verification.payload.criteria[0].evidenceRefs = ["math-research-module:evidence:absent"];
      },
      ({ verification }: Parameters<NonNullable<Parameters<typeof ownerFixture>[0]>>[0]) => {
        verification.payload.targetRefs = ["math-research-module:task:absent"];
      },
    ]) {
      const { policy, reader } = ownerFixture(change);
      await expect(
        validateOwnerScopedReviewWithEvidence(
          policy,
          { repository: policy.repository, head },
          reader,
        ),
      ).rejects.toThrow();
    }
  });
  test("rejects immutable custody/hash substitutions and legacy/common-v2 misclaims", async () => {
    for (const modify of [
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.rawNativeResult.sha256 = "0".repeat(64);
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.evidence.commit = head;
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.verification.commit = fixture.receiptCommit;
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.result.path = "other/result.json";
      },
      (fixture: ReturnType<typeof ownerFixture>) => {
        fixture.receipt.provenance.owner = "hermes-harness-engineering";
      },
    ]) {
      const fixture = ownerFixture();
      modify(fixture);
      await expect(
        validateOwnerScopedReviewWithEvidence(
          fixture.policy,
          { repository: fixture.policy.repository, head },
          fixture.reader,
        ),
      ).rejects.toThrow();
    }
    const fixture = ownerFixture();
    await expect(
      validateReceiptWithEvidence(
        fixture.receipt,
        head,
        tree,
        fixture.policy.repository,
        fixture.reader,
      ),
    ).rejects.toThrow("Unsupported");
  });
});
