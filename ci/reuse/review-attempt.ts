import {
  reviewAttemptTarget,
  validateCompletedReviewObservation,
} from "../quality/review-artifact";
import { requireTrustedReviewContext, type TrustedReviewContext } from "./trusted-review-source";

type Status = Record<string, unknown>;
const record = (v: unknown): Status =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as Status) : {};
export type NativeReviewAttempt = Readonly<{
  id: string;
  pending: Status;
  context: TrustedReviewContext;
}>;
const attempts = new WeakSet<object>();
function need(v: unknown, code: string): asserts v {
  if (!v) throw Error("NATIVE_REVIEW_ATTEMPT_" + code);
}
function owner(status: Status, context: TrustedReviewContext): boolean {
  const actor = record(status.creator),
    expected = context.policy.publisher;
  return (
    !!actor &&
    String(actor.id) === expected.id &&
    actor.login === expected.login &&
    actor.type === "User"
  );
}
async function writeStatus(
  context: TrustedReviewContext,
  id: string,
  state: "pending" | "failure",
  description: string,
): Promise<Status> {
  requireTrustedReviewContext(context);
  await context.revalidate();
  const identity = Bun.spawnSync(["gh", "api", "user"], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  need(
    identity.exitCode === 0 && owner({ creator: JSON.parse(identity.stdout.toString()) }, context),
    "AUTHENTICATED_OWNER_REQUIRED",
  );
  const auth = Bun.spawnSync(["gh", "auth", "token"], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  need(auth.exitCode === 0, "NATIVE_AUTH_REQUIRED");
  const target = reviewAttemptTarget(context.policy.repository, context.head, id);
  console.log(
    JSON.stringify({
      phase: "review-status-request",
      attemptId: id,
      headSha: context.head,
      state,
      targetUrl: target,
      retryAllowed: false,
    }),
  );
  const response = await fetch(
    `https://api.github.com/repos/${context.policy.repository}/statuses/${context.head}`,
    {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(30_000),
      headers: {
        Authorization: "Bearer " + auth.stdout.toString().trim(),
        Accept: "application/vnd.github+json",
        "Content-Type": "application/json",
        "X-GitHub-Api-Version": "2026-03-10",
      },
      body: JSON.stringify({
        state,
        context: "native-independent-review",
        target_url: target,
        description,
      }),
    },
  );
  need(response.status === 201, "STATUS_OUTCOME_UNCERTAIN_RECONCILE_NO_RETRY");
  const status = (await response.json()) as Status;
  need(
    Number.isSafeInteger(status.id) &&
      Number(status.id) > 0 &&
      status.state === state &&
      status.target_url === target &&
      owner(status, context),
    "STATUS_OUTCOME_UNCERTAIN_RECONCILE_NO_RETRY",
  );
  console.log(
    JSON.stringify({
      phase: "review-status-written",
      attemptId: id,
      headSha: context.head,
      state,
      statusId: status.id,
    }),
  );
  return status;
}
export async function beginNativeReviewAttempt(
  context: TrustedReviewContext,
  id: string,
): Promise<NativeReviewAttempt> {
  const pending = await writeStatus(
    context,
    id,
    "pending",
    "Fresh independent native review in progress",
  );
  const attempt = Object.freeze({ id, pending: Object.freeze(pending), context });
  attempts.add(attempt);
  await requireCurrentReviewAttempt(context, attempt);
  return attempt;
}
export function requireNativeReviewAttempt(
  context: TrustedReviewContext,
  attempt: NativeReviewAttempt,
): void {
  requireTrustedReviewContext(context);
  need(attempts.has(attempt) && attempt.context === context, "AUTHENTICATED_ATTEMPT_REQUIRED");
}
export async function requireCurrentReviewAttempt(
  context: TrustedReviewContext,
  attempt: NativeReviewAttempt,
): Promise<unknown[]> {
  requireNativeReviewAttempt(context, attempt);
  const states = await context.readApi(`commits/${context.head}/statuses?per_page=100`);
  need(Array.isArray(states) && states.length <= 100, "STATUS_HISTORY_REQUIRED");
  const latest = states
    .map((s: unknown) => record(s))
    .find((s) => s.context === "native-independent-review") as Status | undefined;
  need(
    latest &&
      latest.id === attempt.pending.id &&
      latest.state === "pending" &&
      latest.target_url ===
        reviewAttemptTarget(context.policy.repository, context.head, attempt.id) &&
      owner(latest, context),
    "SUPERSEDED",
  );
  return states;
}
export async function failNativeReviewAttempt(
  attempt: NativeReviewAttempt,
  bytes: Uint8Array,
  tree: string,
  manifest: string,
): Promise<void> {
  const context = attempt.context;
  requireNativeReviewAttempt(context, attempt);
  const raw = validateCompletedReviewObservation(
    bytes,
    context.policy,
    context.head,
    tree,
    manifest,
  );
  need(
    raw.conclusion === "fail" &&
      raw.author.runtime === context.author.metadata.author.runtime &&
      raw.author.taskId ===
        (context.author.metadata.author.threadId ?? context.author.metadata.author.sessionId),
    "BOUND_NEGATIVE_REVIEW_REQUIRED",
  );
  // A newly completed genuine negative also invalidates a concurrently completed pass.
  // Consume before the POST: ambiguous outcomes require readback, never another write.
  attempts.delete(attempt);
  const status = await writeStatus(
    context,
    attempt.id,
    "failure",
    `Independent native review: ${Number(raw.unresolvedFindings)} unresolved finding(s)`,
  );
  const states = await context.readApi(`commits/${context.head}/statuses?per_page=100`);
  need(
    Array.isArray(states) &&
      states
        .map((s: unknown) => record(s))
        .some((s) => s.id === status.id && s.state === "failure" && owner(s, context)),
    "FAILURE_READBACK_REQUIRED",
  );
}
