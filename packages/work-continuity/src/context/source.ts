import { createHash } from 'node:crypto';
import { readFile, realpath, stat } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { validateArtifact } from '@palantirkc/ontology';
import type { CheckpointRead, ContextIo, SourcePin, SourceStatus } from './types';

export const sha256 = (bytes: Uint8Array | string): string => createHash('sha256').update(bytes).digest('hex');
export const localContextIo: ContextIo = {
  canonical: realpath,
  read: async (path) => {
    const metadata = await stat(path);
    if (!metadata.isFile() || metadata.size > 2_097_152) throw Error('Source must be a regular file of at most 2 MiB');
    return readFile(path);
  },
};
export const decode = (bytes: Uint8Array): string => new TextDecoder('utf-8', { fatal: true }).decode(bytes);
export function assertAbsolutePath(path: string): void {
  if (!isAbsolute(path) || resolve(path) !== path) throw Error('Exact absolute source path required');
}
export function insideRoot(root: string, path: string): boolean {
  const part = relative(root, path);
  return part === '' || (!part.startsWith(`..${sep}`) && part !== '..' && !isAbsolute(part));
}
export async function exactPath(path: string, root: string, io: ContextIo): Promise<void> {
  assertAbsolutePath(root); assertAbsolutePath(path);
  if (!insideRoot(root, path) || await io.canonical(root) !== root || await io.canonical(path) !== path) throw Error('Source path must be canonical and owned by root');
}
export async function readExact(path: string, root: string, io: ContextIo): Promise<Uint8Array> {
  await exactPath(path, root, io);
  const bytes = await io.read(path);
  if (bytes.byteLength > 2_097_152) throw Error('Source exceeds 2 MiB');
  return bytes;
}
export async function checkSources(pins: readonly SourcePin[], root: string, io: ContextIo): Promise<SourceStatus> {
  const checks: SourceStatus['checks'] = [];
  const repair: SourceStatus['repair'] = [];
  for (const pin of pins) {
    let status: SourceStatus['checks'][number]['status'] = 'current';
    let actualSha256: string | undefined;
    try {
      const bytes = await readExact(pin.path, root, io);
      actualSha256 = sha256(bytes);
      if (actualSha256 !== pin.sha256) status = 'drifted';
      else if (pin.recordKind) {
        const checked = validateArtifact(JSON.parse(decode(bytes)));
        if (!checked.valid || checked.value?.kind !== pin.recordKind) status = 'invalid-record';
      }
    } catch (error) {
      status = typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT' ? 'missing' : 'unsafe';
    }
    checks.push({ path: pin.path, expectedSha256: pin.sha256, ...(actualSha256 ? { actualSha256 } : {}), status });
  }
  // The source set must remain stable for the entire read, not merely at each individual read time.
  for (const check of checks) {
    if (check.status === 'current') {
      try {
        const bytes = await readExact(check.path, root, io); check.actualSha256 = sha256(bytes);
        if (check.actualSha256 !== check.expectedSha256) check.status = 'drifted';
      } catch (error) {
        check.status = typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT' ? 'missing' : 'unsafe';
      }
    }
    if (check.status !== 'current') repair.push({ path: check.path, action: check.status === 'missing' ? 'restore-source' : check.status === 'drifted' ? 'reobserve-source' : 'repair-reference', reason: check.status });
  }
  return { status: repair.length ? 'repair-required' : 'current', checks, repair, authority: 'none' };
}

/** Recheck the observed snapshot after rendering. This is not a multi-file lock. */
export async function assertContextSnapshotStable(read: CheckpointRead, path: string, root: string, io: ContextIo, label: string): Promise<void> {
  if (sha256(await readExact(path, root, io)) !== read.sha256) throw Error(`Checkpoint changed while projecting Work ${label}`);
  const finalSources = await checkSources(read.checkpoint.sourcePins, root, io);
  if (JSON.stringify(finalSources.checks) !== JSON.stringify(read.sourceStatus.checks)) throw Error(`Source changed while projecting Work ${label}; reobserve checkpoint sources`);
}
