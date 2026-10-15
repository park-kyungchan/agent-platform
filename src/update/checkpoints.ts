import { mkdir, readFile, readdir, rename, open, link, unlink } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import type { UpdateCheckpoint } from './workflow';

export async function saveCheckpoint<Candidate>(directory: string, checkpoint: UpdateCheckpoint<Candidate>): Promise<void> {
  const history = join(directory, 'history');
  await mkdir(history, { recursive: true });
  const bytes = JSON.stringify(checkpoint, null, 2) + '\n';
  const durableWrite = async (path: string) => {
    const file = await open(path, 'wx', 0o600);
    try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
  };
  const syncDirectory = async (path: string) => {
    const file = await open(path, 'r');
    try { await file.sync(); } finally { await file.close(); }
  };
  // Exclusive immutable revisions prevent two resumptions from authorizing the same next step.
  // Publish only completely written bytes; a killed writer cannot leave a partial authoritative revision.
  const stagedRevision = join(history, `.revision-${randomUUID()}.tmp`);
  await durableWrite(stagedRevision);
  await link(stagedRevision, join(history, String(checkpoint.revision).padStart(6, '0') + '.json'));
  await syncDirectory(history);
  await unlink(stagedRevision); // Only the staging link; the immutable evidence bytes remain.
  const temporary = join(directory, `.state-${randomUUID()}.json`);
  await durableWrite(temporary);
  await rename(temporary, join(directory, 'state.json'));
  await syncDirectory(directory);
}
export async function loadCheckpoint<Candidate>(directory: string): Promise<UpdateCheckpoint<Candidate>> {
  // Immutable history is authoritative even if a process died before publishing its pointer.
  const history = join(directory, 'history');
  const names = (await readdir(history)).filter(name => /^[0-9]{6}\.json$/.test(name)).sort();
  const name = names.at(-1);
  if (!name) throw new Error('Update checkpoint history is empty');
  const checkpoint = JSON.parse(await readFile(join(history, name), 'utf8')) as UpdateCheckpoint<Candidate>;
  if (checkpoint.schemaVersion !== 1 || typeof checkpoint.id !== 'string' || !['backend-only', 'coordinated'].includes(checkpoint.mode)
    || !Number.isSafeInteger(checkpoint.revision) || checkpoint.revision < 1 || checkpoint.revision !== Number(name.slice(0, 6))
    || (checkpoint.targetSha !== null && !/^[0-9a-f]{40}$/.test(checkpoint.targetSha))) throw new Error('Invalid update checkpoint');
  return checkpoint;
}
