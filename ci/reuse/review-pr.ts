import { randomUUID, createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { deriveReviewedSourceManifest } from "../quality/receipt-validator";
import { validateRawReviewArtifact } from "../quality/review-artifact";
import { nativeReviewPolicy, publishNativeReviewArtifact } from "./publish-review-artifact";
import { verifyCommittedSourceAuthor } from "./source-author";
import { reviewerInvocation } from "./review-runtimes";
import { locatedError } from "./located-error";

/** One native command: exact pushed PR head -> fresh independent native output -> verified status. */
export async function reviewPr(head: string, runtime: string, smoke = false) {
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
  const sessionId = randomUUID(),
    command = reviewerInvocation(runtime, sessionId, smoke);
  if (!Bun.which(command[0]!)) throw Error("Requested native reviewer CLI is not installed");
  if (!smoke && runtime === author.metadata.author.runtime)
    throw Error("Choose an independent reviewer runtime family");
  const directory = await mkdtemp(join(tmpdir(), `native-review-${head.slice(0, 9)}-`));
  const proof = {
    schema: smoke ? "native-review-smoke.v1" : "native-independent-review-output.v1",
    reviewStatus: "complete",
    binding: policy.binding,
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: manifest.sha256,
    reviewer: { runtime, taskId: sessionId },
    author: {
      runtime: author.metadata.author.runtime,
      taskId: author.metadata.author.threadId ?? author.metadata.author.sessionId,
    },
    conclusion: "pass",
    unresolvedFindings: 0,
    findings: [],
  };
  const prompt = smoke
    ? `Mechanical transport smoke only. Do not inspect files, run tools, perform source review or claim Acceptance. Return ONLY this JSON object, unchanged in meaning and without markdown. This record is ineligible for native-independent-review status.\n${JSON.stringify(proof, null, 2)}\n`
    : `You are a fresh independent native source reviewer. Read-only: no edits, writes, ref/index/config changes, publication, subagents or private user-data reads. Read AGENTS.md and review the exact pushed head ${head}, tree ${tree}, against its first parent in ${root}. Inspect all public technical changes and actionable correctness/security/regression/scope defects. Never read private student/class-record bodies or print private path names; private custody changes are metadata-only. CI cannot mint independent native review. Raw native output, source/publisher/run/artifact/digest/latest-status joins must be exact. Do not assume checks passed or native activation/Acceptance occurred. Return ONLY valid JSON without fences with the following exact proof fields. If findings remain, set conclusion fail and the exact unresolved count, with severity/path/line/explanation/suggestedFix per finding. Otherwise pass/zero/empty findings.\n${JSON.stringify(proof, null, 2)}\n`;
  const promptPath = join(directory, "prompt.txt"),
    rawPath = join(directory, "native.raw.json"),
    stderrPath = join(directory, "native.stderr.log");
  await writeFile(promptPath, prompt, { mode: 0o600 });
  // Muse takes a positional prompt; Claude's print mode reads stdin. Both use an actual native session UUID.
  const argv = runtime === "native-muse" ? [...command, "--prompt-file", promptPath] : command;
  console.log(
    JSON.stringify({
      phase: smoke ? "native-smoke" : "native-review",
      headSha: head,
      reviewerRuntime: runtime,
      reviewerSessionId: sessionId,
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
    Bun.write(rawPath, child.stdout),
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
  if (smoke) {
    const result = JSON.parse(raw.toString());
    if (JSON.stringify(result) !== JSON.stringify(proof))
      throw Error("Native smoke source/session binding mismatch");
    console.log(
      JSON.stringify({
        phase: "smoke-pass",
        headSha: head,
        reviewerRuntime: runtime,
        reviewerSessionId: sessionId,
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
    if (!head || !runtime || flags.some((f) => f !== "--smoke"))
      throw Error("Use review-pr.ts <pushed-PR-head> <native-reviewer-runtime> [--smoke]");
    await reviewPr(head, runtime, flags.includes("--smoke"));
  } catch (error) {
    console.error(locatedError(error, "review_artifact_refused"));
    process.exitCode = 1;
  }
}
