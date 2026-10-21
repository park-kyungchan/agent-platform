import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assertGitPath, gitReadEnvironment } from "./source.ts";

export interface GitAllocationTarget {
  repositoryRoot: string;
  ownerRef: string;
  expectedHead: string;
  expectedAuthorityRevision: number;
}
/** Data attested ONLY inside the current native owner's authority lease.
 * A request JSON field is never a policy issuer. Native Codex /hooks is unrelated.
 */
export interface NativeSourceGitHookPolicy {
  mode: "native-qualified-no-git-hooks";
  repositoryRoot: string;
  ownerRef: string;
  head: string;
  ownerId: string;
  authorityRevision: number;
  qualificationRef: string;
  expiresAtUtc: string;
  requiredNativeControlsSatisfied: true;
}
export const oid = (id: string): boolean => /^[a-f0-9]{40}$/.test(id);
export const sourceBlobOid = (bytes: string): string => createHash("sha1").update(`blob ${Buffer.byteLength(bytes)}\0`).update(bytes).digest("hex");
export const sourceSha256 = (bytes: string): string => createHash("sha256").update(bytes).digest("hex");

/** Read-only commands retain repository configuration and execute no ref hooks. */
export async function gitOwnerRead(root: string, args: string[]): Promise<string> {
  if (!["rev-parse", "ls-tree", "cat-file", "show", "log", "merge-base", "worktree"].includes(args[0] ?? "")) throw Error("V3_READ_ONLY_GIT_COMMAND_REQUIRED");
  return run(root, args);
}
async function run(root: string, args: string[], input?: string, index?: string, noHooks = false): Promise<string> {
  const env = gitReadEnvironment();
  if (index) env.GIT_INDEX_FILE = index;
  return new Promise<string>((resolve, reject) => {
    const child = execFile("git", ["--no-replace-objects", "--no-pager", "--literal-pathspecs", ...(noHooks ? ["-c", "core.hooksPath=/dev/null"] : []), "-C", root, ...args], { env, encoding: "utf8", timeout: 10_000, maxBuffer: 4_194_304 }, (error, stdout) => error ? reject(Error("V3_GIT_OPERATION_FAILED:" + args[0])) : resolve(stdout));
    child.stdin!.on("error", () => reject(Error("V3_GIT_INPUT_FAILED")));
    child.stdin!.end(input);
  });
}
export async function readOptionalGitRecord(root: string, head: string, path: string): Promise<string | null> {
  assertGitPath(path);
  const row = await gitOwnerRead(root, ["ls-tree", "-z", head, "--", path]);
  if (!row) return null;
  if (!row.startsWith("100644 blob ")) throw Error("V3_REGULAR_GIT_RECORD_REQUIRED");
  return gitOwnerRead(root, ["show", head + ":" + path]);
}
export async function checkedGitOwnerTarget(target: GitAllocationTarget): Promise<string> {
  if (!oid(target.expectedHead) || !/^refs\/heads\/work\/[A-Za-z0-9][A-Za-z0-9._/-]{0,240}$/.test(target.ownerRef) || target.ownerRef.includes("..") || !Number.isSafeInteger(target.expectedAuthorityRevision) || target.expectedAuthorityRevision < 1) throw Error("V3_EXACT_OWNER_TARGET_REQUIRED");
  if (await realpath(target.repositoryRoot) !== target.repositoryRoot) throw Error("V3_REAL_REPOSITORY_ROOT_REQUIRED");
  const head = (await gitOwnerRead(target.repositoryRoot, ["rev-parse", "--verify", target.ownerRef])).trim();
  if (!oid(head)) throw Error("V3_OWNER_HEAD_REQUIRED");
  if ((await gitOwnerRead(target.repositoryRoot, ["worktree", "list", "--porcelain"])).split("\n").includes("branch " + target.ownerRef)) throw Error("V3_OWNER_REF_CHECKED_OUT");
  return head;
}
const qualified = new WeakSet<object>();
export interface QualifiedSourceGitTransaction { readonly policy: NativeSourceGitHookPolicy }
export function qualifySourceGitTransaction(target: GitAllocationTarget, head: string, ownerId: string, policy: NativeSourceGitHookPolicy | undefined): QualifiedSourceGitTransaction {
  if (!policy || policy.mode !== "native-qualified-no-git-hooks" || policy.repositoryRoot !== target.repositoryRoot || policy.ownerRef !== target.ownerRef || policy.head !== head || policy.ownerId !== ownerId || policy.authorityRevision !== target.expectedAuthorityRevision || policy.requiredNativeControlsSatisfied !== true || !policy.qualificationRef?.trim() || !Number.isFinite(Date.parse(policy.expiresAtUtc)) || Date.parse(policy.expiresAtUtc) <= Date.now()) throw Error("V3_NATIVE_QUALIFIED_GIT_HOOK_POLICY_REQUIRED");
  const result = Object.freeze({ policy: Object.freeze(structuredClone(policy)) }); qualified.add(result); return result;
}
/** Single implementation for append-only allocation and explicit migration CAS.
 * No retry. Caller reconciles exact receipts once after an ambiguous CAS result.
 */
export async function publishGitOwnerFiles(target: GitAllocationTarget, head: string, files: Map<string, string>, message: string, transaction: QualifiedSourceGitTransaction): Promise<{ commit: string; cas: "applied" | "observe" }> {
  if (!qualified.has(transaction) || transaction.policy.head !== head || transaction.policy.repositoryRoot !== target.repositoryRoot || transaction.policy.ownerRef !== target.ownerRef || Date.parse(transaction.policy.expiresAtUtc) <= Date.now()) throw Error("V3_QUALIFIED_SOURCE_TRANSACTION_REQUIRED");
  if (!files.size || files.size > 2048) throw Error("V3_SOURCE_TRANSACTION_BOUND");
  for (const [path, bytes] of files) {
    assertGitPath(path);
    if (!path.startsWith("work/") || Buffer.byteLength(bytes) > 4_194_304 || await readOptionalGitRecord(target.repositoryRoot, head, path) !== null) throw Error("V3_APPEND_ONLY_SOURCE_TRANSACTION_REQUIRED");
  }
  const temporary = await mkdtemp(join(tmpdir(), "git-work-owner-index-"));
  const index = join(temporary, "index");
  try {
    await run(target.repositoryRoot, ["read-tree", head], undefined, index, true);
    for (const [path, bytes] of files) {
      const blob = (await run(target.repositoryRoot, ["hash-object", "-w", "--stdin"], bytes, undefined, true)).trim();
      if (blob !== sourceBlobOid(bytes)) throw Error("V3_CREATION_BLOB_RECEIPT_MISMATCH");
      await run(target.repositoryRoot, ["update-index", "--add", "--cacheinfo", "100644," + blob + "," + path], undefined, index, true);
    }
    const tree = (await run(target.repositoryRoot, ["write-tree"], undefined, index, true)).trim();
    const commit = (await run(target.repositoryRoot, ["commit-tree", tree, "-p", head], message + "\n", undefined, true)).trim();
    if (Date.parse(transaction.policy.expiresAtUtc) <= Date.now()) throw Error("V3_GIT_HOOK_QUALIFICATION_EXPIRED_BEFORE_CAS");
    try { await run(target.repositoryRoot, ["update-ref", target.ownerRef, commit, head], undefined, undefined, true); }
    catch { return { commit, cas: "observe" }; }
    return { commit, cas: "applied" };
  } finally { await rm(temporary, { recursive: true, force: true }); }
}
/** Mapping/index receipts are append-only; source records may evolve separately. */
export async function immutableGitLedgerCreation(root: string, head: string, path: string, expectedBytes: string): Promise<string> {
  const commits = (await gitOwnerRead(root, ["log", "--first-parent", "--format=%H", "-n", "2", head, "--", path])).trim().split("\n").filter(Boolean);
  if (commits.length !== 1 || await readOptionalGitRecord(root, commits[0]!, path) !== expectedBytes) throw Error("V3_IMMUTABLE_IDENTITY_LEDGER_REWRITTEN");
  return commits[0]!;
}
