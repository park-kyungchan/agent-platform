import { insist, object, repositoryId, repository } from "./policy";

/** The check issuer reads the independent review status; only checks are writable. */
export const requestedPermissions = Object.freeze({
  actions: "read", checks: "write", contents: "read", metadata: "read", statuses: "read",
} as const);
export function boundedTokenRequest() {
  return { repository_ids: [repositoryId], permissions: { ...requestedPermissions } };
}
/** Pure payload validation; no key, token transport or native capability is available here. */
export function validateBoundedTokenResponse(minted: Record<string, unknown>, now: number): asserts minted is Record<string, unknown> & { token: string; expires_at: string } {
  insist(Number.isSafeInteger(now) && now > 0, "CURRENT_CLOCK_REQUIRED");
  insist(
    typeof minted.token === "string" && minted.token.length >= 20 && minted.token.length <= 512 &&
    typeof minted.expires_at === "string" && Date.parse(minted.expires_at) > now &&
    Date.parse(minted.expires_at) <= now + 3660000,
    "BOUNDED_INSTALLATION_TOKEN_REQUIRED",
  );
  const permissions = object(minted.permissions);
  insist(
    Object.keys(permissions).sort().join() === Object.keys(requestedPermissions).sort().join() &&
    Object.entries(requestedPermissions).every(([k, v]) => permissions[k] === v),
    "TOKEN_PERMISSION_EXPANSION",
  );
  insist(
    minted.repository_selection === "selected" && Array.isArray(minted.repositories) &&
    minted.repositories.length === 1 && object(minted.repositories[0]).id === repositoryId &&
    object(minted.repositories[0]).full_name === repository,
    "TOKEN_REPOSITORY_EXPANSION",
  );
}
