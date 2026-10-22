/** Native bootstrap: stdlib only until the complete installed release has been verified. */
import { createHash } from "node:crypto";
import {
  constants,
  openSync,
  readFileSync,
  realpathSync,
  fstatSync,
  closeSync,
  lstatSync,
  readdirSync,
  readlinkSync,
  writeSync,
  fsyncSync,
} from "node:fs";
import { hostname } from "node:os";
import { join, resolve, relative } from "node:path";
import type { NativeContext, NativeIntent, NativeProfile } from "./native-types";

const nativeContexts = new WeakSet<object>();
const home = "/var/lib/agent-platform/trusted-ci";
const profilePath = "/etc/agent-platform/ci/trusted-release.json";
const hash = (b: Uint8Array | string) => createHash("sha256").update(b).digest("hex");
function requireValue(v: unknown, code: string): asserts v {
  if (!v) throw Error("TRUSTED_CI_" + code);
}
function ownerFile(path: string, mode: number, maximum: number): Buffer {
  requireValue(realpathSync(path) === path, "EXACT_OWNER_FILE_REQUIRED");
  let parent = resolve(path, "..");
  while (parent !== "/") {
    const d = lstatSync(parent);
    requireValue(d.isDirectory() && d.uid === 0 && !(d.mode & 0o022), "OWNER_PARENT_REQUIRED");
    parent = resolve(parent, "..");
  }
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const a = fstatSync(fd);
    requireValue(
      a.isFile() &&
        a.uid === 0 &&
        (a.mode & 0o777) === mode &&
        a.nlink === 1 &&
        a.size > 0 &&
        a.size <= maximum,
      "ROOT_PRIVATE_FILE_REQUIRED",
    );
    const b = readFileSync(fd),
      z = fstatSync(fd);
    requireValue(
      a.size === b.length &&
        a.dev === z.dev &&
        a.ino === z.ino &&
        a.size === z.size &&
        a.mtimeMs === z.mtimeMs &&
        a.ctimeMs === z.ctimeMs,
      "OWNER_FILE_CHANGED",
    );
    return b;
  } finally {
    closeSync(fd);
  }
}
function ownerDirectory(path: string): void {
  const d = lstatSync(path);
  requireValue(
    realpathSync(path) === path && d.isDirectory() && d.uid === 0 && (d.mode & 0o777) === 0o700,
    "ROOT_PRIVATE_OPERATION_DIRECTORY_REQUIRED",
  );
}
function principal(): void {
  requireValue(
    process.getuid?.() === 0 &&
      hostname() === "srv1873899" &&
      /^[a-f0-9-]{36}$/.test(process.env.CODEX_THREAD_ID ?? ""),
    "ACTUAL_NAMED_NATIVE_ROOT_REQUIRED",
  );
  requireValue(
    !process.env.NODE_OPTIONS &&
      !process.env.BUN_OPTIONS &&
      !process.env.BUN_CONFIG &&
      process.env.NODE_TLS_REJECT_UNAUTHORIZED !== "0" &&
      process.env.GITHUB_ACTIONS !== "true",
    "NATIVE_RUNTIME_INJECTION_REFUSED",
  );
  requireValue(
    hash(readFileSync(process.execPath)) ===
      "7ce7d6b654eddeec20afddf2396317fba6dc19ab52788a27e894e1806bf8f24b",
    "QUALIFIED_NATIVE_BUN_REQUIRED",
  );
}
function verifyRelease(p: NativeProfile): void {
  requireValue(
    p.schema === "agent-platform.native-trusted-ci-profile.v2" &&
      p.active === true &&
      p.ownerId === "git-source-owner:1413326539" &&
      p.ownerThread === process.env.CODEX_THREAD_ID &&
      Number.isSafeInteger(p.authorityRevision) &&
      p.authorityRevision > 0,
    "ACTIVE_NATIVE_OWNER_PROFILE_REQUIRED",
  );
  const root = resolve(import.meta.dir, "../../..");
  requireValue(
    p.sourceRoot === root &&
      realpathSync(root) === root &&
      root === "/opt/agent-platform/trusted-ci/releases/" + p.sourceHead &&
      /^[a-f0-9]{40}$/.test(p.sourceHead) &&
      /^[a-f0-9]{40}$/.test(p.sourceTree) &&
      p.policy.release === p.sourceHead,
    "EXACT_INSTALLED_RELEASE_REQUIRED",
  );
  const found = new Set<string>();
  let bytes = 0;
  function walk(dir: string): void {
    const d = lstatSync(dir);
    requireValue(
      d.isDirectory() && d.uid === 0 && !(d.mode & 0o222),
      "READONLY_INSTALLED_DIRECTORY_REQUIRED",
    );
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const f = join(dir, e.name),
        name = relative(root, f);
      requireValue(
        ![".git", ".env", ".npmrc"].includes(e.name),
        "PRIVATE_OR_MUTABLE_RELEASE_INPUT_REFUSED",
      );
      if (e.isDirectory()) walk(f);
      else {
        const s = lstatSync(f);
        requireValue(s.uid === 0 && found.size < 100000, "BOUNDED_INSTALLED_SOURCE_REQUIRED");
        let value: string;
        if (e.isSymbolicLink()) {
          requireValue(realpathSync(f).startsWith(root + "/"), "RELEASE_LINK_ESCAPE");
          value = hash("symlink\0" + readlinkSync(f));
        } else {
          requireValue(
            s.isFile() && !(s.mode & 0o222) && s.nlink === 1 && s.size <= 100_000_000,
            "READONLY_LOADED_FILE_REQUIRED",
          );
          bytes += s.size;
          requireValue(bytes <= 2_000_000_000, "INSTALLED_CLOSURE_BUDGET");
          value = hash(readFileSync(f));
        }
        requireValue(p.fileSha256[name] === value, "INSTALLED_SOURCE_DRIFT");
        found.add(name);
      }
    }
  }
  walk(root);
  requireValue(
    found.size === Object.keys(p.fileSha256).length &&
      [
        "ci/quality/trusted/native.ts",
        "ci/quality/trusted/attest.ts",
        "ci/quality/trusted/github-app.ts",
        "ci/source-check.ts",
        "ci/reuse/source-author.ts",
        "ci/quality/receipt-validator.ts",
        "ci/quality/receipt-validation-gate.ts",
      ].every((s) => found.has(s)),
    "FULL_LOADED_CLOSURE_REQUIRED",
  );
  requireValue(
    p.ghPath === "/usr/bin/gh" &&
      realpathSync(p.ghPath) === p.ghPath &&
      lstatSync(p.ghPath).uid === 0 &&
      !(lstatSync(p.ghPath).mode & 0o022) &&
      hash(readFileSync(p.ghPath)) === p.ghSha256,
    "PINNED_NATIVE_PROVIDER_TRANSPORT_REQUIRED",
  );
}
export function assertNativeContext(context: NativeContext): void {
  requireValue(nativeContexts.has(context), "ACTUAL_NATIVE_CONTEXT_REQUIRED");
  context.assertCurrent();
}
export function readOwnerReconciliation(context: NativeContext): Record<string, unknown> {
  assertNativeContext(context);
  const value = JSON.parse(
    ownerFile(home + "/reconciliations/" + context.intent.id + ".json", 0o600, 128000).toString(),
  );
  requireValue(
    value.operationId === context.intent.id &&
      value.requestId === context.intent.requestId &&
      value.ownerThread === context.profile.ownerThread,
    "EXACT_OWNER_RECONCILIATION_BINDING_REQUIRED",
  );
  return value;
}
export function readAppKeyForNative(context: NativeContext): Buffer {
  assertNativeContext(context);
  return ownerFile("/etc/agent-platform/credentials/github-app-5417476.pem", 0o600, 32768);
}
export async function nativeMain(
  id: string,
  mode: "execute" | "reconcile" = "execute",
): Promise<void> {
  principal();
  requireValue(
    /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(id),
    "EXACT_OPERATION_ID_REQUIRED",
  );
  const profileBytes = ownerFile(profilePath, 0o600, 8_000_000),
    profile = JSON.parse(profileBytes.toString()) as NativeProfile;
  verifyRelease(profile);
  for (const directory of [home, home + "/intents", home + "/results"]) ownerDirectory(directory);
  const intentPath = home + "/intents/" + id + ".json",
    intentBytes = ownerFile(intentPath, 0o600, 128000),
    intent = JSON.parse(intentBytes.toString()) as NativeIntent;
  requireValue(
    /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(intent.requestId),
    "EXACT_NATIVE_REQUEST_REQUIRED",
  );
  requireValue(
    intent.schema === "agent-platform.native-trusted-ci-intent.v2" &&
      intent.id === id &&
      intent.ownerThread === profile.ownerThread &&
      intent.authorityRevision === profile.authorityRevision &&
      intent.policyRelease === profile.sourceHead &&
      ["dispatch", "attest"].includes(intent.action),
    "CURRENT_NATIVE_INTENT_REQUIRED",
  );
  const lockPath = home + "/writer.lock",
    journalPath = home + "/results/" + id + ".jsonl";
  if (mode === "reconcile") ownerFile(journalPath, 0o600, 8_000_000);
  const journalFd =
    mode === "execute"
      ? openSync(
          journalPath,
          constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
          0o600,
        )
      : openSync(journalPath, constants.O_WRONLY | constants.O_APPEND | constants.O_NOFOLLOW);
  const journalInfo = fstatSync(journalFd);
  requireValue(
    journalInfo.isFile() &&
      journalInfo.uid === 0 &&
      journalInfo.nlink === 1 &&
      (journalInfo.mode & 0o777) === 0o600,
    "ROOT_PRIVATE_JOURNAL_REQUIRED",
  );
  function record(value: unknown): void {
    writeSync(journalFd, JSON.stringify({ atUtc: new Date().toISOString(), value }) + "\n");
    fsyncSync(journalFd);
  }
  if (mode === "execute") {
    try {
      const fd = openSync(
        lockPath,
        constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
        0o600,
      );
      writeSync(
        fd,
        JSON.stringify({ id, profileSha256: hash(profileBytes), intentSha256: hash(intentBytes) }) +
          "\n",
      );
      fsyncSync(fd);
      closeSync(fd);
    } catch {
      record({ phase: "held", reason: "EXISTING_WRITER_LOCK_RECONCILE_REQUIRED" });
      closeSync(journalFd);
      throw Error("TRUSTED_CI_EXISTING_WRITER_LOCK_RECONCILE_REQUIRED");
    }
  }
  function assertCurrent(): void {
    principal();
    requireValue(
      hash(ownerFile(profilePath, 0o600, 8_000_000)) === hash(profileBytes) &&
        hash(ownerFile(intentPath, 0o600, 128000)) === hash(intentBytes),
      "NATIVE_AUTHORITY_OR_INTENT_CHANGED",
    );
    const lock = JSON.parse(ownerFile(lockPath, 0o600, 4096).toString());
    requireValue(
      lock.id === id &&
        lock.profileSha256 === hash(profileBytes) &&
        lock.intentSha256 === hash(intentBytes),
      "CURRENT_SERIALIZED_WRITER_REQUIRED",
    );
    const lease = JSON.parse(ownerFile(home + "/current-lease.json", 0o600, 128000).toString());
    requireValue(
      lease.schema === "agent-platform.native-trusted-ci-lease.v1" &&
        lease.operationId === id &&
        lease.ownerThread === profile.ownerThread &&
        lease.authorityRevision === profile.authorityRevision &&
        lease.intentSha256 === hash(intentBytes) &&
        Date.parse(lease.expiresAt) > Date.now() &&
        (mode === "reconcile" || Date.parse(intent.expiresAt) > Date.now()),
      "LIVE_NATIVE_TARGET_LEASE_REQUIRED",
    );
  }
  const context: NativeContext = Object.freeze({ profile, intent, assertCurrent, record });
  nativeContexts.add(context);
  process.env.GIT_CONFIG_NOSYSTEM = "1";
  process.env.GIT_CONFIG_GLOBAL = "/dev/null";
  process.env.GIT_NO_REPLACE_OBJECTS = "1";
  try {
    assertCurrent();
    // First non-stdlib imports occur only after every installed dependency/source byte is checked.
    const { validateTrustedPolicy, validateTarget, trustedDigest } = await import("./policy");
    validateTrustedPolicy(profile.policy);
    validateTarget(intent.target);
    const { fileInventory, verifyInstalledPolicyInputs } = await import("./release");
    requireValue(
      trustedDigest(await fileInventory(profile.sourceRoot)) ===
        profile.policy.controllerClosureSha256,
      "INSTALLED_POLICY_SOURCE_CLOSURE_MISMATCH",
    );
    await verifyInstalledPolicyInputs(profile.sourceRoot, profile.policy);
    record({
      phase: "admitted",
      id,
      mode,
      profileSha256: hash(profileBytes),
      intentSha256: hash(intentBytes),
      nativeThread: profile.ownerThread,
      nativeTaskId: null,
      nativeRunId: null,
    });
    if (mode === "reconcile") await (await import("./attest")).reconcileNative(context);
    else if (intent.action === "dispatch") await (await import("./attest")).dispatchNative(context);
    else await (await import("./attest")).attestNative(context);
    record({
      phase: "finished",
      id,
      mode,
      nativePermissionGrantedBySource: false,
      acceptanceDecision: null,
    });
  } catch (error) {
    record({
      phase: "unknown-or-failed",
      id,
      mode,
      retryAllowed: false,
      reconciliationRequired: true,
      originalFailure: {
        name: error instanceof Error ? error.name : "unknown",
        code:
          error instanceof Error && /^TRUSTED_CI_[A-Z0-9_]+$/.test(error.message)
            ? error.message
            : null,
        messageSha256: hash(error instanceof Error ? error.message : String(error)),
      },
    });
    throw Error("TRUSTED_CI_NATIVE_OPERATION_HELD_RECONCILE_EXACT_RECEIPTS");
  } finally {
    nativeContexts.delete(context);
    closeSync(journalFd);
  }
  // Keep the writer lock and original journal. Root explicitly releases only after exact readback.
}
if (import.meta.main) {
  const args = process.argv.slice(2);
  requireValue(
    args.length === 2 && ["execute", "reconcile"].includes(args[0]!),
    "USAGE_NATIVE_EXECUTE_OR_RECONCILE_ID",
  );
  await nativeMain(args[1]!, args[0] as "execute" | "reconcile");
}
