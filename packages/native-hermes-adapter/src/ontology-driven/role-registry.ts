import {
  SEMANTIC_OWNER,
  exactKeys,
  identity,
  object,
  pin,
  requireValue,
  strings,
  text,
  type Pin,
} from "./contracts";
export interface ProfileRole {
  profile: string;
  roleId: string;
  title: string;
  responsibilities: string[];
  delivery: "native-qualified-tools" | "supplied-evidence-only" | "pinned-source-review";
  capabilityRoute: string;
  toolNames: string[];
}
export interface ProfileRoleRegistry {
  schema: "hermes.profile-role-registry.v1";
  semanticOwner: typeof SEMANTIC_OWNER;
  revision: string;
  basis: "delegated-design";
  intent: Pin;
  authority: "none";
  fixedTeamRequired: false;
  roles: ProfileRole[];
}
export function profileRoleRegistry(value: unknown): ProfileRoleRegistry {
  object(value, "role registry");
  exactKeys(
    value,
    [
      "schema",
      "semanticOwner",
      "revision",
      "basis",
      "intent",
      "authority",
      "fixedTeamRequired",
      "roles",
    ],
    "role registry",
  );
  requireValue(
    value.schema === "hermes.profile-role-registry.v1" &&
      value.semanticOwner === SEMANTIC_OWNER &&
      value.basis === "delegated-design" &&
      value.authority === "none" &&
      value.fixedTeamRequired === false,
    "ROLE_REGISTRY_BOUNDARY",
    "A role is not a native tool grant or mandatory team.",
  );
  identity(value.revision, "role revision");
  pin(value.intent);
  requireValue(
    Array.isArray(value.roles) && value.roles.length > 0 && value.roles.length <= 128,
    "ROLE_DENOMINATOR",
    "Explicit profile bindings required.",
  );
  const profiles = new Set<string>();
  for (const row of value.roles) {
    object(row, "profile role");
    exactKeys(
      row,
      [
        "profile",
        "roleId",
        "title",
        "responsibilities",
        "delivery",
        "capabilityRoute",
        "toolNames",
      ],
      "profile role",
    );
    identity(row.profile, "profile");
    identity(row.roleId, "roleId");
    text(row.title, "title");
    text(row.capabilityRoute, "capabilityRoute");
    strings(row.responsibilities, "responsibilities");
    strings(row.toolNames, "toolNames");
    requireValue(
      row.responsibilities.length > 0 &&
        row.responsibilities.length <= 8 &&
        ["native-qualified-tools", "supplied-evidence-only", "pinned-source-review"].includes(
          row.delivery,
        ),
      "ROLE_DELIVERY",
      row.profile,
    );
    requireValue(
      row.delivery !== "supplied-evidence-only" || row.toolNames.length === 0,
      "SUPPLIED_ROLE_TOOL_GRANT",
      row.profile,
    );
    requireValue(!profiles.has(row.profile), "DUPLICATE_PROFILE_ROLE", row.profile);
    profiles.add(row.profile);
  }
  return structuredClone(value) as ProfileRoleRegistry;
}
export function selectProfileRole(input: unknown, profile: string) {
  const registry = profileRoleRegistry(input),
    role = registry.roles.find((row) => row.profile === profile);
  requireValue(role, "ROLE_NOT_ASSIGNED", profile);
  return {
    schema: "hermes.selected-profile-role.v1" as const,
    semanticOwner: registry.semanticOwner,
    revision: registry.revision,
    actorProfile: profile,
    role: structuredClone(role),
    fixedTeamRequired: false as const,
    authority: "none" as const,
    nativePermissionGranted: false as const,
  };
}
export function renderProfileRoleParagraph(
  input: unknown,
  profile: string,
  contextLocator: string,
) {
  text(contextLocator, "context locator");
  const selected = selectProfileRole(input, profile),
    role = selected.role;
  return (
    `${role.title}: ${role.responsibilities.join(" ")} Role context: ${contextLocator}. ${role.capabilityRoute} ` +
    (role.delivery === "supplied-evidence-only"
      ? "Use caller-supplied evidence; do not claim filesystem or native tool inspection."
      : "Use this route only when its native tools are actually exposed and the current Work/effect admission permits it.") +
    " This responsibility does not grant native permissions or require a fixed team."
  );
}
