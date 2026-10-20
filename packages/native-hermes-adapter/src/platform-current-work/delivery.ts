/** Finite source delivery only. The compiled owner contract is not native permission. */
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdir, open, readlink, realpath, symlink, type FileHandle } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { platformEntryPoints, readExactFile, sourceDigest } from "./provider.ts";

export const namespaceDeliveryLimits = Object.freeze({
  wireBytes: 96 * 1024 * 1024,
  totalBytes: 64 * 1024 * 1024,
  fileBytes: 8 * 1024 * 1024,
  files: 4096,
  aliases: 256,
  directories: 8192,
  journalBytes: 16 * 1024 * 1024,
});
const claimName = ".namespace-delivery-claim.json";
const journalName = ".namespace-delivery-journal.jsonl";
const receiptName = ".namespace-delivery-receipt.json";
const reserved = ".namespace-delivery-";

export interface NamespaceDirectoryIdentity {
  dev: number;
  ino: number;
  uid: number;
  gid: number;
  mode: number;
}
/** Owner supplied/compiled, never selected by transport JSON or runtime arguments. */
export interface NamespaceDeliveryContract {
  schema: "hermes.namespace-delivery-contract.v1";
  operationId: string;
  taskId: string;
  namespace: string;
  uid: number;
  gid: number;
  parent: NamespaceDirectoryIdentity;
  compiler: { path: string; bytes: number; sha256: string };
  legacyManifest: { path: string; bytes: number; sha256: string };
  payloadSha256: string;
  layout: {
    providerDirectory: string;
    bundleDirectory: string;
    dataDirectory: string;
    registrationFile: string;
  };
  /** Finite artifact budget; receipt/claim writes are independently bounded. */
  maxArtifactWrites: number;
}
export interface NamespaceDeliveryFile {
  path: string;
  bytes: number;
  sha256: string;
  mode: 0o600 | 0o644;
  base64: string;
}
export interface NamespaceDeliveryPayload {
  schema: "hermes.namespace-delivery-payload.v1";
  provenance: {
    sourceCommit: string;
    sourceRegistrationSha256: string;
    runtimeRegistrationSha256: string;
    compilerSha256: string;
    bundles: { name: string; bytes: number; sha256: string }[];
  };
  files: NamespaceDeliveryFile[];
  /** Both paths are namespace-relative. Only a declared dependency root may be the target. */
  aliases: { path: string; target: string }[];
}
export interface NamespaceDeliveryReceipt {
  schema: "hermes.namespace-delivery-receipt.v1";
  operationId: string | null;
  taskId: string | null;
  payloadSha256: string | null;
  status: "refused" | "partial" | "delivered-awaiting-owner-verification";
  reason: string | null;
  namespaceCreated: boolean;
  claimDurable: boolean;
  completed: number;
  uncertain: string | null;
  legacyManifestUnchanged: boolean | null;
  receipt: { path: string; sha256: string; bytes: number } | null;
  journal: { path: string; sha256: string; bytes: number } | null;
  persistenceFailure: boolean;
  runtimeClosureVerification: "required-separate-owner-step";
  nativeTaskId: null;
  nativeRunId: null;
  nativeEventId: null;
  nativeAttemptId: null;
  nativePermissionGranted: false;
  executionAuthorityGranted: false;
  nativeActivation: false;
  nativeUseEstablished: false;
  retryAllowed: false;
}
type Row = Record<string, unknown>;
function fail(code: string): never {
  throw Error(code);
}
function row(value: unknown): Row {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("DELIVERY_OBJECT_REQUIRED");
  return value as Row;
}
function closed(value: unknown, fields: string[]): Row {
  const result = row(value);
  if (Object.keys(result).sort().join("|") !== fields.sort().join("|"))
    fail("CLOSED_DELIVERY_FIELDS_REQUIRED");
  return result;
}
function text(value: unknown, max = 1024): asserts value is string {
  if (typeof value !== "string" || !value || value.length > max || /[\u0000-\u001f]/u.test(value))
    fail("BOUNDED_DELIVERY_TEXT_REQUIRED");
}
function digest(value: unknown): asserts value is string {
  if (typeof value !== "string" || !/^[a-f0-9]{64}$/u.test(value)) fail("DELIVERY_DIGEST_REQUIRED");
}
function integer(value: unknown, max = Number.MAX_SAFE_INTEGER): asserts value is number {
  if (!Number.isSafeInteger(value) || Number(value) < 0 || Number(value) > max)
    fail("DELIVERY_INTEGER_REQUIRED");
}
function absolute(value: unknown): asserts value is string {
  text(value, 2048);
  if (!isAbsolute(value) || resolve(value) !== value || value === "/")
    fail("EXACT_DELIVERY_PATH_REQUIRED");
}
function local(value: unknown): asserts value is string {
  text(value);
  if (
    isAbsolute(value) ||
    value.includes("\\") ||
    value.split("/").some((part) => !part || part === "." || part === "..") ||
    value.split("/").some((part) => part.startsWith(reserved))
  )
    fail("CONTAINED_DELIVERY_PATH_REQUIRED");
}
function canonical(value: unknown): string {
  if (value === null || typeof value !== "object") {
    const encoded = JSON.stringify(value);
    if (encoded === undefined) fail("JSON_DELIVERY_VALUE_REQUIRED");
    return encoded;
  }
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  const object = value as Row;
  return (
    "{" +
    Object.keys(object)
      .sort()
      .map((key) => JSON.stringify(key) + ":" + canonical(object[key]))
      .join(",") +
    "}"
  );
}
const jsonBytes = (value: unknown) => Buffer.from(JSON.stringify(value) + "\n");
export const namespacePayloadSha256 = (value: NamespaceDeliveryPayload) =>
  sourceDigest(canonical(value));

function validateContract(value: unknown): NamespaceDeliveryContract {
  const c = closed(value, [
    "schema",
    "operationId",
    "taskId",
    "namespace",
    "uid",
    "gid",
    "parent",
    "compiler",
    "legacyManifest",
    "payloadSha256",
    "layout",
    "maxArtifactWrites",
  ]);
  if (c.schema !== "hermes.namespace-delivery-contract.v1") fail("DELIVERY_CONTRACT_REQUIRED");
  text(c.operationId, 256);
  text(c.taskId, 256);
  absolute(c.namespace);
  integer(c.uid);
  integer(c.gid);
  digest(c.payloadSha256);
  integer(c.maxArtifactWrites, namespaceDeliveryLimits.files + namespaceDeliveryLimits.aliases);
  const parent = closed(c.parent, ["dev", "ino", "uid", "gid", "mode"]);
  for (const key of ["dev", "ino", "uid", "gid"]) integer(parent[key]);
  integer(parent.mode, 0o777);
  if (parent.uid !== c.uid || parent.gid !== c.gid || Number(parent.mode) & 0o022)
    fail("PRIVATE_OWNED_DELIVERY_PARENT_REQUIRED");
  for (const key of ["compiler", "legacyManifest"]) {
    const pin = closed(c[key], ["path", "bytes", "sha256"]);
    absolute(pin.path);
    integer(pin.bytes, key === "compiler" ? 128 * 1024 * 1024 : 1048576);
    digest(pin.sha256);
    if (pin.path === c.namespace || pin.path.startsWith(c.namespace + "/"))
      fail("PROTECTED_PIN_INSIDE_NEW_NAMESPACE");
  }
  const layout = closed(c.layout, [
    "providerDirectory",
    "bundleDirectory",
    "dataDirectory",
    "registrationFile",
  ]);
  for (const name of Object.values(layout)) {
    local(name);
    if (name.includes("/")) fail("DIRECT_LAYOUT_CHILD_REQUIRED");
  }
  if (new Set(Object.values(layout)).size !== 4) fail("DISTINCT_DELIVERY_LAYOUT_REQUIRED");
  return structuredClone(c) as unknown as NamespaceDeliveryContract;
}

function validatePayload(value: unknown, c: NamespaceDeliveryContract) {
  const p = closed(value, ["schema", "provenance", "files", "aliases"]);
  if (p.schema !== "hermes.namespace-delivery-payload.v1") fail("DELIVERY_PAYLOAD_REQUIRED");
  const provenance = closed(p.provenance, [
    "sourceCommit",
    "sourceRegistrationSha256",
    "runtimeRegistrationSha256",
    "compilerSha256",
    "bundles",
  ]);
  if (
    typeof provenance.sourceCommit !== "string" ||
    !/^[a-f0-9]{40}$/u.test(provenance.sourceCommit)
  )
    fail("SOURCE_COMMIT_REQUIRED");
  for (const key of ["sourceRegistrationSha256", "runtimeRegistrationSha256", "compilerSha256"])
    digest(provenance[key]);
  if (provenance.compilerSha256 !== c.compiler.sha256) fail("DELIVERY_COMPILER_BINDING_MISMATCH");
  if (
    !Array.isArray(p.files) ||
    p.files.length < 1 ||
    p.files.length > namespaceDeliveryLimits.files ||
    !Array.isArray(p.aliases) ||
    p.aliases.length > namespaceDeliveryLimits.aliases
  )
    fail("FINITE_DELIVERY_MANIFEST_REQUIRED");
  const files = new Map<string, { pin: NamespaceDeliveryFile; bytes: Buffer }>();
  let total = 0;
  for (const value of p.files) {
    const file = closed(value, ["path", "bytes", "sha256", "mode", "base64"]);
    local(file.path);
    integer(file.bytes, namespaceDeliveryLimits.fileBytes);
    digest(file.sha256);
    if (![0o600, 0o644].includes(Number(file.mode)) || files.has(file.path))
      fail("DELIVERY_FILE_METADATA_INVALID");
    if (
      typeof file.base64 !== "string" ||
      file.base64.length > Math.ceil(namespaceDeliveryLimits.fileBytes / 3) * 4
    )
      fail("CANONICAL_DELIVERY_BASE64_REQUIRED");
    const bytes = Buffer.from(file.base64, "base64");
    total += bytes.length;
    if (
      total > namespaceDeliveryLimits.totalBytes ||
      bytes.length !== file.bytes ||
      bytes.toString("base64") !== file.base64 ||
      sourceDigest(bytes) !== file.sha256
    )
      fail("DELIVERY_PAYLOAD_BYTES_MISMATCH");
    files.set(file.path, { pin: { ...file } as unknown as NamespaceDeliveryFile, bytes });
  }
  const registrationFile = files.get(c.layout.registrationFile);
  if (
    !registrationFile ||
    registrationFile.bytes.length > 1048576 ||
    registrationFile.pin.sha256 !== provenance.runtimeRegistrationSha256
  )
    fail("EXACT_RUNTIME_REGISTRATION_REQUIRED");
  const registration = closed(
    JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(registrationFile.bytes)),
    [
      "schema",
      "providerRoot",
      "packageName",
      "entrypoints",
      "sourcePins",
      "dependencies",
      "dependencyPins",
      "authority",
      "nativePermissionGranted",
    ],
  );
  if (
    registration.schema !== "hermes.platform-source-provider.v2" ||
    registration.providerRoot !== join(c.namespace, c.layout.providerDirectory) ||
    registration.packageName !== "@palantirkc/native-hermes-adapter" ||
    registration.authority !== "none" ||
    registration.nativePermissionGranted !== false ||
    canonical(registration.entrypoints) !== canonical(platformEntryPoints)
  )
    fail("RUNTIME_REGISTRATION_BINDING_MISMATCH");
  const expected = new Set([c.layout.registrationFile]);
  for (const key of ["sourcePins", "dependencyPins"]) {
    const pins = registration[key];
    if (!Array.isArray(pins) || pins.length < 1 || pins.length > 2048)
      fail("FINITE_REGISTRATION_PINS_REQUIRED");
    const seen = new Set<string>();
    for (const value of pins) {
      const pin = closed(value, ["path", "bytes", "sha256"]);
      local(pin.path);
      integer(pin.bytes, 1048576);
      digest(pin.sha256);
      if (seen.has(pin.path)) fail("DUPLICATE_REGISTRATION_PIN");
      seen.add(pin.path);
      const path = c.layout.providerDirectory + "/" + pin.path,
        file = files.get(path);
      if (!file || file.pin.bytes !== pin.bytes || file.pin.sha256 !== pin.sha256)
        fail("REGISTERED_SOURCE_BYTES_REQUIRED");
      expected.add(path);
    }
  }
  if (
    !Array.isArray(provenance.bundles) ||
    provenance.bundles.length !== Object.keys(platformEntryPoints).length
  )
    fail("EXACT_SIX_BUILD_BUNDLES_REQUIRED");
  const names = new Set<string>();
  for (const value of provenance.bundles) {
    const pin = closed(value, ["name", "bytes", "sha256"]);
    text(pin.name);
    integer(pin.bytes, namespaceDeliveryLimits.fileBytes);
    digest(pin.sha256);
    if (!Object.hasOwn(platformEntryPoints, pin.name) || names.has(pin.name))
      fail("EXACT_SIX_BUILD_BUNDLES_REQUIRED");
    names.add(pin.name);
    const path = c.layout.bundleDirectory + "/" + pin.name,
      file = files.get(path);
    if (!file || file.pin.sha256 !== pin.sha256 || file.pin.bytes !== pin.bytes)
      fail("BUILD_BUNDLE_BINDING_MISMATCH");
    expected.add(path);
  }
  for (const path of files.keys())
    if (!expected.has(path) && !path.startsWith(c.layout.dataDirectory + "/"))
      fail("UNDECLARED_DELIVERY_FILE");
  const directories = new Set<string>();
  const addParents = (path: string) => {
    let parent = dirname(path);
    while (parent !== ".") {
      directories.add(parent);
      parent = dirname(parent);
    }
  };
  for (const path of files.keys()) addParents(path);
  const dependencies = registration.dependencies;
  if (!Array.isArray(dependencies) || !dependencies.length || dependencies.length > 32)
    fail("FINITE_DEPENDENCY_ROOTS_REQUIRED");
  const dependencyRoots = new Set<string>();
  for (const value of dependencies) {
    const dep = closed(value, ["name", "version", "root"]);
    text(dep.name, 256);
    text(dep.version, 256);
    local(dep.root);
    if (
      !dep.root.startsWith("node_modules/") ||
      !directories.has(c.layout.providerDirectory + "/" + dep.root)
    )
      fail("REGISTERED_DEPENDENCY_ROOT_REQUIRED");
    dependencyRoots.add(c.layout.providerDirectory + "/" + dep.root);
  }
  const aliases: { path: string; target: string }[] = [],
    aliasNames = new Set<string>();
  for (const value of p.aliases) {
    const alias = closed(value, ["path", "target"]);
    local(alias.path);
    local(alias.target);
    if (
      !alias.path.startsWith(c.layout.providerDirectory + "/") ||
      !alias.path.includes("/node_modules/") ||
      !dependencyRoots.has(alias.target) ||
      aliasNames.has(alias.path) ||
      files.has(alias.path)
    )
      fail("FINITE_RESOLVER_ALIAS_REQUIRED");
    aliasNames.add(alias.path);
    aliases.push({ path: alias.path, target: alias.target });
    addParents(alias.path);
  }
  for (const path of [...files.keys(), ...aliasNames]) {
    if (directories.has(path)) fail("DELIVERY_PATH_PREFIX_CONFLICT");
    for (const alias of aliasNames)
      if (path.startsWith(alias + "/")) fail("DELIVERY_ALIAS_PARENT_FORBIDDEN");
  }
  if (directories.size > namespaceDeliveryLimits.directories) fail("DELIVERY_DIRECTORY_LIMIT");
  if (namespacePayloadSha256(p as unknown as NamespaceDeliveryPayload) !== c.payloadSha256)
    fail("REVIEWED_DELIVERY_PAYLOAD_CHANGED");
  return {
    payload: p as unknown as NamespaceDeliveryPayload,
    files,
    aliases,
    directories: [...directories].sort(
      (a, b) => a.split("/").length - b.split("/").length || a.localeCompare(b),
    ),
  };
}

async function directory(path: string): Promise<FileHandle> {
  absolute(path);
  let held = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    for (const part of path.split("/").slice(1)) {
      const next = await open(
        "/proc/self/fd/" + held.fd + "/" + part,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await held.close();
      held = next;
    }
    return held;
  } catch (error) {
    await held.close();
    throw error;
  }
}
async function identity(held: FileHandle): Promise<NamespaceDirectoryIdentity> {
  const s = await held.stat();
  return { dev: s.dev, ino: s.ino, uid: s.uid, gid: s.gid, mode: s.mode & 0o7777 };
}
async function same(path: string, held: FileHandle) {
  const named = await directory(path);
  try {
    if (canonical(await identity(named)) !== canonical(await identity(held)))
      fail("DELIVERY_DIRECTORY_CHANGED");
  } finally {
    await named.close();
  }
}
const childPath = (held: FileHandle, name: string) => {
  if (basename(name) !== name || name === "." || name === "..")
    fail("DIRECT_DESCRIPTOR_CHILD_REQUIRED");
  return "/proc/self/fd/" + held.fd + "/" + name;
};
async function readHeldBytes(held: FileHandle, bytes: number, offset = 0) {
  const readback = Buffer.alloc(bytes + 1);
  let count = 0;
  while (count < readback.length) {
    const next = await held.read(readback, count, readback.length - count, offset + count);
    if (!next.bytesRead) break;
    count += next.bytesRead;
  }
  if (count !== bytes) fail("DELIVERY_WRITE_READBACK_FAILED");
  return readback.subarray(0, count);
}
async function exclusiveFile(parent: FileHandle, name: string, bytes: Uint8Array, mode = 0o600) {
  const path = childPath(parent, name),
    h = await open(
      path,
      constants.O_RDWR | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
      mode,
    );
  try {
    await h.writeFile(bytes);
    await h.chmod(mode);
    await h.sync();
    await parent.sync();
    const before = await h.stat(),
      readback = await readHeldBytes(h, bytes.length);
    const [held, named] = await Promise.all([h.stat(), lstat(path)]);
    if (
      !named.isFile() ||
      sourceDigest(readback) !== sourceDigest(bytes) ||
      held.dev !== named.dev ||
      held.ino !== named.ino ||
      before.dev !== held.dev ||
      before.ino !== held.ino ||
      before.mtimeMs !== held.mtimeMs ||
      before.ctimeMs !== held.ctimeMs ||
      held.nlink !== 1 ||
      (held.mode & 0o777) !== mode ||
      held.size !== bytes.length
    )
      fail("DELIVERY_WRITE_READBACK_FAILED");
  } finally {
    await h.close();
  }
}

/** All mutations are confined to a newly claimed namespace. Existing targets are reconciliation only. */
export async function deliverHermesNamespace(
  contractValue: unknown,
  payloadValue: unknown,
): Promise<NamespaceDeliveryReceipt> {
  const receipt: NamespaceDeliveryReceipt = {
    schema: "hermes.namespace-delivery-receipt.v1",
    operationId: null,
    taskId: null,
    payloadSha256: null,
    status: "refused",
    reason: null,
    namespaceCreated: false,
    claimDurable: false,
    completed: 0,
    uncertain: null,
    legacyManifestUnchanged: null,
    receipt: null,
    journal: null,
    persistenceFailure: false,
    runtimeClosureVerification: "required-separate-owner-step",
    nativeTaskId: null,
    nativeRunId: null,
    nativeEventId: null,
    nativeAttemptId: null,
    nativePermissionGranted: false,
    executionAuthorityGranted: false,
    nativeActivation: false,
    nativeUseEstablished: false,
    retryAllowed: false,
  };
  let c: NamespaceDeliveryContract | undefined,
    parent: FileHandle | undefined,
    root: FileHandle | undefined,
    journal: FileHandle | undefined;
  const journalDigest = createHash("sha256");
  let journalBytes = 0;
  const append = async (record: unknown) => {
    if (!journal || !root) fail("DELIVERY_JOURNAL_UNAVAILABLE");
    const bytes = jsonBytes(record);
    const nextBytes = journalBytes + bytes.length;
    if (nextBytes > namespaceDeliveryLimits.journalBytes) fail("DELIVERY_JOURNAL_LIMIT");
    await journal.writeFile(bytes);
    await journal.sync();
    const actual = await readHeldBytes(journal, bytes.length, journalBytes),
      held = await journal.stat(),
      named = await lstat(childPath(root, journalName));
    if (
      sourceDigest(actual) !== sourceDigest(bytes) ||
      !named.isFile() ||
      held.dev !== named.dev ||
      held.ino !== named.ino ||
      held.nlink !== 1 ||
      held.size !== nextBytes ||
      (held.mode & 0o777) !== 0o600
    )
      fail("DELIVERY_JOURNAL_READBACK_FAILED");
    journalBytes = nextBytes;
    journalDigest.update(bytes);
  };
  try {
    c = validateContract(contractValue);
    const { files, aliases, directories } = validatePayload(payloadValue, c);
    receipt.operationId = c.operationId;
    receipt.taskId = c.taskId;
    receipt.payloadSha256 = c.payloadSha256;
    if (process.getuid?.() !== c.uid || process.getgid?.() !== c.gid)
      fail("EXACT_DELIVERY_UID_GID_REQUIRED");
    const compiler = await readExactFile(c.compiler.path, 128 * 1024 * 1024, c.compiler.sha256);
    if (
      compiler.length !== c.compiler.bytes ||
      (await realpath(process.execPath)) !== c.compiler.path
    )
      fail("EXACT_DELIVERY_COMPILER_REQUIRED");
    const verifyLegacy = async () => {
      try {
        const legacy = await readExactFile(
          c!.legacyManifest.path,
          1048576,
          c!.legacyManifest.sha256,
        );
        if (legacy.length !== c!.legacyManifest.bytes) fail("LEGACY_MANIFEST_BYTES_CHANGED");
        receipt.legacyManifestUnchanged = true;
      } catch (error) {
        receipt.legacyManifestUnchanged = false;
        throw error;
      }
    };
    await verifyLegacy();
    parent = await directory(dirname(c.namespace));
    if (canonical(await identity(parent)) !== canonical(c.parent))
      fail("DELIVERY_PARENT_FINGERPRINT_CHANGED");
    await same(dirname(c.namespace), parent);
    try {
      await lstat(childPath(parent, basename(c.namespace)));
      fail("EXISTING_NAMESPACE_REQUIRES_RECONCILIATION");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    // Exclusive mkdir is the first once fence. Never remove/reclaim it on failure.
    await mkdir(childPath(parent, basename(c.namespace)), { mode: 0o700 });
    receipt.namespaceCreated = true;
    await parent.sync();
    await same(dirname(c.namespace), parent);
    root = await directory(c.namespace);
    await root.sync();
    await exclusiveFile(
      root,
      claimName,
      jsonBytes({
        schema: "hermes.namespace-delivery-claim.v1",
        operationId: c.operationId,
        taskId: c.taskId,
        contractSha256: sourceDigest(canonical(c)),
        payloadSha256: c.payloadSha256,
        nativeTaskId: null,
        nativeRunId: null,
        nativeEventId: null,
        nativeAttemptId: null,
        retryAllowed: false,
      }),
    );
    receipt.claimDurable = true;
    journal = await open(
      childPath(root, journalName),
      constants.O_RDWR |
        constants.O_CREAT |
        constants.O_EXCL |
        constants.O_NOFOLLOW |
        constants.O_APPEND,
      0o600,
    );
    await journal.sync();
    await root.sync();
    const recheck = async () => {
      await same(dirname(c!.namespace), parent!);
      if (canonical(await identity(parent!)) !== canonical(c!.parent))
        fail("DELIVERY_PARENT_FINGERPRINT_CHANGED");
      await same(c!.namespace, root!);
      await verifyLegacy();
    };
    const perform = async (
      path: string,
      kind: string,
      action: (h: FileHandle) => Promise<void>,
    ) => {
      await recheck();
      receipt.uncertain = path;
      await append({ state: "intent", kind, path });
      const destination = join(c!.namespace, path),
        held = await directory(dirname(destination));
      try {
        await same(dirname(destination), held);
        await action(held);
        await same(dirname(destination), held);
        await recheck();
      } finally {
        await held.close();
      }
      await append({ state: "complete", kind, path });
      receipt.uncertain = null;
    };
    for (const path of directories)
      await perform(path, "directory", async (h) => {
        await mkdir(childPath(h, basename(path)), { mode: 0o700 });
        await h.sync();
      });
    for (const [path, file] of files) {
      if (receipt.completed >= c.maxArtifactWrites) fail("DELIVERY_ARTIFACT_WRITE_CEILING");
      await perform(path, "file", async (h) => {
        await exclusiveFile(h, basename(path), file.bytes, file.pin.mode);
        await readExactFile(
          join(c!.namespace, path),
          namespaceDeliveryLimits.fileBytes,
          file.pin.sha256,
        );
      });
      receipt.completed++;
    }
    for (const alias of aliases) {
      if (receipt.completed >= c.maxArtifactWrites) fail("DELIVERY_ARTIFACT_WRITE_CEILING");
      await perform(alias.path, "alias", async (h) => {
        const text = relative(
          dirname(join(c!.namespace, alias.path)),
          join(c!.namespace, alias.target),
        );
        await symlink(text, childPath(h, basename(alias.path)));
        await h.sync();
        if (
          (await readlink(join(c!.namespace, alias.path))) !== text ||
          (await realpath(join(c!.namespace, alias.path))) !== join(c!.namespace, alias.target)
        )
          fail("DELIVERY_ALIAS_READBACK_FAILED");
      });
      receipt.completed++;
    }
    await recheck();
    for (const [path, file] of files)
      await readExactFile(
        join(c.namespace, path),
        namespaceDeliveryLimits.fileBytes,
        file.pin.sha256,
      );
    receipt.status = "delivered-awaiting-owner-verification";
  } catch (error) {
    receipt.status = receipt.namespaceCreated ? "partial" : "refused";
    receipt.reason =
      error instanceof Error && /^[A-Z][A-Z0-9_]+$/u.test(error.message)
        ? error.message
        : "DELIVERY_IO_OR_INPUT_REFUSAL";
  } finally {
    if (root && c) {
      if (journal) {
        try {
          await append({
            state: "terminal",
            status: receipt.status,
            completed: receipt.completed,
            uncertain: receipt.uncertain,
            reason: receipt.reason,
          });
          const expectedHash = journalDigest.digest("hex"),
            actual = await readHeldBytes(journal, journalBytes);
          if (sourceDigest(actual) !== expectedHash) fail("DELIVERY_JOURNAL_READBACK_FAILED");
          receipt.journal = {
            path: join(c.namespace, journalName),
            bytes: journalBytes,
            sha256: expectedHash,
          };
        } catch {
          receipt.persistenceFailure = true;
          receipt.status = "partial";
          receipt.reason ??= "DELIVERY_JOURNAL_PERSISTENCE_FAILED";
        }
      }
      try {
        // Persist partial/unknown progress even if an artifact or post-check failed. No rollback.
        const bytes = jsonBytes(receipt);
        await exclusiveFile(root, receiptName, bytes);
        await same(c.namespace, root);
        receipt.receipt = {
          path: join(c.namespace, receiptName),
          bytes: bytes.length,
          sha256: sourceDigest(bytes),
        };
      } catch {
        receipt.persistenceFailure = true;
        receipt.status = "partial";
        receipt.reason ??= "DELIVERY_RECEIPT_PERSISTENCE_FAILED";
      }
    } else if (receipt.namespaceCreated) receipt.persistenceFailure = true;
    for (const h of [journal, root, parent]) {
      try {
        await h?.close();
      } catch {
        receipt.persistenceFailure = true;
        receipt.status = receipt.namespaceCreated ? "partial" : "refused";
        receipt.reason ??= "DELIVERY_DESCRIPTOR_CLOSE_FAILED";
      }
    }
  }
  return receipt;
}
