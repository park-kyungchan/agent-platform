import { describe, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { archivedReceipt, buildDesktopNativeCommand, createDesktopReceiptPoller, DEFAULT_DESKTOP_UPDATE_TARGET as CONFIGURED_DESKTOP_UPDATE_TARGET, desktopNativePollState, desktopPreUpdateEligibility, normalizeDesktopReceipt, recoverDesktopUpdate, resolveDesktopRecoveryRequest, sanitizeDesktopNativeOutput, updateDesktop, validateDesktopFleetPlan, verifyDesktopReceipt, verifyDesktopRejectedLaunchProof, verifyDesktopReleaseTarget, verifyReviewedDesktopStash, verifyExistingDesktopReleaseReadOnly } from '../src/adapters/desktop-update';
import type { DesktopArtifact, DesktopSnapshot, DesktopNativeRequest, DesktopUpdateTarget } from '../src/adapters/desktop-update';
import type { ExistingDesktopReleaseProof } from '../src/adapters/desktop-update';

const DEFAULT_DESKTOP_UPDATE_TARGET: DesktopUpdateTarget = { ...CONFIGURED_DESKTOP_UPDATE_TARGET, branch: 'main', channel: undefined,
  expectedReleaseSha: undefined, expectedReleaseTag: undefined, allowedPreUpdateDiff: undefined };
const stableTarget: DesktopUpdateTarget = { ...DEFAULT_DESKTOP_UPDATE_TARGET, channel: 'stable', expectedReleaseSha: '2'.repeat(40), expectedReleaseTag: 'v0.22.0' };

const beforeSha = '1'.repeat(40), completedSha = '2'.repeat(40), runId = '5db16921-1a27-4dc9-ac98-6f223f7b2098';
const updateId = '045b24c2e9b247d79b36e68d01f1ca16';
const startedAtUtc = '2026-10-08T00:00:00.000Z';
const provenance = { archivePath: '/root/logs/update_receipts/update_20261008_000001_123_' + updateId + '.json', sha256: 'a'.repeat(64), byteLength: 400 };
function artifact(): DesktopArtifact {
  return { commit: completedSha, builtAt: '2026-10-08T00:00:02.000Z', dirty: false, source: 'local', updateMechanism: 'self', payload: 'bootstrap', sha256: 'b'.repeat(64), path: '/fixture/stamp.json' };
}
function snapshot(): DesktopSnapshot {
  return { schemaVersion: 1, targetId: 'fixture', observedAtUtc: '2026-10-08T00:00:04.000Z', checkoutSha: completedSha, checkoutDirty: false,
    checkoutStamp: artifact(), installedArtifact: artifact(), processes: [], nativeUserHomeWindows: 'C:\\Users\\fixture', nativeHomeWindows: 'C:\\Users\\fixture\\hermes',
    nativeHomeVerified: true, nativeScript: { path: '/fixture/windows.ps1', sha256: 'c'.repeat(64), automaticCorrelation: true },
    marker: { present: false, runId: null }, runningArtifact: null };
}
function receipt(overrides: Record<string, unknown> = {}) {
  return normalizeDesktopReceipt({ schema: 1, update_id: updateId, correlation_id: runId, action_id: null, started_at: '2026-10-08T00:00:01.000Z',
    finished_at: '2026-10-08T00:00:03.000Z', outcome: 'success', pre_update: { sha: beforeSha }, post_update: { sha: completedSha },
    stages: [{ name: 'build', outcome: 'success' }, { name: 'restart', outcome: 'success' }], followups: [], fleet: [], gateway_restart: {}, ...overrides }, provenance);
}
const expected = { correlationId: runId, startedAtUtc, preSha: beforeSha };

describe('Native Desktop completion identity', () => {
  test('adopts actual native post SHA only after correlated installed artifact proof', () => {
    const result = verifyDesktopReceipt(receipt(), snapshot(), expected);
    expect(result).toMatchObject({ eligibleForBackend: true, canonicalSha: completedSha, installedAligned: true, runningVerified: false, runningState: 'not-running' });
    expect(result.reasons).toEqual([]);
  });
  test('a successful correlated update to another SHA cannot pin the requested release', () => {
    const result = verifyDesktopReceipt(receipt(), snapshot(), { ...expected, expectedReleaseSha: beforeSha });
    expect(result.reasons).toContain('requested-release-sha-mismatch');
    expect(result.canonicalSha).toBeNull(); expect(result.eligibleForBackend).toBe(false);
  });
  test('distinguishes a running process from a verified running artifact', () => {
    const current = snapshot(); current.processes = [{ pid: 10, executablePath: 'C:\\Hermes.exe', createdAtUtc: startedAtUtc }];
    expect(verifyDesktopReceipt(receipt(), current, expected)).toMatchObject({ eligibleForBackend: true, runningState: 'unverified', runningVerified: false });
    current.runningArtifact = { commit: completedSha, bundleSwapPending: false, bundleOutOfSync: false };
    expect(verifyDesktopReceipt(receipt(), current, expected)).toMatchObject({ runningVerified: true, runningState: 'verified' });
  });
  test('rejects old renderer even when checkout and disk are current', () => {
    const current = snapshot(); current.processes = [{ pid: 10, executablePath: 'C:\\Hermes.exe', createdAtUtc: startedAtUtc }];
    current.runningArtifact = { commit: beforeSha, bundleSwapPending: true, bundleOutOfSync: true };
    expect(verifyDesktopReceipt(receipt(), current, expected).eligibleForBackend).toBe(false);
  });
  test('a small Windows/WSL clock offset does not discard the exact correlated run', () => {
    const shifted = receipt({ started_at: '2026-10-07T23:59:58.000Z' });
    expect(verifyDesktopReceipt(shifted, snapshot(), expected).eligibleForBackend).toBe(true);
    expect(verifyDesktopReceipt(shifted, snapshot(), { ...expected, correlationId: 'different-native-run' }).eligibleForBackend).toBe(false);
  });
  test('clock tolerance cannot admit an old correlated receipt outside its bound', () => {
    expect(verifyDesktopReceipt(receipt({ started_at: '2026-10-07T23:59:54.000Z' }), snapshot(), expected).eligibleForBackend).toBe(false);
  });
  test.each([
    ['wrong correlation', { correlation_id: 'another-operation' }], ['missing correlation', { correlation_id: null }],
    ['wrong source', { pre_update: { sha: '3'.repeat(40) } }], ['running receipt', { finished_at: null, outcome: 'running' }],
    ['partial', { outcome: 'partial' }], ['failed', { outcome: 'failed' }], ['stale receipt', { started_at: '2026-10-07T00:00:00Z' }],
    ['finish before start', { finished_at: '2026-10-07T00:00:00Z' }], ['non SHA', { post_update: { sha: 'latest' } }],
  ])('%s cannot establish success', (_, overrides) => {
    const result = verifyDesktopReceipt(receipt(overrides), snapshot(), expected);
    expect(result.eligibleForBackend).toBe(false); expect(result.canonicalSha).toBeNull();
  });
  test('latest pointer is not the run archive', () => {
    const parsed = receipt(); parsed.archivePath = '/root/logs/update_receipts/latest.json';
    expect(verifyDesktopReceipt(parsed, snapshot(), expected).reasons).toContain('not-run-specific-archive');
  });
  test.each([
    ['build followup', { followups: [{ step: 'desktop_build', reason: 'synthetic' }] }],
    ['failed stage', { stages: [{ name: 'build', outcome: 'failed' }] }],
    ['restart incomplete', { gateway_restart: { incomplete: true } }],
    ['failed unit', { gateway_restart: { failed_units: ['fixture'] } }],
    ['stale runtime', { gateway_restart: { fresh_recovery: { stale_runtimes: [{ pid: 123 }] } } }],
    ['failed serve', { gateway_restart: { fresh_recovery: { serve_units: { failed: ['fixture'] } } } }],
    ['restart pending', { fleet: [{ state: 'restart_pending' }] }],
    ['unknown identity', { fleet: [{ state: 'unknown' }] }],
    ['user action', { user_action: { step: 'desktop' } }],
  ])('exit-zero success with %s still owes work', (_, overrides) => {
    expect(verifyDesktopReceipt(receipt(overrides), snapshot(), expected).eligibleForBackend).toBe(false);
  });
  test.each(['dirty-tree', 'wrong-stamp', 'wrong-artifact', 'fallback', 'home-mismatch'])('%s cannot establish installed alignment', reason => {
    const current = snapshot();
    if (reason === 'dirty-tree') current.checkoutDirty = true;
    if (reason === 'wrong-stamp') current.checkoutStamp.commit = beforeSha;
    if (reason === 'wrong-artifact') current.installedArtifact.commit = beforeSha;
    if (reason === 'fallback') current.installedArtifact.source = 'fallback';
    if (reason === 'home-mismatch') current.nativeHomeVerified = false;
    expect(verifyDesktopReceipt(receipt(), current, expected).eligibleForBackend).toBe(false);
  });
  test('receipt projection excludes credentials, argv and arbitrary step text', () => {
    const projected = receipt({ argv: ['synthetic-secret'], arbitrary: 'synthetic-secret', steps: [{ name: 'build', ok: true, detail: 'synthetic-secret' }] });
    expect(JSON.stringify(projected)).not.toContain('synthetic-secret');
  });
});

describe('Planned reviewed native stash disposition', () => {
  const stashSha = 'f'.repeat(40), allowed = { sha256: 'd'.repeat(64), paths: ['apps/desktop/electron/app-version.ts'] };
  const plannedStep = { name: 'local_changes_stash', ok: false, detail: 'parked: ' + stashSha + ' (--keep-stash)' };
  const proof = { stashSha, baseSha: beforeSha, diffSha256: allowed.sha256, paths: [...allowed.paths], receiptSha256: provenance.sha256, observedAtUtc: '2026-10-08T00:00:04Z' };
  test('raw false step stays observed and only verified planned stash disposition is discharged', () => {
    const observed = receipt({ steps: [plannedStep] });
    expect(observed.parkedLocalChanges).toEqual({ ok: false, detail: plannedStep.detail, stashSha });
    expect(verifyDesktopReceipt(observed, snapshot(), expected).eligibleForBackend).toBe(false);
    const verified = verifyDesktopReceipt(observed, snapshot(), { ...expected, allowedPreUpdateDiff: allowed, reviewedStash: proof });
    expect(verified.eligibleForBackend).toBe(true); expect(verified.dischargedOwed).toEqual(['step:local_changes_stash']);
    expect(observed.owed).toContain('step:local_changes_stash');
  });
  test('wrong base, diff, paths, receipt or stash identity cannot discharge the native false step', () => {
    const observed = receipt({ steps: [plannedStep] });
    for (const change of [{ baseSha: completedSha }, { diffSha256: 'e'.repeat(64) }, { paths: ['unexpected.ts'] }, { receiptSha256: 'e'.repeat(64) }, { stashSha: beforeSha }]) {
      expect(verifyDesktopReceipt(observed, snapshot(), { ...expected, allowedPreUpdateDiff: allowed, reviewedStash: { ...proof, ...change } }).eligibleForBackend).toBe(false);
    }
    expect(verifyDesktopReceipt(observed, snapshot(), { ...expected, reviewedStash: proof }).eligibleForBackend).toBe(false);
  });
  test('other parked states, duplicate false rows, other false steps and dirty final source remain failures', () => {
    for (const detail of ['parked: ' + stashSha, 'parked: ' + stashSha + ' (--auto-stash)', plannedStep.detail + ' ']) {
      const observed = receipt({ steps: [{ ...plannedStep, detail }] });
      expect(verifyDesktopReceipt(observed, snapshot(), { ...expected, allowedPreUpdateDiff: allowed, reviewedStash: proof }).eligibleForBackend).toBe(false);
    }
    for (const steps of [[plannedStep, plannedStep], [plannedStep, { name: 'build', ok: false }]]) {
      expect(verifyDesktopReceipt(receipt({ steps }), snapshot(), { ...expected, allowedPreUpdateDiff: allowed, reviewedStash: proof }).eligibleForBackend).toBe(false);
    }
    const dirty = snapshot(); dirty.checkoutDirty = true;
    expect(verifyDesktopReceipt(receipt({ steps: [plannedStep] }), dirty, { ...expected, allowedPreUpdateDiff: allowed, reviewedStash: proof }).eligibleForBackend).toBe(false);
  });
  test('an active native marker prevents stash Git readback before any Windows probe', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'reviewed-stash-active-'));
    await writeFile(join(directory, '.hermes-update-in-progress'), 'run:' + runId + '\n');
    try {
      await expect(verifyReviewedDesktopStash({ ...stableTarget, nativeHomeWsl: directory, powershellExecutable: '/fixture-must-not-execute', allowedPreUpdateDiff: allowed }, receipt({ steps: [plannedStep] }), beforeSha, directory)).rejects.toMatchObject({ code: 'native-update-still-active-or-unresolved' });
    } finally { await rm(directory, { recursive: true }); }
  });
});

describe('Read-only existing Desktop release verification', () => {
  function fixture() {
    const before = snapshot(), after = snapshot(), calls: string[] = [], proofs: ExistingDesktopReleaseProof[] = [];
    before.targetId = stableTarget.id; after.targetId = stableTarget.id;
    let observations = 0;
    const readers = { async inspect() { calls.push('inspect'); return observations++ ? after : before; },
      async verifyInstalled() { calls.push('native-artifact-verification'); }, async saveProof(proof: ExistingDesktopReleaseProof) { calls.push('save-read-only-proof'); proofs.push(proof); },
      async update() { calls.push('updater-must-never-run'); } };
    return { before, after, calls, proofs, readers };
  }
  test('an exact clean release verifies native artifacts and unchanged identity without an updater receipt or writer', async () => {
    const f = fixture(); expect(await verifyExistingDesktopReleaseReadOnly(stableTarget, f.readers)).toEqual(f.after);
    expect(f.calls).toEqual(['inspect', 'native-artifact-verification', 'inspect', 'save-read-only-proof']);
    expect(f.proofs[0]).toMatchObject({ kind: 'verified-existing-desktop-release', readOnly: true, updaterInvoked: false, runningAlignment: 'not-running', expectedReleaseSha: completedSha });
    expect(f.proofs[0]).not.toHaveProperty('receipt');
  });
  test('missing running SHA remains explicitly unverified while known skew refuses reuse', async () => {
    const f = fixture(); f.before.processes = f.after.processes = [{ pid: 10, executablePath: 'C:\\Hermes.exe', createdAtUtc: startedAtUtc }];
    await verifyExistingDesktopReleaseReadOnly(stableTarget, f.readers); expect(f.proofs[0]?.runningAlignment).toBe('unverified');
    const skewed = fixture(); skewed.before.runningArtifact = { commit: beforeSha, bundleSwapPending: false, bundleOutOfSync: false };
    await expect(verifyExistingDesktopReleaseReadOnly(stableTarget, skewed.readers)).rejects.toThrow('running artifact');
    expect(skewed.calls).toEqual(['inspect']);
  });
  test('wrong source, dirty artifacts, fallback identity and active markers cannot enter native verification', async () => {
    for (const change of ['source', 'dirty-tree', 'dirty-stamp', 'fallback', 'marker', 'home']) {
      const f = fixture();
      if (change === 'source') f.before.checkoutSha = beforeSha;
      if (change === 'dirty-tree') f.before.checkoutDirty = true;
      if (change === 'dirty-stamp') f.before.installedArtifact.dirty = true;
      if (change === 'fallback') f.before.installedArtifact.source = 'fallback';
      if (change === 'marker') f.before.marker.present = true;
      if (change === 'home') f.before.nativeHomeVerified = false;
      await expect(verifyExistingDesktopReleaseReadOnly(stableTarget, f.readers)).rejects.toThrow('strictly aligned');
      expect(f.calls).toEqual(['inspect']);
    }
  });
  test('native integrity failure or artifact/process drift cannot produce a reuse proof', async () => {
    const failed = fixture(); failed.readers.verifyInstalled = async () => { throw new Error('native integrity failed'); };
    await expect(verifyExistingDesktopReleaseReadOnly(stableTarget, failed.readers)).rejects.toThrow('native integrity failed'); expect(failed.proofs).toEqual([]);
    for (const drift of ['artifact', 'process']) {
      const f = fixture();
      if (drift === 'artifact') f.after.installedArtifact.sha256 = 'e'.repeat(64);
      else f.after.processes = [{ pid: 11, executablePath: 'C:\\Hermes.exe', createdAtUtc: startedAtUtc }];
      await expect(verifyExistingDesktopReleaseReadOnly(stableTarget, f.readers)).rejects.toThrow('identity changed'); expect(f.proofs).toEqual([]);
    }
  });
  test('reuse always requires an explicit exact release before any read operation', async () => {
    const f = fixture(); await expect(verifyExistingDesktopReleaseReadOnly(DEFAULT_DESKTOP_UPDATE_TARGET, f.readers)).rejects.toThrow('exact release');
    expect(f.calls).toEqual([]);
  });
});

describe('Transient native archive polling', () => {
  async function fixture() {
    const home = await mkdtemp(join(tmpdir(), 'desktop-archive-poll-')), directory = join(home, 'logs/update_receipts'); await mkdir(directory, { recursive: true });
    const counts = { lists: 0, reads: 0, audits: 0 }, target = { ...DEFAULT_DESKTOP_UPDATE_TARGET, nativeHomeWsl: home };
    const write = async (name: string, overrides: Record<string, unknown> = {}) => {
      const path = join(directory, name);
      await writeFile(path, JSON.stringify({ schema: 1, update_id: updateId, correlation_id: runId, started_at: startedAtUtc,
        finished_at: null, outcome: 'running', pre_update: { sha: beforeSha }, post_update: { sha: completedSha }, ...overrides })); return path;
    };
    const poller = createDesktopReceiptPoller(runId, {
      async list() { counts.lists += 1; return (await readdir(directory)).sort().map(name => join(directory, name)); },
      async read(path) { counts.reads += 1; try { return await readFile(path); } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; } },
      async audit() { counts.audits += 1; return archivedReceipt(target, runId); },
    });
    return { home, counts, write, poller };
  }
  const finished = { finished_at: '2026-10-08T00:00:03Z', outcome: 'success' };
  test('active-to-terminal polling rereads one cached path and its current raw hash', async () => {
    const f = await fixture();
    try {
      await f.write('update_00_foreign.json', { correlation_id: 'unrelated' }); await f.write('update_01_foreign.json', { correlation_id: 'unrelated' });
      await f.write('update_02_active.json'); for (let index = 3; index < 20; index++) await f.write('update_' + index + '_foreign.json', { correlation_id: 'unrelated' });
      expect(await f.poller.poll()).toBeNull(); for (let count = 0; count < 50; count++) expect(await f.poller.poll()).toBeNull();
      expect(f.counts).toEqual({ lists: 1, reads: 53, audits: 0 });
      await f.write('update_02_active.json', finished); const first = await f.poller.poll(); expect(first?.outcome).toBe('success');
      await f.write('update_02_active.json', { ...finished, steps: [{ name: 'build', ok: false }] });
      const changed = await f.poller.poll(); expect(changed?.sha256).not.toBe(first?.sha256); expect(changed?.owed).toContain('step:build');
    } finally { await rm(f.home, { recursive: true }); }
  });
  test('cached deletion and correlation drift reset discovery without returning stale bytes', async () => {
    const f = await fixture();
    try {
      const original = await f.write('update_00_active.json'); expect(await f.poller.poll()).toBeNull();
      await f.write('update_00_active.json', { ...finished, correlation_id: 'another-operation' });
      const replacement = await f.write('update_01_replacement.json', finished); expect((await f.poller.poll())?.archivePath).toBe(replacement);
      await rm(replacement); const last = await f.write('update_02_last.json', finished); expect((await f.poller.poll())?.archivePath).toBe(last);
      expect(f.counts.lists).toBe(3); expect(original).not.toBe(last);
    } finally { await rm(f.home, { recursive: true }); }
  });
  test('settled full audit finds a terminal archive even when a cached path stays unfinished', async () => {
    const f = await fixture();
    try {
      await f.write('update_00_active.json'); expect(await f.poller.poll()).toBeNull();
      const terminal = await f.write('update_01_terminal.json', finished); expect(await f.poller.poll()).toBeNull();
      expect((await f.poller.poll({ fullAudit: true }))?.archivePath).toBe(terminal); expect(f.counts.audits).toBe(1);
    } finally { await rm(f.home, { recursive: true }); }
  });
  test('an authoritative settled audit rejects duplicate terminal archives despite a cached match', async () => {
    const f = await fixture();
    try {
      await f.write('update_00_terminal.json', finished); expect((await f.poller.poll())?.outcome).toBe('success');
      await f.write('update_01_duplicate.json', { ...finished, update_id: 'another-native-receipt' });
      await expect(f.poller.poll({ fullAudit: true })).rejects.toThrow('More than one terminal archive');
    } finally { await rm(f.home, { recursive: true }); }
  });
});

describe('Native writer and footprint', () => {
  test.skipIf(!Bun.file(DEFAULT_DESKTOP_UPDATE_TARGET.powershellExecutable).size)('generated native script parameters bind by name on Windows PowerShell', async () => {
    const target = { ...stableTarget, installRootWindows: 'C:\\codex-task-owned-binding-fixture' };
    const native = buildDesktopNativeCommand(target, runId);
    const invocation = "& '" + target.installRootWindows + "\\scripts\\desktop-update\\windows.ps1'";
    expect(native.powershellSource).toContain(invocation);
    const source = "function Invoke-NativeBindingFixture { param([string]$InstallRoot,[string]$Branch,[ValidateSet('stable','canary','main')][string]$Channel,[switch]$NoGateway,[switch]$NoUi,[string]$RelaunchExe); @{root=$InstallRoot;channel=$Channel;noGateway=[bool]$NoGateway;noUi=[bool]$NoUi;relaunch=$RelaunchExe;unbound=$args.Count}|ConvertTo-Json -Compress };\n"
      + native.powershellSource.replace(invocation, 'Invoke-NativeBindingFixture').replace('exit $LASTEXITCODE;', 'exit 0;');
    const child = Bun.spawn([native.command, '-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(source, 'utf16le').toString('base64')], { stdout: 'pipe', stderr: 'pipe', timeout: 10000 });
    const [stdout, , exitCode] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    expect(exitCode).toBe(0);
    expect(JSON.parse(stdout.trim())).toEqual({ root: target.installRootWindows, channel: 'stable', noGateway: true, noUi: true, relaunch: target.installRootWindows + '\\apps\\desktop\\release\\win-unpacked\\Hermes.exe', unbound: 0 });
  });
  test('coordinator diagnostics retain binding failures while removing credential-shaped values', () => {
    const output = sanitizeDesktopNativeOutput('ParameterBindingValidationException Channel=-Channel password=hunter2 Authorization: Bearer abc123 https://user:pass@example.test/build?signature=secret');
    expect(output).toContain('ParameterBindingValidationException'); expect(output).toContain('Channel=-Channel');
    for (const secret of ['hunter2', 'abc123', 'user:pass', 'signature=secret']) expect(output).not.toContain(secret);
  });
  test('stable native command selects Channel without passing Branch or an arbitrary SHA', () => {
    const command = buildDesktopNativeCommand(stableTarget, runId);
    expect(command.nativeArguments).toContain('-Channel'); expect(command.nativeArguments).toContain('stable');
    expect(command.nativeArguments).not.toContain('-Branch'); expect(command.nativeArguments).not.toContain('--sha');
    expect(() => buildDesktopNativeCommand({ ...stableTarget, expectedReleaseSha: undefined }, runId)).toThrow('exact release');
  });
  test('native stable resolver must match the exact official release before invocation', () => {
    const resolved = { requestedChannel: 'stable', channel: 'stable', repository: 'NousResearch/hermes-agent', commit: completedSha, version: '0.22.0' };
    expect(() => verifyDesktopReleaseTarget(resolved, stableTarget)).not.toThrow();
    for (const changed of [{ commit: beforeSha }, { version: '0.22.1' }, { channel: 'canary' }, { repository: 'other/hermes-agent' }]) {
      expect(() => verifyDesktopReleaseTarget({ ...resolved, ...changed }, stableTarget)).toThrow('requested release');
    }
  });
  test('constructs the official coordinator with exact roots and caller correlation', () => {
    const native = buildDesktopNativeCommand(DEFAULT_DESKTOP_UPDATE_TARGET, runId);
    expect(native.nativeArguments).toContain('-NoGateway'); expect(native.nativeArguments).toContain('-NoUi');
    expect(native.powershellSource).toContain(DEFAULT_DESKTOP_UPDATE_TARGET.installRootWindows + '\\scripts\\desktop-update\\windows.ps1');
    expect(native.args).toContain('-EncodedCommand');
    expect(Buffer.from(native.args.at(-1)!, 'base64').toString('utf16le')).toBe(native.powershellSource);
    expect(native.powershellSource).toContain("$env:HERMES_HOME='" + DEFAULT_DESKTOP_UPDATE_TARGET.nativeHomeWindows + "'");
    expect(native.powershellSource).toContain("$env:HERMES_UPDATE_CORRELATION_ID='" + runId + "'");
    expect(native.environment.HERMES_UPDATE_CORRELATION_ID).toBe(runId);
    expect(native.environment.HERMES_HOME).toBe(DEFAULT_DESKTOP_UPDATE_TARGET.nativeHomeWindows);
    expect(native.args).not.toContain('--sha'); expect(native.args).not.toContain('-Sha'); expect(native.args).not.toContain('--force-venv');
  });
  test('untrusted shell syntax stays a literal argv element', () => {
    const target = { ...DEFAULT_DESKTOP_UPDATE_TARGET, installRootWindows: "C:\\fixture\\literal'$()&path" };
    const native = buildDesktopNativeCommand(target, runId);
    expect(native.nativeArguments[native.nativeArguments.indexOf('-InstallRoot') + 1]).toBe(target.installRootWindows);
    expect(native.powershellSource).toContain("C:\\fixture\\literal''$()&path");
    expect(native.command).toBe(DEFAULT_DESKTOP_UPDATE_TARGET.powershellExecutable);
  });
  test('rejects an invalid correlation or non-main target', () => {
    expect(() => buildDesktopNativeCommand(DEFAULT_DESKTOP_UPDATE_TARGET, 'bad;injection')).toThrow();
    expect(() => buildDesktopNativeCommand({ ...DEFAULT_DESKTOP_UPDATE_TARGET, branch: 'other' as 'main' }, runId)).toThrow();
  });
  test('does not infer NoGateway eliminated existing native gateway effects', () => {
    const plan = { install_method: 'git', updatable_in_place: true, expected_sha: beforeSha, runtimes: [{ kind: 'gateway', supervisor: 'windows-service', pid: 33 }] };
    expect(validateDesktopFleetPlan(plan, DEFAULT_DESKTOP_UPDATE_TARGET, beforeSha)).toContain('unapproved-native-runtime:gateway:windows-service');
  });
  test('admits only the declared Desktop-owned serve footprint and verifies plan identity', () => {
    const plan = { install_method: 'git', updatable_in_place: true, expected_sha: beforeSha, runtimes: [{ kind: 'serve', supervisor: 'desktop', pid: 33 }] };
    expect(validateDesktopFleetPlan(plan, DEFAULT_DESKTOP_UPDATE_TARGET, beforeSha)).toEqual([]);
    expect(validateDesktopFleetPlan({ ...plan, expected_sha: completedSha }, DEFAULT_DESKTOP_UPDATE_TARGET, beforeSha)).toContain('native-plan-identity-unverified');
    expect(validateDesktopFleetPlan({}, DEFAULT_DESKTOP_UPDATE_TARGET, beforeSha)).toContain('native-plan-identity-unverified');
  });
});

describe('Reviewed pre-update local edits', () => {
  const allowed = { sha256: 'd'.repeat(64), paths: ['apps/desktop/electron/app-version.ts'] };
  function dirty() {
    const current = snapshot(); current.checkoutDirty = true; current.checkoutDiff = { sha256: allowed.sha256, paths: [...allowed.paths] };
    current.checkoutStamp.dirty = true; current.installedArtifact.dirty = true; return current;
  }
  test('only the exact reviewed closed-app tree is eligible, and it cannot establish completion', () => {
    const target = { ...stableTarget, allowedPreUpdateDiff: allowed }, current = dirty();
    expect(desktopPreUpdateEligibility(target, current).eligible).toBe(true);
    expect(desktopPreUpdateEligibility(stableTarget, current).eligible).toBe(false);
    expect(verifyDesktopReceipt(receipt(), current, expected).eligibleForBackend).toBe(false);
    current.checkoutDiff!.sha256 = 'e'.repeat(64);
    expect(desktopPreUpdateEligibility(target, current).eligible).toBe(false);
  });
  test('extra paths or a running app cannot widen the reviewed diff boundary', () => {
    const target = { ...stableTarget, allowedPreUpdateDiff: allowed }, current = dirty();
    current.checkoutDiff!.paths.push('unexpected.txt'); expect(desktopPreUpdateEligibility(target, current).eligible).toBe(false);
    current.checkoutDiff!.paths = [...allowed.paths]; current.processes = [{ pid: 10, executablePath: 'C:\\Hermes.exe', createdAtUtc: startedAtUtc }];
    expect(desktopPreUpdateEligibility(target, current).eligible).toBe(false);
  });
  test('untracked source is rejected even if tracked diff hash and paths match', () => {
    const current = dirty(); current.checkoutDiff!.untrackedPaths = ['unreviewed.ts'];
    expect(desktopPreUpdateEligibility({ ...stableTarget, allowedPreUpdateDiff: allowed }, current).reasons).toContain('unreviewed-untracked-source');
  });
});

describe('Interrupted native operation recovery contract', () => {
  function request(): DesktopNativeRequest {
    return { schemaVersion: 1, operationId: runId, targetId: DEFAULT_DESKTOP_UPDATE_TARGET.id, mode: 'closed-script', correlationId: runId,
      preSha: beforeSha, startedAtUtc, authorization: 'fixture-current-authorization', installRootWindows: DEFAULT_DESKTOP_UPDATE_TARGET.installRootWindows,
      nativeHomeWindows: DEFAULT_DESKTOP_UPDATE_TARGET.nativeHomeWindows, scope: { branch: 'main', noGateway: true,
        nativeFleetPlan: { install_method: 'git', updatable_in_place: true, expected_sha: beforeSha, runtimes: [] } } };
  }
  test('an interrupted closed operation retains the pre-effect canonical native request', () => {
    const prior = request(); const selected = resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET);
    expect(selected.request).toEqual(prior); expect(selected.correlationId).toBe(runId); expect(selected.reasons).toEqual([]);
  });
  test('stable recovery retains release selection and rejects changed channel or requested SHA', () => {
    const prior = request(); prior.scope = { channel: 'stable', expectedReleaseSha: completedSha, expectedReleaseTag: 'v0.22.0', noGateway: true, nativeFleetPlan: prior.scope.nativeFleetPlan };
    expect(resolveDesktopRecoveryRequest([prior], stableTarget).correlationId).toBe(runId);
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET).reasons).toContain('recovery-scope-mismatch');
    expect(resolveDesktopRecoveryRequest([prior], { ...stableTarget, expectedReleaseSha: beforeSha }).request).toBeNull();
  });
  test('a rejected native launch retry requires unchanged identity and before-body binding proof', () => {
    const original = request(); original.scope = { ...original.scope, branch: undefined, channel: 'stable', expectedReleaseSha: completedSha, expectedReleaseTag: 'v0.22.0' };
    const current = snapshot(); current.checkoutSha = beforeSha; current.checkoutStamp.commit = beforeSha; current.installedArtifact.commit = beforeSha;
    const prior = { operationId: runId, status: 'failed', before: current, after: null, receipt: null, error: { code: 'native-coordinator-failed' }, executionBoundary: 'Native updater invoked; verification is separately recorded' };
    const proof = { schema: 'vps-operations.hermes-native-script-binding-check.v1', readOnly: true, correlationId: runId, nativeRequestStartedAt: startedAtUtc,
      originalNativeScriptSha256: current.nativeScript.sha256, verification: { passed: true },
      binding: { arraySplat: { bodyReached: false, error: { type: 'System.Management.Automation.ParameterBindingValidationException', id: 'ParameterArgumentValidationError' } },
        namedSplat: { bodyReached: true, error: null, value: { installRoot: original.installRootWindows, channel: 'stable', noUi: true, noGateway: true } }, originalNativeScriptInvoked: false },
      currentNativeState: { marker: { present: false }, correlatedReceiptCount: 0, nativeResult: { matchesCorrelation: false, modifiedAt: '2026-10-07T00:00:00Z' }, handoffLog: { modifiedAt: '2026-10-07T00:00:00Z' } } };
    expect(verifyDesktopRejectedLaunchProof(proof, original, prior, current)).toEqual([]);
    proof.binding.arraySplat.bodyReached = true;
    expect(verifyDesktopRejectedLaunchProof(proof, original, prior, current)).toContain('native-before-body-rejection-unproven');
    proof.binding.arraySplat.bodyReached = false; current.nativeScript.sha256 = 'e'.repeat(64);
    expect(verifyDesktopRejectedLaunchProof(proof, original, prior, current)).toContain('rejected-launch-install-identity-changed');
  });
  test('does not guess an unknown open handoff from time or a later receipt', () => {
    const prior = request(); prior.mode = 'open-ipc'; prior.correlationId = null;
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET).reasons).toContain('interrupted-open-handoff-correlation-unresolved');
  });
  test('requires an acknowledged checkpoint owned by the same open operation and target', () => {
    const prior = request(); prior.mode = 'open-ipc'; prior.correlationId = null;
    const checkpoint = { operationId: runId, targetId: prior.targetId, correlationId: 'de39aac1-e5e8-4a7b-ab98-66b32d0bd1ab', nativeHandoffAcknowledged: true };
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET, [checkpoint]).correlationId).toBe(checkpoint.correlationId);
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET, [{ ...checkpoint, nativeHandoffAcknowledged: false }]).request).toBeNull();
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET, [{ ...checkpoint, targetId: 'other' }]).request).toBeNull();
  });
  test('ambiguous previous requests cannot silently start a new operation', () => {
    expect(resolveDesktopRecoveryRequest([], DEFAULT_DESKTOP_UPDATE_TARGET).request).toBeNull();
    expect(resolveDesktopRecoveryRequest([request(), request()], DEFAULT_DESKTOP_UPDATE_TARGET).request).toBeNull();
  });
  test('rejects altered target, missing authorization context and unapproved historical fleet', () => {
    expect(resolveDesktopRecoveryRequest([{ ...request(), targetId: 'other' }], DEFAULT_DESKTOP_UPDATE_TARGET).request).toBeNull();
    expect(resolveDesktopRecoveryRequest([{ ...request(), authorization: '' }], DEFAULT_DESKTOP_UPDATE_TARGET).request).toBeNull();
    const prior = request(); prior.scope.nativeFleetPlan.runtimes = [{ kind: 'gateway', supervisor: 'manual' }];
    expect(resolveDesktopRecoveryRequest([prior], DEFAULT_DESKTOP_UPDATE_TARGET).request).toBeNull();
  });
  test('a durable previous request blocks a new updater before even inspecting Windows', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'desktop-request-fixture-'));
    const path = join(directory, 'desktop-native-request-' + runId + '.json');
    const bytes = JSON.stringify(request()); await writeFile(path, bytes);
    try {
      const target = { ...DEFAULT_DESKTOP_UPDATE_TARGET, installRootWsl: '/fixture-not-a-windows-install' };
      const result = await updateDesktop(target, directory, { authorization: 'fixture' });
      expect(result.error?.code).toBe('desktop-native-recovery-required');
      expect(result.executionBoundary).toBe('Native mutation not executed');
      expect(await readFile(path, 'utf8')).toBe(bytes);
    } finally { await rm(directory, { recursive: true }); }
  });
  test('read-only recovery with no persisted operation refuses guessing or invoking a writer', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'desktop-recovery-fixture-'));
    try {
      const result = await recoverDesktopUpdate({ ...DEFAULT_DESKTOP_UPDATE_TARGET, installRootWsl: '/fixture-not-a-windows-install' }, directory);
      expect(result.status).toBe('blocked'); expect(result.error?.code).toBe('desktop-recovery-unresolved');
      expect(result.executionBoundary).toBe('Native mutation not executed'); expect(result.canonicalSha).toBeNull();
    } finally { await rm(directory, { recursive: true }); }
  });
  test.each([1, 2, 8, 124])('coordinator exit %i fails immediately even without a terminal archive', coordinatorExitCode => {
    expect(desktopNativePollState({ coordinatorExitCode, waitForCoordinator: true, terminalReceiptPresent: false, markerPresent: true })).toBe('native-failed');
  });
  test('a terminal source receipt does not discharge an active coordinator or marker', () => {
    expect(desktopNativePollState({ coordinatorExitCode: null, waitForCoordinator: true, terminalReceiptPresent: true, markerPresent: false })).toBe('waiting');
    expect(desktopNativePollState({ coordinatorExitCode: 0, waitForCoordinator: true, terminalReceiptPresent: true, markerPresent: true })).toBe('waiting');
    expect(desktopNativePollState({ coordinatorExitCode: 0, waitForCoordinator: true, terminalReceiptPresent: true, markerPresent: false })).toBe('settled');
  });
});
