import { sign } from "node:crypto";
import { assertNativeContext, readAppKeyForNative } from "./native";
import type { NativeContext } from "./native-types";
import {
  appId,
  installationId,
  repositoryId,
  insist,
  object,
  checkName,
  validateTrustedPolicy,
  sha256,
} from "./policy";
import { boundedBody, githubReader, repoPath, verifyPolicyRef } from "./provider";
import { nativeProvider, verifyAndBuild } from "./attest";
import { verifyCurrentTarget } from "./source";

export { requestedPermissions } from "./token-policy";
import { boundedTokenRequest, validateBoundedTokenResponse } from "./token-policy";
/** Pure cryptography for fixture tests. It supplies no access to the installed key or App transport. */
export function signAttestorJwt(key: string | Buffer, now: number): string {
  insist(Number.isSafeInteger(now) && now > 0, "CURRENT_CLOCK_REQUIRED");
  const b64 = (v: unknown) => Buffer.from(JSON.stringify(v)).toString("base64url");
  const body =
    b64({ alg: "RS256", typ: "JWT" }) + "." + b64({ iat: now - 60, exp: now + 540, iss: appId });
  return body + "." + sign("RSA-SHA256", Buffer.from(body), key).toString("base64url");
}
async function api(
  method: "GET" | "POST",
  path: string,
  token: string,
  body?: unknown,
): Promise<Record<string, unknown>> {
  insist(path.startsWith("/") && !/[\r\n\0\\]/.test(path), "FIXED_APP_PROVIDER_PATH_REQUIRED");
  const url = new URL("https://api.github.com" + path);
  insist(url.origin === "https://api.github.com", "FIXED_APP_PROVIDER_REQUIRED");
  const r = await fetch(url, {
    method,
    redirect: "error",
    signal: AbortSignal.timeout(60000),
    headers: {
      Authorization: "Bearer " + token,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2026-03-10",
      ...(body ? { "Content-Type": "application/json" } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  const bytes = await boundedBody(r, 2_000_000);
  insist(
    r.status === (method === "GET" ? 200 : 201),
    "APP_HTTP_" + r.status + "_RECONCILE_REQUIRED",
  );
  return object(JSON.parse(bytes.toString()));
}
/** Only a live bootstrap-minted native context can enter; there is no raw-report/payload write API. */
export async function issueNative(context: NativeContext): Promise<void> {
  assertNativeContext(context);
  insist(context.intent.action === "attest", "ATTEST_INTENT_REQUIRED");
  // Immutable catalogue validation precedes key access; provider-qualified external assertions
  // and the same pure issuer gate are mandatory before the single check POST.
  validateTrustedPolicy(context.profile.policy);
  // Actual current target is checked using the existing native provider before any key/token read.
  const nativeReader = nativeProvider(context);
  await verifyCurrentTarget(nativeReader, context.intent.target);
  await verifyPolicyRef(nativeReader, context.profile.policy);
  const policyCommit = object(
    await nativeReader.json(repoPath("git/commits/" + context.profile.sourceHead)),
  );
  insist(
    policyCommit.sha === context.profile.sourceHead &&
      object(policyCommit.tree).sha === context.profile.sourceTree,
    "ACTUAL_INSTALLED_POLICY_TREE_MISMATCH",
  );
  assertNativeContext(context);
  const key = readAppKeyForNative(context);
  let jwt: string;
  try {
    jwt = signAttestorJwt(key, Math.floor(Date.now() / 1000));
  } finally {
    key.fill(0);
  }
  const app = await api("GET", "/app", jwt);
  insist(app.id === appId, "ACTUAL_APP_ID_MISMATCH");
  const install = await api("GET", repoPath("installation"), jwt);
  insist(
    install.id === installationId &&
      install.app_id === appId &&
      object(install.account).id === 241037670,
    "ACTUAL_INSTALLATION_MISMATCH",
  );
  // Broad configured installation grants are left untouched; only this token is narrowed.
  const tokenRequest = boundedTokenRequest();
  assertNativeContext(context);
  context.record({
    phase: "mint-intent",
    appId,
    installationId,
    tokenRequest,
    at: new Date().toISOString(),
    retryAllowed: false,
  });
  const minted = await api(
    "POST",
    `/app/installations/${installationId}/access_tokens`,
    jwt,
    tokenRequest,
  );
  jwt = "";
  validateBoundedTokenResponse(minted, Date.now());
  context.record({
    phase: "mint-result",
    appId,
    installationId,
    repositoryId,
    expiresAt: minted.expires_at,
    credentialValuesRecorded: false,
  });
  const token = minted.token;
  const reader = githubReader(token),
    verified = await verifyAndBuild(context, reader);
  assertNativeContext(context);
  await verified.recheck();
  const body = verified.check;
  assertNativeContext(context);
  context.record({
    phase: "check-create-intent",
    externalId: verified.externalId,
    bodySha256: sha256(JSON.stringify(body)),
    head: verified.subjectHead,
    runId: context.intent.runId,
    attempt: context.intent.attempt,
    retryAllowed: false,
  });
  const created = await api("POST", repoPath("check-runs"), token, body);
  insist(
    Number.isSafeInteger(created.id) && Number(created.id) > 0,
    "CHECK_ID_UNKNOWN_RECONCILE_REQUIRED",
  );
  context.record({
    phase: "check-create-result",
    checkId: created.id,
    externalId: verified.externalId,
  });
  const actual = await api("GET", repoPath("check-runs/" + created.id), token);
  insist(
    actual.id === created.id &&
      object(actual.app).id === appId &&
      actual.name === checkName &&
      actual.head_sha === verified.subjectHead &&
      actual.external_id === verified.externalId &&
      actual.status === "completed" &&
      actual.conclusion === "success" &&
      actual.details_url === body.details_url,
    "EXACT_ISSUER_CHECK_READBACK_REQUIRED",
  );
  await verified.recheck();
  context.record({
    phase: "issued-and-readback",
    checkId: created.id,
    externalId: verified.externalId,
    responseSha256: sha256(JSON.stringify(actual)),
    nativePermissionGrantedBySource: false,
    automaticMerge: false,
  });
}
