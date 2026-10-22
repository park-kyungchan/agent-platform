import { expect, test } from "bun:test";
import { boundedTokenRequest, validateBoundedTokenResponse } from "./token-policy";
const now = Date.parse("2026-10-20T12:00:00Z");
function response() {
  return { token: "fictional-payload-only-credential", expires_at: "2026-10-20T13:00:00Z",
    permissions: { actions: "read", checks: "write", contents: "read", metadata: "read", statuses: "read" },
    repository_selection: "selected", repositories: [{ id: 1413326539, full_name: "park-kyungchan/agent-platform" }] };
}
test("bounded issuer payload retains review reads and only check write for one fixed repository", () => {
  expect(boundedTokenRequest()).toEqual({ repository_ids: [1413326539], permissions: response().permissions });
  expect(() => validateBoundedTokenResponse(response(), now)).not.toThrow();
  for (const permissions of [
    { ...response().permissions, statuses: "write" },
    { ...response().permissions, contents: "write" },
    { ...response().permissions, actions: "write" },
    { ...response().permissions, metadata: "write" },
    { ...response().permissions, checks: "read" },
    { ...response().permissions, issues: "read" },
    { checks: "write", contents: "read", metadata: "read", actions: "read" },
  ]) expect(() => validateBoundedTokenResponse({ ...response(), permissions }, now)).toThrow("TOKEN_PERMISSION_EXPANSION");
  for (const change of [
    { repositories: [...response().repositories, { id: 1, full_name: "other/repo" }] },
    { repositories: [{ id: 1, full_name: "park-kyungchan/agent-platform" }] },
    { repositories: [{ id: 1413326539, full_name: "other/repo" }] },
    { repository_selection: "all" },
  ]) expect(() => validateBoundedTokenResponse({ ...response(), ...change }, now)).toThrow("TOKEN_REPOSITORY_EXPANSION");
});
