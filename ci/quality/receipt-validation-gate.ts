import {
  decodeBoundedReviewFile,
  deriveReviewedSourceManifest,
  validateIndependentReviewReceipt,
  verifyIndependentReviewResult,
  type VerifiedManifest,
  type OwnerReviewPolicy,
  type OwnerReviewReceipt,
  type ImmutableResult,
  validateOwnerReviewPolicy,
  validateOwnerReviewProof,
  immutableOwnerReviewTarget,
  immutableOwnerReviewUrl,
  parseOwnerReviewFile,
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

export type OwnerReviewReader = ReviewEvidenceReader & {
  readCommit: (headSha: string) => Promise<unknown>;
  /** Latest first, obtained from the authenticated exact-head statuses endpoint. */
  readStatuses: (headSha: string) => Promise<unknown>;
};
const record = (value: unknown): Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};

/** Pure read/verify gate. Git custody authenticates the publisher, never a native process. */
export async function validateOwnerScopedReviewWithEvidence(
  policy: OwnerReviewPolicy,
  request: { repository: string; head: string },
  reader: OwnerReviewReader,
): Promise<{
  schema: "owner-scoped-native-review-ci.v1";
  binding: OwnerReviewReceipt["binding"];
  headSha: string;
  reviewedTreeSha: string;
  receipt: ImmutableResult;
  manifest: VerifiedManifest;
  publisherAuthenticated: true;
  nativeAuthenticationEstablished: false;
  nativePermissionGranted: false;
  acceptanceEstablished: false;
}> {
  validateOwnerReviewPolicy(policy);
  const { repository, head } = request;
  if (repository !== policy.repository || !/^[a-f0-9]{40}$/.test(head))
    throw new Error("Owner review request differs from fixed repository or exact head");
  const commit = record(await reader.readCommit(head)),
    tree = record(commit.tree).sha;
  if (commit.sha !== head || typeof tree !== "string" || !/^[a-f0-9]{40}$/.test(tree))
    throw new Error("Owner review source commit/tree unavailable");
  function selectStatus(statuses: unknown) {
    if (!Array.isArray(statuses) || statuses.length > 100)
      throw new Error("Owner review exact-head statuses unavailable or outside bound");
    // Never fall back to an older successful status after a newer hold/failure.
    const status = record(statuses.find((value) => record(value).context === policy.statusContext));
    const creator = record(status.creator);
    if (
      status.state !== "success" ||
      !Number.isSafeInteger(status.id) ||
      Number(status.id) <= 0 ||
      String(creator.id) !== policy.publisher.id ||
      creator.login !== policy.publisher.login ||
      creator.type !== policy.publisher.type
    )
      throw new Error("Owner review latest status is absent, held or from another publisher");
    return status;
  }
  const status = selectStatus(await reader.readStatuses(head));
  const target = immutableOwnerReviewUrl(status.target_url, policy, head, "native-reviews");
  const receiptBytes = decodeBoundedReviewFile(
    await reader.readFile(target.path, target.commit),
    target.path,
    32768,
  );
  const unknownReceipt = parseOwnerReviewFile(receiptBytes),
    receiptRecord = record(unknownReceipt);
  if (receiptRecord.schema !== "owner-scoped-independent-review.v1")
    throw new Error("Owner review receipt schema mismatch");
  const manifest = deriveReviewedSourceManifest(await reader.readTree(tree), tree);
  validateOwnerReviewProof(unknownReceipt, policy, head, tree, manifest.sha256);
  const receipt = unknownReceipt as OwnerReviewReceipt;
  const provenance = record(receiptRecord.provenance);
  if (
    provenance.owner !== policy.semanticOwner ||
    Object.keys(provenance).sort().join("|") !==
      "evidence|owner|rawNativeResult|result|verification"
  )
    throw new Error("Owner review provenance owner or evidence inventory mismatch");
  const refs = {
    result: immutableOwnerReviewTarget(provenance.result, policy, head, "native-review-results"),
    evidence: immutableOwnerReviewTarget(
      provenance.evidence,
      policy,
      head,
      "native-review-evidence",
    ),
    verification: immutableOwnerReviewTarget(
      provenance.verification,
      policy,
      head,
      "native-review-verifications",
    ),
    rawNativeResult: immutableOwnerReviewTarget(
      provenance.rawNativeResult,
      policy,
      head,
      "native-review-raw",
    ),
  };
  if (
    [refs.result, refs.evidence, refs.verification].some(
      (ref) => ref.commit !== refs.result.commit || ref.commit === target.commit,
    ) ||
    refs.rawNativeResult.commit === refs.result.commit ||
    refs.rawNativeResult.commit === target.commit
  )
    throw new Error("Owner review requires distinct raw, evidence and receipt commits");
  // A directly parented finite chain proves publication is constructible and acyclic:
  // H (reviewed source) -> R0 (unchanged raw output) -> E (result/Evidence/Verification) -> R1 (receipt).
  for (const [child, parent] of [
    [refs.rawNativeResult.commit, head],
    [refs.result.commit, refs.rawNativeResult.commit],
    [target.commit, refs.result.commit],
  ]) {
    const publication = record(await reader.readCommit(child!));
    if (
      publication.sha !== child ||
      !Array.isArray(publication.parents) ||
      publication.parents.length !== 1 ||
      record(publication.parents[0]).sha !== parent
    )
      throw new Error("Owner review publication must follow source/raw/evidence/receipt ancestry");
  }
  async function read(ref: ImmutableResult, bound = 65536) {
    return parseOwnerReviewFile(
      decodeBoundedReviewFile(await reader.readFile(ref.path, ref.commit), ref.path, bound),
      ref,
    );
  }
  const result = record(await read(refs.result));
  if (result.schema !== "owner-scoped-independent-review-result.v1")
    throw new Error("Owner review normalized result schema mismatch");
  validateOwnerReviewProof(result, policy, head, tree, manifest.sha256, receipt);
  const custody = record(record(result).custody),
    publisher = record(custody.publisher);
  const rawRef = immutableOwnerReviewTarget(
    custody.rawNativeResult,
    policy,
    head,
    "native-review-raw",
  );
  if (
    custody.classification !== "source-custody-native-parent-observation" ||
    Object.keys(publisher).sort().join("|") !== "id|login|type" ||
    publisher.id !== policy.publisher.id ||
    publisher.login !== policy.publisher.login ||
    publisher.type !== policy.publisher.type ||
    rawRef.commit !== refs.rawNativeResult.commit ||
    rawRef.sha256 !== refs.rawNativeResult.sha256
  )
    throw new Error(
      "Owner review normalized custody does not bind publisher and unchanged raw output",
    );
  const raw = record(await read(refs.rawNativeResult, 262144));
  if (
    raw.schema !== "owner-scoped-native-review-output.v1" ||
    raw.reviewStatus !== "complete" ||
    !Array.isArray(raw.findings) ||
    raw.findings.length !== 0
  )
    throw new Error("Owner review raw native output is incomplete or retains current findings");
  validateOwnerReviewProof(raw, policy, head, tree, manifest.sha256, receipt);

  // Existing immutable canonical Ontology 0.2.0 owns shape/reference semantics.
  const { validateArtifact, validateReferences } = await import("@palantirkc/ontology");
  const evidenceResult = validateArtifact(await read(refs.evidence));
  const verificationResult = validateArtifact(await read(refs.verification));
  if (
    !evidenceResult.valid ||
    evidenceResult.value.kind !== "evidence" ||
    !verificationResult.valid ||
    verificationResult.value.kind !== "verification"
  )
    throw new Error("Owner review requires canonical Evidence and Verification artifacts");
  const evidence = evidenceResult.value,
    verification = verificationResult.value;
  if (!validateReferences([evidence, verification]).valid)
    throw new Error("Owner review canonical artifact reference closure mismatch");
  for (const artifact of [evidence, verification]) {
    if (
      artifact.projectId !== policy.projectId ||
      artifact.domainId !== policy.domainId ||
      artifact.payload.details?.semanticOwner !== policy.semanticOwner
    )
      throw new Error("Owner review artifact semantic ownership mismatch");
    validateOwnerReviewProof(
      artifact.payload.details?.review,
      policy,
      head,
      tree,
      manifest.sha256,
      receipt,
    );
  }
  const rawUrl = `https://github.com/${repository}/blob/${refs.rawNativeResult.commit}/${refs.rawNativeResult.path}`;
  if (
    evidence.payload.method !== "native-codex-independent-source-review" ||
    evidence.payload.sourceUrl !== rawUrl ||
    evidence.payload.upstreamRevision !== head ||
    evidence.payload.result.outcome !== "success" ||
    evidence.payload.result.contentMatched !== true
  )
    throw new Error(
      "Owner review evidence does not describe successful exact raw native observation",
    );
  if (
    verification.payload.targetRefs.length !== 1 ||
    verification.payload.targetRefs[0] !== evidence.id ||
    !verification.payload.criteria.some(
      (criterion) => criterion.id === "independent-native-source-review",
    ) ||
    verification.payload.criteria.some(
      (criterion) =>
        criterion.status !== "pass" ||
        criterion.evidenceRefs.length !== 1 ||
        criterion.evidenceRefs[0] !== evidence.id,
    )
  )
    throw new Error(
      "Owner review verification requires observed passing independent-review criterion",
    );
  const expectedInputs = [refs.result, refs.evidence, refs.rawNativeResult];
  if (
    verification.payload.inputHashes.length !== expectedInputs.length ||
    expectedInputs.some(
      (ref) =>
        !verification.payload.inputHashes.some(
          (input) => input.path === ref.path && input.sha256 === ref.sha256,
        ),
    )
  )
    throw new Error(
      "Owner review verification does not hash-bind result, evidence and raw native output",
    );
  const currentStatus = selectStatus(await reader.readStatuses(head));
  if (currentStatus.id !== status.id || currentStatus.target_url !== status.target_url)
    throw new Error("Owner review status changed while verifying immutable evidence");
  return {
    schema: "owner-scoped-native-review-ci.v1",
    binding: receipt.binding,
    headSha: head,
    reviewedTreeSha: tree,
    receipt: {
      ...target,
      sha256: (await import("node:crypto")).createHash("sha256").update(receiptBytes).digest("hex"),
    },
    manifest,
    publisherAuthenticated: true,
    nativeAuthenticationEstablished: false,
    nativePermissionGranted: false,
    acceptanceEstablished: false,
  };
}

/** CI transport only; fixed reviewed policy stays in the consumer entrypoint. */
export async function ownerScopedReviewGate(policy: OwnerReviewPolicy): Promise<void> {
  validateOwnerReviewPolicy(policy);
  const repository = process.env.GITHUB_REPOSITORY,
    head = process.env.REVIEW_HEAD_SHA;
  const token = process.env.GITHUB_TOKEN;
  if (repository !== policy.repository || !head || !/^[a-f0-9]{40}$/.test(head) || !token)
    throw new Error(
      "Owner review requires fixed repository, exact head and authenticated API read",
    );
  async function api(path: string): Promise<unknown> {
    const response = await fetch(`https://api.github.com/repos/${policy.repository}/${path}`, {
      headers: {
        Authorization: "Bearer " + token,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
      redirect: "error",
    });
    if (!response.ok) throw new Error("Owner review authenticated API read failed");
    return response.json() as Promise<unknown>;
  }
  const result = await validateOwnerScopedReviewWithEvidence(
    policy,
    { repository, head },
    {
      readCommit: (sha) => api(`git/commits/${sha}`),
      readStatuses: (sha) => api(`commits/${sha}/statuses?per_page=100`),
      readTree: (sha) => api(`git/trees/${sha}?recursive=1`),
      readFile: (path, sha) => api(`contents/${path}?ref=${sha}`),
    },
  );
  console.log(JSON.stringify(result));
}
