import { buildEligibilityCheck } from "./eligibility";
import { lstatSync, realpathSync, readFileSync } from "node:fs";
import { assertNativeContext, readOwnerReconciliation } from "./native";
import type { NativeContext } from "./native-types";
import {
  checkName,
  appId,
  object,
  insist,
  repository,
  repositoryId,
  positive,
  parseReport,
  qualifyTrustedExecution,
  trustedDigest,
  same,
  sha256,
  workflowPath,
} from "./policy";
import { fixedProcess } from "./process";
import {
  type ProviderReader,
  repoPath,
  fetchExecution,
  verifyPolicyRef,
  verifyRun,
} from "./provider";
import { verifyCurrentTarget, readSubject, readAuthor } from "./source";
import { verifyReview } from "./review";

async function gh(
  context: NativeContext,
  method: "GET" | "POST",
  path: string,
  body?: unknown,
  include = false,
): Promise<Buffer> {
  assertNativeContext(context);
  insist(
    path === "/user" || path.startsWith(`/repos/${repository}/`),
    "FIXED_NATIVE_PROVIDER_SCOPE_REQUIRED",
  );
  const r = await fixedProcess(
    [
      context.profile.ghPath,
      "api",
      ...(include ? ["--include"] : []),
      "--hostname",
      "github.com",
      "--method",
      method,
      "-H",
      "Accept: application/vnd.github+json",
      "-H",
      "X-GitHub-Api-Version: 2026-03-10",
      path,
      ...(body ? ["--input", "-"] : []),
    ],
    {
      cwd: context.profile.sourceRoot,
      env: { HOME: "/root", GH_HOST: "github.com", GH_PROMPT_DISABLED: "1" },
      stdin: body ? Buffer.from(JSON.stringify(body)) : undefined,
      maxBytes: 8_000_000,
      timeoutMs: 60000,
    },
  );
  insist(r.code === 0 && !r.timedOut, "NATIVE_PROVIDER_OUTCOME_UNKNOWN_RECONCILE_REQUIRED");
  return r.stdout;
}
/** Existing native gh authentication stays with its native owner; no credential bridge/export. */
export function nativeProvider(context: NativeContext): ProviderReader {
  return {
    json: async (path) => JSON.parse((await gh(context, "GET", path)).toString()) as unknown,
    archive: async () => {
      throw Error("TRUSTED_CI_NARROW_APP_ARCHIVE_READER_REQUIRED");
    },
  };
}
function checkoutRoot(context: NativeContext, suffix: "authored" | "tested"): string {
  const head =
    suffix === "authored"
      ? context.intent.target.authored.commit
      : context.intent.target.tested.commit;
  const root = `/var/lib/agent-platform/trusted-ci/checkouts/${head}`;
  insist(
    context.intent.checkoutRoot === "/var/lib/agent-platform/trusted-ci/checkouts" &&
      realpathSync(root) === root &&
      lstatSync(root).uid === 0 &&
      !(lstatSync(root).mode & 0o222),
    "ROOT_READONLY_EXACT_CHECKOUT_REQUIRED",
  );
  const config = root + "/.git/config";
  insist(
    realpathSync(config) === config &&
      lstatSync(config).uid === 0 &&
      !(lstatSync(config).mode & 0o222) &&
      !/\[(?:include|includeIf|filter|credential|http|url|alias|merge)[\s\]"]/i.test(
        readFileSync(config, "utf8"),
      ),
    "CANDIDATE_GIT_EXECUTION_CONFIG_REFUSED",
  );
  return root;
}
export async function verifyAndBuild(context: NativeContext, reader: ProviderReader) {
  assertNativeContext(context);
  const {
    intent: i,
    profile: { policy: p },
  } = context;
  insist(positive(i.runId) && positive(i.attempt), "EXACT_NATIVE_RUN_ATTEMPT_REQUIRED");
  await verifyPolicyRef(reader, p);
  await verifyCurrentTarget(reader, i.target);
  const hRoot = checkoutRoot(context, "authored"),
    mRoot = checkoutRoot(context, "tested");
  insist(
    same(await readSubject(reader, hRoot, i.target.authored.commit), i.target.authored) &&
      same(await readSubject(reader, mRoot, i.target.tested.commit), i.target.tested),
    "ACTUAL_NATIVE_SUBJECT_MISMATCH",
  );
  await readAuthor(mRoot, i.target.tested);
  const review = await verifyReview(reader, p, hRoot, i.target.authored);
  const downloaded = await fetchExecution(reader, p, i.runId, i.attempt, i.requestId),
    report = parseReport(downloaded.reportBytes);
  insist(
    report.runId === i.runId &&
      report.attempt === i.attempt &&
      report.requestId === i.requestId &&
      same(report.target, i.target),
    "NATIVE_REPORT_TUPLE_MISMATCH",
  );
  qualifyTrustedExecution(p, report, Date.now());
  const binding = {
    policyRelease: p.release,
    target: i.target,
    runId: i.runId,
    attempt: i.attempt,
    artifactId: downloaded.artifactId,
    archiveSha256: downloaded.archiveSha256,
    reportSha256: downloaded.reportSha256,
    canonicalReportSha256: trustedDigest(report),
    reviewReceiptSha256: review.receiptSha256,
    sourceAuthorSha256: review.sourceAuthor.metadataSha256,
  };
  context.record({
    phase: "provider-qualified",
    ...binding,
    jobsSha256: downloaded.jobsSha256,
    nativeAuthenticationEstablished: false,
    sourceAuthorNativeTaskId: review.sourceAuthor.metadata.author.nativeTaskId,
    sourceAuthorNativeRunId: review.sourceAuthor.metadata.author.nativeRunId,
  });
  async function recheck(): Promise<void> {
    assertNativeContext(context);
    await verifyPolicyRef(reader, p);
    await verifyCurrentTarget(reader, i.target);
    verifyRun(
      p,
      await reader.json(repoPath(`actions/runs/${i.runId}`)),
      i.runId!,
      i.attempt!,
      i.requestId,
      true,
    );
    const current = await verifyReview(reader, p, hRoot, i.target.authored);
    insist(
      current.receiptSha256 === review.receiptSha256 &&
        same(current.latestStatus, review.latestStatus),
      "CURRENT_REVIEW_CHANGED",
    );
    assertNativeContext(context);
  }
  await recheck();
  return {
    subjectHead:
      i.target.kind === "pull-request" ? i.target.authored.commit : i.target.tested.commit,
    externalId: trustedDigest(binding),
    reportSha256: downloaded.reportSha256,
    recheck,
    check: buildEligibilityCheck(
      p,
      report,
      { reportSha256: downloaded.reportSha256, externalId: trustedDigest(binding) },
      Date.now(),
    ),
  };
}
export async function attestNative(context: NativeContext): Promise<void> {
  assertNativeContext(context);
  await (await import("./github-app")).issueNative(context);
}
export async function dispatchNative(context: NativeContext): Promise<void> {
  assertNativeContext(context);
  const {
      intent: i,
      profile: { policy: p },
    } = context,
    reader = nativeProvider(context);
  insist(
    i.action === "dispatch" && i.runId === null && i.attempt === null,
    "FRESH_DISPATCH_INTENT_REQUIRED",
  );
  insist(
    object(await reader.json("/user")).id === p.dispatcher.id,
    "ACTUAL_DISPATCHER_ID_REQUIRED",
  );
  await verifyPolicyRef(reader, p);
  await verifyCurrentTarget(reader, i.target);
  const body = {
    ref: p.protectedTag,
    inputs: {
      request_id: i.requestId,
      kind: i.target.kind,
      pr: String(i.target.pr),
      head: i.target.authored.commit,
      base: i.target.base,
      merge: i.target.tested.commit,
    },
  };
  context.record({
    phase: "dispatch-intent",
    requestId: i.requestId,
    policyRelease: p.release,
    bodySha256: sha256(JSON.stringify(body)),
    retryAllowed: false,
  });
  assertNativeContext(context);
  const raw = (
    await gh(
      context,
      "POST",
      repoPath(`actions/workflows/${workflowPath.split("/").at(-1)}/dispatches`),
      body,
      true,
    )
  ).toString();
  const split = raw.search(/\r?\n\r?\n/);
  insist(
    split >= 0 && /^HTTP\/[^ ]+ 200(?: |\r?$)/m.test(raw.slice(0, split)),
    "DISPATCH_200_RUN_ID_REQUIRED_RECONCILE",
  );
  const result = object(JSON.parse(raw.slice(split).trim()));
  insist(positive(result.workflow_run_id), "EXACT_DISPATCH_RUN_ID_REQUIRED");
  context.record({
    phase: "dispatch-result",
    runId: result.workflow_run_id,
    requestId: i.requestId,
    responseSha256: sha256(raw),
  });
  const run = object(await reader.json(repoPath(`actions/runs/${result.workflow_run_id}`)));
  verifyRun(p, run, result.workflow_run_id, Number(run.run_attempt), i.requestId, false);
  context.record({
    phase: "dispatch-readback",
    runId: result.workflow_run_id,
    attempt: run.run_attempt,
    responseSha256: sha256(JSON.stringify(run)),
  });
}
/** Read-only exact reconciliation. It never repeats a mint, dispatch or check POST. */
export async function reconcileNative(context: NativeContext): Promise<void> {
  assertNativeContext(context);
  const i = context.intent,
    reader = nativeProvider(context);
  await verifyPolicyRef(reader, context.profile.policy);
  await verifyCurrentTarget(reader, i.target);
  const journal = readFileSync(`/var/lib/agent-platform/trusted-ci/results/${i.id}.jsonl`, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => object(object(JSON.parse(line)).value));
  if (i.action === "dispatch") {
    const recorded = journal.find((x) => x.phase === "dispatch-result");
    const runId = i.runId ?? recorded?.runId ?? readOwnerReconciliation(context).runId;
    insist(positive(runId), "ROOT_MUST_BIND_OBSERVED_EXACT_RUN_ID_FOR_RECONCILIATION");
    const raw = await reader.json(repoPath(`actions/runs/${runId}`));
    const r = object(raw);
    insist(
      r.id === runId &&
        r.head_sha === context.profile.sourceHead &&
        r.event === "workflow_dispatch" &&
        r.display_title === "Trusted source " + i.requestId &&
        object(r.actor).id === context.profile.policy.dispatcher.id &&
        object(r.triggering_actor).id === context.profile.policy.dispatcher.id,
      "EXACT_DISPATCH_RECONCILIATION_REQUIRED",
    );
    context.record({
      phase: "reconciled-run",
      runId,
      attempt: r.run_attempt,
      status: r.status,
      conclusion: r.conclusion,
      responseSha256: sha256(JSON.stringify(raw)),
      retryAllowed: false,
    });
    return;
  }
  const path = context.profile.sourceRoot; // Loaded code identity remains independent of publication identity.
  const intent = journal.find((x) => x.phase === "check-create-intent");
  insist(
    intent && typeof intent.externalId === "string",
    "MINT_OR_PRECHECK_UNKNOWN_REQUIRES_OWNER_EXPIRY_RECONCILIATION",
  );
  const r = object(
    await reader.json(
      repoPath(
        `commits/${intent.head}/check-runs?check_name=${encodeURIComponent(checkName)}&filter=all&per_page=100`,
      ),
    ),
  );
  insist(
    Number(r.total_count) < 100 && Array.isArray(r.check_runs),
    "COMPLETE_RECONCILIATION_PAGE_REQUIRED",
  );
  const matches = r.check_runs
    .map(object)
    .filter((x) => x.external_id === intent.externalId && object(x.app).id === appId);
  insist(
    matches.length === 1 && matches[0]!.head_sha === intent.head && matches[0]!.name === checkName,
    "UNKNOWN_OR_DUPLICATE_POST_OUTCOME_NO_RETRY",
  );
  context.record({
    phase: "reconciled-check",
    checkId: matches[0]!.id,
    externalId: intent.externalId,
    conclusion: matches[0]!.conclusion,
    loadedSourceRoot: path,
    retryAllowed: false,
  });
}
