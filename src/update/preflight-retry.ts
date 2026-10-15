import { createHash } from 'node:crypto';
import { lstat, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';

export interface DesktopPreflightRetryDecision {
  schemaVersion: 1;
  observedAtUtc: string;
  targetId: string;
  eligible: boolean;
  reasons: string[];
  requestOrHandoffArtifacts: string[];
  priorResults: Array<{ name: string; sha256: string | null; operationId: string | null }>;
  executionBoundary: 'Read-only prior evidence inspection; native invocation still requires fresh adapter preflight';
}
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};

/** An interrupted native intent always belongs to recovery; only proven pre-effect refusals may repeat preflight. */
export async function inspectDesktopPreflightRetry(directory: string, targetId: string): Promise<DesktopPreflightRetryDecision> {
  const names = await readdir(directory);
  const decision: DesktopPreflightRetryDecision = { schemaVersion: 1, observedAtUtc: new Date().toISOString(), targetId, eligible: false, reasons: [],
    requestOrHandoffArtifacts: names.filter(name => name.startsWith('desktop-native-request-') || name.startsWith('desktop-native-handoff-')).sort(), priorResults: [],
    executionBoundary: 'Read-only prior evidence inspection; native invocation still requires fresh adapter preflight' };
  if (decision.requestOrHandoffArtifacts.length) decision.reasons.push('durable-native-request-or-handoff-requires-recovery');
  const primary = names.filter(name => name.startsWith('desktop-update-result-'));
  const results = [...primary, ...names.filter(name => name.startsWith('desktop-update-recovery-'))].sort();
  if (!primary.length) decision.reasons.push('no-proven-prior-preflight-result');
  for (const name of results) {
    const evidence = { name, sha256: null as string | null, operationId: null as string | null };
    decision.priorResults.push(evidence);
    try {
      const path = join(directory, name), stat = await lstat(path);
      if (!stat.isFile() || stat.size <= 0 || stat.size > 2 * 1024 * 1024 || !/^desktop-update-(?:result|recovery)-[A-Za-z0-9-]+\.json$/.test(name)) throw new Error('Invalid result file');
      const bytes = await readFile(path);
      evidence.sha256 = createHash('sha256').update(bytes).digest('hex');
      const result = object(JSON.parse(bytes.toString('utf8'))), before = object(result.before);
      evidence.operationId = typeof result.operationId === 'string' ? result.operationId : null;
      if (result.schemaVersion !== 1 || !evidence.operationId || !/^[A-Za-z0-9-]{16,128}$/.test(evidence.operationId)
        || (name.startsWith('desktop-update-result-') && name !== `desktop-update-result-${evidence.operationId}.json`)
        || result.executionBoundary !== 'Native mutation not executed' || result.status !== 'blocked'
        || result.canonicalSha !== null || result.after !== null || result.receipt !== null || result.verification !== null
        || !Number.isFinite(Date.parse(String(result.startedAtUtc))) || !Number.isFinite(Date.parse(String(result.completedAtUtc)))
        || Date.parse(String(result.completedAtUtc)) < Date.parse(String(result.startedAtUtc))
        || (result.before !== null && before.targetId !== targetId)) decision.reasons.push('unknown-or-effectful-prior-result:' + name);
    } catch { decision.reasons.push('unreadable-or-invalid-prior-result:' + name); }
  }
  decision.eligible = decision.reasons.length === 0;
  return decision;
}
