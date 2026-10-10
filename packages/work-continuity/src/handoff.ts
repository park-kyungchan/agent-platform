import { createHash, randomUUID } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import { link, lstat, mkdir, open, readFile, realpath, rename, rmdir, unlink, type FileHandle } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';

/** Explicit source transport and source-owned Issue CAS. This does not own native sessions or dispatch. */
export interface HandoffFile { path: string; sha256: string; byteLength: number; base64: string }
export interface HandoffBundle {
  schema: 'workspace.source-transfer.v1'; issueId: string; sourceRevision: string;
  files: HandoffFile[]; completedEffectIds: string[];
  checks: Array<{ id: string; status: 'passed' | 'failed' | 'unexecuted'; evidencePaths: string[] }>;
  nextAction: { id: string; action: string; path: string };
}
export interface WriterToken { writerId: string; epoch: number; revision: number }
export interface CanonicalHandoff extends WriterToken {
  schema: 'workspace.canonical-handoff.v1'; issueId: string; bundle: HandoffBundle;
  completedWriteIds: string[];
}
const hash = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex');
const fail = (message: string): never => { throw new Error(message); };
const identity = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length <= 1024 && !/[\u0000-\u001f]/.test(value);
const maxTransferBytes = 4 * 1024 * 1024;
function safeRelative(path: string) {
  if (!identity(path) || path.includes('\\') || /^[A-Za-z]:/.test(path)
    || path.split('/').some(x => !x || x === '.' || x === '..')) fail('UNSAFE_RELATIVE_PATH');
}
async function canonicalRoot(root: string) {
  const path = resolve(root);
  if ((await lstat(path)).isSymbolicLink() || await realpath(path) !== path) fail('UNSAFE_ROOT');
  return path;
}
async function fileAt(root: string, relative: string) {
  safeRelative(relative);
  const path = join(root, relative);
  if (!path.startsWith(root + sep)) fail('OUTSIDE_ROOT');
  if (await realpath(path) !== path || !(await lstat(path)).isFile()) fail('UNSAFE_SOURCE');
  return path;
}
export function validateHandoff(bundle: HandoffBundle, maxBytes = 4 * 1024 * 1024) {
  if (bundle?.schema !== 'workspace.source-transfer.v1' || !identity(bundle.issueId) || !identity(bundle.sourceRevision)
    || !Array.isArray(bundle.files) || !bundle.files.length || bundle.files.length > 1024
    || !Array.isArray(bundle.completedEffectIds) || bundle.completedEffectIds.some(x => !identity(x))
    || new Set(bundle.completedEffectIds).size !== bundle.completedEffectIds.length
    || !Array.isArray(bundle.checks) || !bundle.nextAction || !identity(bundle.nextAction.id) || !identity(bundle.nextAction.action)) fail('INVALID_HANDOFF');
  const paths = new Set<string>(); let total = 0;
  for (const file of bundle.files) {
    safeRelative(file.path);
    if (paths.has(file.path)) fail('DUPLICATE_SOURCE');
    paths.add(file.path);
    if (typeof file.base64 !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(file.base64)
      || !Number.isSafeInteger(file.byteLength) || file.byteLength < 0 || !/^[a-f0-9]{64}$/.test(file.sha256)) fail('INVALID_SOURCE_BINDING');
    total += file.byteLength;
    if (total > maxBytes || file.base64.length > maxBytes * 2) fail('TRANSFER_TOO_LARGE');
    const bytes = Buffer.from(file.base64, 'base64');
    if (bytes.length !== file.byteLength || hash(bytes) !== file.sha256) fail('SOURCE_INTEGRITY_MISMATCH');
  }
  safeRelative(bundle.nextAction.path);
  if (!paths.has(bundle.nextAction.path) || bundle.completedEffectIds.includes(bundle.nextAction.id)) fail('INVALID_NEXT_ACTION');
  const ids = new Set<string>();
  for (const check of bundle.checks) {
    if (!identity(check.id) || ids.has(check.id) || !['passed', 'failed', 'unexecuted'].includes(check.status)
      || !Array.isArray(check.evidencePaths) || check.evidencePaths.some(path => !paths.has(path))
      || check.status === 'passed' && !check.evidencePaths.length) fail('INVALID_CHECK_EVIDENCE');
    ids.add(check.id);
  }
  return { files: paths.size, byteLength: total, bundleSha256: hash(JSON.stringify(bundle)) };
}
export async function packHandoff(root: string, paths: readonly string[], state: Omit<HandoffBundle, 'schema' | 'files'>): Promise<HandoffBundle> {
  // Finish count/path/aggregate-size admission before opening any source for content.
  if (!Array.isArray(paths) || !paths.length || paths.length > 1024) fail('INVALID_HANDOFF');
  const selectedPaths = [...paths];
  for (const path of selectedPaths) safeRelative(path);
  if (new Set(selectedPaths).size !== selectedPaths.length) fail('DUPLICATE_SOURCE');
  const base = await canonicalRoot(root);
  const rootStat = await lstat(base, { bigint: true });
  const sources: Array<{ path: string; absolute: string; stat: BigIntStats }> = [];
  let total = 0n;
  for (const path of selectedPaths) {
    const absolute = await fileAt(base, path), stat = await lstat(absolute, { bigint: true });
    if (!stat.isFile()) fail('UNSAFE_SOURCE');
    total += stat.size;
    if (total > BigInt(maxTransferBytes)) fail('TRANSFER_TOO_LARGE');
    sources.push({ path, absolute, stat });
  }
  const sameFile = (a: BigIntStats, b: BigIntStats) => a.dev === b.dev && a.ino === b.ino && a.mode === b.mode
    && a.uid === b.uid && a.gid === b.gid && a.size === b.size && a.mtimeNs === b.mtimeNs && a.ctimeNs === b.ctimeNs;
  const currentPath = async (source: typeof sources[number]) => {
    const currentRoot = await lstat(base, { bigint: true });
    if (!currentRoot.isDirectory() || currentRoot.dev !== rootStat.dev || currentRoot.ino !== rootStat.ino
      || await realpath(base) !== base || await realpath(source.absolute) !== source.absolute
      || !sameFile(source.stat, await lstat(source.absolute, { bigint: true }))) fail('SOURCE_CHANGED_DURING_PACK');
  };
  const readSource = async (source: typeof sources[number]) => {
    // NONBLOCK prevents a substituted FIFO from blocking before fstat can reject it.
    const handle = await open(source.absolute, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    try {
      const stat = await handle.stat({ bigint: true });
      if (!stat.isFile() || !sameFile(source.stat, stat)) fail('SOURCE_CHANGED_DURING_PACK');
      await currentPath(source);
      // One extra byte detects growth, without an unbounded readFile allocation.
      const bytes = Buffer.alloc(Number(stat.size) + 1); let length = 0;
      while (length < bytes.length) {
        const read = await handle.read(bytes, length, bytes.length - length, length);
        if (!read.bytesRead) break;
        length += read.bytesRead;
      }
      if (length !== Number(stat.size) || !sameFile(stat, await handle.stat({ bigint: true }))) fail('SOURCE_CHANGED_DURING_PACK');
      await currentPath(source);
      return bytes.subarray(0, length);
    } finally { await handle.close(); }
  };
  const files: HandoffFile[] = [];
  for (const source of sources) {
    const bytes = await readSource(source);
    files.push({ path: source.path, sha256: hash(bytes), byteLength: bytes.length, base64: bytes.toString('base64') });
  }
  const bundle: HandoffBundle = { schema: 'workspace.source-transfer.v1', ...state, files };
  validateHandoff(bundle);
  // Reject a source changing during collection; source revision/authority remains the existing owner's responsibility.
  for (const [index, source] of sources.entries()) if (hash(await readSource(source)) !== files[index]!.sha256) fail('SOURCE_CHANGED_DURING_PACK');
  return bundle;
}
/** A partial or visible destination must be reconciled by its owner, never removed/retried automatically. */
export class HandoffReceiveUncertainError extends Error {
  readonly code = 'RECEIVER_PUBLICATION_UNCERTAIN';
  constructor(readonly root: string, readonly bundleSha256: string, readonly stage: 'destination-create' | 'write' | 'file-sync' | 'directory-sync' | 'readback' | 'close', cause: unknown) {
    super(`RECEIVER_PUBLICATION_UNCERTAIN:${stage}`, { cause });
    this.name = 'HandoffReceiveUncertainError';
  }
}
/** Destination must be new. Success means fsynced bytes verified through the final scan, not atomic whole-tree visibility.
 * Linux procfs descriptor anchors avoid following substituted ancestors; other platforms fail closed.
 * These bounded checks cannot prevent arbitrary mutation after validation or return. */
export async function receiveHandoff(parent: string, destination: string, bundle: HandoffBundle) {
  const received = structuredClone(bundle), integrity = validateHandoff(received);
  safeRelative(destination);
  if (destination.includes('/')) fail('DESTINATION_MUST_BE_DIRECT_CHILD');
  if (process.platform !== 'linux') fail('UNSUPPORTED_RECEIVER_PLATFORM');
  const base = await canonicalRoot(parent), root = join(base, destination);
  const handles: FileHandle[] = [];
  const directories = new Map<string, { handle: FileHandle; stat: BigIntStats; path: string }>();
  const anchor = (handle: FileHandle) => `/proc/self/fd/${handle.fd}`;
  const same = (a: BigIntStats, b: BigIntStats) => a.dev === b.dev && a.ino === b.ino
    && a.mode === b.mode && a.uid === b.uid && a.gid === b.gid;
  const sameFile = (a: BigIntStats, b: BigIntStats) => b.isFile() && same(a, b) && b.nlink === a.nlink && b.nlink === 1n
    && b.size === a.size && b.mtimeNs === a.mtimeNs && b.ctimeNs === a.ctimeNs;
  let issued = false, stage: HandoffReceiveUncertainError['stage'] = 'destination-create';
  let error: unknown, failed = false;
  const pinDirectory = async (key: string, path: string, absolute: string) => {
    const before = await lstat(path, { bigint: true });
    const handle = await open(path, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    handles.push(handle);
    const stat = await handle.stat({ bigint: true });
    if (!stat.isDirectory() || !same(before, stat) || await realpath(anchor(handle)) !== absolute) fail('UNSAFE_DESTINATION');
    const pinned = { handle, stat, path: absolute }; directories.set(key, pinned); return pinned;
  };
  const currentDirectories = async () => {
    for (const { handle, stat, path } of directories.values()) {
      if (!same(stat, await handle.stat({ bigint: true })) || !same(stat, await lstat(path, { bigint: true }))
        || await realpath(path) !== path || await realpath(anchor(handle)) !== path) fail('DESTINATION_CHANGED_DURING_RECEIVE');
    }
  };
  const files: Array<{ file: HandoffFile; path: string; stat: BigIntStats }> = [];
  try {
    const parentDirectory = await pinDirectory('..', base, base);
    await currentDirectories();
    issued = true;
    try { await mkdir(join(anchor(parentDirectory.handle), destination), { mode: 0o700 }); }
    catch (caught: any) { if (caught.code === 'EEXIST') issued = false; throw caught; }
    await pinDirectory('', join(anchor(parentDirectory.handle), destination), root);
    stage = 'write';
    for (const file of received.files) {
      let key = '', directory = directories.get('')!;
      const parts = file.path.split('/'), name = parts.pop()!;
      for (const part of parts) {
        const next = key ? `${key}/${part}` : part;
        if (!directories.has(next)) {
          await currentDirectories();
          const path = join(anchor(directory.handle), part);
          // Do not adopt a directory inserted by another writer.
          await mkdir(path, { mode: 0o700 });
          await pinDirectory(next, path, join(root, next));
        }
        key = next; directory = directories.get(key)!;
      }
      await currentDirectories();
      const path = join(anchor(directory.handle), name);
      const handle = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
      try {
        await currentDirectories();
        await handle.writeFile(Buffer.from(file.base64, 'base64'));
        stage = 'file-sync'; await handle.sync(); stage = 'write';
        const stat = await handle.stat({ bigint: true });
        if (!stat.isFile() || stat.size !== BigInt(file.byteLength) || stat.nlink !== 1n) fail('UNSAFE_DESTINATION');
        files.push({ file, path, stat });
      } finally { await handle.close(); }
    }
    stage = 'directory-sync';
    // Child entries before their parents, including publication of the destination in its parent.
    for (const directory of [...directories.values()].reverse()) {
      await currentDirectories(); await directory.handle.sync();
    }
    stage = 'readback';
    for (const { file, path, stat } of files) {
      await currentDirectories();
      const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      try {
        if (!sameFile(stat, await handle.stat({ bigint: true }))) fail('RECEIVER_READBACK_MISMATCH');
        const bytes = Buffer.alloc(file.byteLength + 1); let length = 0;
        while (length < bytes.length) {
          const read = await handle.read(bytes, length, bytes.length - length, length);
          if (!read.bytesRead) break;
          length += read.bytesRead;
        }
        if (length !== file.byteLength || hash(bytes.subarray(0, length)) !== file.sha256
          || !sameFile(stat, await handle.stat({ bigint: true })) || !sameFile(stat, await lstat(path, { bigint: true }))) fail('RECEIVER_READBACK_MISMATCH');
      } finally { await handle.close(); }
    }
    // Earlier readback handles are closed: revalidate every leaf after ALL readbacks.
    // Paths still use retained directory descriptors, never substituted pathname ancestors.
    await currentDirectories();
    for (const { path, stat } of files) {
      if (!sameFile(stat, await lstat(path, { bigint: true }))) fail('RECEIVER_READBACK_MISMATCH');
    }
    await currentDirectories(); stage = 'close';
  } catch (caught) { failed = true; error = caught; }
  finally {
    for (const handle of handles.reverse()) {
      try { await handle.close(); } catch (caught) { if (!failed) { failed = true; error = caught; stage = 'close'; } }
    }
  }
  if (failed) {
    if (issued) throw new HandoffReceiveUncertainError(root, integrity.bundleSha256, stage, error);
    throw error;
  }
  return { root, ...integrity, completedEffectIds: [...received.completedEffectIds], nextAction: structuredClone(received.nextAction) };
}
function validToken(token: WriterToken) {
  return identity(token.writerId) && Number.isSafeInteger(token.epoch) && token.epoch > 0 && Number.isSafeInteger(token.revision) && token.revision > 0;
}
function validateCanonical(state: CanonicalHandoff) {
  if (state?.schema !== 'workspace.canonical-handoff.v1' || !validToken(state) || state.issueId !== state.bundle?.issueId
    || !Array.isArray(state.completedWriteIds) || state.completedWriteIds.some(x => !identity(x))
    || new Set(state.completedWriteIds).size !== state.completedWriteIds.length) fail('INVALID_CANONICAL_STATE');
  validateHandoff(state.bundle);
  return state;
}
export async function readCanonicalHandoff(root: string) {
  const base = await canonicalRoot(root);
  return validateCanonical(JSON.parse(await readFile(await fileAt(base, 'current.json'), 'utf8')));
}
async function locked(root: string, action: (base: string) => Promise<CanonicalHandoff>): Promise<CanonicalHandoff> {
  const base = await canonicalRoot(root), lock = join(base, '.writer-lock');
  // An uncertain/crashed writer leaves the lock for owner reconciliation. No automatic takeover/retry daemon.
  try { await mkdir(lock, { mode: 0o700 }); } catch (error: any) { if (error.code === 'EEXIST') fail('WRITER_BUSY_OR_UNCERTAIN'); throw error; }
  let uncertain = false, published: CanonicalHandoff | undefined;
  try { published = await action(base); return published; }
  catch (error) { uncertain = error instanceof HandoffPublicationUncertainError; throw error; }
  finally {
    if (!uncertain) {
      try { await rmdir(lock); }
      catch (error) {
        if (published) throw new HandoffPublicationUncertainError({ writerId: published.writerId, epoch: published.epoch, revision: published.revision }, 'lock-release', error);
        throw error;
      }
    }
  }
}
/** Publication may already be visible. Reconcile current.json and this writer lock before any retry. */
export class HandoffPublicationUncertainError extends Error {
  readonly code = 'CANONICAL_PUBLICATION_UNCERTAIN';
  constructor(readonly expected: WriterToken, readonly stage: 'publication' | 'directory-sync' | 'readback' | 'lock-release', cause: unknown) {
    super(`CANONICAL_PUBLICATION_UNCERTAIN:${stage}`, { cause });
    this.name = 'HandoffPublicationUncertainError';
  }
}
async function replace(base: string, state: CanonicalHandoff, initialize = false) {
  validateCanonical(state);
  const pending = join(base, `.current-${randomUUID()}.json`);
  let created = false, publicationIssued = false;
  let stage: HandoffPublicationUncertainError['stage'] = 'publication';
  try {
    const handle = await open(pending, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    created = true;
    try { await handle.writeFile(JSON.stringify(state, null, 2) + '\n'); await handle.sync(); }
    finally { await handle.close(); }
    publicationIssued = true;
    if (initialize) {
      // Hard-link publication is atomic and cannot overwrite an existing canonical state.
      try { await link(pending, join(base, 'current.json')); }
      catch (error: any) { if (error.code === 'EEXIST') publicationIssued = false; throw error; }
      await unlink(pending);
    } else await rename(pending, join(base, 'current.json'));
    stage = 'directory-sync';
    const directory = await open(base, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    try { await directory.sync(); } finally { await directory.close(); }
    stage = 'readback';
    const current = await readCanonicalHandoff(base);
    if (JSON.stringify(current) !== JSON.stringify(state)) fail('CANONICAL_READBACK_MISMATCH');
    return current;
  } catch (error) {
    if (publicationIssued) throw new HandoffPublicationUncertainError({ writerId: state.writerId, epoch: state.epoch, revision: state.revision }, stage, error);
    if (created) await unlink(pending);
    throw error;
  }
}
export async function initializeCanonicalHandoff(root: string, writerId: string, bundle: HandoffBundle) {
  return locked(root, async base => {
    const state: CanonicalHandoff = { schema: 'workspace.canonical-handoff.v1', issueId: bundle.issueId, writerId, epoch: 1, revision: 1, bundle, completedWriteIds: [] };
    validateCanonical(state);
    return replace(base, state, true);
  });
}
export type HandoffMutation = { kind: 'transfer-writer'; toWriterId: string } | { kind: 'commit'; bundle: HandoffBundle };
/** All writers must use this single authoritative endpoint; local copies cannot fence another host. */
export async function mutateCanonicalHandoff(root: string, expected: WriterToken, writeId: string, mutation: HandoffMutation) {
  return locked(root, async base => {
    const current = await readCanonicalHandoff(base);
    if (!validToken(expected) || current.writerId !== expected.writerId || current.epoch !== expected.epoch || current.revision !== expected.revision) fail('STALE_WRITER');
    if (!identity(writeId) || current.completedWriteIds.includes(writeId)) fail('WRITE_ALREADY_COMPLETED_OR_INVALID');
    const next = structuredClone(current);
    if (mutation.kind === 'transfer-writer') {
      if (!identity(mutation.toWriterId) || mutation.toWriterId === current.writerId) fail('INVALID_NEW_WRITER');
      next.writerId = mutation.toWriterId; next.epoch++;
    } else if (mutation.kind === 'commit') {
      validateHandoff(mutation.bundle);
      if (mutation.bundle.issueId !== current.issueId || current.bundle.completedEffectIds.some(id => !mutation.bundle.completedEffectIds.includes(id))) fail('COMPLETED_EFFECT_HISTORY_LOST');
      next.bundle = structuredClone(mutation.bundle);
    } else fail('INVALID_MUTATION');
    next.revision++; next.completedWriteIds.push(writeId);
    return replace(base, next);
  });
}
