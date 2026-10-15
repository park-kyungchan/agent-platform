export type UpdateMode = 'backend-only' | 'coordinated';
export type UpdatePhase = 'inspect' | 'desktop-update' | 'target-pinned' | 'backend-prepare' | 'backend-deploy' | 'verify' | 'complete';
export interface DesktopIdentity {
  installedSha: string;
  runningSha: string | null;
  running: boolean;
  verified: boolean;
  /** Only the native adapter can admit an exact reviewed dirty pre-update tree. */
  preUpdateEligible?: boolean;
  receiptRef?: string;
}
export interface BackendIdentity {
  sha: string;
  imageId: string;
  healthy: boolean;
  verified: boolean;
}
export interface UpdateCheckpoint<Candidate = unknown> {
  schemaVersion: 1;
  id: string;
  revision: number;
  mode: UpdateMode;
  phase: UpdatePhase;
  status: 'planned' | 'running' | 'failed' | 'complete';
  createdAtUtc: string;
  updatedAtUtc: string;
  targetSha: string | null;
  targetSource: 'upstream-main-at-start' | 'explicit-backend-sha' | 'completed-desktop-artifact' | 'verified-existing-desktop-release' | null;
  desktop: DesktopIdentity | null;
  candidate: Candidate | null;
  backend: BackendIdentity | null;
  deploymentAttempted: boolean;
  failure: { phase: UpdatePhase; message: string } | null;
  acceptance: 'not-recorded';
}
export interface UpdateOperations<Candidate> {
  inspectBackend(): Promise<BackendIdentity>;
  resolveMain(): Promise<string>;
  inspectDesktop(): Promise<DesktopIdentity>;
  updateDesktop(): Promise<DesktopIdentity>;
  verifyExistingDesktop?(): Promise<DesktopIdentity>;
  recoverDesktop?(): Promise<DesktopIdentity | null>;
  prepareBackend(sha: string): Promise<Candidate>;
  deployBackend(candidate: Candidate): Promise<void>;
  verifyBackend(sha: string): Promise<BackendIdentity>;
  save(checkpoint: UpdateCheckpoint<Candidate>): Promise<void>;
}
export interface UpdateRequest<Candidate> {
  id: string;
  mode: UpdateMode;
  apply: boolean;
  backendSha?: string;
  expectedReleaseSha?: string;
  resume?: UpdateCheckpoint<Candidate>;
}
const requireSha = (sha: string) => {
  if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error('A full lowercase source commit SHA is required');
  return sha;
};
const requireDesktop = (identity: DesktopIdentity) => {
  requireSha(identity.installedSha);
  if (!identity.verified || (identity.running && identity.runningSha !== null && identity.runningSha !== identity.installedSha)) throw new Error('Desktop artifact and running identity are not verified');
  return identity;
};
const requireDesktopBeforeUpdate = (identity: DesktopIdentity) => {
  requireSha(identity.installedSha);
  if ((!identity.verified && identity.preUpdateEligible !== true) || (identity.running && identity.runningSha !== null && identity.runningSha !== identity.installedSha)) throw new Error('Desktop pre-update identity is not verified or explicitly eligible');
  return identity;
};

/** Native adapters own lifecycle operations; this function only sequences and checkpoints them. */
export async function runUpdate<Candidate>(request: UpdateRequest<Candidate>, operations: UpdateOperations<Candidate>): Promise<UpdateCheckpoint<Candidate>> {
  if (request.mode === 'coordinated' && request.backendSha) throw new Error('Coordinated target comes from the completed native Desktop updater');
  if (request.expectedReleaseSha) requireSha(request.expectedReleaseSha);
  if (request.resume && (request.resume.mode !== request.mode || request.resume.id !== request.id)) throw new Error('Resume scope does not match the original update');
  const now = new Date().toISOString();
  const state: UpdateCheckpoint<Candidate> = request.resume ? structuredClone(request.resume) : {
    schemaVersion: 1, id: request.id, revision: 0, mode: request.mode, phase: 'inspect', status: request.apply ? 'running' : 'planned',
    createdAtUtc: now, updatedAtUtc: now, targetSha: null, targetSource: null, desktop: null, candidate: null, backend: null,
    deploymentAttempted: false, failure: null, acceptance: 'not-recorded',
  };
  const save = async (phase: UpdatePhase) => {
    state.phase = phase; state.revision += 1; state.updatedAtUtc = new Date().toISOString();
    await operations.save(structuredClone(state));
  };
  try {
    state.failure = null;
    state.status = request.apply ? 'running' : 'planned';
    state.backend = await operations.inspectBackend();
    if (request.mode === 'coordinated') {
      const observedDesktop = await operations.inspectDesktop();
      let currentDesktop = state.targetSha && state.desktop ? requireDesktop(observedDesktop) : requireDesktopBeforeUpdate(observedDesktop);
      if (state.targetSource === 'completed-desktop-artifact' && state.desktop?.installedSha === currentDesktop.installedSha && state.desktop.receiptRef) {
        currentDesktop = { ...currentDesktop, receiptRef: state.desktop.receiptRef };
      }
      if (!request.apply) {
        state.desktop = currentDesktop;
        await save(request.resume?.phase === 'desktop-update' ? 'desktop-update' : 'inspect');
        return state;
      }
      if (state.targetSha && state.desktop) {
        if (currentDesktop.installedSha !== state.targetSha) throw new Error('Desktop changed after target pinning; the saved coordinated update must be reviewed');
        if (request.expectedReleaseSha && currentDesktop.installedSha !== request.expectedReleaseSha) throw new Error('Desktop source differs from the requested release');
        if (state.targetSource === 'verified-existing-desktop-release') {
          if (!operations.verifyExistingDesktop) throw new Error('Resuming a verified existing Desktop release requires fresh native artifact integrity verification');
          await save('inspect');
          const verified = requireDesktop(await operations.verifyExistingDesktop());
          if (verified.installedSha !== state.targetSha) throw new Error('Verified existing Desktop changed after target pinning');
          state.desktop = verified;
        } else state.desktop = currentDesktop;
      } else {
        let desktopSource: 'completed-desktop-artifact' | 'verified-existing-desktop-release' = 'completed-desktop-artifact';
        if (request.resume && state.phase === 'desktop-update') {
          await save('desktop-update');
          const recovered = operations.recoverDesktop ? await operations.recoverDesktop() : null;
          if (!recovered) throw new Error('Interrupted Desktop update requires its existing native result to be reconciled before another native invocation');
          state.desktop = requireDesktop(recovered);
        } else if (request.expectedReleaseSha && currentDesktop.verified && currentDesktop.installedSha === request.expectedReleaseSha && operations.verifyExistingDesktop) {
          await save('inspect');
          state.desktop = requireDesktop(await operations.verifyExistingDesktop());
          desktopSource = 'verified-existing-desktop-release';
        } else {
          await save('desktop-update');
          state.desktop = requireDesktop(await operations.updateDesktop());
        }
        if (request.expectedReleaseSha && state.desktop.installedSha !== request.expectedReleaseSha) throw new Error('Desktop source differs from the requested release');
        state.targetSha = state.desktop.installedSha;
        state.targetSource = desktopSource;
      }
    } else if (!state.targetSha) {
      state.targetSha = requireSha(request.backendSha ?? await operations.resolveMain());
      state.targetSource = request.backendSha ? 'explicit-backend-sha' : 'upstream-main-at-start';
    }
    if (!state.targetSha) throw new Error('No verified target source commit');
    requireSha(state.targetSha);
    await save('target-pinned');
    if (!request.apply) return state;
    if (state.deploymentAttempted && state.candidate && state.backend.sha === state.targetSha && state.backend.healthy) {
      await save('backend-deploy');
      // A interrupted observation after deployment may still owe only native standby convergence.
      await operations.deployBackend(state.candidate);
    } else if (state.backend.sha !== state.targetSha || !state.backend.healthy) {
      await save('backend-prepare');
      // The native helper rechecks/reuses its own immutable candidate and hashed validation cache.
      state.candidate = await operations.prepareBackend(state.targetSha);
      await save('backend-deploy');
      state.deploymentAttempted = true;
      await save('backend-deploy');
      await operations.deployBackend(state.candidate);
    }
    await save('verify');
    state.backend = await operations.verifyBackend(state.targetSha);
    if (!state.backend.healthy || !state.backend.verified || state.backend.sha !== state.targetSha) throw new Error('Backend serving identity did not pass verification');
    if (request.mode === 'coordinated') {
      const completedReceiptRef = state.targetSource === 'completed-desktop-artifact' ? state.desktop?.receiptRef : undefined;
      state.desktop = requireDesktop(await operations.inspectDesktop());
      if (state.desktop.installedSha !== state.targetSha) throw new Error('Desktop/backend source versions diverged during final verification');
      if (request.expectedReleaseSha && state.desktop.installedSha !== request.expectedReleaseSha) throw new Error('Desktop source differs from the requested release');
      if (completedReceiptRef) state.desktop = { ...state.desktop, receiptRef: completedReceiptRef };
    }
    state.status = 'complete';
    await save('complete');
    return state;
  } catch (error) {
    state.status = 'failed';
    state.failure = { phase: state.phase, message: error instanceof Error ? error.message : String(error) };
    await save(state.phase);
    throw error;
  }
}
