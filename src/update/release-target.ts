import { createHash, randomUUID } from 'node:crypto';
import { link, open, readFile, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import type { UpdateCheckpoint, UpdateMode } from './workflow';

export const OFFICIAL_HERMES_UPSTREAM = 'https://github.com/NousResearch/hermes-agent.git';
const RELEASE_TAG = /^v(?:0|[1-9]\d{0,2})\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;
const FULL_SHA = /^[0-9a-f]{40}$/;
const digest = (value: string) => createHash('sha256').update(value).digest('hex');

/** This run-owned evidence is independent of workflow checkpoints, including interrupted Desktop updates. */
export interface ReleaseTarget {
  schemaVersion: 1;
  runId: string;
  mode: UpdateMode;
  requestedReleaseTag: string;
  sourceSha: string;
  upstream: typeof OFFICIAL_HERMES_UPSTREAM;
  resolvedAtUtc: string;
  resolution: { program: 'git'; args: string[]; stdout: string; stdoutSha256: string };
  acceptance: 'not-recorded';
}
export interface ReleaseLookupResult { exitCode: number; stdout: string }
export type ReleaseLookup = (args: string[]) => Promise<ReleaseLookupResult>;

export function validateReleaseTag(tag: string): void {
  if (!RELEASE_TAG.test(tag)) throw new Error('Use a canonical stable release tag such as v0.22.0');
}

/** A lightweight tag points directly at a commit; annotated tags must use their peeled commit. */
export function parseReleaseRefs(stdout: string, tag: string): string {
  validateReleaseTag(tag);
  const directRef = `refs/tags/${tag}`, peeledRef = directRef + '^{}';
  const refs = new Map<string, string>();
  for (const line of stdout.split('\n').filter(line => line.trim())) {
    const match = /^([0-9a-f]{40})\t([^\s]+)\r?$/.exec(line);
    if (!match || ![directRef, peeledRef].includes(match[2]!) || refs.has(match[2]!)) throw new Error('Invalid or ambiguous official release tag response');
    refs.set(match[2]!, match[1]!);
  }
  if (!refs.has(directRef)) throw new Error('Official release tag was not found');
  return refs.get(peeledRef) ?? refs.get(directRef)!;
}

const lookup: ReleaseLookup = async args => {
  const child = Bun.spawn(['git', ...args], { stdout: 'pipe', stderr: 'ignore', timeout: 45_000,
    env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_NO_LAZY_FETCH: '1' } });
  const [stdout, exitCode] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  return { stdout, exitCode };
};
const lookupArgs = (tag: string) => ['ls-remote', '--exit-code', OFFICIAL_HERMES_UPSTREAM, `refs/tags/${tag}`, `refs/tags/${tag}^{}`];

export async function loadReleaseTarget(directory: string): Promise<ReleaseTarget | null> {
  let bytes: string;
  try { bytes = await readFile(join(directory, 'release-target.json'), 'utf8'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; }
  const value = JSON.parse(bytes) as ReleaseTarget;
  if (value.schemaVersion !== 1 || typeof value.runId !== 'string' || !value.runId || !['backend-only', 'coordinated'].includes(value.mode)
    || value.upstream !== OFFICIAL_HERMES_UPSTREAM || !FULL_SHA.test(value.sourceSha) || !Number.isFinite(Date.parse(value.resolvedAtUtc))
    || value.acceptance !== 'not-recorded' || value.resolution?.program !== 'git' || typeof value.resolution.stdout !== 'string'
    || value.resolution.stdoutSha256 !== digest(value.resolution.stdout)) throw new Error('Invalid persisted release target evidence');
  validateReleaseTag(value.requestedReleaseTag);
  if (JSON.stringify(value.resolution.args) !== JSON.stringify(lookupArgs(value.requestedReleaseTag))
    || parseReleaseRefs(value.resolution.stdout, value.requestedReleaseTag) !== value.sourceSha) throw new Error('Persisted release target does not match its original Git evidence');
  return value;
}

export function validateReleaseResume(target: ReleaseTarget | null, mode: UpdateMode, options: { release?: string; sha?: string }, checkpoint?: UpdateCheckpoint): void {
  if (!target) {
    if (options.release) throw new Error('Cannot add a release target to an existing run without persisted release evidence');
    return;
  }
  if (target.mode !== mode || (checkpoint && target.runId !== checkpoint.id)) throw new Error('Persisted release target belongs to a different update run');
  if (options.sha || (options.release && options.release !== target.requestedReleaseTag)) throw new Error('Resume retains its original requested release and source SHA');
  if (checkpoint?.targetSha && checkpoint.targetSha !== target.sourceSha) throw new Error('Saved update target differs from the requested release SHA');
}

export async function resolveAndSaveReleaseTarget(directory: string, run: { id: string; mode: UpdateMode }, tag: string, execute: ReleaseLookup = lookup): Promise<ReleaseTarget> {
  validateReleaseTag(tag);
  const existing = await loadReleaseTarget(directory);
  if (existing) {
    if (existing.runId !== run.id) throw new Error('Release target belongs to a different run');
    validateReleaseResume(existing, run.mode, { release: tag });
    return existing;
  }
  const args = lookupArgs(tag), result = await execute(args);
  if (result.exitCode !== 0) throw new Error('Official release tag lookup failed; no update target was pinned');
  const target: ReleaseTarget = { schemaVersion: 1, runId: run.id, mode: run.mode, requestedReleaseTag: tag,
    sourceSha: parseReleaseRefs(result.stdout, tag), upstream: OFFICIAL_HERMES_UPSTREAM, resolvedAtUtc: new Date().toISOString(),
    resolution: { program: 'git', args, stdout: result.stdout, stdoutSha256: digest(result.stdout) }, acceptance: 'not-recorded' };
  // Publish complete immutable evidence before any native updater can run.
  const temporary = join(directory, `.release-target-${randomUUID()}.json`);
  const file = await open(temporary, 'wx', 0o600);
  try { await file.writeFile(JSON.stringify(target, null, 2) + '\n'); await file.sync(); } finally { await file.close(); }
  await link(temporary, join(directory, 'release-target.json'));
  const directoryHandle = await open(directory, 'r');
  try { await directoryHandle.sync(); } finally { await directoryHandle.close(); }
  await unlink(temporary);
  return target;
}
