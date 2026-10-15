import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspectDesktopPreflightRetry } from '../src/update/preflight-retry';

const directories: string[] = [];
afterEach(async () => { for (const path of directories.splice(0)) await rm(path, { recursive: true, force: true }); });
const targetId = 'windows-hermes-desktop', operationId = 'a'.repeat(32);
const filename = `desktop-update-result-${operationId}.json`;
const blocked = { schemaVersion: 1, operationId, startedAtUtc: '2026-10-14T23:13:41.877Z', completedAtUtc: '2026-10-14T23:13:48.735Z',
  status: 'blocked', canonicalSha: null, before: { targetId, marker: { present: false } }, after: null, receipt: null, verification: null,
  executionBoundary: 'Native mutation not executed' };
async function fixture(result: unknown = blocked) {
  const path = await mkdtemp(join(tmpdir(), 'desktop-preflight-retry-')); directories.push(path);
  if (result !== null) await writeFile(join(path, filename), JSON.stringify(result));
  return path;
}

test('only an explicit completed pre-effect refusal admits a same-run preflight retry', async () => {
  const path = await fixture(), original = await readFile(join(path, filename), 'utf8');
  const decision = await inspectDesktopPreflightRetry(path, targetId);
  expect(decision.eligible).toBe(true);
  expect(decision.priorResults[0]?.sha256).toMatch(/^[0-9a-f]{64}$/);
  expect(decision.priorResults[0]?.operationId).toBe(operationId);
  expect(await readFile(join(path, filename), 'utf8')).toBe(original);
});

test('a durable request or handoff always retains native recovery even when a result claims no mutation', async () => {
  for (const name of [`desktop-native-request-${operationId}.json`, `desktop-native-handoff-${operationId}.json`, 'desktop-native-request-unknown.tmp']) {
    const path = await fixture(); await writeFile(join(path, name), '{}');
    const decision = await inspectDesktopPreflightRetry(path, targetId);
    expect(decision.eligible).toBe(false);
    expect(decision.reasons).toContain('durable-native-request-or-handoff-requires-recovery');
  }
});

test('unknown, effectful or foreign result evidence cannot authorize another updater', async () => {
  for (const result of [{}, { ...blocked, executionBoundary: 'Native updater invoked; verification is separately recorded' },
    { ...blocked, receipt: { outcome: 'success' } }, { ...blocked, before: { targetId: 'other-desktop' } },
    { ...blocked, completedAtUtc: '' }, { ...blocked, operationId: 'b'.repeat(32) }]) {
    expect((await inspectDesktopPreflightRetry(await fixture(result), targetId)).eligible).toBe(false);
  }
});

test('every saved result must prove no native effects, and empty evidence cannot admit a retry', async () => {
  expect((await inspectDesktopPreflightRetry(await fixture(null), targetId)).eligible).toBe(false);
  const path = await fixture();
  await writeFile(join(path, `desktop-update-result-${'b'.repeat(32)}.json`), JSON.stringify({ ...blocked, operationId: 'b'.repeat(32), executionBoundary: 'unknown' }));
  expect((await inspectDesktopPreflightRetry(path, targetId)).eligible).toBe(false);
  await writeFile(join(path, `desktop-update-result-${'b'.repeat(32)}.json`), '{');
  expect((await inspectDesktopPreflightRetry(path, targetId)).eligible).toBe(false);
});
