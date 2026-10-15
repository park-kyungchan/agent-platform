import { describe, expect, test } from 'bun:test';
import { validatePolicy, backendImageArguments, backendServiceSelectionPython, backendJobCompleted, backendPrepareMayRetry, correlateBackendTransaction, evaluateBackendSnapshot, preparedCandidateFromSnapshot, type BackendJob, type BackendSnapshot, type BackendTransaction, type BackendUpdatePolicy, type PreparedBackendCandidate } from '../src/adapters/backend-update';
import { backendIdentity } from '../src/update/cli';
import type { HermesTarget } from '../src/adapters/ssh';

const sha = 'c'.repeat(40);
const overlay = 'd'.repeat(64);
const helper = 'e'.repeat(64);
const image = 'sha256:' + 'f'.repeat(64);
const policy: BackendUpdatePolicy = {
  schemaVersion: 1, root: '/docker/target', updaterRoot: '/docker/target/.main-updater', helperPath: '/docker/target/bin/hermes-main-update', helperSha256: helper,
  upstream: 'https://github.com/NousResearch/hermes-agent.git', services: ['hermes-agent', 'hermes-serve'], overlay: { path: `/docker/target/.main-updater/overlays/${overlay}.patch`, sha256: overlay },
};
const target: HermesTarget = {
  schemaVersion: 1, hostId: 'host', project: 'target', ssh: { address: 'invalid.example', user: 'root', hostKeyAlias: 'fixture', port: 22 },
  gatewayService: 'hermes-serve', profile: 'default', expectedProfiles: ['default', 'other'], nativeHome: '/opt/data', nativeUid: 10000, nativeGid: 10000, nativePython: '/opt/hermes/.venv/bin/python', nativeRepository: '/opt/hermes', gatewaySlot: 'gateway-default',
};
function snapshot(): BackendSnapshot {
  const owner = { hostPid: 1001, localPid: 370, startTicks: 9000, namespace: 'pid:[200]', containerId: 'owner', uid: 10000, matchingFdCount: 1 };
  const current = { sha, tag: `hermes-agent:main-${sha}-overlay-${overlay}`, image_id: image, overlay_sha256: overlay, overlay_tree_sha: 'a'.repeat(40) };
  return {
    schemaVersion: 1, observedAtUtc: new Date().toISOString(), hostId: 'host', project: 'target', helperSha256: helper, updaterLock: { device: 1, inode: 2, held: false }, current,
    candidate: { ...current, status: 'prepared', source_verified: true, helperSha256: helper, validation_key: helper, actual_image_id: image,
      validation_evidence: { doctor: { path: '/docker/target/.main-updater/reports/doctor.log', sha256: 'a'.repeat(64), actualSha256: 'a'.repeat(64) }, plugin: { path: '/docker/target/.main-updater/reports/plugin.json', sha256: 'b'.repeat(64), actualSha256: 'b'.repeat(64) } },
    } as PreparedBackendCandidate,
    containers: ['hermes-agent', 'hermes-serve'].map(service => ({
      service, containerId: service === 'hermes-serve' ? 'owner' : 'standby', imageId: image, revision: sha, overlaySha256: overlay, running: true, health: 'healthy', namespace: service === 'hermes-serve' ? 'pid:[200]' : 'pid:[100]', dataSource: '/data',
      rootSlot: { up: service === 'hermes-serve', localPid: service === 'hermes-serve' ? 370 : null, raw: service === 'hermes-serve' ? 'up (pid 370)' : 'down', identity: service === 'hermes-serve' ? owner : null },
      stamp: { commit: sha, overlaySha256: overlay, overlayTreeSha: 'a'.repeat(40) }, provenance: { revision: sha },
    })),
    gatewayLock: { device: 1, inode: 3, held: true, holders: [owner] }, gatewayOwner: owner, ownerPreserved: true,
    native: {
      identify: { pid: 370, start_time: 9000, code_sha: sha }, nativeServing: true, nativeOwnerPid: 370,
      profiles: ['default', 'other'].map(profile => ({ profile, nativeOn: true, liveServed: true, nativeOwnerPid: 370 })),
      status: { gateway_state: 'running', desired_state: 'running', answering_pid: 370 }, health: { code: 200, commit: sha },
      adapters: [{ name: 'api_server', state: 'connected', errorCode: null, configured: true, writerPid: 370 }],
      configuredActiveAdapters: ['api_server'],
    },
  };
}
const job: BackendJob = { schema: 1, job_id: '20261008T000000Z-123-prepare', unit: 'hermes-main-update-20261008T000000Z-123-prepare', action: 'prepare', launched_at: '2026-10-08T00:00:00.000Z', requested_sha: sha, overlay_sha256: overlay, mainPid: 2001 };
function transaction(): BackendTransaction {
  return { run_id: '20261008T000001Z-' + sha.slice(0, 12), pid: 2001, last_phase: 'complete', result: 'passed', candidate: { sha, overlay_sha256: overlay }, history: [{ at: '2026-10-08T00:00:01.000Z', phase: 'preflight', result: 'running' }, { at: '2026-10-08T00:00:02.000Z', phase: 'complete', result: 'passed' }] };
}

describe('pristine official release Bot Screen variant', () => {
  const variant: BackendUpdatePolicy = { ...policy, overlay: undefined, botDesktop: true, releaseTag: 'v0.21.6' };
  const desktopTag = `hermes-agent:main-${sha}-desktop`;
  function prepared() {
    const observed = snapshot();
    observed.candidate = { ...observed.candidate!, overlay_sha256: null, overlay_tree_sha: null, tag: desktopTag,
      bot_desktop_label: '1', actual_stamp: { commit: sha, dirty: false, displayVersion: '0.21.6' } } as PreparedBackendCandidate;
    return observed;
  }
  test('native arguments select an explicit feature variant and release without changing source', () => {
    expect(backendImageArguments(variant, sha)).toEqual(['--sha', sha, '--bot-desktop', '--release-tag', 'v0.21.6']);
    expect(() => validatePolicy(target, { ...variant, overlay: policy.overlay })).toThrow('pristine');
    expect(() => validatePolicy(target, { ...variant, releaseTag: 'main' })).toThrow('canonical');
  });
  test('default image at the same commit cannot satisfy the requested Screen variant', () => {
    const observed = prepared();
    expect(preparedCandidateFromSnapshot(variant, sha, observed).tag).toBe(desktopTag);
    observed.candidate!.tag = `hermes-agent:main-${sha}`;
    expect(() => preparedCandidateFromSnapshot(variant, sha, observed)).toThrow('immutable');
  });
  test('a tag name cannot substitute for independently observed build features and release identity', () => {
    for (const change of [{ bot_desktop_label: '0' }, { actual_stamp: { commit: sha, dirty: true, displayVersion: '0.21.6' } }, { actual_stamp: { commit: sha, dirty: false, displayVersion: 'git.ccccccc' } }]) {
      const observed = prepared();
      Object.assign(observed.candidate!, change);
      expect(() => preparedCandidateFromSnapshot(variant, sha, observed)).toThrow();
    }
  });
  test('a completion at the same source SHA for the base image cannot complete the desktop job', () => {
    const desktopJob = { ...job, overlay_sha256: undefined, bot_desktop: true, release_tag: 'v0.21.6' };
    const tx = transaction(); tx.candidate = { sha, tag: `hermes-agent:main-${sha}` };
    expect(correlateBackendTransaction(desktopJob, [tx])).toBeNull();
    tx.candidate.tag = desktopTag;
    expect(correlateBackendTransaction(desktopJob, [tx])).toEqual(tx);
  });
  test('a serving image must preserve the required feature and stable version', () => {
    const observed = snapshot();
    expect(evaluateBackendSnapshot(target, variant, sha, observed).checks.requestedBotDesktop).toBe(false);
    expect(backendIdentity(observed, variant).healthy).toBe(false);
    for (const container of observed.containers) { container.botDesktop = true; container.stamp.dirty = false; container.stamp.displayVersion = '0.21.6'; }
    expect(evaluateBackendSnapshot(target, variant, sha, observed).checks.requestedBotDesktop).toBe(true);
    expect(evaluateBackendSnapshot(target, variant, sha, observed).checks.requestedReleaseVersion).toBe(true);
  });
});

describe('native backend completion contracts', () => {
  test('correlates one exact launched job with a successful terminal transaction', () => {
    const tx = transaction();
    expect(correlateBackendTransaction(job, [tx])).toEqual(tx);
    expect(backendJobCompleted(job, tx)).toBe(true);
  });
  test('collected units and successful poll command exit codes do not establish completion', () => {
    expect(backendJobCompleted(job, null)).toBe(false);
    expect(backendJobCompleted(job, { ...transaction(), last_phase: 'build' })).toBe(false);
    expect(backendJobCompleted(job, { ...transaction(), result: 'failed' })).toBe(false);
  });
  test('rejects another job, pin, overlay and historical PID reuse', () => {
    const tx = transaction();
    for (const wrong of [
      { ...tx, pid: 2002 },
      { ...tx, candidate: { ...tx.candidate, sha: 'a'.repeat(40) } },
      { ...tx, candidate: { ...tx.candidate, overlay_sha256: 'a'.repeat(64) } },
      { ...tx, history: [{ at: '2026-10-07T00:00:00.000Z', phase: 'complete', result: 'passed' }] },
    ]) expect(backendJobCompleted(job, wrong)).toBe(false);
  });
  test('fails closed if transaction correlation is ambiguous', () => {
    expect(() => correlateBackendTransaction(job, [transaction(), { ...transaction(), run_id: 'second' }])).toThrow('Ambiguous');
  });
  test('only an exact failed prepare job may retry native built-image validation', () => {
    const failed = { ...transaction(), result: 'failed', last_phase: 'predeploy-validation' };
    expect(backendPrepareMayRetry(job, failed, false)).toBe(true);
    expect(backendPrepareMayRetry(job, failed, true)).toBe(false);
    expect(backendPrepareMayRetry({ ...job, action: 'deploy' }, failed, false)).toBe(false);
    expect(backendPrepareMayRetry(job, { ...failed, pid: 2002 }, false)).toBe(false);
    expect(backendPrepareMayRetry(job, transaction(), false)).toBe(false);
  });
});
describe('prepared native candidate reuse', () => {
  test('requires immutable image, source pin, helper pin and both diagnostic hashes', () => {
    expect(preparedCandidateFromSnapshot(policy, sha, snapshot()).image_id).toBe(image);
  });
  test('retains built evidence as nondeployable after failed runtime validation', () => {
    const observed = snapshot();
    observed.candidate = { ...observed.candidate!, status: 'built' };
    expect(() => preparedCandidateFromSnapshot(policy, sha, observed)).toThrow('prepared evidence');
    expect(observed.candidate.status).toBe('built');
  });
  test('rejects changed tag or validator identity', () => {
    for (const changes of [{ actual_image_id: 'sha256:' + 'a'.repeat(64) }, { validation_key: 'a'.repeat(64) }, { tag: 'hermes-agent:latest' }, { overlay_tree_sha: null }, { source_verified: false }]) {
      const observed = snapshot();
      Object.assign(observed.candidate!, changes);
      expect(() => preparedCandidateFromSnapshot(policy, sha, observed)).toThrow();
    }
  });
  test('rejects tampered, absent or out of scope diagnostic evidence', () => {
    for (const name of ['doctor', 'plugin']) {
      const observed = snapshot();
      const candidate = observed.candidate as PreparedBackendCandidate;
      candidate.validation_evidence![name]!.actualSha256 = '0'.repeat(64);
      expect(() => preparedCandidateFromSnapshot(policy, sha, observed)).toThrow('diagnostic evidence');
    }
    const observed = snapshot();
    (observed.candidate as PreparedBackendCandidate).validation_evidence!.doctor!.path = '/other/doctor.log';
    expect(() => preparedCandidateFromSnapshot(policy, sha, observed)).toThrow('diagnostic evidence');
  });
});
describe('targeted backend native serving verification', () => {
  test('accepts both pinned healthy services and actual native serving ownership', () => {
    expect(evaluateBackendSnapshot(target, policy, sha, snapshot()).passed).toBe(true);
  });
  test('healthy Docker and a login redirect cannot substitute for full health.commit', () => {
    const observed = snapshot();
    observed.native!.health = { code: 302 };
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.fullHealthCommit).toBe(false);
    observed.native!.health = { code: 200, commit: sha.slice(0, 12) };
    expect(evaluateBackendSnapshot(target, policy, sha, observed).passed).toBe(false);
  });
  test('rejects siblings that share the callable control socket but fail native serving', () => {
    const observed = snapshot();
    observed.native!.nativeServing = false;
    observed.native!.nativeOwnerPid = null;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.nativeServing).toBe(false);
  });
  test('requires process start identity, actual UID and unchanged owner incarnation', () => {
    const observed = snapshot();
    observed.native!.identify!.start_time = 9001;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.freshIdentity).toBe(false);
    observed.gatewayOwner!.uid = 0;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.exactSingletonOwner).toBe(false);
    observed.ownerPreserved = false;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.ownerIncarnationPreserved).toBe(false);
  });
  test('fails on a second root slot without changing shared desired state', () => {
    const observed = snapshot();
    observed.containers[0]!.rootSlot.up = true;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.standbyParked).toBe(false);
    expect(observed.native!.status.desired_state).toBe('running');
  });
  test('requires every configured profile to be live served by the owner', () => {
    const observed = snapshot();
    observed.native!.profiles[1]!.liveServed = false;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.expectedProfilesServing).toBe(false);
  });
  test('disabled historical adapter failures remain observations without failing health', () => {
    const observed = snapshot();
    observed.native!.adapters!.push({ name: 'telegram', state: 'fatal', errorCode: 'telegram_auth_error', configured: false, writerPid: 14661 });
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(true);
    expect(observed.native!.adapters![1]!.state).toBe('fatal');
  });
  test('configured fatal, failed, unknown and reconnecting states fail adapter health', () => {
    for (const state of ['fatal', 'failed', 'unknown', 'retrying', 'disconnected', null]) {
      const observed = snapshot();
      observed.native!.adapters![0]!.state = state;
      expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
    }
  });
  test('connected active rows require the current owner writer PID and no active error code', () => {
    const observed = snapshot();
    observed.native!.adapters![0]!.writerPid = 371;
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
    observed.native!.adapters![0]!.writerPid = 370;
    observed.native!.adapters![0]!.errorCode = 'active_adapter_error';
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
  });
  test('empty or only disabled rows cannot establish active adapter health', () => {
    const observed = snapshot();
    observed.native!.adapters = [];
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
    observed.native!.adapters.push({ name: 'telegram', state: 'fatal', errorCode: 'telegram_auth_error', configured: false, writerPid: 14661 });
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
  });
  test('every configured active and required adapter needs a current observed row', () => {
    const observed = snapshot();
    observed.native!.configuredActiveAdapters!.push('other:slack');
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.activeAdaptersHealthy).toBe(false);
    observed.native!.configuredActiveAdapters = ['api_server'];
    expect(evaluateBackendSnapshot(target, { ...policy, requiredActiveAdapters: ['api_server', 'other:slack'] }, sha, observed).checks.activeAdaptersHealthy).toBe(false);
  });
  test('rejects stale observations and scope drift', () => {
    const observed = snapshot();
    observed.observedAtUtc = new Date(Date.now() - 120000).toISOString();
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.currentObservation).toBe(false);
    observed.containers[0]!.dataSource = '/other-data';
    observed.containers[0]!.revision = 'a'.repeat(40);
    observed.containers[0]!.imageId = 'sha256:' + 'a'.repeat(64);
    const result = evaluateBackendSnapshot(target, policy, sha, observed);
    expect(result.checks.sharedDataMount).toBe(false);
    expect(result.checks.healthyImagesAtPinnedCommit).toBe(false);
    expect(result.checks.sameImmutableImage).toBe(false);
    observed.hostId = 'different-host';
    expect(evaluateBackendSnapshot(target, policy, sha, observed).checks.targetScope).toBe(false);
  });
});


describe('declared single-runtime topology', () => {
  const single = { ...policy, services: ['hermes-serve'] };
  test('policy accepts only intended-only or legacy two, retaining exact helper and path guards', () => {
    expect(() => validatePolicy(target, policy)).not.toThrow();
    expect(() => validatePolicy(target, single)).not.toThrow();
    for (const services of [[], ['hermes-agent'], ['hermes-serve', 'hermes-serve'], ['hermes-agent', 'hermes-serve', 'other']]) {
      expect(() => validatePolicy(target, { ...single, services })).toThrow();
    }
    expect(() => validatePolicy(target, { ...single, helperSha256: 'bad' })).toThrow();
    expect(() => validatePolicy(target, { ...single, helperPath: '/outside/helper' })).toThrow();
  });
  test('exact single service retains native ownership, profile and image requirements', () => {
    const observed = snapshot(); observed.containers = observed.containers.filter(c => c.service === 'hermes-serve');
    expect(evaluateBackendSnapshot(target, single, sha, observed).passed).toBe(true);
    expect(backendIdentity(observed, single).healthy).toBe(true);
    expect(backendIdentity(observed, policy).healthy).toBe(false);
    expect(backendIdentity(snapshot(), single).healthy).toBe(false);
    for (const mutate of [
      (s: BackendSnapshot) => { s.containers[0]!.service = 'hermes-agent'; },
      (s: BackendSnapshot) => { s.containers.push(structuredClone(s.containers[0]!)); },
      (s: BackendSnapshot) => { s.containers[0]!.provenance.revision = 'a'.repeat(40); },
      (s: BackendSnapshot) => { s.containers[0]!.imageId = 'sha256:' + 'a'.repeat(64); },
    ]) {
      const changed = structuredClone(observed); mutate(changed);
      expect(backendIdentity(changed, single).healthy).toBe(false);
      expect(evaluateBackendSnapshot(target, single, sha, changed).passed).toBe(false);
    }
    observed.native!.profiles[1]!.nativeOn = false;
    expect(evaluateBackendSnapshot(target, single, sha, observed).passed).toBe(false);
  });
  test('actual snapshot selection rejects hidden legacy siblings, missing and duplicate services', () => {
    const source = backendServiceSelectionPython + `
import json
def container(service):return {'Config':{'Labels':{'com.docker.compose.service':service}}}
rows=[]
for services,observed in [(['hermes-serve'],['hermes-serve']),(['hermes-agent','hermes-serve'],['hermes-agent','hermes-serve']),(['hermes-serve'],['hermes-serve','traefik']),(['hermes-serve'],['hermes-agent','hermes-serve']),(['hermes-serve'],[]),(['hermes-serve'],['hermes-serve','hermes-serve']),(['hermes-agent','hermes-serve'],['hermes-serve'])]:
 try:select_backend_containers([container(s) for s in observed],services);rows.append(True)
 except AssertionError:rows.append(False)
print(json.dumps(rows))
`;
    const run = Bun.spawnSync(['python3', '-c', source], { stdout: 'pipe', stderr: 'pipe' });
    expect(run.exitCode).toBe(0);
    expect(JSON.parse(run.stdout.toString())).toEqual([true,true,true,false,false,false,false]);
  });
});
