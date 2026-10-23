import { describe, expect, test } from 'bun:test';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import { fontReadabilityIdentity, validatePolicy, backendImageArguments, backendServiceSelectionPython, backendJobCompleted, backendPrepareMayRetry, correlateBackendTransaction, evaluateBackendSnapshot, preparedCandidateFromSnapshot, type BackendJob, type BackendSnapshot, type BackendTransaction, type BackendUpdatePolicy, type PreparedBackendCandidate } from '../src/adapters/backend-update';
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


describe('opt-in exact font-derived release',()=>{
 const font=fontReadabilityIdentity;
 const variant:BackendUpdatePolicy={...policy,botDesktop:true,releaseTag:font.releaseTag,fontReadability:true,overlay:{path:`${policy.updaterRoot}/overlays/${font.patchSha256}.patch`,sha256:font.patchSha256}};
 test('requires explicit exact tuple; no arbitrary Bot/release overlay is admitted',()=>{
  expect(()=>validatePolicy(target,variant)).not.toThrow();
  expect(backendImageArguments(variant,font.sha)).toContain('--font-readability');
  expect(()=>backendImageArguments(variant,sha)).toThrow('source SHA');
  for(const change of [{fontReadability:false},{botDesktop:false},{releaseTag:'v0.22.1'},{overlay:policy.overlay}])expect(()=>validatePolicy(target,{...variant,...change})).toThrow();
 });
 test('prepared readback requires derived stamp and font evidence; reported same image is insufficient',()=>{
  const observed=snapshot();const candidate=observed.candidate! as PreparedBackendCandidate & {actual_image_id:string};
  Object.assign(candidate,{sha:font.sha,tag:`hermes-agent:main-${font.sha}-desktop-overlay-${font.patchSha256}`,overlay_sha256:font.patchSha256,bot_desktop_label:'1',actual_stamp:{commit:font.sha,dirty:true,displayVersion:'0.22.2',overlaySha256:font.patchSha256,overlayTreeSha:'a'.repeat(40)}});
  expect(()=>preparedCandidateFromSnapshot(variant,font.sha,observed)).toThrow('diagnostic');
  candidate.validation_evidence!.fonts={path:policy.updaterRoot+'/reports/font.json',sha256:'c'.repeat(64),actualSha256:'c'.repeat(64)};
  expect(()=>preparedCandidateFromSnapshot(variant,font.sha,observed)).not.toThrow();
  candidate.actual_stamp!.dirty=false;expect(()=>preparedCandidateFromSnapshot(variant,font.sha,observed)).toThrow('release version');
 });
});


test('font verification never qualifies metadata without a matching fresh immutable-image proof',()=>{
 const font=fontReadabilityIdentity,desired:BackendUpdatePolicy={...policy,fontReadability:true,botDesktop:true,releaseTag:font.releaseTag,overlay:{path:`${policy.updaterRoot}/overlays/${font.patchSha256}.patch`,sha256:font.patchSha256}};
 const observed=snapshot();Object.assign(observed.current!,{sha:font.sha,tag:`hermes-agent:main-${font.sha}-desktop-overlay-${font.patchSha256}`,overlay_sha256:font.patchSha256});
 for(const c of observed.containers){Object.assign(c,{revision:font.sha,overlaySha256:font.patchSha256,botDesktop:true});Object.assign(c.stamp,{commit:font.sha,overlaySha256:font.patchSha256,dirty:true,displayVersion:'0.22.2'});c.provenance.revision=font.sha;}
 observed.native!.identify!.code_sha=font.sha;observed.native!.health.commit=font.sha;
 expect(evaluateBackendSnapshot(target,desired,font.sha,observed).checks.freshImageFontReadability).toBe(false);
 const proof={schema:1,check:'font-readability',result:'passed',uid:10000,files:10,fresh_container:true,live_data_mounted:false,imageId:image,overlaySha256:font.patchSha256,overlayTreeSha:'a'.repeat(40)} as const;
 observed.fontProbeIncarnationPreserved=true;observed.fontReadabilityProof=proof;expect(evaluateBackendSnapshot(target,desired,font.sha,observed).passed).toBe(true);
 for(const change of [{imageId:'sha256:'+'0'.repeat(64)},{overlayTreeSha:'0'.repeat(40)},{fresh_container:false},{uid:0},{live_data_mounted:true}]){
  observed.fontReadabilityProof={...proof,...change} as unknown as typeof proof;expect(evaluateBackendSnapshot(target,desired,font.sha,observed).checks.freshImageFontReadability).toBe(false);
 }
});

test('actual typed workflow and already-deployed verification require fresh font read without ordinary-route calls',async()=>{
 const root=resolve(import.meta.dir,'..'),dir=await mkdtemp(join(tmpdir(),'typed-font-verify-'));
 const font=fontReadabilityIdentity,desired:BackendUpdatePolicy={...policy,parkStandby:false,fontReadability:true,botDesktop:true,releaseTag:font.releaseTag,overlay:{path:`${policy.updaterRoot}/overlays/${font.patchSha256}.patch`,sha256:font.patchSha256}};
 const derived=snapshot();Object.assign(derived.current!,{sha:font.sha,tag:`hermes-agent:main-${font.sha}-desktop-overlay-${font.patchSha256}`,overlay_sha256:font.patchSha256});
 for(const c of derived.containers){Object.assign(c,{revision:font.sha,overlaySha256:font.patchSha256,botDesktop:true});Object.assign(c.stamp,{commit:font.sha,overlaySha256:font.patchSha256,dirty:true,displayVersion:'0.22.2'});c.provenance.revision=font.sha;}
 derived.native!.identify!.code_sha=font.sha;derived.native!.health.commit=font.sha;
 const file=join(dir,'oracle.ts');
 const script=`import {mock} from 'bun:test';
const root=${JSON.stringify(root)},desired=${JSON.stringify(desired)},target=${JSON.stringify(target)},derived=${JSON.stringify(derived)},dir=${JSON.stringify(dir)},scenario=Bun.argv[2];
const font=${JSON.stringify(font)},calls=[],saved=[];
let observed=structuredClone(derived);const ordinary=scenario==='ordinary';
const policy=ordinary?{...desired,fontReadability:false,overlay:undefined}:desired;
if(scenario==='pristine'||ordinary){observed.current.tag='hermes-agent:main-'+font.sha+'-desktop';observed.current.overlay_sha256=null;for(const c of observed.containers){c.overlaySha256=null;c.stamp.overlaySha256=undefined;c.stamp.dirty=false;}}
mock.module(root+'/src/adapters/ssh.ts',()=>({executeRemotePython:async(_t,operation,source)=>{
 operation=operation.startsWith('backend-verify-font-readability-')?'verify-font-readability':operation.startsWith('backend-inspect-')?'inspect':operation.startsWith('backend-verify-')?'verify':operation;
 calls.push(operation);
 if(operation==='verify-font-readability'){
  if(!source.includes('verify-font-readability')||!source.includes(derived.current.image_id)||!source.includes(derived.current.overlay_tree_sha))throw Error('UNBOUND_FONT_PROBE');
  if(scenario.endsWith('bad'))return {exitCode:7,stdout:'',stderr:'fresh same-image font0600 unreadable'};
  return {exitCode:0,stdout:JSON.stringify({schema:1,check:'font-readability',result:'passed',uid:10000,files:10,fresh_container:true,live_data_mounted:false,imageId:derived.current.image_id,overlaySha256:font.patchSha256,overlayTreeSha:derived.current.overlay_tree_sha})};
 }
 if(operation!=='inspect'&&operation!=='verify')throw Error('UNEXPECTED_NATIVE_OPERATION:'+operation);
 return {exitCode:0,stdout:JSON.stringify(observed)};
}}));
const native=await import(root+'/src/adapters/backend-update.ts'),{backendIdentity}=await import(root+'/src/update/cli.ts'),{runUpdate}=await import(root+'/src/update/workflow.ts');
const candidate={...derived.current,status:'prepared',source_verified:true,helperSha256:desired.helperSha256};
let status;
try{
 if(scenario.startsWith('deploy-')){const out=await native.deployBackend(target,policy,candidate,dir,{authorization:'owned synthetic source-only test'});status=out.verification.passed?'complete':'failed';}
 else{const out=await runUpdate({id:'source-only-typed-font-oracle',mode:'backend-only',apply:true,backendSha:font.sha},{
 inspectBackend:async()=>backendIdentity(observed,policy),resolveMain:async()=>{throw Error('UNEXPECTED_MAIN')},inspectDesktop:async()=>{throw Error('UNEXPECTED_DESKTOP')},updateDesktop:async()=>{throw Error('UNEXPECTED_DESKTOP')},
 prepareBackend:async()=>{calls.push('prepare');if(scenario!=='pristine')throw Error('UNEXPECTED_PREPARE');return candidate;},
 deployBackend:async()=>{calls.push('deploy');observed=structuredClone(derived);await native.deployBackend(target,policy,candidate,dir,{authorization:'owned synthetic source-only test'});},
 verifyBackend:async()=>backendIdentity(await native.verifyBackend(target,policy,font.sha,dir),policy),save:async(s)=>saved.push(s)});status=out.status;}
}catch(error){status='failed';if(!scenario.endsWith('bad'))throw error;}
console.log(JSON.stringify({scenario,status,calls,completeCheckpoint:saved.some(s=>s.phase==='complete'),lastStatus:saved.at(-1)?.status,nativeTransport:'mock-only-no-SSH-Docker-systemd'}));`;
 try{
  await writeFile(file,script);
  for(const scenario of ['derived-good','derived-bad','deploy-good','deploy-bad','pristine','ordinary']){
   const p=Bun.spawn([process.execPath,file,scenario],{cwd:root,stdout:'pipe',stderr:'pipe'});const [out,err,code]=await Promise.all([new Response(p.stdout).text(),new Response(p.stderr).text(),p.exited]);expect({scenario,code,err}).toMatchObject({scenario,code:0});const result=JSON.parse(out);
   expect(result.status).toBe(scenario.endsWith('bad')?'failed':'complete');
   expect(result.calls.includes('verify-font-readability')).toBe(scenario!=='ordinary');
   expect(result.completeCheckpoint).toBe(!scenario.endsWith('bad')&&!scenario.startsWith('deploy-'));
   if(scenario==='pristine'){expect(result.calls).toContain('prepare');expect(result.calls).toContain('deploy');}
   else{expect(result.calls).not.toContain('prepare');expect(result.calls).not.toContain('deploy');}
  }
 }finally{await rm(dir,{recursive:true,force:true});}
});


test('font probe binds completion to fresh full serving and unchanged required incarnation identities',async()=>{
 const root=resolve(import.meta.dir,'..'),dir=await mkdtemp(join(tmpdir(),'font-incarnation-'));
 const font=fontReadabilityIdentity;
 const desired:BackendUpdatePolicy={...policy,fontReadability:true,botDesktop:true,releaseTag:font.releaseTag,overlay:{path:`${policy.updaterRoot}/overlays/${font.patchSha256}.patch`,sha256:font.patchSha256}};
 const derived=snapshot();Object.assign(derived.current!,{sha:font.sha,tag:`hermes-agent:main-${font.sha}-desktop-overlay-${font.patchSha256}`,overlay_sha256:font.patchSha256});
 for(const c of derived.containers){Object.assign(c,{revision:font.sha,overlaySha256:font.patchSha256,botDesktop:true});Object.assign(c.stamp,{commit:font.sha,overlaySha256:font.patchSha256,dirty:true,displayVersion:'0.22.2'});c.provenance.revision=font.sha;}
 derived.native!.identify!.code_sha=font.sha;derived.native!.health.commit=font.sha;
 const file=join(dir,'oracle.ts');
 const script=`import {mock} from 'bun:test';
import {mkdir,readdir,readFile} from 'node:fs/promises';
const root=${JSON.stringify(root)},target=${JSON.stringify(target)},desired=${JSON.stringify(desired)},before=${JSON.stringify(derived)},base=${JSON.stringify(dir)},scenario=Bun.argv[2];
const dir=base+'/'+scenario;await mkdir(dir);const calls=[],saved=[];let probes=0,observations=0;
const ordinary=scenario==='ordinary',policy=ordinary?{...desired,fontReadability:false,overlay:undefined}:desired;
if(ordinary){before.current.tag='hermes-agent:main-'+before.current.sha+'-desktop';before.current.overlay_sha256=null;for(const c of before.containers){c.overlaySha256=null;c.stamp.overlaySha256=undefined;c.stamp.dirty=false;}}
const after=structuredClone(before);
// Incidental observation timing/order/slot formatting must not be an identity.
after.observedAtUtc=new Date(Date.now()+1).toISOString();after.containers.reverse();after.containers.forEach(c=>c.rootSlot.raw+=' synthetic age');
const owner=after.gatewayOwner,container=after.containers.find(c=>c.service===target.gatewayService);
if(scenario==='owner-death'){after.native.nativeServing=false;after.ownerPreserved=false;}
if(scenario==='container'){container.containerId='replacement';owner.containerId='replacement';}
if(scenario==='image'){for(const c of after.containers)c.imageId='sha256:'+'1'.repeat(64);after.current.image_id=container.imageId;}
if(scenario==='host-pid')owner.hostPid++;
if(scenario==='local-pid'){owner.localPid++;container.rootSlot.localPid++;after.native.identify.pid++;after.native.nativeOwnerPid++;after.native.status.answering_pid++;for(const p of after.native.profiles)p.nativeOwnerPid++;for(const a of after.native.adapters)a.writerPid++;}
if(scenario==='start'){owner.startTicks++;after.native.identify.start_time++;}
if(scenario==='namespace'){owner.namespace='pid:[new]';container.namespace=owner.namespace;}
if(scenario==='gateway-lock')after.gatewayLock.inode++;
if(scenario==='lock-device')after.gatewayLock.device++;
if(scenario==='updater-lock')after.updaterLock.inode++;
if(scenario==='holder')after.gatewayLock.holders=[{...owner,hostPid:999}];
if(scenario==='source'){after.current.sha='1'.repeat(40);for(const c of after.containers){c.revision=after.current.sha;c.stamp.commit=after.current.sha;c.provenance.revision=after.current.sha;}}
if(scenario==='patch'){after.current.overlay_sha256='1'.repeat(64);for(const c of after.containers){c.overlaySha256=after.current.overlay_sha256;c.stamp.overlaySha256=after.current.overlay_sha256;}}
if(scenario==='tree'){after.current.overlay_tree_sha='1'.repeat(40);for(const c of after.containers)c.stamp.overlayTreeSha=after.current.overlay_tree_sha;}
if(scenario==='container-tree')container.stamp.overlayTreeSha='1'.repeat(40);
if(scenario==='mount')for(const c of after.containers)c.dataSource='/replacement';
if(scenario==='missing-host-pid'){delete owner.hostPid;delete before.gatewayOwner.hostPid;}
if(scenario==='missing-tree'){delete container.stamp.overlayTreeSha;delete before.containers.find(c=>c.service===target.gatewayService).stamp.overlayTreeSha;}
if(scenario==='missing-updater-lock'){delete after.updaterLock.inode;delete before.updaterLock.inode;}
mock.module(root+'/src/adapters/ssh.ts',()=>({executeRemotePython:async(_t,operation,source)=>{
 if(operation.startsWith('backend-verify-font-readability-')){
  probes++;calls.push('font');
  if(!source.includes(before.current.image_id)||!source.includes(before.current.overlay_tree_sha))throw Error('UNBOUND_PROBE');
  if(scenario==='probe-transport')return {exitCode:7,stdout:'',reason:'synthetic-probe-failure',remoteOutcome:'unknown'};
  let proof={schema:1,check:'font-readability',result:'passed',uid:10000,files:10,fresh_container:true,live_data_mounted:false,imageId:before.current.image_id,overlaySha256:before.current.overlay_sha256,overlayTreeSha:before.current.overlay_tree_sha};
  if(scenario==='proof-mismatch')proof.imageId='sha256:'+'0'.repeat(64);
  if(scenario==='proof-missing')proof={};
  return {exitCode:0,stdout:JSON.stringify(proof)};
 }
 if(!operation.startsWith('backend-verify-'))throw Error('UNEXPECTED_OPERATION:'+operation);
 observations++;calls.push('serving');
 if(!source.includes('host_gateway_serving')||!source.includes('ownerPreserved')||!source.includes('--user')||!source.includes('HERMES_HOME='))throw Error('NOT_FULL_GUARDED_SERVING');
 if(probes&&scenario==='post-transport')return {exitCode:7,stdout:'',reason:'synthetic-post-failure',remoteOutcome:'unknown'};
 return {exitCode:0,stdout:JSON.stringify(probes?after:before)};
}}));
const native=await import(root+'/src/adapters/backend-update.ts'),{backendIdentity}=await import(root+'/src/update/cli.ts'),{runUpdate}=await import(root+'/src/update/workflow.ts');
let returned=null,error=null,result;
try{result=await runUpdate({id:'synthetic-font-incarnation',mode:'backend-only',apply:true,backendSha:before.current.sha},{
 inspectBackend:async()=>backendIdentity(before,policy),resolveMain:async()=>{throw Error('NO_MAIN')},inspectDesktop:async()=>{throw Error('NO_DESKTOP')},updateDesktop:async()=>{throw Error('NO_DESKTOP')},prepareBackend:async()=>{throw Error('NO_PREPARE')},deployBackend:async()=>{throw Error('NO_DEPLOY')},
 verifyBackend:async()=>{returned=await native.verifyBackend(target,policy,before.current.sha,dir);return backendIdentity(returned,policy)},save:async s=>saved.push(s)});
}catch(e){error=e.message;}
const evidence=[];for(const f of await readdir(dir)){if(f.startsWith('backend-verification-'))evidence.push(JSON.parse(await readFile(dir+'/'+f,'utf8')));}
console.log(JSON.stringify({scenario,calls,probes,observations,status:result?.status,error,complete:saved.some(s=>s.phase==='complete'),returned,evidence}));`;
 try{
  await writeFile(file,script);
  for(const scenario of ['owner-death','stable','ordinary','container','image','host-pid','local-pid','start','namespace','gateway-lock','lock-device','updater-lock','holder','source','patch','tree','container-tree','mount','missing-host-pid','missing-tree','missing-updater-lock','post-transport','probe-transport','proof-missing','proof-mismatch']){
   const p=Bun.spawn([process.execPath,file,scenario],{cwd:root,stdout:'pipe',stderr:'pipe'});
   const [out,err,code]=await Promise.all([new Response(p.stdout).text(),new Response(p.stderr).text(),p.exited]);
   expect({scenario,code,err}).toMatchObject({scenario,code:0});const result=JSON.parse(out);
   const pass=['stable','ordinary'].includes(scenario);
   expect({scenario,complete:result.complete}).toEqual({scenario,complete:pass});
   expect(result.probes).toBe(scenario==='ordinary'?0:1);
   expect(result.observations).toBe(scenario==='ordinary'||scenario==='probe-transport'?1:2);
   expect(result.calls).toEqual(scenario==='ordinary'?['serving']:scenario==='probe-transport'?['serving','font']:['serving','font','serving']);
   if(result.returned){
    expect(result.returned.verification.passed).toBe(pass);
    expect(result.evidence).toEqual([result.returned]);
    if(scenario==='stable')expect(result.returned.containers[0].service).toBe('hermes-serve');
   }else expect(['post-transport','probe-transport']).toContain(scenario);
   if(scenario.endsWith('transport'))expect(result.error??result.status).toBeTruthy();
  }
 }finally{await rm(dir,{recursive:true,force:true});}
});
