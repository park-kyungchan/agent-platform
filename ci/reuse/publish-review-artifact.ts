import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  authenticatedReviewReader,
  reviewArtifactWorkflow,
  reviewArtifactBranch,
  validateRawReviewArtifact,
  validatePublishedReviewArtifact,
  type ReviewArtifactPolicy,
} from "../quality/review-artifact";
import {
  deriveReviewedSourceManifest,
  decodeBoundedReviewFile,
} from "../quality/receipt-validator";
import { locatedError } from "./located-error";
import { requireTrustedReviewContext, type TrustedReviewContext } from "./trusted-review-source";

/** Native publisher only: preserve the actual reviewer bytes, upload once, verify, then attest. */
export async function publishNativeReviewArtifact(context: TrustedReviewContext, rawPath: string) {
  requireTrustedReviewContext(context);
  const { head, policy } = context;
  await context.revalidate();
  if (process.env.CI || process.env.GITHUB_ACTIONS)
    throw Error("Native publisher context required");
  if (!/^[a-f0-9]{40}$/.test(head) || !rawPath)
    throw Error("Use publish-review-artifact.ts <exact-head> <raw-native.json>");
  function gh(args: string[]) {
    const result = Bun.spawnSync(["gh", ...args], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    if (result.exitCode !== 0) throw Error("Authenticated native gh prerequisite failed");
    return result.stdout.toString().trim();
  }
  const identity = JSON.parse(gh(["api", "user"]));
  if (
    String(identity.id) !== policy.publisher.id ||
    identity.login !== policy.publisher.login ||
    identity.type !== "User"
  )
    throw Error("Fixed native publisher unavailable");
  // Capture existing native authentication in memory; never log it or send it to artifact storage.
  const token = gh(["auth", "token"]),
    reader = authenticatedReviewReader(policy.repository, token);
  const commit = (await reader.json(`git/commits/${head}`)) as {
    sha: string;
    tree: { sha: string };
  };
  if (commit.sha !== head) throw Error("Exact pushed source commit unavailable");
  const manifest = deriveReviewedSourceManifest(
    await reader.json(`git/trees/${commit.tree.sha}?recursive=1`),
    commit.tree.sha,
  );
  const bytes = await readFile(resolve(rawPath)),
    raw = validateRawReviewArtifact(bytes, policy, head!, commit.tree.sha, manifest.sha256);
  const authorBytes = decodeBoundedReviewFile(
    await reader.json(`contents/.github/source-author.json?ref=${head}`),
    ".github/source-author.json",
    32768,
  );
  const author = JSON.parse(Buffer.from(authorBytes).toString()).author;
  if (
    raw.author.runtime !== author.runtime ||
    raw.author.taskId !== (author.threadId ?? author.sessionId)
  )
    throw Error("Review author differs from committed native source author");
  const workflow = (await reader.json(`actions/workflows/native-review-evidence.yml`)) as {
    path: string;
    state: string;
  };
  if (workflow.path !== reviewArtifactWorkflow || workflow.state !== "active")
    throw Error("Default-branch evidence workflow unavailable; merge its reviewed source first");
  await context.revalidate();
  const requestId = randomUUID();
  async function post(path: string, value: unknown, expected: number) {
    const response = await fetch(`https://api.github.com/repos/${policy.repository}/${path}`, {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(30_000),
      headers: {
        Authorization: "Bearer " + token,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2026-03-10",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(value),
    });
    if (response.status !== expected)
      throw Error("Native publication outcome requires reconciliation; do not retry");
    return expected === 204 ? null : response.json();
  }
  console.log(JSON.stringify({ phase: "dispatch", requestId, headSha: head, retryAllowed: false }));
  await post(
    "actions/workflows/native-review-evidence.yml/dispatches",
    {
      ref: reviewArtifactBranch,
      inputs: { head, request_id: requestId, raw_base64: bytes.toString("base64") },
    },
    204,
  );
  const deadline = Date.now() + 600_000;
  let run: { id: number; status: string; conclusion: string; run_attempt: number } | undefined;
  while (Date.now() < deadline) {
    const page = (await reader.json(
      "actions/workflows/native-review-evidence.yml/runs?event=workflow_dispatch&per_page=100",
    )) as { workflow_runs: ({ display_title: string } & NonNullable<typeof run>)[] };
    const matches = page.workflow_runs.filter(
      (item) => item.display_title === `Native review evidence ${requestId}`,
    );
    if (matches.length > 1)
      throw Error("Ambiguous evidence runs; reconcile exact request, do not retry");
    run = matches[0];
    if (run?.status === "completed") break;
    await Bun.sleep(2000);
  }
  if (!run || run.status !== "completed" || run.conclusion !== "success" || run.run_attempt !== 1)
    throw Error("Evidence run did not settle successfully; reconcile request, do not retry");
  const page = (await reader.json(`actions/runs/${run.id}/artifacts?per_page=100`)) as {
    artifacts: { id: number; name: string; digest: string }[];
  };
  const artifacts = page.artifacts.filter((item) => item.name === `native-review-${head}`);
  if (artifacts.length !== 1 || !/^sha256:[a-f0-9]{64}$/.test(artifacts[0]!.digest))
    throw Error("Exact evidence artifact unavailable");
  const artifact = artifacts[0]!,
    targetUrl = `https://github.com/${policy.repository}/actions/runs/${run.id}/artifacts/${artifact.id}#sha256=${artifact.digest.slice(7)}`;
  // Native publisher attests only after its exact artifact/raw/source joins have been checked.
  const status = {
    id: 1,
    context: "native-independent-review",
    state: "success",
    creator: identity,
    target_url: targetUrl,
  };
  const preflight = {
    ...reader,
    json: async (path: string) =>
      path === `commits/${head}/statuses?per_page=100` ? [status] : reader.json(path),
  };
  await validatePublishedReviewArtifact(
    { policy, head: head!, status, rawBytes: bytes },
    preflight,
  );
  console.log(
    JSON.stringify({
      phase: "artifact-verified",
      requestId,
      headSha: head,
      runId: run.id,
      artifactId: artifact.id,
      targetUrl,
    }),
  );
  await context.revalidate();
  await post(
    `statuses/${head}`,
    {
      state: "success",
      context: "native-independent-review",
      target_url: targetUrl,
      description: "Independent native raw review artifact verified",
    },
    201,
  );
  const statuses = (await reader.json(
    `commits/${head}/statuses?per_page=100`,
  )) as (typeof status)[];
  const latest = statuses.find((item) => item.context === "native-independent-review");
  const result = await validatePublishedReviewArtifact(
    { policy, head: head!, status: latest, rawBytes: bytes },
    reader,
  );
  console.log(JSON.stringify({ phase: "published", requestId, ...result }));
}
if (import.meta.main) {
  console.error(
    locatedError(
      new Error(
        "Use the pinned trusted review-pr controller; standalone publication is unsupported",
      ),
      "review_artifact_refused",
    ),
  );
  process.exitCode = 1;
}
