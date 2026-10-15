import { afterEach, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspectDesktopRejectedLaunchRetry } from '../src/update/rejected-launch-retry';

const directories: string[] = [];
afterEach(async () => { for (const path of directories.splice(0)) await rm(path, { recursive: true, force: true }); });
const operationId = 'a'.repeat(32), targetId = 'windows-hermes-desktop', startedAtUtc = '2026-10-14T23:16:45.397Z';
const digest = (value: string) => createHash('sha256').update(value).digest('hex');
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'desktop-rejected-launch-')); directories.push(root);
  const request = { schemaVersion: 1, operationId, targetId, mode: 'closed-script', correlationId: operationId, startedAtUtc, scope: { channel: 'stable' } };
  const result = { operationId, status: 'failed', error: { code: 'native-coordinator-failed' }, executionBoundary: 'Native updater invoked; verification is separately recorded', receipt: null, after: null };
  const resultBytes = JSON.stringify(result), resultPath = join(root, `desktop-update-result-${operationId}.json`);
  const proof = { schema: 'vps-operations.hermes-native-script-binding-check.v1', readOnly: true, correlationId: operationId,
    nativeRequestStartedAt: startedAtUtc, verification: { passed: true }, failedResult: { path: resultPath }, failedNativeResultSha256: digest(resultBytes) };
  await writeFile(join(root, `desktop-native-request-${operationId}.json`), JSON.stringify(request));
  await writeFile(resultPath, resultBytes);
  await writeFile(join(root, 'native-launch-rejection-proof.json'), JSON.stringify(proof));
  return { root, resultPath, resultBytes, proof };
}

test('same-run dispatch pins original request correlation and exact proof/result bytes', async () => {
  const { root, resultPath, resultBytes } = await fixture();
  const decision = await inspectDesktopRejectedLaunchRetry(root, targetId);
  expect(decision.reasons).toEqual([]);
  expect(decision.candidate?.operationId).toBe(operationId);
  expect(decision.candidate?.proofSha256).toBe(digest(await readFile(join(root, 'native-launch-rejection-proof.json'), 'utf8')));
  expect(await readFile(resultPath, 'utf8')).toBe(resultBytes);
});
test('unknown proof correlation, altered failed result and duplicate requests retain recovery', async () => {
  const { root, resultPath, resultBytes, proof } = await fixture();
  await writeFile(join(root, 'native-launch-rejection-proof.json'), JSON.stringify({ ...proof, correlationId: 'b'.repeat(32) }));
  expect((await inspectDesktopRejectedLaunchRetry(root, targetId)).candidate).toBeNull();
  await writeFile(join(root, 'native-launch-rejection-proof.json'), JSON.stringify(proof));
  await writeFile(resultPath, resultBytes + ' ');
  expect((await inspectDesktopRejectedLaunchRetry(root, targetId)).candidate).toBeNull();
  await writeFile(resultPath, resultBytes);
  await writeFile(join(root, 'desktop-native-request-unknown.tmp'), '{}');
  expect((await inspectDesktopRejectedLaunchRetry(root, targetId)).candidate).toBeNull();
});
test('a previous retry intent or handoff prevents another launch nomination', async () => {
  for (const name of [`desktop-native-rejected-launch-retry-${operationId}.json`, `desktop-native-handoff-${operationId}.json`]) {
    const { root } = await fixture(); await writeFile(join(root, name), '{}');
    expect((await inspectDesktopRejectedLaunchRetry(root, targetId)).candidate).toBeNull();
  }
});
