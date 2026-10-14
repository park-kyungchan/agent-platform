import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const workflow = Bun.YAML.parse(
  await readFile(resolve(root, ".github/workflows/native-review-arrival.yml"), "utf8"),
) as {
  on: Record<string, unknown>;
  permissions: Record<string, string>;
  jobs: {
    reconcile: {
      permissions: Record<string, string>;
      steps: Array<{ uses?: string; with?: { script?: string } }>;
    };
  };
};
const script = workflow.jobs.reconcile.steps[0]!.with!.script!;
const AsyncFunction = Object.getPrototypeOf(async () => undefined).constructor as new (
  ...parameters: string[]
) => (...args: unknown[]) => Promise<Record<string, unknown>>;
const reconcile = new AsyncFunction(
  "github",
  "context",
  "core",
  "process",
  "Buffer",
  "URL",
  "require",
  script,
);
const head = "a".repeat(40),
  tree = "b".repeat(40),
  receiptCommit = "c".repeat(40);
const creatorId = 241037670;
const repository = "owner/platform";
type Scenario = {
  event?: "status" | "workflow_run";
  state?: string;
  creatorId?: number;
  latestStatusId?: number;
  head?: string;
  receiptHead?: string;
  receiptTree?: string;
  receiptConclusion?: string;
  attempt?: number;
  sourceResult?: string;
  qualityResult?: string;
  integrationResult?: string;
  missingIntegration?: boolean;
  extraJobName?: string;
  extraJobConclusion?: string;
  duplicateReview?: boolean;
  missingMarker?: boolean;
  invalidReceiptStep?: boolean;
  staleCurrentHead?: boolean;
  runStatus?: string;
  noMatchingRun?: boolean;
  pr?: boolean;
  supersededRun?: boolean;
  beforeWriteAttempt?: number;
  receiptVersion?: 2;
  resultDigestMismatch?: boolean;
  staleManifest?: boolean;
};

type ApiCall = { route: string; parameters: Record<string, unknown> };

async function execute(scenario: Scenario = {}, writes: ApiCall[] = []) {
  const calls: Array<{ route: string; parameters: Record<string, unknown> }> = [];
  const selectedHead = scenario.head ?? head;
  const status = {
    id: scenario.latestStatusId ?? 12,
    context: "native-independent-review",
    state: scenario.state ?? "success",
    creator: { id: scenario.creatorId ?? creatorId, login: "trusted-reviewer" },
    target_url: `https://github.com/${repository}/blob/${receiptCommit}/native-reviews/${head}.json`,
    created_at: "2026-10-14T00:00:20Z",
  };
  const treeEntries = [
    { path: "ci", mode: "040000", type: "tree", sha: "d".repeat(40) },
    { path: "ci/source-check.ts", mode: "100644", type: "blob", sha: "e".repeat(40) },
    { path: "package.json", mode: "100644", type: "blob", sha: "f".repeat(40) },
  ];
  const manifestSha256 = createHash("sha256")
    .update("agent-platform.reviewed-source-manifest.v1\n" + JSON.stringify(treeEntries))
    .digest("hex");
  const resultCommit = "9".repeat(40);
  const immutableResult = {
    schema: "agent-platform.independent-review-result.v1",
    recordClass: "verification",
    binding: {
      codeOwner: "agent-platform",
      evidenceOwner: "codex-harness-engineering",
      scope: "canonical-source-ci-quality",
    },
    headSha: scenario.receiptHead ?? head,
    reviewedTreeSha: scenario.receiptTree ?? tree,
    reviewedSourceManifestSha256: scenario.staleManifest ? "0".repeat(64) : manifestSha256,
    reviewer: { runtime: "native-codex", taskId: "independent-task" },
    author: { runtime: "native-codex", taskId: "author-task" },
    conclusion: scenario.receiptConclusion ?? "pass",
    unresolvedFindings: 0,
  };
  const resultBytes = Buffer.from(JSON.stringify(immutableResult));
  const receipt =
    scenario.receiptVersion === 2
      ? {
          ...immutableResult,
          schema: "native-independent-review.v2",
          provenance: {
            owner: "codex-harness-engineering",
            resultRef: `https://github.com/${repository}/blob/${resultCommit}/native-review-results/${head}.json`,
            sha256: scenario.resultDigestMismatch
              ? "0".repeat(64)
              : createHash("sha256").update(resultBytes).digest("hex"),
          },
        }
      : {
          schema: "native-independent-review.v1",
          headSha: scenario.receiptHead ?? head,
          reviewedTreeSha: scenario.receiptTree ?? tree,
          reviewer: { runtime: "native-codex", taskId: "independent-task" },
          author: { taskId: "author-task" },
          conclusion: scenario.receiptConclusion ?? "pass",
          unresolvedFindings: 0,
          provenance: {
            owner: "hermes-harness-engineering",
            resultRef: "source-owned-result",
            sha256: "d".repeat(64),
          },
        };
  const receiptBytes = Buffer.from(JSON.stringify(receipt));
  const run = {
    id: 100,
    run_number: 20,
    head_sha: head,
    workflow_id: 30,
    path: ".github/workflows/source.yml",
    head_repository: { full_name: repository },
    event: scenario.pr ? "pull_request" : "push",
    run_attempt: scenario.attempt ?? 1,
    status: scenario.runStatus ?? "completed",
    conclusion: "failure",
    head_branch: "main",
    pull_requests: scenario.pr ? [{ number: 42 }] : [],
  };
  const jobs = [
    { id: 1, name: "Canonical source checks", conclusion: scenario.sourceResult ?? "success" },
    {
      id: 2,
      name: "Quality and architecture checks",
      conclusion: scenario.qualityResult ?? "success",
    },
    ...(scenario.missingIntegration
      ? []
      : [
          {
            id: 5,
            name: "Prospective merge integration",
            conclusion: scenario.integrationResult ?? (scenario.pr ? "success" : "skipped"),
          },
        ]),
    {
      id: 3,
      name: "Exact head native independent review",
      conclusion: "failure",
      steps: [
        {
          name: "Probe independent review status availability",
          conclusion: "success",
          started_at: "2026-10-14T00:00:10Z",
        },
        ...(scenario.missingMarker === false
          ? []
          : [{ name: "Native review status not yet available", conclusion: "failure" }]),
        ...(scenario.invalidReceiptStep
          ? [
              {
                name: "Read immutable native review receipt for this exact head",
                conclusion: "failure",
              },
            ]
          : []),
      ],
    },
    { id: 4, name: "Source and native review gate", conclusion: "failure" },
    ...(scenario.extraJobName
      ? [
          {
            id: 6,
            name: scenario.extraJobName,
            conclusion: scenario.extraJobConclusion ?? "success",
          },
        ]
      : []),
  ];
  if (scenario.duplicateReview)
    jobs.push({ id: 7, name: "Exact head native independent review", conclusion: "failure" });
  let runReads = 0;
  const request = async (route: string, parameters: Record<string, unknown>) => {
    calls.push({ route, parameters });
    if (route.startsWith("POST ")) {
      writes.push({ route, parameters });
      return { data: undefined };
    }
    if (route.endsWith("/commits/{ref}/statuses")) return { data: [status] };
    if (route.endsWith("/contents/{path}")) {
      const receiptPath = `native-reviews/${head}.json`;
      const resultPath = `native-review-results/${head}.json`;
      const isReceipt = parameters.path === receiptPath && parameters.ref === receiptCommit;
      const isResult = parameters.path === resultPath && parameters.ref === resultCommit;
      if (!isReceipt && !isResult) throw new Error("Unexpected immutable review file read");
      const bytes = isReceipt ? receiptBytes : resultBytes;
      return {
        data: {
          type: "file",
          path: parameters.path,
          encoding: "base64",
          content: bytes.toString("base64"),
          size: bytes.length,
        },
      };
    }
    if (route.endsWith("/git/commits/{commit_sha}"))
      return { data: { sha: head, tree: { sha: tree } } };
    if (route.endsWith("/git/trees/{tree_sha}")) {
      if (parameters.tree_sha !== tree || parameters.recursive !== "1")
        throw new Error("Expected exact reviewed tree with a recursive manifest read");
      return { data: { sha: tree, truncated: false, tree: [...treeEntries].reverse() } };
    }
    if (route.endsWith("/actions/workflows/{workflow_id}"))
      return { data: { id: 30, path: run.path } };
    if (route.endsWith("/actions/workflows/{workflow_id}/runs"))
      return {
        data: {
          total_count: scenario.noMatchingRun ? 0 : 1,
          workflow_runs: scenario.noMatchingRun ? [] : [run],
        },
      };
    if (route.endsWith("/actions/runs/{run_id}")) {
      runReads++;
      return {
        data: {
          ...run,
          run_attempt:
            runReads > 1 ? (scenario.beforeWriteAttempt ?? run.run_attempt) : run.run_attempt,
        },
      };
    }
    if (route.endsWith("/branches/{branch}"))
      return { data: { commit: { sha: scenario.staleCurrentHead ? "e".repeat(40) : head } } };
    if (route.endsWith("/pulls/{pull_number}"))
      return {
        data: {
          state: "open",
          head: {
            sha: scenario.staleCurrentHead ? "e".repeat(40) : head,
            repo: { full_name: repository },
          },
        },
      };
    if (route.endsWith("/actions/runs/{run_id}/jobs"))
      return { data: { total_count: jobs.length, jobs } };
    throw new Error("Unexpected API route: " + route);
  };
  const context = {
    repo: { owner: "owner", repo: "platform" },
    eventName: scenario.event ?? "status",
    payload: {
      repository: { full_name: repository },
      sha: selectedHead,
      id: 12,
      context: "native-independent-review",
      state: "success",
      sender: { id: creatorId, login: "trusted-reviewer" },
      target_url: status.target_url,
      workflow_run: {
        id: scenario.supersededRun ? 99 : run.id,
        name: "Source verification",
        path: run.path,
        status: "completed",
        head_sha: selectedHead,
      },
    },
  };
  const result = await reconcile(
    { request },
    context,
    { info: () => undefined },
    {
      env: {
        NATIVE_REVIEW_STATUS_CREATOR_ID: String(creatorId),
        NATIVE_REVIEW_STATUS_CREATOR_LOGIN: "trusted-reviewer",
      },
    },
    Buffer,
    URL,
    require,
  );
  return { result, writes, calls };
}

describe("trusted native review arrival reconciliation", () => {
  test("reruns only the failed review job and its gate dependents", async () => {
    const { result, writes } = await execute();
    expect(result.outcome).toBe("rerun-requested");
    expect(writes).toHaveLength(1);
    expect(writes[0]!.route).toBe("POST /repos/{owner}/{repo}/actions/jobs/{job_id}/rerun");
    expect(writes[0]!.parameters.job_id).toBe(3);
    expect(writes[0]!.parameters.enable_debug_logging).toBe(false);
  });
  test("v2 verifies the actual head tree and immutable result before requesting the one review rerun", async () => {
    const { result, writes, calls } = await execute({ receiptVersion: 2 });
    expect(result.outcome).toBe("rerun-requested");
    expect(writes).toHaveLength(1);
    expect(writes[0]!.route).toBe("POST /repos/{owner}/{repo}/actions/jobs/{job_id}/rerun");
    expect(writes[0]!.parameters.job_id).toBe(3);
    const manifestRead = calls.findIndex((call) => call.route.endsWith("/git/trees/{tree_sha}"));
    const resultRead = calls.findIndex(
      (call) =>
        call.route.endsWith("/contents/{path}") &&
        call.parameters.path === `native-review-results/${head}.json`,
    );
    const rerunWrite = calls.findIndex((call) => call.route.startsWith("POST "));
    expect(manifestRead).toBeGreaterThanOrEqual(0);
    expect(resultRead).toBeGreaterThan(manifestRead);
    expect(rerunWrite).toBeGreaterThan(resultRead);
    expect(calls[resultRead]!.parameters.ref).toBe("9".repeat(40));
  });
  test("v2 corrupted result digest or stale manifest cannot trigger any rerun", async () => {
    for (const scenario of [{ resultDigestMismatch: true }, { staleManifest: true }]) {
      const writes: ApiCall[] = [];
      await expect(execute({ receiptVersion: 2, ...scenario }, writes)).rejects.toThrow(
        "exact head and tree",
      );
      expect(writes).toHaveLength(0);
    }
  });
  test("source completion wakes up the opposite arrival order without polling", async () => {
    expect((await execute({ event: "workflow_run" })).result.outcome).toBe("rerun-requested");
    const early = await execute({ runStatus: "in_progress" });
    expect(early.writes).toHaveLength(0);
    expect(early.result.reason).toContain("second wakeup");
  });
  test("rejects a creator mismatch", async () => {
    await expect(execute({ creatorId: creatorId + 1 })).rejects.toThrow("unexpected creator");
  });
  test("rejects head or reviewed tree mismatch", async () => {
    await expect(execute({ receiptHead: "f".repeat(40) })).rejects.toThrow("exact head and tree");
    await expect(execute({ receiptTree: "f".repeat(40) })).rejects.toThrow("exact head and tree");
    await expect(execute({ head: "f".repeat(40) })).rejects.toThrow(
      "immutable and in this repository",
    );
  });
  test("invalid independent receipt never triggers a rerun", async () => {
    await expect(execute({ receiptConclusion: "fail" })).rejects.toThrow("exact head and tree");
  });
  test("failed source or quality disallows retry", async () => {
    expect((await execute({ sourceResult: "failure" })).writes).toHaveLength(0);
    expect((await execute({ qualityResult: "failure" })).writes).toHaveLength(0);
    expect((await execute({ qualityResult: "skipped" })).writes).toHaveLength(0);
  });
  test("pull requests require the distinct prospective merge integration to succeed", async () => {
    expect((await execute({ pr: true, integrationResult: "failure" })).writes).toHaveLength(0);
    expect((await execute({ pr: true, integrationResult: "skipped" })).writes).toHaveLength(0);
    expect((await execute({ pr: true, missingIntegration: true })).writes).toHaveLength(0);
    expect((await execute({ pr: true, integrationResult: "success" })).writes).toHaveLength(1);
  });
  test("non-pull-request runs admit only the explicitly skipped integration job", async () => {
    expect((await execute({ integrationResult: "skipped" })).writes).toHaveLength(1);
    expect((await execute({ integrationResult: "failure" })).writes).toHaveLength(0);
    expect((await execute({ integrationResult: "success" })).writes).toHaveLength(0);
    expect((await execute({ missingIntegration: true })).writes).toHaveLength(0);
  });
  test("step names cannot impersonate admitted jobs and no extra job is accepted", async () => {
    for (const extraJobName of [
      "Probe independent review status availability",
      "Native review status not yet available",
      "Unexpected successful job",
    ]) {
      const extra = await execute({ extraJobName, extraJobConclusion: "failure" });
      expect(extra.writes).toHaveLength(0);
      expect(extra.result.reason).toContain("denominator");
    }
    expect(
      (await execute({ extraJobName: "Unexpected successful job", extraJobConclusion: "success" }))
        .writes,
    ).toHaveLength(0);
    expect((await execute({ duplicateReview: true })).writes).toHaveLength(0);
  });
  test("attempt cap and an intervening rerun make duplicate arrivals no-ops", async () => {
    expect((await execute({ attempt: 2 })).writes).toHaveLength(0);
    expect((await execute({ beforeWriteAttempt: 2 })).writes).toHaveLength(0);
  });
  test("current branch or pull request head is required", async () => {
    expect((await execute({ staleCurrentHead: true })).writes).toHaveLength(0);
    expect((await execute({ pr: true, staleCurrentHead: true })).writes).toHaveLength(0);
    expect((await execute({ pr: true })).writes).toHaveLength(1);
  });
  test("no matching source run is an explicit no-op", async () => {
    expect((await execute({ noMatchingRun: true })).result.reason).toBe("No matching source run");
  });
  test("superseded status and completion events are ignored", async () => {
    expect((await execute({ latestStatusId: 13 })).writes).toHaveLength(0);
    expect((await execute({ event: "workflow_run", supersededRun: true })).writes).toHaveLength(0);
  });
  test("only the explicit unavailable-status failure is eligible", async () => {
    expect((await execute({ missingMarker: false })).writes).toHaveLength(0);
    expect((await execute({ invalidReceiptStep: true })).writes).toHaveLength(0);
  });
  test("pending native review cannot trigger a replay", async () => {
    expect((await execute({ state: "pending" })).writes).toHaveLength(0);
  });
  test("observer is bounded to two wakeups and has no source checkout", () => {
    expect(Object.keys(workflow.on).sort()).toEqual(["status", "workflow_run"]);
    expect(workflow.permissions.actions).toBe("read");
    expect(workflow.jobs.reconcile.permissions.actions).toBe("write");
    expect(
      workflow.jobs.reconcile.steps.some((step) => step.uses?.startsWith("actions/checkout@")),
    ).toBe(false);
  });
  test("required gate preserves existing names and requires quality evidence", async () => {
    const source = Bun.YAML.parse(
      await readFile(resolve(root, ".github/workflows/source.yml"), "utf8"),
    ) as {
      jobs: Record<
        string,
        {
          name: string;
          if?: string;
          needs?: string[];
          steps: Array<{ run?: string; uses?: string; with?: { ref?: string } }>;
        }
      >;
    };
    expect(source.jobs.source!.name).toBe("Canonical source checks");
    expect(source.jobs["native-review"]!.name).toBe("Exact head native independent review");
    expect(source.jobs["source-gate"]!.name).toBe("Source and native review gate");
    expect(source.jobs["source-gate"]!.needs).toEqual([
      "source",
      "quality",
      "merge-integration",
      "native-review",
    ]);
    expect(source.jobs["source-gate"]!.steps[0]!.run).toContain(
      'test -n "$QUALITY_ARTIFACT_DIGEST"',
    );
    expect(source.jobs["source-gate"]!.steps[0]!.run).toContain(
      "pull_request:success|push:skipped|workflow_dispatch:skipped",
    );
    expect(source.jobs.source!.steps[0]!.with!.ref).toBe(
      "${{ github.event.pull_request.head.sha || github.sha }}",
    );
    expect(source.jobs.quality!.steps[0]!.with!.ref).toBe(
      "${{ github.event.pull_request.head.sha || github.sha }}",
    );
    expect(source.jobs["merge-integration"]!.if).toBe("${{ github.event_name == 'pull_request' }}");
    expect(source.jobs["merge-integration"]!.steps[0]!.uses).toMatch(
      /^actions\/checkout@[a-f0-9]{40}$/,
    );
    expect(source.jobs["merge-integration"]!.steps[0]!.with!.ref).toBeUndefined();
    expect(source.jobs["merge-integration"]!.steps.at(-1)!.run).toBe("bun ci/source-check.ts");
  });
});
