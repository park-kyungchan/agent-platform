import { randomUUID, createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { deriveReviewedSourceManifest } from "../quality/receipt-validator";
import {
  validateRawReviewArtifact,
  authenticatedReviewReader,
  validateReviewArtifact,
} from "../quality/review-artifact";
import { nativeReviewPolicy, publishNativeReviewArtifact } from "./publish-review-artifact";
import { verifyCommittedSourceAuthor } from "./source-author";
import { reviewerInvocation, codexReviewThread } from "./review-runtimes";
import { locatedError } from "./located-error";

/** One native command: exact pushed PR head -> fresh independent native output -> verified status. */
export async function reviewPr(head: string, runtime: string, smoke = false, checkOnly = false) {
  if (process.env.CI || process.env.GITHUB_ACTIONS)
    throw Error("Native review cannot originate in CI");
  const root = process.cwd(),
    policy = await nativeReviewPolicy();
  function git(args: string[]) {
    const r = Bun.spawnSync(["git", "-C", root, ...args], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    if (r.exitCode !== 0) throw Error("Exact native Git source unavailable");
    return r.stdout.toString().trim();
  }
  if (
    !/^[a-f0-9]{40}$/.test(head) ||
    git(["rev-parse", "HEAD"]) !== head ||
    git(["status", "--porcelain=v1", "--untracked-files=no"])
  )
    throw Error("Clean exact PR head checkout required");
  const author = await verifyCommittedSourceAuthor({
    workspace: root,
    repository: policy.repository,
    projectId: JSON.parse(await readFile(resolve("project.json"), "utf8")).projectId,
    sourceHead: head,
  });
  const remote = Bun.spawnSync(["gh", "api", `repos/${policy.repository}/git/commits/${head}`], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  if (remote.exitCode !== 0 || JSON.parse(remote.stdout.toString()).sha !== head)
    throw Error("Exact pushed PR head unavailable");
  const tree = git(["rev-parse", `${head}^{tree}`]);
  const entries = git(["ls-tree", "-r", "-t", "-z", "--full-tree", tree])
    .split("\0")
    .filter(Boolean)
    .map((line) => {
      const [meta, path] = line.split("\t"),
        [mode, type, sha] = meta!.split(" ");
      return { path, mode, type, sha };
    });
  const manifest = deriveReviewedSourceManifest(
    { sha: tree, truncated: false, tree: entries },
    tree,
  );
  const reviewBase = git(["merge-base", head, "origin/main"]);
  if (checkOnly) {
    const auth = Bun.spawnSync(["gh", "auth", "token"], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    if (auth.exitCode !== 0) throw Error("Native authenticated review check unavailable");
    const reader = authenticatedReviewReader(policy.repository, auth.stdout.toString().trim());
    const statuses = await reader.json(`commits/${head}/statuses?per_page=100`);
    if (!Array.isArray(statuses)) throw Error("Native review statuses unavailable");
    const status = statuses.find((s) => s?.context === "native-independent-review");
    const result = await validateReviewArtifact({ policy, head, status }, reader);
    if (
      result.reviewer.runtime !== runtime ||
      result.author.runtime !== author.metadata.author.runtime ||
      result.author.taskId !==
        (author.metadata.author.threadId ?? author.metadata.author.sessionId) ||
      result.manifest.sha256 !== manifest.sha256
    )
      throw Error("Current native review/source/runtime binding differs");
    console.log(JSON.stringify({ phase: "verified-existing-native-review", ...result }));
    return;
  }
  const sessionId = randomUUID();
  const directory = await mkdtemp(join(tmpdir(), `native-review-${head.slice(0, 9)}-`));
  const rawPath = join(directory, "native.raw.json"),
    eventsPath = join(directory, "native.events.jsonl"),
    stderrPath = join(directory, "native.stderr.log");
  const command = reviewerInvocation(runtime, sessionId, smoke, rawPath);
  if (!Bun.which(command[0]!)) throw Error("Requested native reviewer CLI is not installed");
  const proof = {
    schema: smoke ? "native-review-smoke.v1" : "native-independent-review-output.v1",
    reviewStatus: "complete",
    binding: policy.binding,
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer: {
      runtime,
      taskId: runtime === "native-codex" ? "OBSERVE_CODEX_THREAD_ID" : sessionId,
    },
    author: {
      runtime: author.metadata.author.runtime,
      taskId: author.metadata.author.threadId ?? author.metadata.author.sessionId,
    },
    conclusion: "pass",
    unresolvedFindings: 0,
    findings: [],
  };
  const prompt = smoke
    ? `Mechanical transport smoke only. Do not inspect files, run tools, perform source review or claim Acceptance. Return ONLY this JSON object, unchanged in meaning and without markdown. For native-codex, replace OBSERVE_CODEX_THREAD_ID with your actual native CODEX_THREAD_ID observed in a read-only tool call. This record is ineligible for native-independent-review status.\n${JSON.stringify(proof, null, 2)}\n`
    : `You are a fresh independent native source reviewer. Read-only: no edits, writes, ref/index/config changes, publication, subagents or private user-data reads. Read AGENTS.md and review the exact pushed head ${head}, tree ${tree}, against the target-main merge base ${reviewBase} in ${root}. Review the entire PR diff ${reviewBase}..${head}, not only the last commit. Read immutable committed bytes with git show ${head}:path. Inspect all public technical changes and actionable correctness/security/regression/scope defects. Never read private student/class-record bodies or print private path names; private custody changes are metadata-only. CI cannot mint independent native review. Raw native output, source/publisher/run/artifact/digest/latest-status joins must be exact. Do not assume checks passed or native activation/Acceptance occurred. For native-codex, read your actual CODEX_THREAD_ID from your native tool environment and replace OBSERVE_CODEX_THREAD_ID; never copy the author or invocation ID. Return ONLY valid JSON without fences with the following exact proof fields. If findings remain, set conclusion fail and the exact unresolved count, with severity/path/line/explanation/suggestedFix per finding. Otherwise pass/zero/empty findings.\n${JSON.stringify(proof, null, 2)}\n`;
  const promptPath = join(directory, "prompt.txt");
  await writeFile(promptPath, prompt, { mode: 0o600 });
  // Muse takes a positional prompt; Claude's print mode reads stdin. Both use an actual native session UUID.
  const argv = runtime === "native-muse" ? [...command, "--prompt-file", promptPath] : command;
  console.log(
    JSON.stringify({
      phase: smoke ? "native-smoke" : "native-review",
      headSha: head,
      reviewerRuntime: runtime,
      reviewerSessionId: runtime === "native-codex" ? null : sessionId,
      invocationId: sessionId,
      ...(runtime === "native-codex" ? { eventsPath } : {}),
      rawPath,
      stderrPath,
    }),
  );
  const child = Bun.spawn(argv, {
    cwd: root,
    stdin: runtime === "native-muse" ? "ignore" : Buffer.from(prompt),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [, , exit] = await Promise.all([
    Bun.write(runtime === "native-codex" ? eventsPath : rawPath, child.stdout),
    Bun.write(stderrPath, child.stderr),
    child.exited,
  ]);
  if (exit !== 0)
    throw Error("Native reviewer failed; inspect unchanged raw/stderr, no status published");
  if (
    git(["rev-parse", "HEAD"]) !== head ||
    git(["status", "--porcelain=v1", "--untracked-files=no"])
  )
    throw Error("Source changed during native review; no status published");
  const raw = await readFile(rawPath);
  if (runtime === "native-codex") {
    const actualThread = codexReviewThread(await readFile(eventsPath));
    const observed = JSON.parse(raw.toString());
    if (
      observed.reviewer?.runtime !== runtime ||
      observed.reviewer?.taskId !== actualThread ||
      actualThread === (author.metadata.author.threadId ?? author.metadata.author.sessionId)
    )
      throw Error("Actual native Codex reviewer identity differs or repeats author");
    proof.reviewer.taskId = actualThread;
    console.log(
      JSON.stringify({
        phase: "native-identity-bound",
        headSha: head,
        reviewerRuntime: runtime,
        reviewerSessionId: actualThread,
        rawPath,
        eventsPath,
        stderrPath,
      }),
    );
  }
  if (smoke) {
    const result = JSON.parse(raw.toString());
    if (JSON.stringify(result) !== JSON.stringify(proof))
      throw Error("Native smoke source/session binding mismatch");
    console.log(
      JSON.stringify({
        phase: "smoke-pass",
        headSha: head,
        reviewerRuntime: runtime,
        reviewerSessionId: proof.reviewer.taskId,
        rawPath,
        rawSha256: createHash("sha256").update(raw).digest("hex"),
        reviewStatusEligible: false,
      }),
    );
    return;
  }
  validateRawReviewArtifact(raw, policy, head, tree, manifest.sha256);
  await publishNativeReviewArtifact(head, rawPath);
}
if (import.meta.main) {
  try {
    const [head, runtime, ...flags] = Bun.argv.slice(2);
    if (
      !head ||
      !runtime ||
      flags.some((f) => !["--smoke", "--check"].includes(f)) ||
      (flags.includes("--smoke") && flags.includes("--check"))
    )
      throw Error("Use review-pr.ts <pushed-PR-head> <native-reviewer-runtime> [--smoke|--check]");
    await reviewPr(head, runtime, flags.includes("--smoke"), flags.includes("--check"));
  } catch (error) {
    console.error(locatedError(error, "review_artifact_refused"));
    process.exitCode = 1;
  }
}
