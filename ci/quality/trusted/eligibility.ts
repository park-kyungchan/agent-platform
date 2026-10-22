import {
  checkName,
  repository,
  eligibilityClaim,
  qualifyTrustedExecution,
  trustedDigest,
  type TrustedPolicy,
  type ExecutionReport,
} from "./policy";

/** Pure issuer gate, shared by production after provider/review joins and private control flow tests.
 * Calling it grants no transport, key, native context or publication capability.
 */
export function buildEligibilityCheck(
  policy: TrustedPolicy,
  report: ExecutionReport,
  binding: { reportSha256: string; externalId: string },
  now: number,
) {
  qualifyTrustedExecution(policy, report, now);
  if (![binding.reportSha256, binding.externalId].every((v) => /^[a-f0-9]{64}$/.test(v)))
    throw Error("TRUSTED_CI_EXACT_ISSUER_BINDING_REQUIRED");
  return {
    name: checkName,
    head_sha:
      report.target.kind === "pull-request"
        ? report.target.authored.commit
        : report.target.tested.commit,
    external_id: binding.externalId,
    details_url: `https://github.com/${repository}/actions/runs/${report.runId}/attempts/${report.attempt}`,
    status: "completed",
    conclusion: "success",
    output: {
      title: "Bounded P semantic source eligibility",
      summary: `Claim ${eligibilityClaim}; policy ${policy.release}; catalogue ${policy.catalogueSha256}; coverage ${policy.coverageSha256}; ${policy.probes.length} externally compared cases per required subject; H ${report.target.authored.commit}; B ${report.target.base}; tested ${report.target.tested.commit}; report SHA256 ${binding.reportSha256}; canonical report ${trustedDigest(report)}. Full legacy workload remains required, its shared-process assertion completion is unauthenticated. Existing legacy required checks, independent review, explicit Lead integration and native owner activation remain separate.`,
    },
  };
}
