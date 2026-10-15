import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadReleaseTarget, OFFICIAL_HERMES_UPSTREAM, parseReleaseRefs, resolveAndSaveReleaseTarget, validateReleaseResume } from '../src/update/release-target';
import type { UpdateCheckpoint } from '../src/update/workflow';

const directories: string[] = [];
afterEach(async () => { for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true }); });
const tag = 'v0.22.0', tagSha = 'a'.repeat(40), commitSha = 'b'.repeat(40);
const direct = `${tagSha}\trefs/tags/${tag}\n`;
const annotated = direct + `${commitSha}\trefs/tags/${tag}^{}\n`;
const directory = async () => { const value = await mkdtemp(join(tmpdir(), 'hermes-release-')); directories.push(value); return value; };

test('official tag resolution uses peeled annotated commits and direct lightweight commits', () => {
  expect(parseReleaseRefs(direct, tag)).toBe(tagSha);
  expect(parseReleaseRefs(annotated, tag)).toBe(commitSha);
  for (const stdout of ['', `${commitSha}\trefs/tags/${tag}^{}\n`, direct + direct, `${tagSha}\trefs/heads/main\n`, `${tagSha.toUpperCase()}\trefs/tags/${tag}\n`]) {
    expect(() => parseReleaseRefs(stdout, tag)).toThrow();
  }
});

test('release evidence precedes workflow checkpoints and resume never resolves a moving tag again', async () => {
  const root = await directory();
  let lookups = 0;
  const target = await resolveAndSaveReleaseTarget(root, { id: 'run-1', mode: 'coordinated' }, tag, async args => {
    lookups++;
    expect(args).toEqual(['ls-remote', '--exit-code', OFFICIAL_HERMES_UPSTREAM, `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
    return { exitCode: 0, stdout: annotated };
  });
  expect(target.sourceSha).toBe(commitSha);
  expect(await loadReleaseTarget(root)).toEqual(target);
  expect((await readFile(join(root, 'release-target.json'), 'utf8'))).toContain('"requestedReleaseTag": "v0.22.0"');
  expect(await resolveAndSaveReleaseTarget(root, { id: 'run-1', mode: 'coordinated' }, tag, async () => {
    throw new Error('Resume must not contact moving upstream refs');
  })).toEqual(target);
  expect(lookups).toBe(1);
  validateReleaseResume(target, 'coordinated', {}, { id: 'run-1', targetSha: commitSha } as UpdateCheckpoint);
  expect(() => validateReleaseResume(target, 'coordinated', { release: 'v0.23.0' })).toThrow('original requested release');
  expect(() => validateReleaseResume(target, 'coordinated', { sha: commitSha })).toThrow('original requested release');
  expect(() => validateReleaseResume(target, 'coordinated', {}, { id: 'run-1', targetSha: tagSha } as UpdateCheckpoint)).toThrow('differs');
  expect(() => validateReleaseResume(target, 'backend-only', {})).toThrow('different update run');
  expect(() => validateReleaseResume(null, 'coordinated', { release: tag })).toThrow('Cannot add');
});

test('failed lookup and modified persisted evidence cannot establish an update target', async () => {
  const root = await directory();
  await expect(resolveAndSaveReleaseTarget(root, { id: 'run-1', mode: 'backend-only' }, tag, async () => ({ exitCode: 2, stdout: '' }))).rejects.toThrow('lookup failed');
  expect(await loadReleaseTarget(root)).toBeNull();
  const target = await resolveAndSaveReleaseTarget(root, { id: 'run-1', mode: 'backend-only' }, tag, async () => ({ exitCode: 0, stdout: annotated }));
  await writeFile(join(root, 'release-target.json'), JSON.stringify({ ...target, sourceSha: tagSha }));
  await expect(loadReleaseTarget(root)).rejects.toThrow('original Git evidence');
});
