import { sha256 } from '../context/source';
import { qualifySourceClosure, sourceKey, type PortableSource, type SourceObservation } from './qualification';

/** Owner-supplied complete population; no inferred admission, credentials or native session store. */
export interface OriginalInventory {
  schema: 'codex.original-inventory.v1'; checkpointId: string; capturedAtUtc: string;
  population: { owner: string; scope: string; complete: boolean; admittedCount: number };
  originals: Array<{ source: PortableSource; primaryPersistedAtUtc: string;
    independentCopy?: { sha256: string; byteLength: number; verifiedAtUtc: string } }>;
}
export interface RecoveryPolicy {
  revision: string; head: 'current' | 'unknown' | 'publishing';
  admittedRepositories: string[]; deletedOriginalHashes: string[]; forgottenSourceHashes: string[];
}
export interface IndependentOriginalReader {
  identity: string;
  /** Actual owner evidence, not a provider-name inference or an authorization grant. */
  independence: { primaryAccessDisabled: boolean; authenticatedWithoutPrimary: boolean; evidenceRef: string };
  read(source: PortableSource, signal: AbortSignal): Promise<SourceObservation>;
}
export interface CleanOriginalDestination {
  identity: string;
  /** Observe clean destination; honor abort. This operation must not mutate existing data. */
  prepare(signal: AbortSignal): Promise<{ clean: boolean }>;
  /** Implementations create exclusively and return durable locators, never bearer URLs. */
  create(source: PortableSource, bytes: Uint8Array, signal: AbortSignal): Promise<string>;
  read(locator: string, signal: AbortSignal): Promise<Uint8Array>;
}
export interface RecoveryBounds { maxOriginals: number; maxBytes: number; maxElapsedMs: number }
const date = (value: string) => Number.isFinite(Date.parse(value));
const validSource = (source: PortableSource) => Boolean(source.repositoryId && source.revision)
  && /^[a-f0-9]{64}$/.test(source.sha256) && Number.isSafeInteger(source.byteLength) && source.byteLength >= 0
  && !source.relativePath.includes('\\') && !/[\u0000-\u001f]/.test(source.relativePath) && !/^[A-Za-z]:/.test(source.relativePath)
  && source.relativePath.split('/').every(part => Boolean(part) && part !== '.' && part !== '..');
export function inspectOriginalInventory(inventory: OriginalInventory, bounds: RecoveryBounds) {
  const problems: string[] = [];
  if (inventory.schema !== 'codex.original-inventory.v1' || !inventory.checkpointId || !date(inventory.capturedAtUtc)
    || !inventory.population.owner || !inventory.population.scope || !inventory.population.complete
    || inventory.population.admittedCount !== inventory.originals.length) problems.push('Complete explicit owner population is required.');
  if (![bounds.maxOriginals, bounds.maxBytes, bounds.maxElapsedMs].every(n => Number.isSafeInteger(n) && n > 0)) problems.push('Positive bounded limits are required.');
  if (bounds.maxElapsedMs > 2_147_483_647) problems.push('Elapsed bound exceeds supported native timer range.');
  const keys = inventory.originals.map(row => sourceKey(row.source));
  if (new Set(keys).size !== keys.length) problems.push('Duplicate logical originals.');
  const locators = inventory.originals.map(row => JSON.stringify([row.source.repositoryId, row.source.revision, row.source.relativePath]));
  if (new Set(locators).size !== locators.length) problems.push('Ambiguous original locators.');
  if (inventory.originals.some(row => !validSource(row.source) || !date(row.primaryPersistedAtUtc)
    || Date.parse(row.primaryPersistedAtUtc) > Date.parse(inventory.capturedAtUtc))) problems.push('Invalid original identity or persistence time.');
  const totalBytes = inventory.originals.reduce((sum, row) => sum + row.source.byteLength, 0);
  if (!Number.isSafeInteger(totalBytes) || totalBytes > bounds.maxBytes || keys.length > bounds.maxOriginals) problems.push('Inventory exceeds bounded population or bytes.');
  return { status: problems.length ? 'blocked' as const : 'valid' as const, originalCount: keys.length, totalBytes, problems };
}

/** Full population arithmetic; an upload timestamp alone is never verified coverage. */
export function measureOriginalCoverage(inventory: OriginalInventory, observedAtUtc: string, rpoSeconds = 3600) {
  if (!date(observedAtUtc) || !Number.isFinite(rpoSeconds) || rpoSeconds < 0) throw Error('Invalid coverage clock or target');
  const now = Date.parse(observedAtUtc);
  const rows = inventory.originals.map(row => {
    const copy = row.independentCopy, persisted = Date.parse(row.primaryPersistedAtUtc);
    const verified = Boolean(copy && copy.sha256 === row.source.sha256 && copy.byteLength === row.source.byteLength
      && date(copy.verifiedAtUtc) && Date.parse(copy.verifiedAtUtc) >= persisted && Date.parse(copy.verifiedAtUtc) <= now);
    const lagSeconds = ((verified ? Date.parse(copy!.verifiedAtUtc) : now) - persisted) / 1000;
    return { key: sourceKey(row.source), status: verified ? 'verified' as const : 'uncovered' as const,
      lagSeconds, target: Number.isFinite(lagSeconds) && lagSeconds >= 0 && lagSeconds <= rpoSeconds ? 'within-target' as const : 'breach' as const };
  });
  return { populationComplete: inventory.population.complete && inventory.population.admittedCount === rows.length,
    denominator: rows.length, verifiedCount: rows.filter(row => row.status === 'verified').length,
    uncoveredCount: rows.filter(row => row.status === 'uncovered').length, breachCount: rows.filter(row => row.target === 'breach').length,
    rpoSeconds, observedAtUtc, rows };
}

export async function restoreOriginalArchive(input: {
  inventory: OriginalInventory; bounds: RecoveryBounds; reader: IndependentOriginalReader;
  destination: CleanOriginalDestination; currentPolicy: () => Promise<RecoveryPolicy>; rtoSeconds?: number;
}) {
  // Snapshot caller-owned inputs before awaiting. Credentials never enter these records.
  const inventory = structuredClone(input.inventory), bounds = structuredClone(input.bounds);
  const validation = inspectOriginalInventory(inventory, bounds), started = performance.now();
  const rows: Array<{ key: string; status: string; locator?: string; reusable?: boolean }> = [];
  const problems = [...validation.problems];
  const independence = input.reader.independence;
  if (!input.reader.identity || !independence.primaryAccessDisabled || !independence.authenticatedWithoutPrimary || !independence.evidenceRef) problems.push('Observed independent authentication with primary access disabled is required.');
  const controller = new AbortController(), signal = controller.signal;
  // A referenced native timer also fires when the only work is a hung promise.
  const deadlineTimer = setTimeout(() => controller.abort(), validation.status === 'valid' ? bounds.maxElapsedMs : 1);
  // Bound waiting even when an adapter ignores abort. An issued write may still
  // finish later; timeout never certifies cancellation or permission to replay it.
  function bounded<T>(operation: () => Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      const abort = () => reject(Error('Recovery deadline reached'));
      if (signal.aborted) { abort(); return; }
      signal.addEventListener('abort', abort, { once: true });
      Promise.resolve().then(operation).then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
    });
  }
  try {
  let policy: RecoveryPolicy = { revision: '', head: 'unknown', admittedRepositories: [], deletedOriginalHashes: [], forgottenSourceHashes: [] };
  if (!problems.length) try { policy = structuredClone(await bounded(input.currentPolicy)); }
  catch { problems.push('Current policy unavailable within recovery bound.'); }
  if (!policy.revision || policy.head !== 'current') problems.push('Current stable policy is required.');
  if (inventory.originals.some(row => !policy.admittedRepositories.includes(row.source.repositoryId))) problems.push('Original repository admission is absent.');
  const policyPin = JSON.stringify(policy);
  const stablePolicy = async () => { try { return JSON.stringify(await bounded(input.currentPolicy)) === policyPin; } catch { return false; } };
  if (!problems.length) try { if (!(await bounded(() => input.destination.prepare(signal))).clean) problems.push('A clean exclusive destination is required.'); }
  catch { problems.push('Destination readiness unavailable within recovery bound.'); }
  if (!problems.length) for (const row of inventory.originals) {
    const key = sourceKey(row.source);
    if (signal.aborted || performance.now() - started >= bounds.maxElapsedMs) { problems.push('Recovery elapsed bound reached.'); break; }
    if (!(await stablePolicy())) { problems.push('Policy changed; already restored bytes remain quarantined.'); break; }
    if (policy.deletedOriginalHashes.includes(row.source.sha256)) { rows.push({ key, status: 'deleted-original-withheld', reusable: false }); continue; }
    let writeIssued = false, locator: string | undefined;
    try {
      const observation = await bounded(() => input.reader.read(structuredClone(row.source), signal));
      const check = qualifySourceClosure([row.source], [observation], new Set(policy.admittedRepositories)).checks[0]!;
      if (check.status !== 'verified' || observation.status !== 'available') { rows.push({ key, status: check.status }); continue; }
      // A reader may return mutable buffers; bind the verified owned copy to the write.
      const bytes = new Uint8Array(observation.bytes);
      if (sha256(bytes) !== row.source.sha256 || bytes.byteLength !== row.source.byteLength) { rows.push({ key, status: 'hash-mismatch' }); continue; }
      if (signal.aborted || performance.now() - started >= bounds.maxElapsedMs || !(await stablePolicy())) { problems.push('Bound or current policy changed before write.'); break; }
      writeIssued = true;
      locator = await bounded(() => input.destination.create(structuredClone(row.source), bytes, signal));
      const restored = await bounded(() => input.destination.read(locator!, signal));
      if (sha256(restored) !== row.source.sha256 || restored.byteLength !== row.source.byteLength) { rows.push({ key, status: 'destination-corrupt', locator }); continue; }
      rows.push({ key, status: 'restored', locator, reusable: !policy.forgottenSourceHashes.includes(row.source.sha256) });
    } catch { rows.push({ key, status: writeIssued ? locator ? 'destination-readback-unverified' : 'destination-write-uncertain' : signal.aborted ? 'timeout' : 'transport-or-destination-error', ...(locator ? { locator } : {}), reusable: false }); }
  }
  const policyCurrent = await stablePolicy();
  if (!policyCurrent) { problems.push('Final policy is stale; restored bytes remain quarantined.'); for (const row of rows) row.reusable = false; }
  const elapsedSeconds = (performance.now() - started) / 1000;
  if (elapsedSeconds * 1000 > bounds.maxElapsedMs) problems.push('Recovery elapsed bound exceeded; destination may contain quarantined bytes.');
  const restoredCount = rows.filter(row => row.status === 'restored').length;
  const deletedCount = rows.filter(row => row.status === 'deleted-original-withheld').length;
  const complete = !problems.length && restoredCount + deletedCount === inventory.originals.length;
  const rtoSeconds = input.rtoSeconds ?? 14400;
  if (!Number.isFinite(rtoSeconds) || rtoSeconds < 0) throw Error('Invalid restoration target');
  return { schema: 'codex.original-restore-result.v1' as const, checkpointId: inventory.checkpointId,
    status: complete ? 'restored-current-policy' as const : 'incomplete' as const,
    denominator: inventory.originals.length, restoredCount, deletedCount, unverifiedCount: inventory.originals.length - restoredCount - deletedCount,
    rows, problems, elapsedSeconds, rtoSeconds, rtoTargetMet: complete && elapsedSeconds <= rtoSeconds,
    policyRevision: policy.revision, policyCurrent, readerIdentity: input.reader.identity, destinationIdentity: input.destination.identity,
    independenceEvidenceRef: independence.evidenceRef, nativePermissionGranted: false as const,
    limitations: ['Owner-supplied adapter and population evidence require independent review.', 'Abort bounds waiting, not cancellation of already issued effects; uncertain writes require exact destination observation before replay.', 'No provider authentication, deletion, scheduler, Memory write or production-volume guarantee is implemented.'] };
  } finally { clearTimeout(deadlineTimer); }
}
