/** One canonical, builtin-only versioned source binding contract. These records
 * describe source; neither their digest nor their location authenticates a native owner. */
import { createHash } from "node:crypto";
import { isAbsolute, resolve } from "node:path";

export const sourceLimits = Object.freeze({
  files: 12000,
  fileBytes: 134217728,
  totalBytes: 536870912,
  metadataBytes: 2097152,
});
export type SourceFileClass = "source" | "definition" | "dependency" | "tool" | "asset";
export interface SourceFilePin {
  path: string;
  sha256: string;
  bytes: number;
  mode: 0o444 | 0o555;
  class: SourceFileClass;
}
export interface SourceContentBinding {
  schema: "common.source-content.v1";
  repositoryId: string;
  commit: string;
  tree: string;
  files: SourceFilePin[];
  entries: { id: string; path: string }[];
  dependencies: { name: string; version: string; manifest: string; root: string }[];
  tools: { id: string; version: string; path: string }[];
}
export interface SourceInstalledSelection {
  kind: "dependency" | "tool";
  id: string;
  version: string;
  root: string;
  inventoryDigest: string;
  path: string;
}
export interface SourceIdentity {
  host: string;
  mountNamespace: string;
  uid: number;
  gid: number;
  profile: string;
  role: string;
}
export interface SourceLocationBinding {
  schema: "common.source-location.v1";
  contentDigest: string;
  root: string;
  kind: "immutable-release" | "audit" | "dependency";
  identity: SourceIdentity;
  readOnly: true;
}
export type SourceProviderId = "codex-context" | "codex-native-source" | "hermes-context-source";
export interface SourceProviderRegistration {
  schema: "common.source-provider.v1";
  id: SourceProviderId;
  contentDigest: string;
  locationDigest: string;
  generation: string;
  entries: { role: string; entry: string }[];
  authority: "none";
  nativePermissionGranted: false;
}
export interface OriginalSourceRecovery {
  schema: "common.original-source-recovery.v1";
  providerId: SourceProviderId;
  contentDigest: string;
  locationDigest: string;
  registrationDigest: string;
  originalReceipt: { owner: string; uri: string; revision: string; sha256: string };
}
export interface SourceBindingInput {
  content: SourceContentBinding;
  location: SourceLocationBinding;
  registration: SourceProviderRegistration;
  registrationPath: string;
  mode: "current" | "recovery";
  recovery?: OriginalSourceRecovery;
}
export const sourceHash = (bytes: Uint8Array | string): string =>
  createHash("sha256").update(bytes).digest("hex");
const forbidden = new Set([
  ".git",
  ".codex",
  ".config",
  ".hermes",
  ".ssh",
  ".private",
  ".local",
  ".cache",
  "state",
  "sessions",
  "credentials",
  "secrets",
  "backups",
  "work",
  "runs",
  "history",
  "owner-fixture",
  "native-home",
  "runtime-home",
]);
const forbiddenNames =
  /^(?:\.env(?:\..*)?|\.npmrc|auth\.json|credentials\.json|authorized_keys|known_hosts|id_rsa|id_ed25519|.*\.(?:sqlite(?:3)?|db)(?:-(?:wal|shm))?)$/iu;
export function sourceRelativePath(path: unknown, dependency = false): string {
  if (
    typeof path !== "string" ||
    !path ||
    path.length > 1024 ||
    isAbsolute(path) ||
    /[\\\u0000-\u001f\u007f]/u.test(path)
  )
    throw Error("SOURCE_UNSAFE_RELATIVE_PATH");
  const parts = path.split("/");
  if (
    parts.some(
      (part) =>
        !part ||
        part === "." ||
        part === ".." ||
        forbidden.has(part.toLowerCase()) ||
        forbiddenNames.test(part) ||
        (!dependency && part === "node_modules"),
    )
  )
    throw Error("SOURCE_EXCLUDED_OR_UNSAFE_PATH");
  return path;
}
export function sourceAbsolutePath(path: unknown): string {
  if (
    typeof path !== "string" ||
    !isAbsolute(path) ||
    resolve(path) !== path ||
    /[\\\u0000-\u001f\u007f]/u.test(path) ||
    path
      .split("/")
      .some(
        (part) =>
          ["current", "latest"].includes(part) ||
          forbidden.has(part.toLowerCase()) ||
          forbiddenNames.test(part),
      )
  )
    throw Error("SOURCE_EXACT_PHYSICAL_PATH_REQUIRED");
  return path;
}
function object(value: unknown, names: readonly string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value)) ||
    Reflect.ownKeys(value).some((key) => typeof key !== "string") ||
    Object.values(Object.getOwnPropertyDescriptors(value)).some(
      (row) => !Object.hasOwn(row, "value") || !row.enumerable,
    ) ||
    Object.keys(value).sort().join("\0") !== [...names].sort().join("\0")
  )
    throw Error("SOURCE_EXACT_RECORD_REQUIRED");
  return value as Record<string, unknown>;
}
function text(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 1024 ||
    /[\u0000-\u001f\u007f]/u.test(value)
  )
    throw Error("SOURCE_BOUNDED_TEXT_REQUIRED");
  return value;
}
function digest(value: unknown): string {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/u.test(value))
    throw Error("SOURCE_DIGEST_REQUIRED");
  return value;
}
function oid(value: unknown): string {
  if (typeof value !== "string" || !/^[a-f0-9]{40}$/u.test(value))
    throw Error("SOURCE_FULL_GIT_ID_REQUIRED");
  return value;
}
function array(value: unknown, max: number): unknown[] {
  if (!Array.isArray(value) || value.length > max || Object.keys(value).length !== value.length)
    throw Error("SOURCE_BOUNDED_DENSE_ARRAY_REQUIRED");
  return value;
}
function unique<T>(values: T[], key: (value: T) => string): T[] {
  const sorted = values.sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
  if (new Set(sorted.map(key)).size !== sorted.length) throw Error("SOURCE_DUPLICATE_IDENTITY");
  return sorted;
}
function provider(value: unknown): SourceProviderId {
  if (
    typeof value !== "string" ||
    !["codex-context", "codex-native-source", "hermes-context-source"].includes(value)
  )
    throw Error("SOURCE_PROVIDER_REQUIRED");
  return value as SourceProviderId;
}
export function normalizeContent(input: unknown): SourceContentBinding {
  const v = object(input, [
    "schema",
    "repositoryId",
    "commit",
    "tree",
    "files",
    "entries",
    "dependencies",
    "tools",
  ]);
  if (v.schema !== "common.source-content.v1") throw Error("SOURCE_CONTENT_VERSION_REQUIRED");
  let total = 0;
  const files = unique(
    array(v.files, sourceLimits.files).map((item) => {
      const row = object(item, ["path", "sha256", "bytes", "mode", "class"]);
      if (
        !["source", "definition", "dependency", "tool", "asset"].includes(String(row.class)) ||
        !Number.isSafeInteger(row.bytes) ||
        typeof row.mode !== "number" ||
        Number(row.bytes) < 0 ||
        Number(row.bytes) > sourceLimits.fileBytes ||
        ![0o444, 0o555].includes(Number(row.mode))
      )
        throw Error("SOURCE_FILE_CONTRACT_REQUIRED");
      total += Number(row.bytes);
      return {
        path: sourceRelativePath(row.path, row.class === "dependency"),
        sha256: digest(row.sha256),
        bytes: Number(row.bytes),
        mode: row.mode as 0o444 | 0o555,
        class: row.class as SourceFileClass,
      };
    }),
    (row) => row.path,
  );
  if (
    !files.length ||
    total > sourceLimits.totalBytes ||
    files.some((row) => row.path === "source-content.json")
  )
    throw Error("SOURCE_CONTENT_BOUND_OR_RESERVED_PATH");
  for (let i = 0; i < files.length - 1; i++)
    if (files[i + 1]!.path.startsWith(files[i]!.path + "/"))
      throw Error("SOURCE_FILE_DIRECTORY_COLLISION");
  const entries = unique(
    array(v.entries, 64).map((item) => {
      const row = object(item, ["id", "path"]);
      const path = sourceRelativePath(row.path);
      if (
        !files.some(
          (file) => file.path === path && file.class === "source" && /\.[cm]?[jt]s$/u.test(path),
        )
      )
        throw Error("SOURCE_ENTRY_NOT_DECLARED");
      return { id: text(row.id), path };
    }),
    (row) => row.id,
  );
  if (!entries.length) throw Error("SOURCE_ENTRY_REQUIRED");
  const dependencies = unique(
    array(v.dependencies, 128).map((item) => {
      const row = object(item, ["name", "version", "manifest", "root"]);
      const name = text(row.name),
        root = sourceRelativePath(row.root, true),
        manifest = sourceRelativePath(row.manifest, true);
      if (
        !/^(@[a-z0-9-]+\/)?[a-z0-9._-]+$/u.test(name) ||
        root !== "node_modules/" + name ||
        manifest !== root + "/package.json" ||
        !files.some((file) => file.path === manifest && file.class === "dependency")
      )
        throw Error("SOURCE_DEPENDENCY_CONTRACT_REQUIRED");
      return { name, version: text(row.version), manifest, root };
    }),
    (row) => row.name,
  );
  const tools = unique(
    array(v.tools, 32).map((item) => {
      const row = object(item, ["id", "version", "path"]),
        path = sourceRelativePath(row.path);
      if (
        !path.startsWith("tools/") ||
        !files.some((file) => file.path === path && file.class === "tool")
      )
        throw Error("SOURCE_TOOL_NOT_DECLARED");
      return { id: text(row.id), version: text(row.version), path };
    }),
    (row) => row.id,
  );
  if (
    new Set(tools.map((tool) => tool.path)).size !== tools.length ||
    files.some(
      (file) =>
        file.class === "dependency" &&
        dependencies.filter((dep) => file.path.startsWith(dep.root + "/")).length !== 1,
    ) ||
    files.some((file) => file.class === "tool" && !tools.some((tool) => tool.path === file.path))
  )
    throw Error("SOURCE_ORPHAN_EXTERNAL_CLASSIFICATION");
  if (!tools.length) throw Error("SOURCE_LOADED_TOOL_REQUIRED");
  return {
    schema: "common.source-content.v1",
    repositoryId: text(v.repositoryId),
    commit: oid(v.commit),
    tree: oid(v.tree),
    files,
    entries,
    dependencies,
    tools,
  };
}
/** The native installed-source owner must independently join this exact package
 * or tool inventory to its actual installation/provenance, not echo JSON. */
export function sourceInstalledSelection(
  content: SourceContentBinding,
  pin: SourceFilePin,
): SourceInstalledSelection {
  const dep = content.dependencies.find(
    (row) => pin.class === "dependency" && pin.path.startsWith(row.root + "/"),
  );
  const tool = content.tools.find((row) => pin.class === "tool" && row.path === pin.path);
  if (!dep && !tool) throw Error("SOURCE_EXTERNAL_OWNER_REQUIRED");
  const root = dep?.root ?? tool!.path.slice(0, tool!.path.lastIndexOf("/"));
  return {
    kind: dep ? "dependency" : "tool",
    id: dep?.name ?? tool!.id,
    version: dep?.version ?? tool!.version,
    root,
    inventoryDigest: sourceInstalledInventoryDigest(
      content.files.filter((file) =>
        dep
          ? file.class === "dependency" && file.path.startsWith(root + "/")
          : file.path === tool!.path,
      ),
    ),
    path: pin.path,
  };
}
/** Fixed inventory tuple wire order, independently reproducible from an actual
 * installation. Object property order and request labels confer no provenance. */
export function sourceInstalledInventoryDigest(files: readonly SourceFilePin[]): string {
  return sourceHash(
    JSON.stringify(
      [...files]
        .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
        .map((pin) => [pin.path, pin.sha256, pin.bytes, pin.mode, pin.class]),
    ),
  );
}
/** A single ESM package route avoids import/require/browser/conditional resolver
 * disagreement. More general installed packages need a finite owner bundle. */
export function verifySourceDependencyManifest(
  bytes: Uint8Array,
  dep: SourceContentBinding["dependencies"][number],
): void {
  const manifest: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest))
    throw Error("SOURCE_DEPENDENCY_IDENTITY_MISMATCH");
  const row = manifest as Record<string, unknown>;
  if (row.name !== dep.name || row.version !== dep.version)
    throw Error("SOURCE_DEPENDENCY_IDENTITY_MISMATCH");
  if (
    row.type !== "module" ||
    (row.exports !== undefined && typeof row.exports !== "string") ||
    (row.main !== undefined && typeof row.main !== "string") ||
    ["imports", "browser", "module"].some((key) => Object.hasOwn(row, key))
  )
    throw Error("SOURCE_RESTRICTED_PACKAGE_LOADER_REQUIRED");
}
export function normalizeLocation(input: unknown): SourceLocationBinding {
  const v = object(input, ["schema", "contentDigest", "root", "kind", "identity", "readOnly"]),
    i = object(v.identity, ["host", "mountNamespace", "uid", "gid", "profile", "role"]);
  if (
    v.schema !== "common.source-location.v1" ||
    v.readOnly !== true ||
    !["immutable-release", "audit", "dependency"].includes(String(v.kind)) ||
    ![i.uid, i.gid].every((n) => Number.isSafeInteger(n) && Number(n) >= 0)
  )
    throw Error("SOURCE_LOCATION_CONTRACT_REQUIRED");
  return {
    schema: "common.source-location.v1",
    contentDigest: digest(v.contentDigest),
    root: sourceAbsolutePath(v.root),
    kind: v.kind as SourceLocationBinding["kind"],
    identity: {
      host: text(i.host),
      mountNamespace: text(i.mountNamespace),
      uid: Number(i.uid),
      gid: Number(i.gid),
      profile: text(i.profile),
      role: text(i.role),
    },
    readOnly: true,
  };
}
export function normalizeProvider(input: unknown): SourceProviderRegistration {
  const v = object(input, [
    "schema",
    "id",
    "contentDigest",
    "locationDigest",
    "generation",
    "entries",
    "authority",
    "nativePermissionGranted",
  ]);
  if (
    v.schema !== "common.source-provider.v1" ||
    v.authority !== "none" ||
    v.nativePermissionGranted !== false
  )
    throw Error("SOURCE_PROVIDER_CONTRACT_REQUIRED");
  return {
    schema: "common.source-provider.v1",
    id: provider(v.id),
    contentDigest: digest(v.contentDigest),
    locationDigest: digest(v.locationDigest),
    generation: text(v.generation),
    entries: unique(
      array(v.entries, 64).map((item) => {
        const row = object(item, ["role", "entry"]);
        return { role: text(row.role), entry: text(row.entry) };
      }),
      (row) => row.role,
    ),
    authority: "none",
    nativePermissionGranted: false,
  };
}
export function normalizeRecovery(input: unknown): OriginalSourceRecovery {
  const v = object(input, [
      "schema",
      "providerId",
      "contentDigest",
      "locationDigest",
      "registrationDigest",
      "originalReceipt",
    ]),
    r = object(v.originalReceipt, ["owner", "uri", "revision", "sha256"]);
  if (v.schema !== "common.original-source-recovery.v1")
    throw Error("SOURCE_RECOVERY_VERSION_REQUIRED");
  return {
    schema: "common.original-source-recovery.v1",
    providerId: provider(v.providerId),
    contentDigest: digest(v.contentDigest),
    locationDigest: digest(v.locationDigest),
    registrationDigest: digest(v.registrationDigest),
    originalReceipt: {
      owner: text(r.owner),
      uri: text(r.uri),
      revision: text(r.revision),
      sha256: digest(r.sha256),
    },
  };
}
// Fixed typed normalization, not another arbitrary JSON codec or schema generator.
export const contentBytes = (value: unknown): string =>
  JSON.stringify(normalizeContent(value)) + "\n";
export const contentDigest = (value: unknown): string => sourceHash(contentBytes(value));
export const locationBytes = (value: unknown): string =>
  JSON.stringify(normalizeLocation(value)) + "\n";
export const locationDigest = (value: unknown): string => sourceHash(locationBytes(value));
export const providerBytes = (value: unknown): string =>
  JSON.stringify(normalizeProvider(value)) + "\n";
export const providerDigest = (value: unknown): string => sourceHash(providerBytes(value));
export const recoveryDigest = (value: unknown): string =>
  sourceHash(JSON.stringify(normalizeRecovery(value)) + "\n");
export function normalizeBinding(input: SourceBindingInput): SourceBindingInput {
  object(
    input,
    input.recovery === undefined
      ? ["content", "location", "registration", "registrationPath", "mode"]
      : ["content", "location", "registration", "registrationPath", "mode", "recovery"],
  );
  const content = normalizeContent(input.content),
    location = normalizeLocation(input.location),
    registration = normalizeProvider(input.registration);
  if (
    !["current", "recovery"].includes(input.mode) ||
    location.contentDigest !== contentDigest(content) ||
    registration.contentDigest !== location.contentDigest ||
    registration.locationDigest !== locationDigest(location) ||
    registration.entries.some((row) => !content.entries.some((entry) => entry.id === row.entry))
  )
    throw Error("SOURCE_CONTENT_LOCATION_JOIN_MISMATCH");
  const recovery = input.recovery === undefined ? undefined : normalizeRecovery(input.recovery);
  if (
    (input.mode === "recovery") !== !!recovery ||
    (recovery &&
      (recovery.providerId !== registration.id ||
        recovery.contentDigest !== registration.contentDigest ||
        recovery.locationDigest !== registration.locationDigest ||
        recovery.registrationDigest !== providerDigest(registration)))
  )
    throw Error("SOURCE_ORIGINAL_RECOVERY_JOIN_MISMATCH");
  return {
    content,
    location,
    registration,
    registrationPath: sourceAbsolutePath(input.registrationPath),
    mode: input.mode,
    ...(recovery ? { recovery } : {}),
  };
}
