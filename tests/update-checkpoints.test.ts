import { test, expect } from 'bun:test';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadCheckpoint, saveCheckpoint } from '../src/update/checkpoints';
import type { UpdateCheckpoint } from '../src/update/workflow';
const checkpoint = (revision: number): UpdateCheckpoint<string> => ({ schemaVersion: 1, id: 'run', revision, mode: 'backend-only', phase: 'target-pinned', status: 'running', createdAtUtc: '2026-10-08T00:00:00Z', updatedAtUtc: '2026-10-08T00:00:00Z', targetSha: 'a'.repeat(40), targetSource: 'upstream-main-at-start', desktop: null, candidate: null, backend: null, deploymentAttempted: false, failure: null, acceptance: 'not-recorded' });
test('history recovers a committed target when the mutable pointer is stale', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'hermes-checkpoint-'));
  await saveCheckpoint(directory, checkpoint(1));
  await saveCheckpoint(directory, { ...checkpoint(2), phase: 'backend-prepare' });
  await writeFile(join(directory, 'state.json'), JSON.stringify(checkpoint(1)));
  expect((await loadCheckpoint(directory)).revision).toBe(2);
  expect((await loadCheckpoint(directory)).phase).toBe('backend-prepare');
});
test('a competing resume cannot overwrite an immutable revision', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'hermes-checkpoint-'));
  await saveCheckpoint(directory, checkpoint(1));
  await expect(saveCheckpoint(directory, { ...checkpoint(1), targetSha: 'b'.repeat(40) })).rejects.toThrow();
  expect((await loadCheckpoint(directory)).targetSha).toBe('a'.repeat(40));
  expect(JSON.parse(await readFile(join(directory, 'state.json'), 'utf8')).targetSha).toBe('a'.repeat(40));
});
test('unpublished partial staging bytes do not replace the committed history', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'hermes-checkpoint-'));
  await saveCheckpoint(directory, checkpoint(1));
  await writeFile(join(directory, 'history/.revision-interrupted.tmp'), '{"schemaVersion":');
  expect((await loadCheckpoint(directory)).revision).toBe(1);
});
