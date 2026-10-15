import { createHash, randomUUID } from 'node:crypto';
import { lstat, mkdir, open, readFile, readdir, realpath } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import defaultTarget from '../../architecture/desktop-update-target.json';

export interface DesktopUpdateTarget {
  schemaVersion: 1;
  id: string;
  installRootWsl: string;
  installRootWindows: string;
  nativeHomeWsl: string;
  nativeHomeWindows: string;
  nativeUserHomeWindows: string;
  powershellExecutable: string;
  nativeGitExecutableWindows: string;
  playwrightProjectRootWsl: string;
  branch: 'main';
  channel?: 'stable';
  expectedReleaseSha?: string;
  expectedReleaseTag?: string;
  allowedPreUpdateDiff?: { sha256: string; paths: string[] };
  receiptTimeoutMs: number;
  allowedNativeRuntimes: Array<{ kind: string; supervisor: string }>;
}

export const DEFAULT_DESKTOP_UPDATE_TARGET: DesktopUpdateTarget = defaultTarget as DesktopUpdateTarget;
const FULL_SHA = /^[0-9a-f]{40}$/;
const DIGEST = /^[0-9a-f]{64}$/;
const RELEASE_TAG = /^v(?:0|[1-9]\d{0,2})\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;
const RUN_ID = /^[A-Za-z0-9-]{16,128}$/;
const sha256 = (data: string | Uint8Array): string => createHash('sha256').update(data).digest('hex');
const windowsPath = (value: string): string => value.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
const psQuote = (value: string): string => "'" + value.replaceAll("'", "''") + "'";
type Json = Record<string, unknown>;
const object = (value: unknown): Json => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Json : {};
const rows = (value: unknown): Json[] => Array.isArray(value) ? value.map(object) : [];
const text = (value: unknown): string | null => typeof value === 'string' && value ? value : null;

export interface DesktopArtifact {
  commit: string | null;
  builtAt: string | null;
  dirty: boolean | null;
  source: string | null;
  updateMechanism: string | null;
  payload: string | null;
  sha256: string;
  path: string;
}
export interface DesktopProcess { pid: number; executablePath: string; createdAtUtc: string }
export interface DesktopReceipt {
  schema: number;
  updateId: string | null;
  correlationId: string | null;
  actionId: string | null;
  startedAtUtc: string | null;
  finishedAtUtc: string | null;
  outcome: string | null;
  preSha: string | null;
  postSha: string | null;
  owed: string[];
  parkedLocalChanges?: { ok: false; detail: string; stashSha: string };
  archivePath: string;
  sha256: string;
  byteLength: number;
}
export interface DesktopSnapshot {
  schemaVersion: 1;
  observedAtUtc: string;
  targetId: string;
  checkoutSha: string;
  checkoutDirty: boolean;
  checkoutDiff?: { sha256: string; paths: string[]; untrackedPaths?: string[] };
  checkoutStamp: DesktopArtifact;
  installedArtifact: DesktopArtifact;
  processes: DesktopProcess[];
  nativeUserHomeWindows: string;
  nativeHomeWindows: string;
  nativeHomeVerified: boolean;
  nativeScript: { path: string; sha256: string; automaticCorrelation: boolean };
  marker: { present: boolean; runId: string | null };
  runningArtifact: { commit: string | null; bundleSwapPending: boolean | null; bundleOutOfSync: boolean | null } | null;
}
export interface DesktopIdentityVerification {
  eligibleForBackend: boolean;
  canonicalSha: string | null;
  installedAligned: boolean;
  runningVerified: boolean;
  runningState: 'verified' | 'not-running' | 'unverified';
  reasons: string[];
  dischargedOwed?: string[];
}
export interface DesktopReviewedStashVerification {
  stashSha: string;
  baseSha: string;
  diffSha256: string;
  paths: string[];
  receiptSha256: string;
  observedAtUtc: string;
}
export interface DesktopUpdateResult {
  schemaVersion: 1;
  operationId: string;
  startedAtUtc: string;
  completedAtUtc: string;
  status: 'blocked' | 'failed' | 'installed-only' | 'verified-running';
  canonicalSha: string | null;
  before: DesktopSnapshot | null;
  after: DesktopSnapshot | null;
  receipt: DesktopReceipt | null;
  verification: DesktopIdentityVerification | null;
  reviewedStash?: DesktopReviewedStashVerification | null;
  nativeFleetPlan: Json | null;
  observedMarker?: { present: boolean; runId: string | null };
  error: { code: string; message: string; recovery: string } | null;
  executionBoundary: 'Native mutation not executed' | 'Native updater invoked; verification is separately recorded';
}
export interface DesktopNativeRequest {
  schemaVersion: 1;
  operationId: string;
  targetId: string;
  mode: 'closed-script' | 'open-ipc';
  correlationId: string | null;
  preSha: string;
  startedAtUtc: string;
  authorization: string;
  installRootWindows: string;
  nativeHomeWindows: string;
  scope: { branch?: 'main'; channel?: 'stable'; expectedReleaseSha?: string; expectedReleaseTag?: string;
    allowedPreUpdateDiff?: { sha256: string; paths: string[] }; noGateway: true; nativeFleetPlan: Json };
}
export interface DesktopRejectedLaunchRetry { operationId: string; proofRef: string; proofSha256: string }

/** This narrow admission covers only the proven stable-channel parameter binder rejection. */
export function verifyDesktopRejectedLaunchProof(proofValue: unknown, request: DesktopNativeRequest, priorValue: unknown, current: DesktopSnapshot): string[] {
  const proof = object(proofValue), prior = object(priorValue), binding = object(proof.binding), array = object(binding.arraySplat), named = object(binding.namedSplat);
  const state = object(proof.currentNativeState), nativeResult = object(state.nativeResult), log = object(state.handoffLog), namedValue = object(named.value);
  const priorBefore = object(prior.before), priorScript = object(priorBefore.nativeScript), reasons: string[] = [];
  const requestTime = Date.parse(request.startedAtUtc), oldResultTime = Date.parse(String(nativeResult.modifiedAt)), oldLogTime = Date.parse(String(log.modifiedAt));
  if (proof.schema !== 'vps-operations.hermes-native-script-binding-check.v1' || proof.readOnly !== true
    || proof.correlationId !== request.correlationId || proof.nativeRequestStartedAt !== request.startedAtUtc
    || request.mode !== 'closed-script' || request.correlationId !== request.operationId || request.scope.channel !== 'stable') reasons.push('rejected-launch-proof-scope-mismatch');
  if (proof.originalNativeScriptSha256 !== current.nativeScript.sha256 || priorScript.sha256 !== current.nativeScript.sha256
    || current.checkoutSha !== request.preSha || priorBefore.checkoutSha !== request.preSha
    || object(priorBefore.installedArtifact).sha256 !== current.installedArtifact.sha256
    || object(priorBefore.checkoutStamp).sha256 !== current.checkoutStamp.sha256) reasons.push('rejected-launch-install-identity-changed');
  if (array.bodyReached !== false || object(array.error).type !== 'System.Management.Automation.ParameterBindingValidationException'
    || object(array.error).id !== 'ParameterArgumentValidationError' || named.bodyReached !== true || named.error !== null
    || binding.originalNativeScriptInvoked !== false || object(proof.verification).passed !== true
    || windowsPath(String(namedValue.installRoot)) !== windowsPath(request.installRootWindows) || namedValue.channel !== 'stable'
    || namedValue.noUi !== true || namedValue.noGateway !== true) reasons.push('native-before-body-rejection-unproven');
  if (prior.operationId !== request.operationId || prior.status !== 'failed' || object(prior.error).code !== 'native-coordinator-failed'
    || prior.receipt !== null || prior.after !== null || prior.executionBoundary !== 'Native updater invoked; verification is separately recorded') reasons.push('original-native-launch-outcome-unproven');
  if (object(state.marker).present !== false || state.correlatedReceiptCount !== 0 || nativeResult.matchesCorrelation !== false
    || !Number.isFinite(oldResultTime) || !Number.isFinite(oldLogTime) || oldResultTime >= requestTime || oldLogTime >= requestTime
    || current.marker.present) reasons.push('native-no-effect-state-unproven');
  return reasons;
}

function validateDesktopTarget(target: DesktopUpdateTarget): void {
  if (target.branch !== 'main' || (target.channel !== undefined && target.channel !== 'stable')) throw new Error('Invalid native Desktop update selection');
  if ((target.expectedReleaseSha === undefined) !== (target.expectedReleaseTag === undefined)
    || (target.expectedReleaseSha !== undefined && (!FULL_SHA.test(target.expectedReleaseSha) || !RELEASE_TAG.test(target.expectedReleaseTag!) || target.channel !== 'stable'))) throw new Error('An exact release requires its stable channel, full SHA and release tag');
  const allowed = target.allowedPreUpdateDiff;
  if (allowed && (!DIGEST.test(allowed.sha256) || !allowed.paths.length || new Set(allowed.paths).size !== allowed.paths.length
    || allowed.paths.some(path => !path || path.startsWith('/') || path.includes('\\') || path.split('/').some(part => part === '..' || part === '.' || !part)))) throw new Error('Invalid reviewed pre-update diff identity');
}

function desktopSelection(target: DesktopUpdateTarget): Pick<DesktopNativeRequest['scope'], 'branch' | 'channel' | 'expectedReleaseSha' | 'expectedReleaseTag' | 'allowedPreUpdateDiff'> {
  validateDesktopTarget(target);
  return { ...(target.channel ? { channel: target.channel } : { branch: target.branch }),
    ...(target.expectedReleaseSha ? { expectedReleaseSha: target.expectedReleaseSha, expectedReleaseTag: target.expectedReleaseTag } : {}),
    ...(target.allowedPreUpdateDiff ? { allowedPreUpdateDiff: { sha256: target.allowedPreUpdateDiff.sha256, paths: [...target.allowedPreUpdateDiff.paths].sort() } } : {}) };
}

/** A reviewed dirty tree is eligible only for the native updater's preserved-stash path. */
export function desktopPreUpdateEligibility(target: DesktopUpdateTarget, snapshot: DesktopSnapshot): { eligible: boolean; reasons: string[] } {
  validateDesktopTarget(target);
  const reasons: string[] = [];
  const aligned = FULL_SHA.test(snapshot.checkoutSha) && snapshot.checkoutSha === snapshot.checkoutStamp.commit
    && snapshot.checkoutSha === snapshot.installedArtifact.commit && snapshot.nativeHomeVerified
    && snapshot.installedArtifact.source !== 'fallback' && snapshot.installedArtifact.updateMechanism === 'self' && snapshot.installedArtifact.payload === 'bootstrap';
  if (!aligned) reasons.push('native-install-identity-mismatch');
  if (snapshot.checkoutDirty) {
    const allowed = target.allowedPreUpdateDiff, diff = snapshot.checkoutDiff;
    if (!allowed || !diff || diff.sha256 !== allowed.sha256
      || JSON.stringify([...diff.paths].sort()) !== JSON.stringify([...allowed.paths].sort())) reasons.push('unreviewed-dirty-checkout');
    if (diff?.untrackedPaths?.length) reasons.push('unreviewed-untracked-source');
    if (snapshot.processes.length) reasons.push('reviewed-dirty-update-requires-closed-desktop');
  } else if (snapshot.checkoutStamp.dirty !== false || snapshot.installedArtifact.dirty !== false) reasons.push('dirty-installed-artifact-without-reviewed-diff');
  if (snapshot.marker.present) reasons.push('native-update-marker-present');
  return { eligible: reasons.length === 0, reasons };
}

export function resolveDesktopRecoveryRequest(values: unknown[], target: DesktopUpdateTarget, checkpoints: unknown[] = []): { request: DesktopNativeRequest | null; correlationId: string | null; reasons: string[] } {
  if (values.length !== 1) return { request: null, correlationId: null, reasons: ['recovery-requires-exactly-one-native-request'] };
  const value = object(values[0]), scope = object(value.scope), reasons: string[] = [];
  const selectedScope = desktopSelection(target);
  if (value.schemaVersion !== 1 || !RUN_ID.test(String(value.operationId)) || value.targetId !== target.id
    || !['closed-script', 'open-ipc'].includes(String(value.mode)) || !FULL_SHA.test(String(value.preSha)) || !Number.isFinite(Date.parse(String(value.startedAtUtc)))) reasons.push('invalid-recovery-request');
  if (windowsPath(String(value.installRootWindows)) !== windowsPath(target.installRootWindows) || windowsPath(String(value.nativeHomeWindows)) !== windowsPath(target.nativeHomeWindows)
    || (scope.branch ?? null) !== (selectedScope.branch ?? null) || (scope.channel ?? null) !== (selectedScope.channel ?? null)
    || (scope.expectedReleaseSha ?? null) !== (selectedScope.expectedReleaseSha ?? null) || (scope.expectedReleaseTag ?? null) !== (selectedScope.expectedReleaseTag ?? null)
    || JSON.stringify(scope.allowedPreUpdateDiff ?? null) !== JSON.stringify(selectedScope.allowedPreUpdateDiff ?? null)
    || scope.noGateway !== true || !text(value.authorization)) reasons.push('recovery-scope-mismatch');
  reasons.push(...validateDesktopFleetPlan(scope.nativeFleetPlan, target, String(value.preSha)));
  let correlationId = text(value.correlationId);
  if (value.mode === 'closed-script' && correlationId !== value.operationId) reasons.push('closed-native-correlation-mismatch');
  if (value.mode === 'open-ipc') {
    const matches = checkpoints.map(object).filter(row => row.operationId === value.operationId && row.targetId === value.targetId);
    if (matches.length !== 1 || matches[0]?.nativeHandoffAcknowledged !== true || !RUN_ID.test(String(matches[0]?.correlationId))) reasons.push('interrupted-open-handoff-correlation-unresolved');
    else correlationId = String(matches[0]!.correlationId);
  }
  if (!correlationId || !RUN_ID.test(correlationId)) reasons.push('native-correlation-missing');
  return { request: reasons.length ? null : value as unknown as DesktopNativeRequest, correlationId: reasons.length ? null : correlationId, reasons };
}

export function normalizeDesktopReceipt(value: unknown, provenance: { archivePath: string; sha256: string; byteLength: number }): DesktopReceipt {
  const raw = object(value), restart = object(raw.gateway_restart), recovery = object(restart.fresh_recovery);
  const owed = rows(raw.followups).map(row => 'followup:' + (text(row.step) ?? 'unknown'));
  if (raw.user_action) owed.push('user-action');
  if (restart.incomplete === true) owed.push('gateway-restart-incomplete');
  if (Array.isArray(restart.failed_units) && restart.failed_units.length) owed.push('gateway-failed-units');
  if (text(restart.phase_error)) owed.push('gateway-phase-error');
  if (Array.isArray(recovery.failed) && recovery.failed.length) owed.push('native-recovery-failed');
  if (Array.isArray(recovery.stale_runtimes) && recovery.stale_runtimes.length) owed.push('stale-runtimes');
  if (Array.isArray(object(recovery.serve_units).failed) && (object(recovery.serve_units).failed as unknown[]).length) owed.push('serve-unit-failed');
  for (const row of rows(raw.fleet)) if (['stale', 'down', 'restart_pending', 'unknown'].includes(String(row.state))) owed.push('fleet:' + row.state);
  for (const row of rows(raw.stages)) if (['build', 'restart'].includes(String(row.name)) && ['failed', 'partial', 'interrupted'].includes(String(row.outcome))) owed.push('stage:' + row.name);
  for (const row of rows(raw.steps)) if (row.ok === false) owed.push('step:' + (text(row.name) ?? 'unknown'));
  const stashRows = rows(raw.steps).filter(row => row.name === 'local_changes_stash' && row.ok === false);
  const stashDetail = stashRows.length === 1 ? text(stashRows[0]!.detail) : null;
  const parked = stashDetail?.match(/^parked: ([0-9a-f]{40}) \(--keep-stash\)$/);
  return { schema: Number(raw.schema), updateId: text(raw.update_id), correlationId: text(raw.correlation_id), actionId: text(raw.action_id),
    startedAtUtc: text(raw.started_at), finishedAtUtc: text(raw.finished_at), outcome: text(raw.outcome),
    preSha: text(object(raw.pre_update).sha), postSha: text(object(raw.post_update).sha), owed: [...new Set(owed)],
    ...(parked ? { parkedLocalChanges: { ok: false as const, detail: stashDetail!, stashSha: parked[1]! } } : {}), ...provenance };
}

export function verifyDesktopReceipt(receipt: DesktopReceipt, snapshot: DesktopSnapshot, expected: { correlationId: string; startedAtUtc: string; preSha: string; expectedReleaseSha?: string;
  allowedPreUpdateDiff?: DesktopUpdateTarget['allowedPreUpdateDiff']; reviewedStash?: DesktopReviewedStashVerification | null }): DesktopIdentityVerification {
  const reasons: string[] = [];
  const started = Date.parse(receipt.startedAtUtc ?? ''), finished = Date.parse(receipt.finishedAtUtc ?? '');
  if (receipt.schema !== 1 || !receipt.updateId || !RUN_ID.test(receipt.updateId)) reasons.push('invalid-receipt-identity');
  if (!receipt.archivePath.endsWith('_' + receipt.updateId + '.json') || receipt.archivePath.endsWith('/latest.json')) reasons.push('not-run-specific-archive');
  if (!/^[0-9a-f]{64}$/.test(receipt.sha256) || receipt.byteLength <= 0) reasons.push('missing-original-receipt-provenance');
  if (receipt.correlationId !== expected.correlationId) reasons.push('correlation-mismatch');
  // Windows and WSL clocks can differ briefly; exact correlation and pre-SHA still bind this run.
  if (!Number.isFinite(started) || !Number.isFinite(finished) || finished < started || started < Date.parse(expected.startedAtUtc) - 5000) reasons.push('nonterminal-or-stale-receipt');
  if (receipt.outcome !== 'success') reasons.push('native-outcome:' + (receipt.outcome ?? 'missing'));
  if (receipt.preSha !== expected.preSha) reasons.push('pre-update-sha-mismatch');
  if (expected.expectedReleaseSha && receipt.postSha !== expected.expectedReleaseSha) reasons.push('requested-release-sha-mismatch');
  const allowed = expected.allowedPreUpdateDiff, stash = expected.reviewedStash, parked = receipt.parkedLocalChanges;
  const reviewedStashMatches = !!allowed && !!stash && !!parked && parked.ok === false
    && parked.detail === 'parked: ' + parked.stashSha + ' (--keep-stash)' && FULL_SHA.test(parked.stashSha)
    && stash.stashSha === parked.stashSha && stash.baseSha === expected.preSha && stash.diffSha256 === allowed.sha256
    && stash.receiptSha256 === receipt.sha256 && JSON.stringify([...stash.paths].sort()) === JSON.stringify([...allowed.paths].sort());
  const dischargedOwed = reviewedStashMatches && receipt.owed.includes('step:local_changes_stash') ? ['step:local_changes_stash'] : [];
  reasons.push(...receipt.owed.filter(reason => !dischargedOwed.includes(reason)));
  const installedAligned = FULL_SHA.test(receipt.postSha ?? '') && receipt.postSha === snapshot.checkoutSha
    && snapshot.checkoutSha === snapshot.checkoutStamp.commit && snapshot.checkoutSha === snapshot.installedArtifact.commit
    && !snapshot.checkoutDirty && snapshot.checkoutStamp.dirty === false && snapshot.installedArtifact.dirty === false
    && snapshot.installedArtifact.source !== 'fallback' && snapshot.nativeHomeVerified;
  if (!installedAligned) reasons.push('installed-artifact-identity-mismatch');
  const runningVerified = snapshot.processes.length > 0 && snapshot.runningArtifact?.commit === receipt.postSha
    && snapshot.runningArtifact.bundleSwapPending === false && snapshot.runningArtifact.bundleOutOfSync === false;
  if (snapshot.runningArtifact && !runningVerified) reasons.push('running-artifact-identity-mismatch');
  return { eligibleForBackend: reasons.length === 0 && installedAligned, canonicalSha: reasons.length === 0 && installedAligned ? receipt.postSha : null,
    installedAligned, runningVerified, runningState: runningVerified ? 'verified' : snapshot.processes.length ? 'unverified' : 'not-running', reasons,
    ...(dischargedOwed.length ? { dischargedOwed } : {}) };
}

export function validateDesktopFleetPlan(value: unknown, target: DesktopUpdateTarget, checkoutSha: string): string[] {
  const plan = object(value), reasons: string[] = [];
  if (plan.install_method !== 'git' || plan.updatable_in_place !== true || plan.expected_sha !== checkoutSha || !Array.isArray(plan.runtimes)) reasons.push('native-plan-identity-unverified');
  for (const row of rows(plan.runtimes)) if (!target.allowedNativeRuntimes.some(allowed => allowed.kind === row.kind && allowed.supervisor === row.supervisor)) reasons.push('unapproved-native-runtime:' + String(row.kind) + ':' + String(row.supervisor));
  return reasons;
}

export function desktopNativePollState(input: { coordinatorExitCode: number | null; waitForCoordinator: boolean; terminalReceiptPresent: boolean; markerPresent: boolean }): 'native-failed' | 'settled' | 'waiting' {
  if (input.coordinatorExitCode !== null && input.coordinatorExitCode !== 0) return 'native-failed';
  if (input.terminalReceiptPresent && !input.markerPresent && (!input.waitForCoordinator || input.coordinatorExitCode === 0)) return 'settled';
  return 'waiting';
}

export function buildDesktopNativeCommand(target: DesktopUpdateTarget, correlationId: string): { command: string; args: string[]; nativeArguments: string[]; nativeParameters: Record<string, string | boolean>; environment: Record<string, string>; powershellSource: string } {
  validateDesktopTarget(target);
  if (!RUN_ID.test(correlationId)) throw new Error('Invalid native update correlation id');
  const script = target.installRootWindows + '\\scripts\\desktop-update\\windows.ps1';
  const exe = target.installRootWindows + '\\apps\\desktop\\release\\win-unpacked\\Hermes.exe';
  const environment = { HERMES_HOME: target.nativeHomeWindows, HERMES_INSTALL_ROOT: target.installRootWindows,
    HERMES_UPDATE_CORRELATION_ID: correlationId, PYTHONDONTWRITEBYTECODE: '1' };
  const nativeArguments = ['-InstallRoot', target.installRootWindows, ...(target.channel ? ['-Channel', target.channel] : ['-Branch', target.branch]), '-NoGateway', '-NoUi', '-RelaunchExe', exe];
  const nativeParameters: Record<string, string | boolean> = { InstallRoot: target.installRootWindows,
    ...(target.channel ? { Channel: target.channel } : { Branch: target.branch }), NoGateway: true, NoUi: true, RelaunchExe: exe };
  // WSL child env is not assumed to cross the Windows boundary through WSLENV.
  // Set only the selected native context inside the Windows process itself.
  const powershellSource = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue';\n"
    + Object.entries(environment).map(([key, value]) => `$env:${key}=${psQuote(value)};`).join('\n')
    + `\n$env:HERMES_RUNTIME_DIR=$null; $nativeParameters=@{${Object.entries(nativeParameters).map(([key, value]) => key + '=' + (typeof value === 'boolean' ? '$true' : psQuote(value))).join(';')}};\n& ${psQuote(script)} @nativeParameters; exit $LASTEXITCODE;`;
  return { command: target.powershellExecutable, args: ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', Buffer.from(powershellSource, 'utf16le').toString('base64')],
    nativeArguments, nativeParameters, environment, powershellSource };
}

export function sanitizeDesktopNativeOutput(value: string): string {
  return value.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '')
    .replace(/-----BEGIN [^-]*PRIVATE KEY-----[\s\S]*?-----END [^-]*PRIVATE KEY-----/g, '[redacted-private-key]')
    .replace(/(https?:\/\/)[^\s<>"']+/gi, match => {
      try { const url = new URL(match); if (url.username || url.password) { url.username = 'redacted'; url.password = ''; }
        if (url.search) url.search = '?redacted'; return url.toString(); } catch { return '[redacted-url]'; }
    })
    .replace(/\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+/gi, '[redacted-authorization]')
    .replace(/(["']?(?:authorization|password|passwd|secret|api[_-]?key|access[_-]?token|refresh[_-]?token|auth[_-]?token|credential)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;<]+)/gi, '$1[redacted]');
}

async function captureDesktopNativeOutput(stream: ReadableStream<Uint8Array>): Promise<{ sha256: string; byteLength: number; truncated: boolean; sanitizedText: string }> {
  const hash = createHash('sha256'), chunks: Uint8Array[] = [], limit = 1024 * 1024;
  let byteLength = 0, retained = 0;
  for await (const chunk of stream) { hash.update(chunk); byteLength += chunk.byteLength;
    if (retained < limit) { const selected = chunk.slice(0, limit - retained); chunks.push(selected); retained += selected.byteLength; } }
  return { sha256: hash.digest('hex'), byteLength, truncated: byteLength > limit, sanitizedText: sanitizeDesktopNativeOutput(Buffer.concat(chunks).toString('utf8')) };
}

async function record(directory: string, operation: string, value: unknown): Promise<void> {
  await mkdir(directory, { recursive: true });
  const file = await open(join(directory, operation + '.json'), 'wx', 0o600);
  try { await file.writeFile(JSON.stringify(value, null, 2) + '\n'); await file.sync(); }
  finally { await file.close(); }
  const parent = await open(directory, 'r');
  try { await parent.sync(); } finally { await parent.close(); }
}
async function regularFile(path: string, maxBytes = 1024 * 1024): Promise<Buffer> {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maxBytes) throw new Error('Expected bounded regular file: ' + path);
  return readFile(path);
}
async function artifact(path: string): Promise<DesktopArtifact> {
  const bytes = await regularFile(path, 65536), data = object(JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')));
  return { commit: text(data.commit), builtAt: text(data.builtAt), dirty: typeof data.dirty === 'boolean' ? data.dirty : null,
    source: text(data.source), updateMechanism: text(data.updateMechanism), payload: text(data.payload), sha256: sha256(bytes), path };
}
async function command(args: string[], environment: Record<string, string> = {}): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  // This helper runs only task-owned read-only probes. Native update execution
  // has a separate spawn below and is never terminated by a probe timeout.
  const child = Bun.spawn(args, { stdout: 'pipe', stderr: 'pipe', env: { ...process.env, ...environment }, timeout: 30000 });
  const [stdout, stderr, exitCode] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
  return { stdout, stderr, exitCode };
}
async function powershell(target: DesktopUpdateTarget, source: string): Promise<Json> {
  const encoded = Buffer.from("$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false);\n" + source, 'utf16le').toString('base64');
  const result = await command([target.powershellExecutable, '-NoProfile', '-NonInteractive', '-EncodedCommand', encoded]);
  if (result.exitCode !== 0) throw new Error('Native metadata probe failed (exit ' + result.exitCode + '); stderr hash ' + sha256(result.stderr));
  return object(JSON.parse(result.stdout.trim().replace(/^\uFEFF/, '')));
}
async function marker(target: DesktopUpdateTarget): Promise<{ present: boolean; runId: string | null }> {
  try {
    const body = (await regularFile(join(target.nativeHomeWsl, '.hermes-update-in-progress'), 16384)).toString('utf8');
    return { present: true, runId: body.match(/^run:([A-Za-z0-9-]{16,128})\r?$/m)?.[1] ?? null };
  } catch (error) { if (object(error).code === 'ENOENT') return { present: false, runId: null }; throw error; }
}

export async function verifyReviewedDesktopStash(target: DesktopUpdateTarget, receipt: DesktopReceipt, preSha: string, runDirectory: string): Promise<DesktopReviewedStashVerification | null> {
  const parked = receipt.parkedLocalChanges, allowed = target.allowedPreUpdateDiff;
  if (!parked || !allowed) return null;
  validateDesktopTarget(target);
  if (receipt.outcome !== 'success' || !Number.isFinite(Date.parse(receipt.finishedAtUtc ?? '')) || !FULL_SHA.test(preSha)
    || receipt.preSha !== preSha || !FULL_SHA.test(receipt.postSha ?? '') || !FULL_SHA.test(parked.stashSha)
    || parked.detail !== 'parked: ' + parked.stashSha + ' (--keep-stash)') return null;
  if ((await marker(target)).present) throw Object.assign(new Error('Reviewed stash readback waits for the existing native marker to settle'), { code: 'native-update-still-active-or-unresolved' });
  const observed = await powershell(target, `$git=${psQuote(target.nativeGitExecutableWindows)}; $root=${psQuote(target.installRootWindows)}; $stash=${psQuote(parked.stashSha)};
$marker=${psQuote(target.nativeHomeWindows + '\\.hermes-update-in-progress')}; if(Test-Path -LiteralPath $marker){throw 'Native update still active; stash readback refused'};
$env:GIT_OPTIONAL_LOCKS='0'; $env:GIT_NO_LAZY_FETCH='1'; $env:GIT_TERMINAL_PROMPT='0';
$base=(& $git --no-optional-locks -c protocol.allow=never -C $root rev-parse ($stash+'^1')) -join ''; if($LASTEXITCODE -ne 0){throw 'Native stash base readback failed'};
$head=(& $git --no-optional-locks -c protocol.allow=never -C $root rev-parse HEAD) -join ''; if($LASTEXITCODE -ne 0){throw 'Native stash source readback failed'};
$diffProcess=New-Object System.Diagnostics.Process; $diffProcess.StartInfo.FileName=$git;
$diffProcess.StartInfo.Arguments='--no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C "'+$root+'" diff --binary --full-index --no-ext-diff --no-textconv --no-renames '+$stash+'^1 '+$stash+' --';
$diffProcess.StartInfo.UseShellExecute=$false; $diffProcess.StartInfo.CreateNoWindow=$true; $diffProcess.StartInfo.RedirectStandardOutput=$true; $diffProcess.StartInfo.RedirectStandardError=$true;
if(-not $diffProcess.Start()){throw 'Native stash diff probe failed'};
$diffErrors=$diffProcess.StandardError.ReadToEndAsync(); $diffBytes=New-Object System.IO.MemoryStream;
$diffProcess.StandardOutput.BaseStream.CopyTo($diffBytes); $diffProcess.WaitForExit();
if($diffProcess.ExitCode -ne 0 -or $diffBytes.Length -gt 2097152){throw 'Native stash diff identity could not be bounded'};
$digest=[System.Security.Cryptography.SHA256]::Create(); $diffHash=([System.BitConverter]::ToString($digest.ComputeHash($diffBytes.ToArray()))).Replace('-','').ToLowerInvariant(); $digest.Dispose(); $diffBytes.Dispose(); $diffProcess.Dispose();
$paths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --name-only --no-ext-diff --no-textconv --no-renames ($stash+'^1') $stash --); if($LASTEXITCODE -ne 0){throw 'Native stash paths failed'};
if(Test-Path -LiteralPath $marker){throw 'Native update started during stash readback'};
@{stashSha=$stash;baseSha=$base;headSha=$head;diffSha256=$diffHash;paths=@($paths|Sort-Object -Unique)}|ConvertTo-Json -Depth 3 -Compress`);
  const paths = Array.isArray(observed.paths) ? observed.paths.map(String) : [];
  if (observed.stashSha !== parked.stashSha || observed.baseSha !== preSha || observed.headSha !== receipt.postSha
    || observed.diffSha256 !== allowed.sha256 || JSON.stringify([...paths].sort()) !== JSON.stringify([...allowed.paths].sort())) throw Object.assign(new Error('The parked native stash differs from the reviewed pre-update patch'), { code: 'reviewed-stash-identity-mismatch' });
  const verified: DesktopReviewedStashVerification = { stashSha: parked.stashSha, baseSha: preSha, diffSha256: allowed.sha256,
    paths, receiptSha256: receipt.sha256, observedAtUtc: new Date().toISOString() };
  await record(runDirectory, 'desktop-reviewed-stash-' + receipt.updateId + '-' + randomUUID(), verified);
  return verified;
}

export async function inspectDesktop(target: DesktopUpdateTarget, runDirectory: string, options: { cdpUrl?: string } = {}): Promise<DesktopSnapshot> {
  validateDesktopTarget(target);
  if (target.schemaVersion !== 1 || !target.installRootWsl.startsWith('/mnt/') || !target.nativeHomeWsl.startsWith('/mnt/')) throw new Error('Invalid Windows Desktop target');
  if (!Number.isInteger(target.receiptTimeoutMs) || target.receiptTimeoutMs < 1000 || target.receiptTimeoutMs > 3600000) throw new Error('Native receipt observation duration must be bounded between 1 second and 1 hour');
  if ((await marker(target)).present) throw Object.assign(new Error('Native Desktop inspection waits for the existing update marker to settle'), { code: 'native-update-still-active-or-unresolved' });
  if (await realpath(target.installRootWsl) !== target.installRootWsl || await realpath(target.nativeHomeWsl) !== target.nativeHomeWsl) throw new Error('Desktop roots must be canonical paths');
  const [installMapping, homeMapping] = await Promise.all([command(['wslpath', '-w', target.installRootWsl]), command(['wslpath', '-w', target.nativeHomeWsl])]);
  if (installMapping.exitCode !== 0 || homeMapping.exitCode !== 0 || windowsPath(installMapping.stdout.trim()) !== windowsPath(target.installRootWindows)
    || windowsPath(homeMapping.stdout.trim()) !== windowsPath(target.nativeHomeWindows)) throw new Error('WSL and Windows target paths do not name the same install/home');
  const [checkoutStamp, installedArtifact, nativeScript, updateMarker, native] = await Promise.all([
    artifact(join(target.installRootWsl, 'install-stamp.json')),
    artifact(join(target.installRootWsl, 'apps/desktop/release/win-unpacked/resources/install-stamp.json')),
    regularFile(join(target.installRootWsl, 'scripts/desktop-update/windows.ps1'), 256 * 1024), marker(target),
    powershell(target, `$userHome=[Environment]::GetFolderPath('UserProfile'); $applicationHome=if($env:HERMES_HOME){$env:HERMES_HOME}else{Join-Path $env:LOCALAPPDATA 'hermes'};
$git=${psQuote(target.nativeGitExecutableWindows)}; $root=${psQuote(target.installRootWindows)};
$env:GIT_OPTIONAL_LOCKS='0'; $env:GIT_NO_LAZY_FETCH='1'; $env:GIT_TERMINAL_PROMPT='0';
$head=(& $git --no-optional-locks -C $root rev-parse HEAD) -join ''; if($LASTEXITCODE -ne 0){throw 'Native Git HEAD probe failed'};
& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --no-ext-diff --no-textconv --no-renames --quiet HEAD --;
$dirtyCode=$LASTEXITCODE; if($dirtyCode -notin @(0,1)){throw 'Native Git dirty probe could not establish a result'};
$untrackedPaths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root ls-files --others --exclude-standard); if($LASTEXITCODE -ne 0){throw 'Native Git untracked paths failed'};
$checkoutDirty=($dirtyCode -eq 1 -or $untrackedPaths.Count -gt 0);
$diffIdentity=$null;
if($checkoutDirty){
  $diffProcess=New-Object System.Diagnostics.Process; $diffProcess.StartInfo.FileName=$git;
  $diffProcess.StartInfo.Arguments='--no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C "'+$root+'" diff --binary --full-index --no-ext-diff --no-textconv --no-renames HEAD --';
  $diffProcess.StartInfo.UseShellExecute=$false; $diffProcess.StartInfo.CreateNoWindow=$true; $diffProcess.StartInfo.RedirectStandardOutput=$true; $diffProcess.StartInfo.RedirectStandardError=$true;
  if(-not $diffProcess.Start()){throw 'Native Git diff probe failed'};
  $diffErrors=$diffProcess.StandardError.ReadToEndAsync(); $diffBytes=New-Object System.IO.MemoryStream;
  $diffProcess.StandardOutput.BaseStream.CopyTo($diffBytes); $diffProcess.WaitForExit();
  if($diffProcess.ExitCode -ne 0 -or $diffBytes.Length -gt 2097152){throw 'Native Git diff identity could not be bounded'};
  $digest=[System.Security.Cryptography.SHA256]::Create(); $diffHash=([System.BitConverter]::ToString($digest.ComputeHash($diffBytes.ToArray()))).Replace('-','').ToLowerInvariant(); $digest.Dispose(); $diffBytes.Dispose(); $diffProcess.Dispose();
  $diffPaths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --name-only --no-ext-diff --no-textconv --no-renames HEAD --); if($LASTEXITCODE -ne 0){throw 'Native Git diff paths failed'};
  $diffIdentity=@{sha256=$diffHash;paths=@($diffPaths|Sort-Object -Unique);untrackedPaths=@($untrackedPaths|Sort-Object -Unique)};
};
$artifactRoot=${psQuote(target.installRootWindows + '\\apps\\desktop\\release\\')};
$owned=@(Get-CimInstance Win32_Process -Filter "Name = 'Hermes.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($artifactRoot,[StringComparison]::OrdinalIgnoreCase) });
$ownedIds=@($owned|ForEach-Object{[int]$_.ProcessId});
$rows=@($owned | Where-Object { $ownedIds -notcontains [int]$_.ParentProcessId } | Sort-Object ProcessId | ForEach-Object { @{pid=[int]$_.ProcessId; executablePath=$_.ExecutablePath; createdAtUtc=$_.CreationDate.ToUniversalTime().ToString('o')} });
@{nativeUserHomeWindows=$userHome;nativeHomeWindows=$applicationHome;processes=$rows;checkoutSha=$head;checkoutDirty=$checkoutDirty;checkoutDiff=$diffIdentity}|ConvertTo-Json -Depth 4 -Compress`),
  ]);
  if (!FULL_SHA.test(String(native.checkoutSha)) || typeof native.checkoutDirty !== 'boolean') throw new Error('Checkout identity could not be verified');
  const nativeHome = text(native.nativeHomeWindows) ?? '', userHome = text(native.nativeUserHomeWindows) ?? '';
  const snapshot: DesktopSnapshot = { schemaVersion: 1, observedAtUtc: new Date().toISOString(), targetId: target.id,
    checkoutSha: String(native.checkoutSha), checkoutDirty: native.checkoutDirty, checkoutStamp, installedArtifact,
    ...(native.checkoutDirty ? { checkoutDiff: { sha256: String(object(native.checkoutDiff).sha256), paths: Array.isArray(object(native.checkoutDiff).paths) ? (object(native.checkoutDiff).paths as unknown[]).map(String) : [],
      untrackedPaths: Array.isArray(object(native.checkoutDiff).untrackedPaths) ? (object(native.checkoutDiff).untrackedPaths as unknown[]).map(String) : [] } } : {}),
    processes: rows(native.processes).map(row => ({ pid: Number(row.pid), executablePath: String(row.executablePath), createdAtUtc: String(row.createdAtUtc) })),
    nativeUserHomeWindows: userHome, nativeHomeWindows: nativeHome,
    nativeHomeVerified: windowsPath(userHome) === windowsPath(target.nativeUserHomeWindows) && windowsPath(nativeHome) === windowsPath(target.nativeHomeWindows),
    nativeScript: { path: join(target.installRootWsl, 'scripts/desktop-update/windows.ps1'), sha256: sha256(nativeScript),
      automaticCorrelation: nativeScript.includes(Buffer.from('$env:HERMES_UPDATE_CORRELATION_ID = $script:UpdateCorrelation')) && nativeScript.includes(Buffer.from('run_id')) },
    marker: updateMarker, runningArtifact: null };
  if (options.cdpUrl && snapshot.processes.length) {
    const connection = await connectDesktop(target, options.cdpUrl);
    try { snapshot.runningArtifact = runningArtifact(connection.version); }
    finally { await connection.browser.close(); }
  }
  await record(runDirectory, 'desktop-inspection-' + randomUUID(), snapshot);
  return snapshot;
}

async function nativeFleetPlan(target: DesktopUpdateTarget): Promise<Json> {
  // The exact install launcher supplies the selected Python/bootstrap command.
  // Its update_inventory module is read-only; append only a sanitized JSON projection.
  return powershell(target, `$env:HERMES_HOME=${psQuote(target.nativeHomeWindows)}; $env:HERMES_INSTALL_ROOT=${psQuote(target.installRootWindows)}; $env:PYTHONDONTWRITEBYTECODE='1';
$launcher=${psQuote(target.installRootWindows + '\\.hermes\\bin\\hermes.exe')};
$runtime=(((& $launcher --print-runtime-command --module hermes_cli.update_inventory) -join "\n")|ConvertFrom-Json);
if($LASTEXITCODE -ne 0 -or $runtime.Count -lt 4 -or $runtime[2] -cne '-c'){throw 'Selected native runtime command is unsupported'};
$runtime[3]+=${psQuote("; import json; from hermes_cli.update_inventory import collect_runtime_inventory; p=collect_runtime_inventory().to_dict(); print(json.dumps({**{k:p.get(k) for k in ('install_method','updatable_in_place','expected_sha','profiles')}, 'runtimes':[{k:r.get(k) for k in ('kind','profile','pid','supervisor','code_sha','restart_via')} for r in p.get('runtimes',[])]}))")};
$runtimeArgs=@($runtime|Select-Object -Skip 1); & $runtime[0] @runtimeArgs; if($LASTEXITCODE -ne 0){throw 'Native inventory failed'}`);
}

export async function verifyNativeStashSupport(target: DesktopUpdateTarget): Promise<void> {
  const result = await powershell(target, `$env:HERMES_HOME=${psQuote(target.nativeHomeWindows)}; $env:HERMES_INSTALL_ROOT=${psQuote(target.installRootWindows)}; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
$savedPreference=$ErrorActionPreference; $ErrorActionPreference='Continue';
try {$helpText=((& ${psQuote(target.installRootWindows + '\\.hermes\\bin\\hermes.exe')} update --help 2>&1) -join "\n"); $helpExit=$LASTEXITCODE} finally {$ErrorActionPreference=$savedPreference};
@{nativeExitCode=$helpExit;keepStash=($helpText -match '(?m)--keep-stash(?:\\s|$)')}|ConvertTo-Json -Compress`);
  if (result.nativeExitCode !== 0) throw Object.assign(new Error('Native update --help exited ' + String(result.nativeExitCode) + '; no native update was invoked'), { code: 'native-update-help-failed' });
  if (result.keepStash !== true) throw Object.assign(new Error('Reviewed local edits require verified native --keep-stash support'), { code: 'native-keep-stash-unavailable' });
}

export function verifyDesktopReleaseTarget(value: unknown, target: DesktopUpdateTarget): void {
  validateDesktopTarget(target);
  const resolved = object(value);
  if (target.channel !== 'stable' || resolved.requestedChannel !== 'stable' || resolved.channel !== 'stable'
    || resolved.repository !== 'NousResearch/hermes-agent' || !FULL_SHA.test(String(resolved.commit))
    || (target.expectedReleaseSha && resolved.commit !== target.expectedReleaseSha)
    || (target.expectedReleaseTag && 'v' + resolved.version !== target.expectedReleaseTag)) throw Object.assign(new Error('Native stable source target differs from the requested release'), { code: 'native-release-target-mismatch' });
}

export async function nativeReleaseTarget(target: DesktopUpdateTarget): Promise<Json> {
  // Windows PowerShell 5.1 strips embedded native-argument double quotes.
  // The native bootstrap stays intact; the appended projection is transported as data.
  const projection = "import json; from pathlib import Path; from hermes_cli.source_releases import resolve_source_target; t=resolve_source_target('stable', git_cmd=[" + JSON.stringify(target.nativeGitExecutableWindows) + "], cwd=Path(" + JSON.stringify(target.installRootWindows) + ")); print(json.dumps({'requestedChannel':t.requested_channel,'channel':t.channel,'repository':t.repository,'commit':t.commit,'version':t.version,'buildId':t.build_id}))";
  const append = "; exec(__import__('base64').b64decode('" + Buffer.from(projection).toString('base64') + "'))";
  const result = await powershell(target, `$env:HERMES_HOME=${psQuote(target.nativeHomeWindows)}; $env:HERMES_INSTALL_ROOT=${psQuote(target.installRootWindows)}; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
$launcher=${psQuote(target.installRootWindows + '\\.hermes\\bin\\hermes.exe')};
$runtime=(((& $launcher --print-runtime-command --module hermes_cli.source_releases) -join "\n")|ConvertFrom-Json);
if($LASTEXITCODE -ne 0 -or $runtime.Count -lt 4 -or $runtime[2] -cne '-c'){throw 'Selected native runtime command is unsupported'};
$runtime[3]+=${psQuote(append)};
$runtimeArgs=@($runtime|Select-Object -Skip 1); & $runtime[0] @runtimeArgs; if($LASTEXITCODE -ne 0){throw 'Native stable target resolution failed'}`);
  verifyDesktopReleaseTarget(result, target);
  return result;
}

async function verifyInstalledDesktop(target: DesktopUpdateTarget): Promise<void> {
  // Official read-only PE/ASAR/renderer/freshness verification, using this
  // installation's launcher. A terminal CLI source receipt alone cannot
  // prove the later Desktop coordinator's artifact verification succeeded.
  const result = await powershell(target, `$env:HERMES_HOME=${psQuote(target.nativeHomeWindows)}; $env:HERMES_INSTALL_ROOT=${psQuote(target.installRootWindows)}; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
& ${psQuote(target.installRootWindows + '\\.hermes\\bin\\hermes.exe')} --run-module hermes_cli.desktop_update_verify | Out-Null;
if($LASTEXITCODE -ne 0){throw 'Native installed artifact verification failed'}; @{verified=$true}|ConvertTo-Json -Compress`);
  if (result.verified !== true) throw new Error('Native installed artifact integrity was not verified');
}

export interface ExistingDesktopReleaseProof {
  schemaVersion: 1;
  kind: 'verified-existing-desktop-release';
  readOnly: true;
  expectedReleaseSha: string;
  expectedReleaseTag: string;
  nativeArtifactIntegrityVerified: true;
  runningAlignment: 'verified' | 'unverified' | 'not-running';
  before: DesktopSnapshot;
  after: DesktopSnapshot;
  updaterInvoked: false;
}
export interface ExistingDesktopReleaseReaders {
  inspect(): Promise<DesktopSnapshot>;
  verifyInstalled(): Promise<void>;
  saveProof(proof: ExistingDesktopReleaseProof): Promise<void>;
}
function existingDesktopReleaseIdentity(target: DesktopUpdateTarget, snapshot: DesktopSnapshot): ExistingDesktopReleaseProof['runningAlignment'] {
  const sha = target.expectedReleaseSha;
  if (!sha || snapshot.targetId !== target.id || snapshot.checkoutSha !== sha || snapshot.checkoutDirty || !snapshot.nativeHomeVerified || snapshot.marker.present
    || snapshot.checkoutStamp.commit !== sha || snapshot.checkoutStamp.dirty !== false || snapshot.installedArtifact.commit !== sha || snapshot.installedArtifact.dirty !== false
    || !snapshot.installedArtifact.source || snapshot.installedArtifact.source === 'fallback' || snapshot.installedArtifact.updateMechanism !== 'self' || snapshot.installedArtifact.payload !== 'bootstrap') throw new Error('Existing Desktop release source and installed artifact are not strictly aligned');
  const running = snapshot.runningArtifact;
  if (running && ((running.commit !== null && running.commit !== sha) || running.bundleSwapPending === true || running.bundleOutOfSync === true)) throw new Error('Existing Desktop running artifact differs from the requested release');
  return !snapshot.processes.length ? 'not-running' : running?.commit === sha && running.bundleSwapPending === false && running.bundleOutOfSync === false ? 'verified' : 'unverified';
}

/** Read-only ports keep an already installed release independent of the updater lifecycle. */
export async function verifyExistingDesktopReleaseReadOnly(target: DesktopUpdateTarget, readers: ExistingDesktopReleaseReaders): Promise<DesktopSnapshot> {
  validateDesktopTarget(target);
  if (!target.expectedReleaseSha || !target.expectedReleaseTag) throw new Error('Existing Desktop verification requires an exact release SHA and tag');
  const before = await readers.inspect();
  existingDesktopReleaseIdentity(target, before);
  await readers.verifyInstalled();
  const after = await readers.inspect();
  const runningAlignment = existingDesktopReleaseIdentity(target, after);
  if (before.checkoutSha !== after.checkoutSha || before.checkoutStamp.sha256 !== after.checkoutStamp.sha256
    || before.installedArtifact.sha256 !== after.installedArtifact.sha256 || before.nativeScript.sha256 !== after.nativeScript.sha256
    || before.nativeUserHomeWindows !== after.nativeUserHomeWindows || before.nativeHomeWindows !== after.nativeHomeWindows
    || JSON.stringify(before.processes) !== JSON.stringify(after.processes)) throw new Error('Existing Desktop identity changed during native integrity verification');
  await readers.saveProof({ schemaVersion: 1, kind: 'verified-existing-desktop-release', readOnly: true,
    expectedReleaseSha: target.expectedReleaseSha, expectedReleaseTag: target.expectedReleaseTag, nativeArtifactIntegrityVerified: true,
    runningAlignment, before, after, updaterInvoked: false });
  return after;
}

export async function verifyExistingDesktopRelease(target: DesktopUpdateTarget, runDirectory: string, options: { cdpUrl?: string } = {}): Promise<DesktopSnapshot> {
  return verifyExistingDesktopReleaseReadOnly(target, {
    inspect: () => inspectDesktop(target, runDirectory, options),
    verifyInstalled: () => verifyInstalledDesktop(target),
    saveProof: proof => record(runDirectory, 'desktop-existing-release-verification-' + randomUUID(), proof),
  });
}

interface NativePage {
  evaluate<T>(fn: () => T | Promise<T>): Promise<T>;
}
interface NativeBrowser { contexts(): Array<{ pages(): NativePage[] }>; close(): Promise<void> }
interface NativeBridge {
  getVersion(): Promise<Json>;
  updates: { apply(): Promise<Json> };
}
async function connectDesktop(target: DesktopUpdateTarget, cdpUrl: string): Promise<{ browser: NativeBrowser; page: NativePage; version: Json }> {
  const endpoint = new URL(cdpUrl);
  if (!['http:', 'https:', 'ws:', 'wss:'].includes(endpoint.protocol) || endpoint.username || endpoint.password || endpoint.search) throw new Error('CDP must be an explicit HTTP/WebSocket endpoint without URL credentials or query parameters');
  const require = createRequire(join(target.playwrightProjectRootWsl, 'package.json'));
  const entry = require.resolve('playwright');
  const module = await import(pathToFileURL(entry).href) as { chromium: { connectOverCDP(url: string, options: { noDefaults: true; timeout: number }): Promise<NativeBrowser> } };
  // Preserve the existing context's policies rather than applying Playwright
  // defaults (including download handling) during a read-only attachment.
  const browser = await module.chromium.connectOverCDP(cdpUrl, { noDefaults: true, timeout: 10000 });
  const matches: Array<{ page: NativePage; version: Json }> = [];
  for (const page of browser.contexts().flatMap(context => context.pages())) {
    try {
      const version = await page.evaluate<Json>(async () => {
        const bridge = (globalThis as unknown as { hermesDesktop?: NativeBridge }).hermesDesktop;
        return bridge ? bridge.getVersion() : {};
      });
      if (windowsPath(String(version.hermesRoot ?? '')) === windowsPath(target.installRootWindows)) matches.push({ page, version });
    } catch { /* Unrelated or already closed pages have no matching native version bridge. */ }
  }
  if (matches.length !== 1) { await browser.close(); throw new Error('CDP must expose exactly one matching Desktop native bridge; no page is created or closed'); }
  return { browser, ...matches[0]! };
}
function runningArtifact(version: Json): NonNullable<DesktopSnapshot['runningArtifact']> {
  return { commit: text(version.commit), bundleSwapPending: typeof version.bundleSwapPending === 'boolean' ? version.bundleSwapPending : null,
    bundleOutOfSync: typeof version.bundleOutOfSync === 'boolean' ? version.bundleOutOfSync : null };
}
export async function archivedReceipt(target: DesktopUpdateTarget, correlationId: string): Promise<DesktopReceipt | null> {
  const directory = join(target.nativeHomeWsl, 'logs/update_receipts'), matches: DesktopReceipt[] = [];
  let names: string[];
  try { names = await readdir(directory); } catch (error) { if (object(error).code === 'ENOENT') return null; throw error; }
  for (const name of names) {
    if (!/^update_[A-Za-z0-9_-]+\.json$/.test(name)) continue;
    const path = join(directory, name), bytes = await regularFile(path, 2 * 1024 * 1024);
    const raw = object(JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')));
    if (raw.correlation_id === correlationId && raw.finished_at) matches.push(normalizeDesktopReceipt(raw, { archivePath: path, sha256: sha256(bytes), byteLength: bytes.length }));
  }
  if (matches.length > 1) throw new Error('More than one terminal archive matches the native correlation; explicit reconciliation is required');
  return matches[0] ?? null;
}
export interface DesktopReceiptArchiveReaders {
  list(): Promise<string[]>;
  read(path: string): Promise<Uint8Array | null>;
  audit(): Promise<DesktopReceipt | null>;
}
/** Cache one path for active polling; bytes and terminal admission are never cached. */
export function createDesktopReceiptPoller(correlationId: string, readers: DesktopReceiptArchiveReaders): { poll(options?: { fullAudit?: boolean }): Promise<DesktopReceipt | null> } {
  let cachedPath: string | null = null;
  const readMatching = async (path: string): Promise<{ matches: boolean; receipt: DesktopReceipt | null }> => {
    const bytes = await readers.read(path);
    if (!bytes) return { matches: false, receipt: null };
    const raw = object(JSON.parse(Buffer.from(bytes).toString('utf8').replace(/^\uFEFF/, '')));
    if (raw.correlation_id !== correlationId) return { matches: false, receipt: null };
    return { matches: true, receipt: raw.finished_at ? normalizeDesktopReceipt(raw, { archivePath: path, sha256: sha256(bytes), byteLength: bytes.length }) : null };
  };
  return { async poll(options = {}) {
    if (options.fullAudit) {
      const terminal = await readers.audit(); cachedPath = terminal?.archivePath ?? null; return terminal;
    }
    if (cachedPath) {
      const current = await readMatching(cachedPath);
      if (current.matches) return current.receipt;
      cachedPath = null;
    }
    for (const path of await readers.list()) {
      const current = await readMatching(path);
      if (current.matches) { cachedPath = path; return current.receipt; }
    }
    return null;
  } };
}
function desktopReceiptPoller(target: DesktopUpdateTarget, correlationId: string): ReturnType<typeof createDesktopReceiptPoller> {
  const directory = join(target.nativeHomeWsl, 'logs/update_receipts');
  return createDesktopReceiptPoller(correlationId, {
    async list() {
      try { return (await readdir(directory)).filter(name => /^update_[A-Za-z0-9_-]+\.json$/.test(name)).map(name => join(directory, name)); }
      catch (error) { if (object(error).code === 'ENOENT') return []; throw error; }
    },
    async read(path) {
      try { return await regularFile(path, 2 * 1024 * 1024); }
      catch (error) { if (object(error).code === 'ENOENT') return null; throw error; }
    },
    audit: () => archivedReceipt(target, correlationId),
  });
}
const pause = (ms: number): Promise<void> => new Promise(resolve => setTimeout(resolve, ms));
async function observedRunId(target: DesktopUpdateTarget, startedAtUtc: string): Promise<string | null> {
  const active = await marker(target);
  if (active.runId) return active.runId;
  try {
    const raw = object(JSON.parse((await regularFile(join(target.nativeHomeWsl, '.hermes-update-result.json'), 65536)).toString('utf8')));
    return Number(raw.started_at) >= Math.floor(Date.parse(startedAtUtc) / 1000) && RUN_ID.test(String(raw.run_id)) ? String(raw.run_id) : null;
  } catch (error) { if (object(error).code === 'ENOENT') return null; throw error; }
}

export async function updateDesktop(target: DesktopUpdateTarget, runDirectory: string, options: { authorization: string; cdpUrl?: string; retryRejectedLaunch?: DesktopRejectedLaunchRetry }): Promise<DesktopUpdateResult> {
  const result: DesktopUpdateResult = { schemaVersion: 1, operationId: randomUUID(), startedAtUtc: new Date().toISOString(), completedAtUtc: '', status: 'blocked',
    canonicalSha: null, before: null, after: null, receipt: null, verification: null, nativeFleetPlan: null, error: null, executionBoundary: 'Native mutation not executed' };
  let connection: Awaited<ReturnType<typeof connectDesktop>> | null = null;
  const attemptId = randomUUID();
  let originalRequest: DesktopNativeRequest | null = null;
  let rejectedLaunchProof: unknown = null, rejectedLaunchPrior: unknown = null;
  const fail = (code: string, message: string, recovery: string): never => { throw Object.assign(new Error(message), { code, recovery }); };
  try {
    if (!options.authorization.trim()) fail('authorization-missing', 'A current explicit authorization reference is required', 'Supply the authorized coordinated Desktop/backend operation reference.');
    let priorRequests: string[] = [];
    try { priorRequests = (await readdir(runDirectory)).filter(name => /^desktop-native-request-[A-Za-z0-9-]+\.json$/.test(name)); }
    catch (error) { if (object(error).code !== 'ENOENT') throw error; }
    if (options.retryRejectedLaunch) {
      const retry = options.retryRejectedLaunch;
      if (!DIGEST.test(retry.proofSha256) || !RUN_ID.test(retry.operationId) || priorRequests.length !== 1) fail('desktop-rejected-launch-retry-unproven', 'An exact existing native request and hashed rejection proof are required', 'Reconcile the original native request and its before-body rejection evidence.');
      const selected = resolveDesktopRecoveryRequest([JSON.parse((await regularFile(join(runDirectory, priorRequests[0]!))).toString('utf8'))], target);
      if (!selected.request || selected.request.operationId !== retry.operationId) fail('desktop-rejected-launch-retry-unproven', selected.reasons.join(', '), 'Retain the exact original native scope and correlation.');
      originalRequest = selected.request!; result.operationId = originalRequest.operationId; result.startedAtUtc = originalRequest.startedAtUtc;
      const proofBytes = await regularFile(retry.proofRef, 2 * 1024 * 1024);
      if (sha256(proofBytes) !== retry.proofSha256) fail('desktop-rejected-launch-proof-changed', 'The pinned native rejection proof changed', 'Review the original proof bytes before any retry.');
      rejectedLaunchProof = JSON.parse(proofBytes.toString('utf8'));
      const proof = object(rejectedLaunchProof), priorBytes = await regularFile(join(runDirectory, 'desktop-update-result-' + originalRequest.operationId + '.json'), 2 * 1024 * 1024);
      if (proof.failedNativeResultSha256 !== sha256(priorBytes)
        || JSON.stringify(proof.nativeArguments) !== JSON.stringify(buildDesktopNativeCommand(target, originalRequest.correlationId!).nativeArguments)) fail('desktop-rejected-launch-evidence-mismatch', 'The rejection proof is not bound to the original failed result and exact native arguments', 'Pin the original result bytes and argument values in the reviewed proof.');
      if (proof.originalParameterBlockSha256 !== undefined) {
        const nativeScript = (await regularFile(join(target.installRootWsl, 'scripts/desktop-update/windows.ps1'), 256 * 1024)).toString('utf8');
        const parameterBlock = nativeScript.match(/^param\([\s\S]*?^\)/m)?.[0];
        if (!parameterBlock || sha256(parameterBlock) !== proof.originalParameterBlockSha256) fail('desktop-rejected-launch-parameter-block-changed', 'The original native parameter block identity differs', 'Reconcile the exact native script parameter binder before retrying.');
      }
      rejectedLaunchPrior = JSON.parse(priorBytes.toString('utf8'));
      if ((await readdir(runDirectory)).includes('desktop-native-rejected-launch-retry-' + originalRequest.operationId + '.json')) fail('desktop-rejected-launch-retry-already-requested', 'The original rejected launch already has a durable retry intent', 'Recover that existing launch; another updater will not be invoked.');
    } else if (priorRequests.length) fail('desktop-native-recovery-required', 'This run already has a durable native request; a fresh updater will not be invoked', 'Call recoverDesktopUpdate for the existing operation and reconcile its current native proof.');
    result.before = await inspectDesktop(target, runDirectory);
    const before = result.before;
    if (originalRequest) {
      const reasons = verifyDesktopRejectedLaunchProof(rejectedLaunchProof, originalRequest, rejectedLaunchPrior, before);
      if (reasons.length || await archivedReceipt(target, originalRequest.correlationId!)) fail('desktop-rejected-launch-retry-unproven', reasons.join(', ') || 'A correlated native archive already exists', 'Recover the original native outcome; no retry is admitted.');
    }
    const eligibility = desktopPreUpdateEligibility(target, before);
    if (!eligibility.eligible) fail('native-install-preflight-failed', eligibility.reasons.join(', '), 'Reconcile the exact installation identity and any reviewed local edits before updating.');
    if (before.marker.present) fail('native-update-marker-present', 'A native update marker already exists', 'Use native update status/recovery; this adapter never deletes or replaces the marker.');
    if (before.processes.length && target.channel) fail('desktop-release-ipc-unsupported', 'The running native Update bridge cannot bind an explicit stable release selection', 'Close Desktop normally, then use the correlated native coordinator with -Channel stable.');
    if (before.processes.length && !options.cdpUrl) fail('desktop-cdp-required', 'Desktop is running; its native Update bridge requires the explicitly selected CDP endpoint', 'Provide that app CDP endpoint, or close Desktop normally and rerun the correlated closed-app path.');
    if (before.processes.length && !before.nativeScript.automaticCorrelation) fail('native-correlation-unavailable', 'The running legacy Desktop handoff cannot bind its native receipt to this operation', 'Close Desktop normally and use the correlated native script path, or use its existing Update button manually.');
    if (before.processes.length) {
      connection = await connectDesktop(target, options.cdpUrl!);
      before.runningArtifact = runningArtifact(connection.version);
      if (before.runningArtifact.commit !== before.installedArtifact.commit || before.runningArtifact.bundleSwapPending !== false) fail('running-desktop-skew', 'Running and installed Desktop artifacts differ or cannot be compared', 'Use the native relaunch path and re-inspect the running artifact.');
    }
    if (before.checkoutDirty) await verifyNativeStashSupport(target);
    if (target.channel) await record(runDirectory, 'desktop-release-target-' + result.operationId + (originalRequest ? '-attempt-' + attemptId : ''), await nativeReleaseTarget(target));
    result.nativeFleetPlan = await nativeFleetPlan(target);
    const footprint = validateDesktopFleetPlan(result.nativeFleetPlan, target, before.checkoutSha);
    if (footprint.length) fail('native-fleet-footprint-unapproved', footprint.join(', '), 'Review the native plan and explicitly reconcile any gateway/manual service footprint. NoGateway does not eliminate all native lifecycle effects.');
    if (originalRequest && JSON.stringify(originalRequest.scope.nativeFleetPlan) !== JSON.stringify(result.nativeFleetPlan)) fail('desktop-rejected-launch-scope-changed', 'The native fleet footprint differs from the rejected request', 'Review the changed native scope before any lifecycle effect.');
    // Revalidate metadata immediately before entering the one native writer.
    const current = await inspectDesktop(target, runDirectory);
    if (current.checkoutSha !== before.checkoutSha || !desktopPreUpdateEligibility(target, current).eligible || current.checkoutDirty !== before.checkoutDirty
      || JSON.stringify(current.checkoutDiff ?? null) !== JSON.stringify(before.checkoutDiff ?? null) || current.nativeScript.sha256 !== before.nativeScript.sha256
      || current.checkoutStamp.sha256 !== before.checkoutStamp.sha256 || current.installedArtifact.sha256 !== before.installedArtifact.sha256 || current.marker.present
      || JSON.stringify(current.processes) !== JSON.stringify(before.processes)) fail('desktop-preflight-changed', 'Desktop source/artifact/process/marker identity changed during preflight', 'Re-inspect and retry the same authorized scope.');
    let correlationId = result.operationId;
    let coordinatorExit: Promise<number> | null = null;
    const request: DesktopNativeRequest = originalRequest ?? { schemaVersion: 1, operationId: result.operationId, targetId: target.id,
      mode: connection ? 'open-ipc' : 'closed-script', correlationId: connection ? null : correlationId,
      preSha: before.checkoutSha, startedAtUtc: result.startedAtUtc, authorization: options.authorization,
      installRootWindows: target.installRootWindows, nativeHomeWindows: target.nativeHomeWindows,
      scope: { ...desktopSelection(target), noGateway: true, nativeFleetPlan: result.nativeFleetPlan } };
    if (!originalRequest && (await readdir(runDirectory)).some(name => /^desktop-native-request-[A-Za-z0-9-]+\.json$/.test(name))) fail('desktop-native-recovery-required', 'A native request appeared during preflight', 'Recover the existing run; do not invoke concurrent updaters.');
    // Durable intent precedes every native side effect. An interrupted open
    // handoff stays unresolved until its acknowledged native run is checkpointed.
    if (originalRequest) await record(runDirectory, 'desktop-native-rejected-launch-retry-' + result.operationId, { schemaVersion: 1, operationId: result.operationId,
      correlationId: originalRequest.correlationId, attemptId, observedAtUtc: new Date().toISOString(), proofRef: options.retryRejectedLaunch!.proofRef,
      proofSha256: options.retryRejectedLaunch!.proofSha256, nativeScriptSha256: current.nativeScript.sha256, originalRequestPreserved: true });
    else await record(runDirectory, 'desktop-native-request-' + result.operationId, request);
    result.executionBoundary = 'Native updater invoked; verification is separately recorded';
    if (connection) {
      // Native IPC owns stop/backends/quit/handoff. Options are intentionally not passed:
      // upstream ignores them and supplies its own run id, which we observe below.
      let handoffAcknowledged = false, handoffRejected = false;
      void connection.page.evaluate(async () => (globalThis as unknown as { hermesDesktop: NativeBridge }).hermesDesktop.updates.apply())
        .then(reply => { handoffAcknowledged = reply.ok === true && reply.handedOff === true; handoffRejected = !handoffAcknowledged; })
        .catch(() => { handoffRejected = true; });
      const claimDeadline = Date.now() + 60000;
      let nativeRun: string | null = null;
      while (Date.now() < claimDeadline && (!nativeRun || !handoffAcknowledged) && !handoffRejected) {
        nativeRun = await observedRunId(target, result.startedAtUtc); if (!nativeRun || !handoffAcknowledged) await pause(500);
      }
      if (!nativeRun || !handoffAcknowledged || handoffRejected) fail('native-handoff-unverified', 'Native IPC did not acknowledge a handoff with a correlated run id', 'Observe the native outcome; loss of the app connection is not proof of success. Do not start a second updater or force a kill.');
      correlationId = nativeRun!;
      await record(runDirectory, 'desktop-native-handoff-' + result.operationId, { schemaVersion: 1, operationId: result.operationId, targetId: target.id,
        correlationId, nativeHandoffAcknowledged: true, observedAtUtc: new Date().toISOString() });
    } else {
      const native = buildDesktopNativeCommand(target, correlationId);
      const child = Bun.spawn([native.command, ...native.args], { env: { ...process.env, ...native.environment }, stdout: 'pipe', stderr: 'pipe' });
      coordinatorExit = Promise.all([child.exited, captureDesktopNativeOutput(child.stdout), captureDesktopNativeOutput(child.stderr)])
        .then(async ([exitCode, stdout, stderr]) => {
          await record(runDirectory, 'desktop-native-coordinator-' + result.operationId + '-' + attemptId, { schemaVersion: 1, operationId: result.operationId,
            correlationId, attemptId, exitCode, completedAtUtc: new Date().toISOString(), stdout, stderr });
          return exitCode;
        });
    }
    const deadline = Date.now() + target.receiptTimeoutMs;
    const receiptPoller = desktopReceiptPoller(target, correlationId);
    let coordinatorCode: number | null = null;
    let coordinatorObservationFailed = false;
    if (coordinatorExit) void coordinatorExit.then(code => { coordinatorCode = code; }, () => { coordinatorObservationFailed = true; });
    while (Date.now() < deadline) {
      if (coordinatorObservationFailed) fail('native-coordinator-output-observation-failed', 'Native coordinator output could not be durably observed', 'Observe the same native run; no second updater is invoked.');
      const active = await marker(target);
      result.observedMarker = active;
      result.receipt = await receiptPoller.poll({ fullAudit: !active.present && (coordinatorExit === null || coordinatorCode !== null) });
      const pollState = desktopNativePollState({ coordinatorExitCode: coordinatorCode, waitForCoordinator: coordinatorExit !== null, terminalReceiptPresent: result.receipt !== null, markerPresent: active.present });
      if (pollState !== 'waiting') break;
      await pause(1000);
    }
    if (coordinatorCode !== null && coordinatorCode !== 0) fail('native-coordinator-failed', 'Native Desktop coordinator exited ' + coordinatorCode, 'Observe its persisted request, current marker and any correlated partial receipt. No second updater is invoked.');
    if (!result.receipt || (await marker(target)).present || (coordinatorExit && coordinatorCode === null)) fail('native-update-observation-timeout', 'The native updater has no settled correlated terminal proof yet', 'Observe the same native run; this adapter never kills it or launches an automatic retry.');
    result.receipt = await archivedReceipt(target, correlationId);
    if (!result.receipt) fail('native-terminal-receipt-missing', 'The authoritative terminal archive is missing after the native coordinator settled', 'Recover the same native run; no cached observation can establish completion.');
    result.after = await inspectDesktop(target, runDirectory);
    await verifyInstalledDesktop(target);
    if (options.cdpUrl && result.after.processes.length) {
      if (connection) { await connection.browser.close(); connection = null; }
      connection = await connectDesktop(target, options.cdpUrl);
      result.after.runningArtifact = runningArtifact(connection.version);
    }
    result.reviewedStash = await verifyReviewedDesktopStash(target, result.receipt!, before.checkoutSha, runDirectory);
    result.verification = verifyDesktopReceipt(result.receipt!, result.after, { correlationId, startedAtUtc: result.startedAtUtc, preSha: before.checkoutSha, expectedReleaseSha: target.expectedReleaseSha,
      allowedPreUpdateDiff: target.allowedPreUpdateDiff, reviewedStash: result.reviewedStash });
    if (!result.verification.eligibleForBackend) fail('desktop-terminal-proof-failed', result.verification.reasons.join(', '), 'Resolve the native owed/identity state; no canonical backend SHA is admitted.');
    result.canonicalSha = result.verification.canonicalSha;
    result.status = result.verification.runningVerified ? 'verified-running' : 'installed-only';
  } catch (error) {
    const details = object(error);
    result.status = result.executionBoundary === 'Native mutation not executed' ? 'blocked' : 'failed';
    result.error = { code: text(details.code) ?? 'desktop-adapter-failed', message: error instanceof Error ? error.message : 'Desktop adapter failed',
      recovery: text(details.recovery) ?? 'Inspect the saved bounded observation and native outcome; do not force-kill, overwrite locks, or run concurrent updaters.' };
  } finally {
    // For connectOverCDP, close disconnects the task-owned CDP transport.
    // Existing pages, contexts and the user's app are never explicitly closed.
    if (connection) await connection.browser.close().catch(() => undefined);
    result.completedAtUtc = new Date().toISOString();
    await record(runDirectory, 'desktop-update-result-' + result.operationId + (options.retryRejectedLaunch ? '-attempt-' + attemptId : ''), result);
  }
  return result;
}

export async function recoverDesktopUpdate(target: DesktopUpdateTarget, runDirectory: string, options: { cdpUrl?: string } = {}): Promise<DesktopUpdateResult> {
  const result: DesktopUpdateResult = { schemaVersion: 1, operationId: randomUUID(), startedAtUtc: new Date().toISOString(), completedAtUtc: '', status: 'blocked',
    canonicalSha: null, before: null, after: null, receipt: null, verification: null, nativeFleetPlan: null, error: null,
    executionBoundary: 'Native mutation not executed' };
  const recoveryId = result.operationId;
  try {
    const names = await readdir(runDirectory);
    const requestNames = names.filter(name => /^desktop-native-request-[A-Za-z0-9-]+\.json$/.test(name));
    const checkpointNames = names.filter(name => /^desktop-native-handoff-[A-Za-z0-9-]+\.json$/.test(name));
    const readJson = async (name: string): Promise<unknown> => JSON.parse((await regularFile(join(runDirectory, name), 2 * 1024 * 1024)).toString('utf8'));
    const selected = resolveDesktopRecoveryRequest(await Promise.all(requestNames.map(readJson)), target, await Promise.all(checkpointNames.map(readJson)));
    if (!selected.request || !selected.correlationId) throw Object.assign(new Error(selected.reasons.join(', ')), { code: 'desktop-recovery-unresolved' });
    const request = selected.request;
    result.operationId = request.operationId;
    result.startedAtUtc = request.startedAtUtc;
    result.nativeFleetPlan = request.scope.nativeFleetPlan;
    const priorNames = names.filter(name => name === 'desktop-update-result-' + request.operationId + '.json');
    if (priorNames.length > 1) throw new Error('Ambiguous prior Desktop result');
    const prior = priorNames.length ? object(await readJson(priorNames[0]!)) : {};
    result.after = await inspectDesktop(target, runDirectory, options);
    if (result.after.marker.present) throw Object.assign(new Error('The existing native run has not released its marker; no new updater was invoked'), { code: 'native-update-still-active-or-unresolved' });
    result.receipt = await archivedReceipt(target, selected.correlationId);
    if (!result.receipt) throw Object.assign(new Error('No terminal archive belongs to the persisted native correlation'), { code: 'native-terminal-receipt-missing' });
    const priorReceipt = object(prior.receipt);
    if (text(priorReceipt.sha256) && priorReceipt.sha256 !== result.receipt.sha256) throw Object.assign(new Error('The previously observed terminal archive changed; reconcile its provenance explicitly'), { code: 'native-terminal-archive-changed' });
    await verifyInstalledDesktop(target);
    result.reviewedStash = await verifyReviewedDesktopStash(target, result.receipt, request.preSha, runDirectory);
    result.verification = verifyDesktopReceipt(result.receipt, result.after, { correlationId: selected.correlationId, startedAtUtc: request.startedAtUtc, preSha: request.preSha, expectedReleaseSha: request.scope.expectedReleaseSha,
      allowedPreUpdateDiff: request.scope.allowedPreUpdateDiff, reviewedStash: result.reviewedStash });
    if (!result.verification.eligibleForBackend) throw Object.assign(new Error(result.verification.reasons.join(', ')), { code: 'desktop-recovery-proof-failed' });
    result.canonicalSha = result.verification.canonicalSha;
    result.status = result.verification.runningVerified ? 'verified-running' : 'installed-only';
  } catch (error) {
    result.error = { code: text(object(error).code) ?? 'desktop-recovery-unresolved', message: error instanceof Error ? error.message : 'Desktop recovery observation failed',
      recovery: 'Observe/reconcile the same persisted native run. Recovery never invokes another updater, guesses an interrupted open handoff, kills processes, or replaces markers.' };
  } finally {
    result.completedAtUtc = new Date().toISOString();
    await record(runDirectory, 'desktop-update-recovery-' + recoveryId, result);
  }
  return result;
}
