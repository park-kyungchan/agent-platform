import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { resolve, join } from 'node:path';
import type { HermesTarget } from '../adapters/ssh';
import { releaseStampMatches, fontReadabilityIdentity, inspectBackend, resolveLatestMain, prepareBackend, deployBackend, verifyBackend, stageBackendOverlay, type BackendSnapshot, type BackendUpdatePolicy, type PreparedBackendCandidate } from '../adapters/backend-update';
import type { DesktopSnapshot, DesktopUpdateTarget } from '../adapters/desktop-update';
import { runUpdate, type BackendIdentity, type DesktopIdentity, type UpdateCheckpoint, type UpdateMode } from './workflow';
import { loadCheckpoint, saveCheckpoint } from './checkpoints';
import { loadReleaseTarget, resolveAndSaveReleaseTarget, validateReleaseResume, validateReleaseTag } from './release-target';
import { inspectDesktopPreflightRetry } from './preflight-retry';
import { inspectDesktopRejectedLaunchRetry } from './rejected-launch-retry';

const usage = 'update <backend-only|coordinated> [--plan|--apply] [--sha FULL_BACKEND_SHA|--release vX.Y.Z] [--resume RUN_DIRECTORY] [--desktop-cdp URL] [--authorization TEXT]; update verify --sha FULL_SHA; update stage-overlay --sha FULL_SHA --patch FILE --apply --authorization TEXT';
interface Options { apply: boolean; sha?: string; release?: string; resume?: string; cdp?: string; authorization?: string; patch?: string }
export function parseUpdateArguments(args: string[]): { command: UpdateMode | 'verify' | 'stage-overlay'; options: Options } {
  const [command, ...tokens] = args;
  if (!['backend-only', 'coordinated', 'verify', 'stage-overlay'].includes(command ?? '')) throw new Error(usage);
  const options: Options = { apply: false };
  let actionSeen = false;
  const fields: Record<string, keyof Omit<Options, 'apply'>> = { '--sha': 'sha', '--release': 'release', '--resume': 'resume', '--desktop-cdp': 'cdp', '--authorization': 'authorization', '--patch': 'patch' };
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index]!;
    if (token === '--apply' || token === '--plan') {
      if (actionSeen) throw new Error('Choose one of --plan or --apply');
      options.apply = token === '--apply'; actionSeen = true; continue;
    }
    const field = fields[token];
    const value = tokens[++index];
    if (!field || value === undefined || value.startsWith('--') || options[field] !== undefined) throw new Error('Unknown, duplicate or incomplete option: ' + token);
    options[field] = value;
  }
  if (options.sha && !/^[0-9a-f]{40}$/.test(options.sha)) throw new Error('Use a full lowercase commit SHA');
  if (options.release) validateReleaseTag(options.release);
  if (options.release && (command === 'verify' || command === 'stage-overlay')) throw new Error('--release belongs only to an update mode');
  if (options.release && options.sha) throw new Error('Choose one target: --release or --sha');
  if (command === 'coordinated' && options.sha) throw new Error('Desktop native completion determines the coordinated SHA');
  if (command !== 'coordinated' && options.cdp) throw new Error('Desktop CDP belongs only to coordinated mode');
  if (options.apply && !options.authorization?.trim()) throw new Error('Record the current explicit update instruction with --authorization');
  if ((command === 'verify' || command === 'stage-overlay') && !options.sha) throw new Error(command + ' requires --sha');
  if (command === 'stage-overlay' && (!options.patch || !options.apply)) throw new Error('stage-overlay requires --patch and --apply');
  if (command !== 'stage-overlay' && options.patch) throw new Error('--patch belongs only to stage-overlay');
  if ((command === 'verify' || command === 'stage-overlay') && options.resume) throw new Error('--resume belongs only to an update mode');
  if (command === 'verify' && options.apply) throw new Error('verify is read-only');
  return { command: command as UpdateMode | 'verify' | 'stage-overlay', options };
}
export const backendIdentity = (snapshot: BackendSnapshot, policy: BackendUpdatePolicy): BackendIdentity => {
  const current = snapshot.current;
  if (!current) throw new Error('Backend native current state is missing');
  return { sha: current.sha, imageId: current.image_id,
    healthy: snapshot.helperSha256 === policy.helperSha256 && (!policy.fontReadability || current.sha === fontReadabilityIdentity.sha && current.tag === `hermes-agent:main-${current.sha}-desktop-overlay-${fontReadabilityIdentity.patchSha256}`) && snapshot.containers.length === policy.services.length && policy.services.every(service => snapshot.containers.filter(container => container.service === service).length === 1) && snapshot.containers.every(container => container.running && container.health === 'healthy' && container.imageId === current.image_id && container.revision === current.sha && container.stamp.commit === current.sha && container.provenance.revision === current.sha && (container.overlaySha256 ?? null) === (policy.overlay?.sha256 ?? null) && (!policy.botDesktop || container.botDesktop === true) && releaseStampMatches(policy, container.stamp)),
    verified: snapshot.verification?.passed === true };
};
export const desktopIdentity = (snapshot: DesktopSnapshot, preUpdateEligible?: boolean): DesktopIdentity => {
  const running = snapshot.runningArtifact, installedSha = snapshot.installedArtifact.commit ?? '';
  // Preserve known mismatches, while equal SHA alone cannot prove that a running bundle has settled.
  const runningSha = running?.commit && (running.commit !== installedSha || (running.bundleSwapPending === false && running.bundleOutOfSync === false)) ? running.commit : null;
  return {
    installedSha, runningSha, running: snapshot.processes.length > 0,
    verified: snapshot.nativeHomeVerified && !snapshot.checkoutDirty && snapshot.checkoutSha === snapshot.checkoutStamp.commit
      && snapshot.checkoutSha === snapshot.installedArtifact.commit && snapshot.installedArtifact.dirty === false && snapshot.checkoutStamp.dirty === false && snapshot.installedArtifact.source !== 'fallback'
      && running?.bundleSwapPending !== true && running?.bundleOutOfSync !== true,
    ...(preUpdateEligible === undefined ? {} : { preUpdateEligible }),
  };
};

export async function runUpdateCli(args: string[], projectRoot: string): Promise<void> {
  const { command, options } = parseUpdateArguments(args);
  const target: HermesTarget = JSON.parse(await readFile(join(projectRoot, 'architecture/targets.json'), 'utf8'));
  const policyPath = join(projectRoot, 'architecture/backend-update-policy.json');
  let policy: BackendUpdatePolicy = JSON.parse(await readFile(policyPath, 'utf8'));
  let releaseTarget = options.resume ? await loadReleaseTarget(resolve(options.resume)) : null;
  let resume: UpdateCheckpoint<PreparedBackendCandidate> | undefined;
  if (options.resume) {
    try { resume = await loadCheckpoint<PreparedBackendCandidate>(resolve(options.resume)); }
    catch (error) {
      // A Desktop preflight may fail before the first workflow checkpoint; the durable release evidence still owns this run.
      const noCheckpoint = (error as NodeJS.ErrnoException).code === 'ENOENT'
        || (error instanceof Error && error.message === 'Update checkpoint history is empty');
      if (!noCheckpoint || !releaseTarget) throw error;
    }
    validateReleaseResume(releaseTarget, command as UpdateMode, options, resume);
    if (resume && options.sha && resume.targetSha !== options.sha) throw new Error('Resume retains its original pinned SHA');
  }
  const id = resume?.id ?? releaseTarget?.runId ?? randomUUID();
  const directory = options.resume ? resolve(options.resume) : join(projectRoot, 'runs', new Date().toISOString().replaceAll(/[:.]/g, '-') + '-update-' + command + '-' + id.slice(0, 8));
  if (!options.resume) await mkdir(directory, { recursive: false });
  const save = async (name: string, value: unknown) => writeFile(join(directory, `${name}-${randomUUID()}.json`), JSON.stringify(value, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  await save('request', { schemaVersion: 1, id, command, options, actualAuthority: options.apply ? options.authorization : null, effects: options.apply ? 'Only the selected native update mode' : 'Read-only target inspection', acceptance: 'not-recorded' });
  if (command === 'verify') {
    const observed = await verifyBackend(target, policy, options.sha!, directory);
    console.log(JSON.stringify({ runDirectory: directory, mode: 'read-only-verification', sourceSha: options.sha, ...observed.verification }, null, 2));
    if (!observed.verification?.passed) process.exitCode = 2;
    return;
  }
  if (command === 'stage-overlay') {
    const overlay = await stageBackendOverlay(target, policy, options.sha!, resolve(options.patch!), directory);
    await save('staged-overlay', { sourceSha: options.sha, overlay, nextStep: 'Review the staged patch and update backend-update-policy.json overlay identity before resuming the same pinned update' });
    console.log(JSON.stringify({ runDirectory: directory, overlay, policyChanged: false }, null, 2));
    return;
  }
  if (!options.resume && options.release) releaseTarget = await resolveAndSaveReleaseTarget(directory, { id, mode: command }, options.release);
  if (releaseTarget) policy = { ...policy, releaseTag: releaseTarget.requestedReleaseTag };
  const backendAttempt = join(directory, `backend-attempt-${policy.helperSha256}-${policy.overlay?.sha256 ?? 'source-only'}`);
  await mkdir(backendAttempt, { recursive: true });
  // Desktop imports and configuration are only loaded inside coordinated-mode operations.
  const getDesktop = async () => {
    const module = await import('../adapters/desktop-update');
    const configuredTarget: DesktopUpdateTarget = JSON.parse(await readFile(join(projectRoot, 'architecture/desktop-update-target.json'), 'utf8'));
    const desktopTarget: DesktopUpdateTarget = releaseTarget ? { ...configuredTarget, channel: 'stable', expectedReleaseSha: releaseTarget.sourceSha, expectedReleaseTag: releaseTarget.requestedReleaseTag } : configuredTarget;
    return { module, desktopTarget };
  };
  const requireReleaseDesktop = (identity: DesktopIdentity) => {
    if (releaseTarget && identity.installedSha !== releaseTarget.sourceSha) throw new Error('Completed Desktop SHA differs from the requested release; backend promotion is blocked');
    return identity;
  };
  const result = await runUpdate({ id, mode: command, apply: options.apply, backendSha: command === 'backend-only' ? releaseTarget?.sourceSha ?? options.sha : undefined, expectedReleaseSha: releaseTarget?.sourceSha, resume }, {
    async inspectBackend() { return backendIdentity(await inspectBackend(target, policy, directory), policy); },
    async resolveMain() { return resolveLatestMain(target, policy, directory); },
    async inspectDesktop() {
      const { module, desktopTarget } = await getDesktop();
      const snapshot = await module.inspectDesktop(desktopTarget, directory, { cdpUrl: options.cdp });
      return desktopIdentity(snapshot, module.desktopPreUpdateEligibility(desktopTarget, snapshot).eligible);
    },
    async verifyExistingDesktop() {
      const { module, desktopTarget } = await getDesktop();
      return requireReleaseDesktop(desktopIdentity(await module.verifyExistingDesktopRelease(desktopTarget, directory, { cdpUrl: options.cdp }), false));
    },
    async updateDesktop() {
      const { module, desktopTarget } = await getDesktop();
      const result = await module.updateDesktop(desktopTarget, directory, { authorization: options.authorization!, cdpUrl: options.cdp });
      if (!result.canonicalSha || !result.after || !result.verification?.eligibleForBackend) throw new Error(result.error?.message ?? 'Desktop native update has no verified completed artifact');
      return requireReleaseDesktop({ ...desktopIdentity(result.after), receiptRef: result.receipt?.archivePath });
    },
    async recoverDesktop() {
      const { module, desktopTarget } = await getDesktop();
      const rejectedLaunch = await inspectDesktopRejectedLaunchRetry(directory, desktopTarget.id);
      await save('desktop-rejected-launch-retry-decision', rejectedLaunch);
      const retry = await inspectDesktopPreflightRetry(directory, desktopTarget.id);
      await save('desktop-preflight-retry-decision', retry);
      const result = rejectedLaunch.candidate
        ? await module.updateDesktop(desktopTarget, directory, { authorization: options.authorization!, cdpUrl: options.cdp, retryRejectedLaunch: rejectedLaunch.candidate })
        : retry.eligible
        ? await module.updateDesktop(desktopTarget, directory, { authorization: options.authorization!, cdpUrl: options.cdp })
        : await module.recoverDesktopUpdate(desktopTarget, directory, { cdpUrl: options.cdp });
      if (!result.canonicalSha || !result.after || !result.verification?.eligibleForBackend) throw new Error(result.error?.message ?? 'The existing native Desktop update has no reconciled terminal result yet');
      return requireReleaseDesktop({ ...desktopIdentity(result.after), receiptRef: result.receipt?.archivePath });
    },
    async prepareBackend(sha) {
      if (releaseTarget && sha !== releaseTarget.sourceSha) throw new Error('Backend target differs from the requested release SHA');
      return prepareBackend(target, policy, sha, backendAttempt);
    },
    async deployBackend(candidate) { await deployBackend(target, policy, candidate, backendAttempt, { authorization: options.authorization! }); },
    async verifyBackend(sha) { return backendIdentity(await verifyBackend(target, policy, sha, directory), policy); },
    async save(checkpoint) {
      await saveCheckpoint(directory, checkpoint);
      console.error(`${checkpoint.mode}: ${checkpoint.phase} (${checkpoint.status})`);
    },
  });
  console.log(JSON.stringify({ runDirectory: directory, mode: result.mode, status: result.status, sourceSha: result.targetSha, targetSource: result.targetSource, requestedRelease: releaseTarget?.requestedReleaseTag, requestedSourceSha: releaseTarget?.sourceSha,
    installedVersionsAligned: result.mode === 'coordinated' && result.targetSha !== null && result.desktop?.installedSha === result.targetSha && result.backend?.sha === result.targetSha,
    desktopRunningAlignment: result.mode !== 'coordinated' ? 'not-requested' : !result.desktop?.running ? 'not-running' : result.desktop.runningSha === null ? 'unverified' : result.desktop.runningSha === result.backend?.sha ? 'verified' : 'mismatch',
    backendVerified: result.backend?.verified, acceptance: result.acceptance }, null, 2));
}
