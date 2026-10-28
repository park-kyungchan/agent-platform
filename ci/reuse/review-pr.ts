import { randomUUID } from "node:crypto";
import { authenticatedReviewReader, validateReviewArtifact } from "../quality/review-artifact";
import { publishNativeReviewArtifact } from "./publish-review-artifact";
import { createTrustedReviewContext, reviewControllerRepository } from "./trusted-review-source";
import {
  beginNativeReviewAttempt,
  failNativeReviewAttempt,
  invokeNativeReview,
  smokeNativeReview,
  readCompletedNativeObservation,
  nativeReviewSource,
} from "./review-attempt";
import { locatedError } from "./located-error";

export async function reviewPr(
  repository: string,
  head: string,
  runtime: string,
  controllerSha: string,
  smoke = false,
  checkOnly = false,
) {
  if (process.env.CI || process.env.GITHUB_ACTIONS)
    throw Error("Native review cannot originate in CI");
  if (!smoke && !checkOnly && repository === reviewControllerRepository && head === controllerSha)
    throw Error(
      "Formal review head must be distinct from its independently qualified controller pin",
    );
  const context = await createTrustedReviewContext(controllerSha, repository, head);
  if (checkOnly) {
    const auth = Bun.spawnSync(["gh", "auth", "token"], {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    if (auth.exitCode !== 0) throw Error("Native authenticated review check unavailable");
    const reader = authenticatedReviewReader(
      context.policy.repository,
      auth.stdout.toString().trim(),
    );
    const statuses = await reader.json(`commits/${head}/statuses?per_page=100`);
    if (!Array.isArray(statuses)) throw Error("Native review statuses unavailable");
    const status = statuses.find((s) => s?.context === "native-independent-review"),
      result = await validateReviewArtifact({ policy: context.policy, head, status }, reader),
      { manifest } = nativeReviewSource(context);
    if (
      result.reviewer.runtime !== runtime ||
      result.author.runtime !== context.author.metadata.author.runtime ||
      result.author.taskId !==
        (context.author.metadata.author.threadId ?? context.author.metadata.author.sessionId) ||
      result.manifest.sha256 !== manifest.sha256
    )
      throw Error("Current native review/source/runtime binding differs");
    console.log(JSON.stringify({ phase: "verified-existing-native-review", ...result }));
    return;
  }
  if (smoke) {
    await smokeNativeReview(context, runtime);
    return;
  }
  const attempt = await beginNativeReviewAttempt(context, randomUUID());
  const observation = await invokeNativeReview(context, attempt, runtime);
  if (readCompletedNativeObservation(context, attempt, observation).raw.conclusion === "fail") {
    await failNativeReviewAttempt(attempt, observation);
    throw Error("Native review found unresolved findings; authenticated failure published");
  }
  await publishNativeReviewArtifact(context, observation, attempt);
}
if (import.meta.main) {
  try {
    const [repository, head, runtime, controllerSha, ...flags] = Bun.argv.slice(2);
    if (
      !repository ||
      !head ||
      !runtime ||
      !controllerSha ||
      flags.some((f) => !["--smoke", "--check"].includes(f)) ||
      (flags.includes("--smoke") && flags.includes("--check"))
    )
      throw Error(
        "Use the separate pinned controller: review-pr.ts <repository> <head> <runtime> <controller-sha> [--smoke|--check]",
      );
    await reviewPr(
      repository,
      head,
      runtime,
      controllerSha,
      flags.includes("--smoke"),
      flags.includes("--check"),
    );
  } catch (error) {
    console.error(locatedError(error, "review_artifact_refused"));
    process.exitCode = 1;
  }
}
