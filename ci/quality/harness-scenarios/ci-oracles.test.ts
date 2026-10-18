import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { reviewGate, type ReviewGateOptions } from "../../../ci/source-check.ts";
import { deriveReviewedSourceManifest } from "../../../ci/quality/receipt-validator.ts";

function gateFixture(arrivalMs = 0) {
  const head = "a".repeat(40),
    tree = "b".repeat(40),
    receiptCommit = "c".repeat(40),
    resultCommit = "d".repeat(40);
  const repository = "fictional/oracles";
  const source = {
    sha: tree,
    truncated: false,
    tree: [{ path: "source.ts", type: "blob", mode: "100644", sha: "e".repeat(40) }],
  };
  const result = {
    schema: "agent-platform.independent-review-result.v1",
    recordClass: "verification",
    binding: {
      codeOwner: "agent-platform",
      evidenceOwner: "codex-harness-engineering",
      scope: "canonical-source-ci-quality",
    },
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: deriveReviewedSourceManifest(source, tree).sha256,
    reviewer: { runtime: "native-codex", taskId: "fictional-reviewer" },
    author: { runtime: "native-codex", taskId: "fictional-author" },
    conclusion: "pass",
    unresolvedFindings: 0,
  };
  const bytes = Buffer.from(JSON.stringify(result));
  const receipt = {
    ...result,
    schema: "native-independent-review.v2",
    provenance: {
      owner: "codex-harness-engineering",
      resultRef: `https://github.com/${repository}/blob/${resultCommit}/native-review-results/${head}.json`,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    },
  };
  const status = {
    id: 42,
    context: "native-independent-review",
    state: "success",
    creator: { id: 123, login: "fictional-publisher" },
    target_url: `https://github.com/${repository}/blob/${receiptCommit}/native-reviews/${head}.json`,
  };
  let elapsed = 0;
  const paths: string[] = [];
  const content = (path: string, value: Buffer) => ({
    path,
    type: "file",
    encoding: "base64",
    size: value.length,
    content: value.toString("base64"),
  });
  const options: ReviewGateOptions = {
    env: {
      GITHUB_REPOSITORY: repository,
      REVIEW_HEAD_SHA: head,
      GITHUB_TOKEN: "fictional-never-sent",
      NATIVE_REVIEW_STATUS_CREATOR_ID: "123",
      NATIVE_REVIEW_STATUS_CREATOR_LOGIN: "fictional-publisher",
      GITHUB_ACTIONS: "true",
      GITHUB_EVENT_NAME: "push",
      GITHUB_REF: "refs/heads/main",
      GITHUB_REF_PROTECTED: "true",
      GITHUB_SHA: head,
    },
    now: () => elapsed,
    delay: async (ms) => {
      elapsed += ms;
    },
    readApi: async (path) => {
      paths.push(path);
      if (path === `commits/${head}/statuses?per_page=100`)
        return elapsed >= arrivalMs ? [status] : [];
      if (path === `contents/native-reviews/${head}.json?ref=${receiptCommit}`)
        return content(`native-reviews/${head}.json`, Buffer.from(JSON.stringify(receipt)));
      if (path === `git/commits/${head}`) return { sha: head, tree: { sha: tree } };
      if (path === `git/trees/${tree}?recursive=1`) return source;
      if (path === `contents/native-review-results/${head}.json?ref=${resultCommit}`)
        return content(`native-review-results/${head}.json`, bytes);
      throw Error(`Unadmitted offline API request ${path}`);
    },
  };
  return { options, status, receipt, paths, elapsed: () => elapsed };
}

test("PT-QC-06: exact proof arriving inside the existing wait succeeds; late proof stays absent without source-defect inference", async () => {
  const positive = gateFixture(15000);
  await reviewGate(positive.options);
  expect(positive.elapsed()).toBe(15000);
  expect(positive.paths.some((p) => p.startsWith("contents/native-reviews/"))).toBe(true);
  // Captured example was 4,072 seconds late, far outside the existing 60 second admission window.
  const late = gateFixture(4072000);
  const absent = await reviewGate(late.options).then(
    () => null,
    (error: unknown) => error,
  );
  expect(absent).toMatchObject({ message: expect.stringContaining("status missing") });
  expect(late.elapsed()).toBe(60000);
  expect(late.paths.every((p) => p.startsWith("commits/"))).toBe(true);
  const arrived = gateFixture();
  await reviewGate(arrived.options);
  expect(arrived.elapsed()).toBe(0);
  // New read of offline evidence only; no CI rerun, status publication or source change occurs.
});

test("PT-QC-06: wrong publisher, held status, changed tree and unresolved finding cannot masquerade as late absence", async () => {
  for (const fault of ["publisher", "held", "tree", "finding"] as const) {
    const f = gateFixture();
    if (fault === "publisher") f.status.creator.id = 456;
    if (fault === "held") f.status.state = "failure";
    if (fault === "tree") f.receipt.reviewedTreeSha = "f".repeat(40);
    if (fault === "finding") f.receipt.unresolvedFindings = 1;
    expect(
      await reviewGate(f.options).then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
    expect(f.elapsed()).toBe(0);
  }
  await reviewGate(gateFixture().options);
});

test("PT-QC-06 historical calibration: 18 unique incidents bind late proof to exact source, not 18 independent source defects", async () => {
  const record = JSON.parse(
    await readFile(
      resolve(
        import.meta.dir,
        "../../../work/agent-platform/ponytail-harness-evaluation-20261018/inputs/PT-CI-REPEATED-MISTAKES-V2.json",
      ),
      "utf8",
    ),
  );
  const content = record.content;
  const cluster = content.clusters.find((c: { id: string }) => c.id === "CI-Q-01");
  const incidents = cluster.occurrences as {
    repository: string;
    runId: number;
    headCommit: string;
    headTree: string;
    attempt1ReviewCompletedAt: string;
    statusCreatedAt: string;
    latestConclusion: string;
  }[];
  expect(content.scope.runsInspected).toBe(45);
  expect(content.scope.mainPushRuns).toBe(19);
  expect(incidents).toHaveLength(18);
  expect(new Set(incidents.map((i) => `${i.repository}:${i.runId}`)).size).toBe(18);
  for (const i of incidents) {
    expect(i.headCommit).toMatch(/^[a-f0-9]{40}$/);
    expect(i.headTree).toMatch(/^[a-f0-9]{40}$/);
    expect(Date.parse(i.statusCreatedAt)).toBeGreaterThan(Date.parse(i.attempt1ReviewCompletedAt));
    expect(i.latestConclusion).toBe("success");
  }
  expect(content.rootReadback.newCIEffects).toBe(0);
});
