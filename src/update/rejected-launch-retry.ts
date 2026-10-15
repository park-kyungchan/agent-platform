import { createHash } from 'node:crypto';
import { lstat, readFile, readdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import type { DesktopRejectedLaunchRetry } from '../adapters/desktop-update';

const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const digest = (value: Uint8Array) => createHash('sha256').update(value).digest('hex');
async function boundedFile(path: string): Promise<Buffer> {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.size <= 0 || stat.size > 2 * 1024 * 1024) throw new Error('Invalid evidence file');
  return readFile(path);
}

/** Only nominate exact same-run evidence; the native adapter validates the proof and fresh installation before any effect. */
export async function inspectDesktopRejectedLaunchRetry(directory: string, targetId: string): Promise<{
  schemaVersion: 1; observedAtUtc: string; candidate: DesktopRejectedLaunchRetry | null; reasons: string[];
  executionBoundary: 'Read-only dispatch nomination; native adapter proof and live preflight remain required';
}> {
  const decision = { schemaVersion: 1 as const, observedAtUtc: new Date().toISOString(), candidate: null as DesktopRejectedLaunchRetry | null, reasons: [] as string[],
    executionBoundary: 'Read-only dispatch nomination; native adapter proof and live preflight remain required' as const };
  const names = await readdir(directory), proofRef = join(resolve(directory), 'native-launch-rejection-proof.json');
  if (!names.includes('native-launch-rejection-proof.json')) { decision.reasons.push('no-same-run-native-rejection-proof'); return decision; }
  const requests = names.filter(name => name.startsWith('desktop-native-request-'));
  if (requests.length !== 1 || !/^desktop-native-request-[A-Za-z0-9-]+\.json$/.test(requests[0]!)) decision.reasons.push('exactly-one-original-native-request-required');
  if (names.some(name => name.startsWith('desktop-native-handoff-') || name.startsWith('desktop-native-rejected-launch-retry-'))) decision.reasons.push('existing-native-handoff-or-retry-requires-recovery');
  if (decision.reasons.length) return decision;
  try {
    const proofBytes = await boundedFile(proofRef), proof = object(JSON.parse(proofBytes.toString('utf8')));
    const request = object(JSON.parse((await boundedFile(join(directory, requests[0]!))).toString('utf8'))), scope = object(request.scope);
    const operationId = String(request.operationId), resultPath = join(resolve(directory), `desktop-update-result-${operationId}.json`);
    if (!/^[A-Za-z0-9-]{16,128}$/.test(operationId) || requests[0] !== `desktop-native-request-${operationId}.json`
      || request.schemaVersion !== 1 || request.targetId !== targetId || request.mode !== 'closed-script' || request.correlationId !== operationId || scope.channel !== 'stable'
      || proof.schema !== 'vps-operations.hermes-native-script-binding-check.v1' || proof.readOnly !== true
      || proof.correlationId !== operationId || proof.nativeRequestStartedAt !== request.startedAtUtc || object(proof.verification).passed !== true
      || object(proof.failedResult).path !== resultPath) throw new Error('Proof does not belong to this original request');
    const resultBytes = await boundedFile(resultPath), result = object(JSON.parse(resultBytes.toString('utf8')));
    if (proof.failedNativeResultSha256 !== digest(resultBytes) || result.operationId !== operationId || result.status !== 'failed'
      || object(result.error).code !== 'native-coordinator-failed' || result.executionBoundary !== 'Native updater invoked; verification is separately recorded'
      || result.receipt !== null || result.after !== null) throw new Error('Original launch rejection is not pinned');
    decision.candidate = { operationId, proofRef, proofSha256: digest(proofBytes) };
  } catch { decision.reasons.push('invalid-or-unbound-native-rejection-evidence'); }
  return decision;
}
