import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { reviewGate, type ReviewGateOptions } from "./source-check.ts";
import { deriveReviewedSourceManifest } from "./quality/receipt-validator.ts";

function fixture() {
  const head = "a".repeat(40),
    tree = "b".repeat(40),
    resultCommit = "c".repeat(40),
    receiptCommit = "d".repeat(40);
  const repository = "fixture/source";
  const entries = [{ path: "source.ts", mode: "100644", type: "blob", sha: "e".repeat(40) }];
  const manifest = deriveReviewedSourceManifest(
    { sha: tree, truncated: false, tree: entries },
    tree,
  );
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
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer: { runtime: "native-codex", taskId: "/fictional/reviewer" },
    author: { runtime: "native-codex", taskId: "/fictional/author" },
    conclusion: "pass",
    unresolvedFindings: 0,
  };
  let resultBytes = Buffer.from(JSON.stringify(result));
  const receipt = {
    ...result,
    schema: "native-independent-review.v2",
    provenance: {
      owner: "codex-harness-engineering",
      resultRef: `https://github.com/${repository}/blob/${resultCommit}/native-review-results/${head}.json`,
      sha256: createHash("sha256").update(resultBytes).digest("hex"),
    },
  };
  const status = {
    id: 42,
    context: "native-independent-review",
    state: "success",
    creator: { id: 123, login: "fixture-publisher" },
    target_url: `https://github.com/${repository}/blob/${receiptCommit}/native-reviews/${head}.json`,
  };
  const env: NodeJS.ProcessEnv = {
    GITHUB_REPOSITORY: repository,
    REVIEW_HEAD_SHA: head,
    GITHUB_TOKEN: "fictional-test-token",
    NATIVE_REVIEW_STATUS_CREATOR_ID: "123",
    NATIVE_REVIEW_STATUS_CREATOR_LOGIN: "fixture-publisher",
    GITHUB_ACTIONS: "true",
    GITHUB_EVENT_NAME: "push",
    GITHUB_REF: "refs/heads/main",
    GITHUB_REF_PROTECTED: "true",
    GITHUB_SHA: head,
  };
  let elapsed = 0,
    statusReads = 0;
  const delays: number[] = [],
    paths: string[] = [];
  let statuses: (read: number, elapsed: number) => unknown = () => [status];
  let commitHead = head;
  const content = (path: string, bytes: Buffer) => ({
    type: "file",
    path,
    encoding: "base64",
    size: bytes.length,
    content: bytes.toString("base64"),
  });
  const options: ReviewGateOptions = {
    env,
    now: () => elapsed,
    delay: async (milliseconds) => {
      delays.push(milliseconds);
      elapsed += milliseconds;
    },
    readApi: async (path) => {
      paths.push(path);
      if (path === `commits/${head}/statuses?per_page=100`) return statuses(++statusReads, elapsed);
      if (path === `contents/native-reviews/${head}.json?ref=${receiptCommit}`)
        return content(`native-reviews/${head}.json`, Buffer.from(JSON.stringify(receipt)));
      if (path === `git/commits/${head}`) return { sha: commitHead, tree: { sha: tree } };
      if (path === `git/trees/${tree}?recursive=1`)
        return { sha: tree, truncated: false, tree: entries };
      if (path === `contents/native-review-results/${head}.json?ref=${resultCommit}`)
        return content(`native-review-results/${head}.json`, resultBytes);
      throw Error("Unexpected synthetic API path; no network is available");
    },
  };
  return {
    options,
    env,
    head,
    status,
    receipt,
    bindPendingStatus(id: number) {
      Object.assign(result, { nativeReviewPendingStatusId: id });
      Object.assign(receipt, { nativeReviewPendingStatusId: id });
      resultBytes = Buffer.from(JSON.stringify(result));
      receipt.provenance.sha256 = createHash("sha256").update(resultBytes).digest("hex");
    },
    delays,
    paths,
    get elapsed() {
      return elapsed;
    },
    get statusReads() {
      return statusReads;
    },
    setStatuses(value: typeof statuses) {
      statuses = value;
    },
    setCommitHead(value: string) {
      commitHead = value;
    },
  };
}

test("trusted main push waits for late status then validates the full canonical v2 chain", async () => {
  const f = fixture();
  f.setStatuses((_read, elapsed) => (elapsed < 25_000 ? [] : [f.status]));
  await reviewGate(f.options);
  expect(f.elapsed).toBeGreaterThanOrEqual(25_000);
  expect(f.elapsed).toBeLessThanOrEqual(30_000);
  expect(f.statusReads).toBeGreaterThan(1);
  expect(f.paths.some((path) => path.includes("native-review-results/"))).toBe(true);
});

test("prepublished PR proof passes immediately through the existing canonical validation", async () => {
  const f = fixture();
  f.env.GITHUB_EVENT_NAME = "pull_request";
  await reviewGate(f.options);
  expect(f.elapsed).toBe(0);
  expect(f.statusReads).toBe(3);
});

test("a present valid status on a full page is validated rather than treated as absence", async () => {
  const f = fixture();
  f.setStatuses(() => [
    f.status,
    ...Array.from({ length: 99 }, (_, n) => ({
      id: n + 100,
      context: "other-check",
      state: "success",
    })),
  ]);
  await reviewGate(f.options);
  expect(f.elapsed).toBe(0);
  expect(f.statusReads).toBe(3);
});

test("genuine absence stops at 300 seconds without fetching a receipt", async () => {
  const f = fixture();
  f.setStatuses(() => []);
  await expect(reviewGate(f.options)).rejects.toThrow("status missing");
  expect(f.elapsed).toBe(300_000);
  expect(f.statusReads).toBeGreaterThan(1);
  expect(f.delays.every((delay) => delay <= 5000)).toBe(true);
  expect(f.paths.every((path) => path.startsWith("commits/"))).toBe(true);
});

test("publication at 65 and 295 seconds validates the full canonical chain", async () => {
  for (const arrival of [65_000, 295_000]) {
    const f = fixture();
    f.setStatuses((_read, elapsed) => (elapsed < arrival ? [] : [f.status]));
    await reviewGate(f.options);
    expect(f.elapsed).toBe(arrival);
    expect(f.delays.every((delay) => delay === 5000)).toBe(true);
    expect(f.paths.some((path) => path.includes("native-review-results/"))).toBe(true);
  }
});

test("publication at or after 300 seconds cannot replace original absence", async () => {
  for (const arrival of [300_000, 305_000]) {
    const f = fixture();
    f.setStatuses((_read, elapsed) => (elapsed < arrival ? [] : [f.status]));
    await expect(reviewGate(f.options)).rejects.toThrow("status missing");
    expect(f.elapsed).toBe(300_000);
    expect(f.paths.every((path) => path.startsWith("commits/"))).toBe(true);
  }
});

test("an API response returning only after the publication deadline is still late", async () => {
  const f = fixture();
  const read = f.options.readApi!;
  f.options.readApi = async (path) => {
    const result = await read(path);
    if (path.includes("/statuses?")) await f.options.delay!(300_001);
    return result;
  };
  await expect(reviewGate(f.options)).rejects.toThrow("status missing");
  expect(f.statusReads).toBe(1);
  expect(f.paths.every((path) => path.startsWith("commits/"))).toBe(true);
});

test("present holds and wrong creator are immediate failures rather than retryable absence", async () => {
  for (const state of ["pending", "failure", "error", "wrong-creator", "wrong-login"]) {
    const f = fixture();
    if (state === "wrong-creator") f.status.creator.id = 456;
    else if (state === "wrong-login") f.status.creator.login = "foreign-publisher";
    else f.status.state = state;
    f.setStatuses((read) =>
      read === 1
        ? [f.status]
        : [{ ...f.status, state: "success", creator: { id: 123, login: "fixture-publisher" } }],
    );
    await expect(reviewGate(f.options)).rejects.toThrow("unsuccessful or from unexpected creator");
    expect(f.elapsed).toBe(0);
    expect(f.statusReads).toBe(1);
  }
});

test("malformed responses and full-page absence are never eligible for waiting", async () => {
  const other = (id: number) => ({ id, context: "other-check", state: "success" });
  for (const value of [
    null,
    {},
    [null],
    [{ context: "other-check" }],
    [{ ...other(1), state: "unknown" }],
    Array.from({ length: 100 }, (_, n) => other(n + 1)),
    Array.from({ length: 101 }, (_, n) => other(n + 1)),
  ]) {
    const f = fixture();
    f.setStatuses(() => value);
    await expect(reviewGate(f.options)).rejects.toThrow();
    expect(f.elapsed).toBe(0);
    expect(f.statusReads).toBe(1);
  }
});

test("API errors fail immediately, including after an earlier genuine absence", async () => {
  for (const initialAbsence of [false, true]) {
    const f = fixture();
    f.setStatuses((read) => {
      if (initialAbsence && read === 1) return [];
      throw Error("Synthetic API503");
    });
    await expect(reviewGate(f.options)).rejects.toThrow("Synthetic API503");
    expect(f.statusReads).toBe(initialAbsence ? 2 : 1);
    expect(f.elapsed).toBe(initialAbsence ? 5000 : 0);
  }
});

test("PR, dispatch, unprotected, foreign-ref and mismatched-head contexts remain fail-fast", async () => {
  for (const change of [
    { GITHUB_EVENT_NAME: "pull_request" },
    { GITHUB_EVENT_NAME: "workflow_dispatch" },
    { GITHUB_REF_PROTECTED: "false" },
    { GITHUB_REF: "refs/heads/other" },
    { GITHUB_SHA: "f".repeat(40) },
    { GITHUB_ACTIONS: "false" },
  ]) {
    const f = fixture();
    Object.assign(f.env, change);
    f.setStatuses(() => []);
    await expect(reviewGate(f.options)).rejects.toThrow("status missing");
    expect(f.elapsed).toBe(0);
    expect(f.statusReads).toBe(1);
  }
});

test("immutable receipt URL and exact receipt/commit identity checks still reject after readiness", async () => {
  for (const change of ["url", "mutable-ref", "receipt-head", "commit-head"]) {
    const f = fixture();
    if (change === "url")
      f.status.target_url = `https://github.com/foreign/source/blob/${"d".repeat(40)}/native-reviews/${f.head}.json`;
    if (change === "mutable-ref")
      f.status.target_url = f.status.target_url.replace("d".repeat(40), "main");
    if (change === "receipt-head") f.receipt.headSha = "f".repeat(40);
    if (change === "commit-head") f.setCommitHead("f".repeat(40));
    f.setStatuses((_read, elapsed) => (elapsed === 0 ? [] : [f.status]));
    await expect(reviewGate(f.options)).rejects.toThrow();
    expect(f.elapsed).toBe(5000);
    expect(f.statusReads).toBe(change === "receipt-head" ? 3 : 2);
  }
});

test("late legacy success cannot restore old proof after a newer pending or negative review", async () => {
  for (const state of ["pending", "failure", "error"]) {
    const f = fixture(),
      old = { ...f.status, id: 40 };
    f.status.id = 44;
    f.setStatuses(() => [
      f.status,
      {
        ...f.status,
        id: 43,
        state,
        target_url: `https://github.com/${f.env.GITHUB_REPOSITORY}/commit/${f.head}#native-independent-review`,
      },
      old,
    ]);
    await expect(reviewGate(f.options)).rejects.toThrow("FRESH_LEGACY_ATTEMPT_REQUIRED");
  }
  const fresh = fixture();
  fresh.bindPendingStatus(43);
  fresh.status.id = 44;
  fresh.setStatuses(() => [
    fresh.status,
    {
      ...fresh.status,
      id: 43,
      state: "pending",
      target_url: `https://github.com/${fresh.env.GITHUB_REPOSITORY}/commit/${fresh.head}#native-independent-review`,
    },
  ]);
  await expect(reviewGate(fresh.options)).resolves.toBeUndefined();
  const oldMarker = fixture(),
    old = { ...oldMarker.status, id: 40 };
  oldMarker.status.id = 44;
  oldMarker.setStatuses(() => [
    oldMarker.status,
    {
      ...oldMarker.status,
      id: 43,
      state: "failure",
      target_url: "https://github.com/fixture/source/pull/1",
    },
    old,
  ]);
  await expect(reviewGate(oldMarker.options)).rejects.toThrow("FRESH_LEGACY_ATTEMPT_REQUIRED");
});
