import { createHash, randomUUID } from 'node:crypto';
import { mkdir, open, readFile, rename } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { executeRemotePython, type HermesTarget } from './ssh';
import { planGatewayOwnerHandoff, gatewayHandoffDriverPython, gatewayHandoffProtocolPython } from './gateway-owner-handoff';

/** The native updater remains the only writer of source, images, overlays and Compose. */
export interface BackendUpdatePolicy {
  schemaVersion: 1;
  helperPath: string;
  helperSha256: string;
  root: string;
  updaterRoot: string;
  upstream: string;
  services: string[];
  overlay?: { path: string; sha256: string };
  /** Official Dockerfile opt-in; keeps source pristine and gives the image a distinct tag. */
  botDesktop?: boolean;
  /** Published release already verified by the requesting project; native Git must match its SHA. */
  releaseTag?: string;
  updaterLock?: { device: number; inode: number };
  pollIntervalMs?: number;
  jobTimeoutMs?: number;
  parkStandby?: boolean;
  /** Defaults to api_server, which supplies the independently checked native health API. */
  requiredActiveAdapters?: string[];
}
export interface BackendImage {
  sha: string;
  tag: string;
  image_id: string;
  overlay_sha256: string | null;
  overlay_tree_sha?: string | null;
}
export interface PreparedBackendCandidate extends BackendImage {
  status: 'prepared' | 'already-current';
  helperSha256: string;
  source_verified: true;
  prepared_run?: string;
  report?: string;
  validation_key?: string;
  validation_evidence?: Record<string, { path: string; sha256: string; actualSha256?: string }>;
  job?: BackendJob;
  /** Independent native readback, separate from the updater's saved candidate claims. */
  bot_desktop_label?: string;
  actual_stamp?: { commit?: string; dirty?: boolean; displayVersion?: string | null };
}
export interface GatewayOwner {
  hostPid: number;
  localPid: number;
  startTicks: number;
  namespace: string;
  containerId: string | null;
  uid: number;
  matchingFdCount: number;
}
export interface BackendContainer {
  service: string;
  containerId: string;
  imageId: string;
  revision: string;
  overlaySha256: string | null;
  botDesktop?: boolean;
  running: boolean;
  health: string;
  namespace: string | null;
  dataSource: string;
  rootSlot: { up: boolean; localPid: number | null; raw: string; identity?: GatewayOwner | null };
  stamp: { commit?: string; overlaySha256?: string; overlayTreeSha?: string; dirty?: boolean; displayVersion?: string | null };
  provenance: { revision?: string };
}
export interface BackendSnapshot {
  schemaVersion: 1;
  observedAtUtc: string;
  hostId: string;
  project: string;
  helperSha256: string;
  updaterLock: { device: number; inode: number; held: boolean };
  containers: BackendContainer[];
  current: BackendImage | null;
  candidate: PreparedBackendCandidate | (BackendImage & { status: string; source_verified?: boolean }) | null;
  gatewayLock: { device: number; inode: number; held: boolean; holders: GatewayOwner[] };
  gatewayOwner: GatewayOwner | null;
  native?: {
    identify: { pid?: number; start_time?: number; code_sha?: string } | null;
    nativeServing: boolean;
    nativeOwnerPid: number | null;
    profiles: { profile: string; nativeOn: boolean; liveServed: boolean; nativeOwnerPid: number | null }[];
    status: { gateway_state?: string; desired_state?: string; answering_pid?: number };
    health: { code: number; commit?: string };
    adapters?: { name: string; state: string | null; errorCode: string | null; configured: boolean; writerPid: number | null }[];
    configuredActiveAdapters?: string[];
  };
  ownerPreserved?: boolean;
  verification?: { passed: boolean; checks: Record<string, boolean> };
}
export interface BackendJob {
  schema: 1;
  job_id: string;
  unit: string;
  action: 'prepare' | 'deploy';
  launched_at: string;
  requested_sha: string;
  overlay_sha256?: string;
  bot_desktop?: boolean;
  release_tag?: string | null;
  mainPid: number;
}
export interface BackendTransaction {
  run_id: string;
  pid: number;
  last_phase: string;
  result: string;
  candidate: { sha: string; tag?: string; overlay_sha256?: string | null };
  history: { at: string; phase: string; result: string }[];
}
interface JobCheckpoint {
  schemaVersion: 1;
  action: 'prepare' | 'deploy';
  sha: string;
  overlaySha256: string | null;
  botDesktop?: boolean;
  releaseTag?: string | null;
  helperSha256: string;
  requestedAtUtc: string;
  baselineJobs: string[];
  job?: BackendJob;
  transaction?: BackendTransaction;
}
const shaPattern = /^[0-9a-f]{40}$/;
const digestPattern = /^[0-9a-f]{64}$/;
const imagePattern = /^sha256:[0-9a-f]{64}$/;
const releaseTagPattern = /^v(?:0|[1-9]\d{0,2})\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;
const literal = (value: unknown) => `json.loads(${JSON.stringify(JSON.stringify(value))})`;

export function validatePolicy(target: HermesTarget, policy: BackendUpdatePolicy): void {
  if (policy.schemaVersion !== 1 || !digestPattern.test(policy.helperSha256)) throw new Error('Invalid backend updater policy');
  for (const path of [policy.root, policy.updaterRoot, policy.helperPath]) {
    if (!path.startsWith('/') || path.includes('\0') || path.split('/').includes('..')) throw new Error('Invalid backend policy path');
  }
  if (!policy.updaterRoot.startsWith(policy.root + '/') || !policy.helperPath.startsWith(policy.root + '/')) throw new Error('Updater must belong to the target root');
  if (!policy.services.includes(target.gatewayService) || new Set(policy.services).size !== policy.services.length || ![1, 2].includes(policy.services.length) || policy.services.some(s => !/^[a-zA-Z0-9_-]+$/.test(s))) throw new Error('Expected only the intended Hermes service or the two bounded legacy services');
  if (!['https://github.com/NousResearch/hermes-agent', 'https://github.com/NousResearch/hermes-agent.git'].includes(policy.upstream)) throw new Error('Backend upstream must be NousResearch/hermes-agent');
  if (policy.overlay && (!digestPattern.test(policy.overlay.sha256) || policy.overlay.path !== `${policy.updaterRoot}/overlays/${policy.overlay.sha256}.patch`)) throw new Error('Overlay must use its content-addressed native path');
  if (policy.botDesktop !== undefined && typeof policy.botDesktop !== 'boolean') throw new Error('Bot desktop must be a boolean');
  if (policy.botDesktop && policy.overlay) throw new Error('Official bot desktop variant requires pristine source without an overlay');
  if (policy.releaseTag !== undefined && !releaseTagPattern.test(policy.releaseTag)) throw new Error('Expected a canonical stable release tag');
  if (policy.releaseTag && policy.overlay) throw new Error('Official release tag requires pristine source without an overlay');
  if (policy.pollIntervalMs !== undefined && (!Number.isFinite(policy.pollIntervalMs) || policy.pollIntervalMs < 1 || policy.pollIntervalMs > 20_000)) throw new Error('Backend poll interval must be between 1 and 20000 ms');
  if (policy.jobTimeoutMs !== undefined && (!Number.isFinite(policy.jobTimeoutMs) || policy.jobTimeoutMs < 1 || policy.jobTimeoutMs > 6 * 60 * 60 * 1000)) throw new Error('Backend job timeout must be positive and at most six hours');
  if (policy.requiredActiveAdapters !== undefined && (!Array.isArray(policy.requiredActiveAdapters) || !policy.requiredActiveAdapters.length || new Set(policy.requiredActiveAdapters).size !== policy.requiredActiveAdapters.length || policy.requiredActiveAdapters.some(name => !/^[a-zA-Z0-9_.:-]+$/.test(name)))) throw new Error('Required active adapter names must be nonempty, unique identifiers');
}
function validateSha(sha: string): void { if (!shaPattern.test(sha)) throw new Error('Expected a full lowercase upstream SHA'); }

async function saveJson(path: string, value: unknown): Promise<void> {
  const temp = `${path}.${randomUUID()}.tmp`;
  const handle = await open(temp, 'wx', 0o600);
  try { await handle.writeFile(JSON.stringify(value, null, 2) + '\n'); await handle.sync(); }
  finally { await handle.close(); }
  await rename(temp, path);
  const directory = await open(dirname(path), 'r');
  try { await directory.sync(); }
  finally { await directory.close(); }
}
async function remote<T>(target: HermesTarget, operation: string, source: string, runDir: string): Promise<T> {
  // Prepare/deploy jobs run under the native updater's separate jobTimeoutMs.
  // Only the multi-step owner handoff needs a longer foreground transport budget.
  const receipt = await executeRemotePython(target, `backend-${operation}-${randomUUID()}`, source, runDir, {
    timeoutMs: operation === 'gateway-owner-handoff' ? 600_000 : 120_000,
    maxStdoutBytes: 16 * 1024 * 1024,
  });
  if (receipt.exitCode !== 0) throw new Error(`Backend ${operation} transport ${receipt.reason} (exit ${receipt.exitCode}; remote outcome ${receipt.remoteOutcome}); inspect its saved execution receipt; automatic replay prohibited`);
  try { return JSON.parse(receipt.stdout) as T; }
  catch { throw new Error(`Backend ${operation} did not return JSON; inspect its saved execution receipt`); }
}

/** Do not hide a still-running retired sibling when a single-runtime policy is selected. */
export const backendServiceSelectionPython = String.raw`
def select_backend_containers(containers,services):
 legacy={'hermes-agent','hermes-serve'}
 assert not any(c['Config']['Labels'].get('com.docker.compose.service') in legacy-set(services) for c in containers),'Undeclared Hermes runtime remains active'
 selected=[c for c in containers if c['Config']['Labels'].get('com.docker.compose.service') in services]
 assert len(selected)==len(services) and {c['Config']['Labels'].get('com.docker.compose.service') for c in selected}==set(services),'Target Compose services missing or duplicated'
 return selected
`;

const snapshotPython = backendServiceSelectionPython + String.raw`
import json,os,pathlib,subprocess,datetime,hashlib,fcntl,stat,re
P=pathlib.Path('/proc'); root=pathlib.Path(policy['updaterRoot'])
def command(args,timeout=8):
 return subprocess.check_output(args,text=True,timeout=timeout).strip()
def safe_json(path):
 path=pathlib.Path(path)
 if not path.exists():return None
 assert not path.is_symlink() and path.is_file() and path.stat().st_size<1048576,'Unsafe evidence file'
 return json.loads(path.read_text())
def metadata(path):
 s=path.lstat();assert stat.S_ISREG(s.st_mode),'Lock must be a regular non-symlink file'
 fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB);held=False;fcntl.flock(fd,fcntl.LOCK_UN)
  except BlockingIOError:held=True
 finally:os.close(fd)
 return {'device':s.st_dev,'inode':s.st_ino,'held':held}
helper=pathlib.Path(policy['helperPath']);assert not helper.is_symlink() and helper.is_file(),'Unsafe updater path'
helper_hash=hashlib.sha256(helper.read_bytes()).hexdigest()
ids=command(['docker','ps','-q','--filter','label=com.docker.compose.project='+target['project']]).split()
cs=json.loads(command(['docker','inspect',*ids])) if ids else []
cs=select_backend_containers(cs,policy['services'])
sources={m['Source'] for c in cs for m in c['Mounts'] if m['Destination']==target['nativeHome']};assert len(sources)==1,'Target data mounts disagree'
lock=pathlib.Path(next(iter(sources)))/'.local/state/hermes/gateway-locks/host-gateway.lock';lock_meta=metadata(lock)
namespaces={os.readlink(P/str(c['State']['Pid'])/'ns/pid'):c['Id'] for c in cs}
def process_identity(pid):
 p=P/str(pid);raw=(p/'stat').read_text();fields=raw[raw.rfind(')')+2:].split();status=(p/'status').read_text().splitlines();namespace=os.readlink(p/'ns/pid')
 count=0
 for fd in (p/'fd').iterdir():
  try:s=fd.stat();count+=int((s.st_dev,s.st_ino)==(lock_meta['device'],lock_meta['inode']))
  except FileNotFoundError:pass
 return {'hostPid':pid,'localPid':int(next(x for x in status if x.startswith('NSpid:')).split()[-1]),'startTicks':int(fields[19]),'namespace':namespace,'containerId':namespaces.get(namespace),'uid':int(next(x for x in status if x.startswith('Uid:')).split()[1]),'matchingFdCount':count}
key=f"{os.major(lock_meta['device']):02x}:{os.minor(lock_meta['device']):02x}:{lock_meta['inode']}"
holders=[process_identity(int(line.split()[4])) for line in (P/'locks').read_text().splitlines() if key in line.split() and '->' not in line.split()]
lock_meta['holders']=holders
def slot_identity(namespace,local_pid):
 for holder in holders:
  if holder['namespace']==namespace and holder['localPid']==local_pid:return holder
 for process in P.iterdir():
  if not process.name.isdigit():continue
  try:
   if os.readlink(process/'ns/pid')!=namespace:continue
   lines=(process/'status').read_text().splitlines()
   if int(next(x for x in lines if x.startswith('NSpid:')).split()[-1])==local_pid:return process_identity(int(process.name))
  except (FileNotFoundError,ProcessLookupError):continue
 return None
state=safe_json(root/'state.json') or {};candidate=state.get('candidate')
if candidate:
 candidate=dict(candidate)
 for evidence in (candidate.get('validation_evidence') or {}).values():
  path=pathlib.Path(evidence['path']);assert path.resolve().is_relative_to((root/'reports').resolve()) and not path.is_symlink(),'Unsafe validation evidence path'
  evidence['actualSha256']=hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() and path.stat().st_size<1048576 else None
 candidate['actual_image_id']=command(['docker','image','inspect','--format','{{.Id}}',candidate['tag']])
 candidate['bot_desktop_label']=command(['docker','image','inspect','--format','{{index .Config.Labels "io.nousresearch.hermes.bot-desktop"}}',candidate['actual_image_id']])
 candidate['actual_stamp']=json.loads(command(['docker','run','--rm','--network','none','--entrypoint','cat',candidate['actual_image_id'],'/opt/hermes/install-stamp.json']))
rows=[]
for c in cs:
 service=c['Config']['Labels']['com.docker.compose.service'];labels=json.loads(command(['docker','image','inspect','--format','{{json .Config.Labels}}',c['Image']])) or {}
 slot=command(['docker','exec','--user','0:0',c['Id'],'/command/s6-svstat','/run/service/'+target['gatewaySlot']]);match=re.search(r'\bpid (\d+)\b',slot)
 def container_json(path):return json.loads(command(['docker','exec',c['Id'],'cat',path]))
 namespace=os.readlink(P/str(c['State']['Pid'])/'ns/pid');local_pid=int(match.group(1)) if match else None
 rows.append({'service':service,'containerId':c['Id'],'imageId':c['Image'],'revision':labels.get('org.opencontainers.image.revision',''),'overlaySha256':labels.get('io.nousresearch.hermes.overlay.sha256') or None,'botDesktop':labels.get('io.nousresearch.hermes.bot-desktop')=='1','running':c['State']['Running'],'health':c['State'].get('Health',{}).get('Status',c['State']['Status']),'namespace':namespace,'dataSource':next(iter(sources)),'rootSlot':{'up':slot.startswith('up '),'localPid':local_pid,'raw':slot,'identity':slot_identity(namespace,local_pid) if local_pid else None},'stamp':container_json(target['nativeRepository']+'/install-stamp.json'),'provenance':container_json('/etc/hermes/image-provenance.json')})
out={'schemaVersion':1,'observedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'hostId':target['hostId'],'project':target['project'],'helperSha256':helper_hash,'updaterLock':metadata(root/'lock'),'containers':rows,'current':state.get('current'),'candidate':candidate,'gatewayLock':lock_meta,'gatewayOwner':holders[0] if len(holders)==1 else None}
`;

function pythonContext(target: HermesTarget, policy: BackendUpdatePolicy): string {
  validatePolicy(target, policy);
  return `import json\ntarget=${literal(target)}\npolicy=${literal(policy)}\n`;
}

export async function inspectBackend(target: HermesTarget, policy: BackendUpdatePolicy, runDir: string): Promise<BackendSnapshot> {
  return remote(target, 'inspect', pythonContext(target, policy) + snapshotPython + '\nprint(json.dumps(out))\n', runDir);
}
export async function resolveLatestMain(target: HermesTarget, policy: BackendUpdatePolicy, runDir: string): Promise<string> {
  const result = await remote<{ sha: string }>(target, 'resolve-main', pythonContext(target, policy) + String.raw`
import subprocess,re,datetime
result=subprocess.run(['git','ls-remote',policy['upstream'],'refs/heads/main'],text=True,capture_output=True,timeout=20)
assert result.returncode==0,'Main lookup failed'
rows=[line.split() for line in result.stdout.splitlines()]
assert len(rows)==1 and len(rows[0])==2 and rows[0][1]=='refs/heads/main' and re.fullmatch('[0-9a-f]{40}',rows[0][0]),'Invalid main response'
print(json.dumps({'sha':rows[0][0],'resolvedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'ref':'refs/heads/main','upstream':policy['upstream']}))
`, runDir);
  validateSha(result.sha);
  return result.sha;
}

export function evaluateBackendSnapshot(target: HermesTarget, policy: BackendUpdatePolicy, sha: string, snapshot: BackendSnapshot): NonNullable<BackendSnapshot['verification']> {
  const owner = snapshot.gatewayOwner;
  const intended = snapshot.containers.find(c => c.service === target.gatewayService);
  const native = snapshot.native;
  const activeAdapters = (native?.adapters ?? []).filter(adapter => adapter.configured !== false);
  const requiredAdapters = new Set([...(policy.requiredActiveAdapters ?? ['api_server']), ...(native?.configuredActiveAdapters ?? [])]);
  const checks = {
    currentObservation: Number.isFinite(Date.parse(snapshot.observedAtUtc)) && Date.now() - Date.parse(snapshot.observedAtUtc) <= 60_000 && Date.now() - Date.parse(snapshot.observedAtUtc) >= -5000,
    targetScope: snapshot.hostId === target.hostId && snapshot.project === target.project,
    helperIdentity: snapshot.helperSha256 === policy.helperSha256,
    updaterLockIdentity: !policy.updaterLock || (snapshot.updaterLock.device === policy.updaterLock.device && snapshot.updaterLock.inode === policy.updaterLock.inode),
    boundedServices: snapshot.containers.length === policy.services.length && policy.services.every(s => snapshot.containers.filter(c => c.service === s).length === 1),
    sameImmutableImage: snapshot.containers.every(c => c.imageId === intended?.imageId && imagePattern.test(c.imageId)),
    nativeCurrentImage: snapshot.current?.sha === sha && snapshot.current.image_id === intended?.imageId && (snapshot.current.overlay_sha256 ?? null) === (policy.overlay?.sha256 ?? null),
    healthyImagesAtPinnedCommit: snapshot.containers.every(c => c.running && c.health === 'healthy' && c.revision === sha && c.stamp.commit === sha && c.provenance.revision === sha && (c.overlaySha256 ?? null) === (c.stamp.overlaySha256 ?? null)),
    expectedOverlay: !policy.overlay || snapshot.containers.every(c => c.overlaySha256 === policy.overlay?.sha256),
    requestedBotDesktop: !policy.botDesktop || snapshot.containers.every(c => c.botDesktop === true),
    requestedReleaseVersion: !policy.releaseTag || snapshot.containers.every(c => c.stamp.dirty === false && c.stamp.displayVersion === policy.releaseTag!.slice(1)),
    sharedDataMount: snapshot.containers.every(c => c.dataSource === intended?.dataSource),
    exactSingletonOwner: snapshot.gatewayLock.held && snapshot.gatewayLock.holders.length === 1 && !!owner && owner.containerId === intended?.containerId && owner.matchingFdCount > 0 && owner.uid === target.nativeUid,
    ownerSlotIdentity: !!owner && !!intended?.rootSlot.up && intended.rootSlot.localPid === owner.localPid && intended.namespace === owner.namespace,
    standbyParked: snapshot.containers.filter(c => c.service !== target.gatewayService).every(c => !c.rootSlot.up),
    nativeServing: !!native?.nativeServing && native.nativeOwnerPid === owner?.localPid,
    freshIdentity: !!owner && native?.identify?.pid === owner.localPid && native.identify.start_time === owner.startTicks && native.identify.code_sha === sha,
    liveStatus: native?.status.gateway_state === 'running' && native.status.desired_state === 'running' && native.status.answering_pid === owner?.localPid,
    expectedProfilesServing: target.expectedProfiles.every(profile => native?.profiles.some(p => p.profile === profile && p.nativeOn && p.liveServed && p.nativeOwnerPid === owner?.localPid)),
    activeAdaptersHealthy: activeAdapters.length > 0 && activeAdapters.every(adapter => adapter.state === 'connected' && adapter.writerPid === owner?.localPid && (adapter.errorCode == null || adapter.errorCode === '')) && [...requiredAdapters].every(name => activeAdapters.some(adapter => adapter.name === name)),
    fullHealthCommit: native?.health.code === 200 && native.health.commit === sha,
    ownerIncarnationPreserved: snapshot.ownerPreserved === true,
  };
  return { passed: Object.values(checks).every(Boolean), checks };
}

const nativeProbePython = String.raw`
import json,urllib.request
from pathlib import Path
from gateway.control_socket import identify_gateway,query_gateway_control
from gateway.host_attach import host_gateway_serving,invalidate_host_gateway_cache
from hermes_cli.profiles import list_profiles
from gateway.run import _profile_runtime_scope
from gateway.config import load_gateway_config
invalidate_host_gateway_cache()
identity=identify_gateway(Path(request['nativeHome']),timeout=2)
owner=host_gateway_serving(request['profile'],wait_for_channel=0)
status=query_gateway_control(Path(request['nativeHome']),'status',timeout=2) or {}
infos=list_profiles(lazy_skill_count=True);profiles=[]
for profile in request['expectedProfiles']:
 info=next((p for p in infos if p.name==profile),None);live=host_gateway_serving(profile,wait_for_channel=0)
 profiles.append({'profile':profile,'nativeOn':bool(info and info.gateway_running),'liveServed':live is not None,'nativeOwnerPid':live.pid if live else None})
configured={}
platforms=status.get('platforms') or {}
adapter_profiles={'default'}|{name.rsplit(':',1)[0] for name in platforms if ':' in name}
for profile in infos:
 if profile.name not in adapter_profiles:continue
 with _profile_runtime_scope(profile.path,hydrate_secrets=False):
  config=load_gateway_config()
  for platform,setting in config.platforms.items():
   name=platform.value if profile.name=='default' else profile.name+':'+platform.value
   configured[name]=bool(setting.enabled)
# Retain machine codes only; error_message/error/message/last_error are never read or emitted.
adapters=[{'name':name,'state':details.get('state'),'errorCode':details.get('error_code'),'configured':configured.get(name,True),'writerPid':details.get('writer_pid')} for name,details in platforms.items() if isinstance(details,dict)]
try:
 with urllib.request.urlopen('http://127.0.0.1:9120/api/health',timeout=3) as response:
  body=json.load(response);health={'code':response.status,'commit':body.get('commit')}
except Exception as exc:health={'code':getattr(exc,'code',0),'commit':None}
print(json.dumps({'identify':identity,'nativeServing':owner is not None,'nativeOwnerPid':owner.pid if owner else None,'profiles':profiles,'status':{k:status[k] for k in ['gateway_state','desired_state','answering_pid'] if k in status},'health':health,'adapters':adapters,'configuredActiveAdapters':sorted(name for name,enabled in configured.items() if enabled)}))
`;
export async function verifyBackend(target: HermesTarget, policy: BackendUpdatePolicy, sha: string, runDir: string): Promise<BackendSnapshot> {
  validateSha(sha);
  const source = pythonContext(target, policy) + snapshotPython + `\nnative_code=${JSON.stringify('import json\nrequest=' + literal(target) + '\n' + nativeProbePython)}\n` + String.raw`
owner=out['gatewayOwner'];assert owner is not None,'Exactly one native gateway owner required'
assert owner['uid']==target['nativeUid'],'Owner UID differs from the configured native UID'
intended=next(c for c in cs if c['Config']['Labels']['com.docker.compose.service']==target['gatewayService'])
assert owner['containerId']==intended['Id'],'Gateway owner is outside the intended service'
native=command(['docker','exec','--user',str(owner['uid'])+':'+str(target['nativeGid']),'--workdir',target['nativeRepository'],'--env','HOME='+target['nativeHome'],'--env','HERMES_HOME='+target['nativeHome'],'--env','PYTHONDONTWRITEBYTECODE=1',owner['containerId'],target['nativePython'],'-B','-c',native_code],timeout=20)
out['native']=json.loads(native)
after=process_identity(owner['hostPid']);after_lock=metadata(lock)
out['ownerPreserved']=after==owner and (after_lock['device'],after_lock['inode'],after_lock['held'])==(lock_meta['device'],lock_meta['inode'],True)
out['observedAtUtc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
print(json.dumps(out))
`;
  const snapshot = await remote<BackendSnapshot>(target, 'verify', source, runDir);
  snapshot.verification = evaluateBackendSnapshot(target, policy, sha, snapshot);
  await saveJson(join(runDir, `backend-verification-${randomUUID()}.json`), snapshot);
  return snapshot;
}

/** Result=success on a collected systemd unit is deliberately not completion evidence. */
export function correlateBackendTransaction(job: BackendJob, transactions: BackendTransaction[]): BackendTransaction | null {
  const matches = transactions.filter(tx => tx.pid === job.mainPid && tx.candidate.sha === job.requested_sha && (tx.candidate.overlay_sha256 || null) === (job.overlay_sha256 || null) && (!job.bot_desktop || tx.candidate.tag === `hermes-agent:main-${job.requested_sha}-desktop`) && tx.history[0] && Date.parse(tx.history[0].at) >= Date.parse(job.launched_at) - 2000);
  if (matches.length > 1) throw new Error('Ambiguous native updater transaction correlation');
  return matches[0] ?? null;
}
export function backendJobCompleted(job: BackendJob, transaction: BackendTransaction | null): boolean {
  return !!transaction && correlateBackendTransaction(job, [transaction]) !== null && transaction.last_phase === 'complete' && transaction.result === 'passed';
}
export function backendPrepareMayRetry(job: BackendJob, transaction: BackendTransaction | null, unitActive: boolean): boolean {
  return job.action === 'prepare' && !unitActive && !!transaction && correlateBackendTransaction(job, [transaction]) !== null && transaction.result === 'failed';
}

function helperGuard(policy: BackendUpdatePolicy): string {
  return String.raw`
import pathlib,hashlib,os,fcntl
helper=pathlib.Path(policy['helperPath']);assert helper.is_file() and not helper.is_symlink(),'Unsafe updater path'
assert hashlib.sha256(helper.read_bytes()).hexdigest()==policy['helperSha256'],'Updater identity changed'
lock=pathlib.Path(policy['updaterRoot'])/'lock';assert not lock.is_symlink(),'Unsafe updater lock'
identity=lock.stat()
expected=policy.get('updaterLock')
if expected:assert (identity.st_dev,identity.st_ino)==(expected['device'],expected['inode']),'Updater lock identity changed'
fd=os.open(lock,os.O_RDONLY|os.O_NOFOLLOW)
try:
 fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB);fcntl.flock(fd,fcntl.LOCK_UN)
except BlockingIOError:raise RuntimeError('Native updater already active; resume its existing run before starting another job')
finally:os.close(fd)
`;
}
export function backendImageArguments(policy: BackendUpdatePolicy, sha: string): string[] {
  validateSha(sha);
  return ['--sha', sha, ...(policy.botDesktop ? ['--bot-desktop'] : []), ...(policy.releaseTag ? ['--release-tag', policy.releaseTag] : []), ...(policy.overlay ? ['--overlay-file', policy.overlay.path, '--overlay-sha256', policy.overlay.sha256] : [])];
}
async function startOrResumeJob(target: HermesTarget, policy: BackendUpdatePolicy, action: 'prepare' | 'deploy', sha: string, runDir: string, preconditions?: BackendSnapshot): Promise<BackendJob> {
  await mkdir(runDir, { recursive: true });
  const checkpointPath = join(runDir, `backend-${action}-checkpoint.json`);
  let checkpoint: JobCheckpoint | null;
  try { checkpoint = JSON.parse(await readFile(checkpointPath, 'utf8')); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; checkpoint = null; }
  if (checkpoint && (checkpoint.action !== action || checkpoint.sha !== sha || checkpoint.overlaySha256 !== (policy.overlay?.sha256 ?? null) || (checkpoint.botDesktop ?? false) !== (policy.botDesktop ?? false) || (checkpoint.releaseTag ?? null) !== (policy.releaseTag ?? null) || checkpoint.helperSha256 !== policy.helperSha256)) throw new Error('Run directory is already pinned to a different backend candidate');
  if (checkpoint?.job) {
    const progress = await readJobProgress(target, policy, checkpoint.job, runDir);
    if (backendPrepareMayRetry(checkpoint.job, correlateBackendTransaction(checkpoint.job, progress.transactions), progress.unitActive)) {
      await rename(checkpointPath, join(runDir, `backend-prepare-failed-checkpoint-${randomUUID()}.json`));
      checkpoint = null;
    } else return checkpoint.job;
  }
  if (!checkpoint) {
    const baselineJobs = await remote<string[]>(target, 'job-baseline', pythonContext(target, policy) + `\nimport pathlib\nprint(json.dumps([p.name for p in pathlib.Path(policy['updaterRoot']).joinpath('jobs').glob('*.json')]))\n`, runDir);
    checkpoint = { schemaVersion: 1, action, sha, overlaySha256: policy.overlay?.sha256 ?? null, botDesktop: policy.botDesktop ?? false, releaseTag: policy.releaseTag ?? null, helperSha256: policy.helperSha256, requestedAtUtc: new Date().toISOString(), baselineJobs };
    await saveJson(checkpointPath, checkpoint);
  } else {
    // A disconnect after launch must recover that job instead of creating a second writer.
    const recovered = await remote<BackendJob | null>(target, 'recover-job', pythonContext(target, policy) + `\ncheckpoint=${literal(checkpoint)}\n` + String.raw`
import pathlib,subprocess,datetime
matches=[]
for path in pathlib.Path(policy['updaterRoot']).joinpath('jobs').glob('*.json'):
 if path.name in checkpoint['baselineJobs']:continue
 job=json.loads(path.read_text())
 requested_at=datetime.datetime.fromisoformat(checkpoint['requestedAtUtc'].replace('Z','+00:00')).timestamp()
 launched_at=datetime.datetime.fromisoformat(job['launched_at'].replace('Z','+00:00')).timestamp()
 if job.get('action')==checkpoint['action'] and job.get('requested_sha')==checkpoint['sha'] and (job.get('overlay_sha256') or None)==checkpoint['overlaySha256'] and bool(job.get('bot_desktop',False))==bool(checkpoint.get('botDesktop',False)) and (job.get('release_tag') or None)==checkpoint.get('releaseTag') and launched_at>=requested_at-5:
  job['mainPid']=int(subprocess.check_output(['systemctl','show',job['unit'],'--property=ExecMainPID','--value'],text=True,timeout=5).strip() or '0');matches.append(job)
assert len(matches)<=1,'Ambiguous recovered job'
if matches:assert matches[0]['mainPid']>0,'Recovered job already collected; reconcile transaction receipts before continuing'
print(json.dumps(matches[0] if matches else None))
`, runDir);
    if (recovered) { checkpoint.job = recovered; await saveJson(checkpointPath, checkpoint); return recovered; }
    throw new Error('Interrupted launch has no recoverable native job; inspect receipts before starting a new run directory');
  }
  const argv = [policy.helperPath, 'start', '--action', action, ...backendImageArguments(policy, sha), ...(action === 'deploy' ? ['--approve-deploy'] : [])];
  const launched = await remote<BackendJob>(target, `start-${action}`, pythonContext(target, policy) + helperGuard(policy) + `\nargv=${literal(argv)}\nexpected=${literal(preconditions ?? null)}\n` + String.raw`
import subprocess,re
if expected:
 for c in expected['containers']:
  actual=json.loads(subprocess.check_output(['docker','inspect',c['containerId']],text=True,timeout=5))[0]
  assert actual['Id']==c['containerId'] and actual['State']['Running'] and actual['Image']==c['imageId'],'Backend runtime changed before deploy'
 owner=expected['gatewayOwner'];assert owner is not None,'Gateway owner required before deploy'
 p=pathlib.Path('/proc')/str(owner['hostPid']);raw=(p/'stat').read_text()
 assert int(raw[raw.rfind(')')+2:].split()[19])==owner['startTicks'] and os.readlink(p/'ns/pid')==owner['namespace'],'Gateway incarnation changed before deploy'
 native_lock=pathlib.Path(expected['containers'][0]['dataSource'])/'.local/state/hermes/gateway-locks/host-gateway.lock';s=native_lock.stat()
 assert (s.st_dev,s.st_ino)==(expected['gatewayLock']['device'],expected['gatewayLock']['inode']),'Native lock changed before deploy'
 assert any(f.stat().st_dev==s.st_dev and f.stat().st_ino==s.st_ino for f in (p/'fd').iterdir()),'Gateway lock ownership changed before deploy'
env=dict(os.environ,HERMES_UPDATER_SKIP_PREFLIGHT_FILE_SCAN='1')
result=subprocess.run(argv,env=env,text=True,capture_output=True,timeout=20)
assert result.returncode==0,'Native job launch failed: '+result.stderr[-1000:]
match=re.search(r'^Background job: (hermes-main-update-[0-9TZ-]+-(?:prepare|deploy))$',result.stdout,re.M);assert match,'Native job unit missing'
unit=match.group(1);job_path=pathlib.Path(policy['updaterRoot'])/'jobs'/(unit.removeprefix('hermes-main-update-')+'.json')
job=json.loads(job_path.read_text());assert job['unit']==unit,'Job receipt unit mismatch'
job['mainPid']=int(subprocess.check_output(['systemctl','show',unit,'--property=ExecMainPID','--value'],text=True,timeout=5).strip() or '0')
assert job['mainPid']>0,'Native job PID unavailable; reconcile receipts instead of relaunching'
print(json.dumps(job))
`, runDir);
  if (launched.action !== action || launched.requested_sha !== sha || (launched.overlay_sha256 ?? null) !== (policy.overlay?.sha256 ?? null) || (launched.bot_desktop ?? false) !== (policy.botDesktop ?? false) || (launched.release_tag ?? null) !== (policy.releaseTag ?? null)) throw new Error('Native launch receipt differs from the requested backend pin');
  checkpoint.job = launched;
  await saveJson(checkpointPath, checkpoint);
  return launched;
}

async function readJobProgress(target: HermesTarget, policy: BackendUpdatePolicy, job: BackendJob, runDir: string): Promise<{ transactions: BackendTransaction[]; unitActive: boolean }> {
  return remote(target, `poll-${job.action}`, pythonContext(target, policy) + `\njob=${literal(job)}\n` + String.raw`
import pathlib,subprocess,datetime
rows=[]
for path in pathlib.Path(policy['updaterRoot']).joinpath('transactions').glob('*.json'):
 if path.stat().st_mtime<datetime.datetime.fromisoformat(job['launched_at']).timestamp()-2:continue
 tx=json.loads(path.read_text())
 if tx.get('pid')==job['mainPid']:rows.append(tx)
state=subprocess.run(['systemctl','show',job['unit'],'--property=ActiveState','--value'],capture_output=True,text=True,timeout=5).stdout.strip()
print(json.dumps({'transactions':rows,'unitActive':state in ['active','activating','deactivating']}))
`, runDir);
}
async function awaitJob(target: HermesTarget, policy: BackendUpdatePolicy, job: BackendJob, runDir: string): Promise<BackendTransaction> {
  const deadline = Date.now() + (policy.jobTimeoutMs ?? 60 * 60 * 1000);
  let previousPhase = '';
  while (Date.now() < deadline) {
    const progress = await readJobProgress(target, policy, job, runDir);
    const transaction = correlateBackendTransaction(job, progress.transactions);
    if (transaction) {
      await saveJson(join(runDir, `backend-${job.action}-transaction.json`), transaction);
      if (transaction.last_phase !== previousPhase) {
        process.stderr.write(`Backend ${job.action}: ${transaction.last_phase} (${transaction.result})\n`);
        previousPhase = transaction.last_phase;
      }
      if (transaction.result === 'failed') throw new Error(`Native backend ${job.action} failed at ${transaction.last_phase}; durable candidate and receipts remain available`);
      if (backendJobCompleted(job, transaction)) return transaction;
    }
    if (!progress.unitActive) throw new Error('Native unit stopped without a correlated successful terminal transaction; inspect receipts');
    await Bun.sleep(policy.pollIntervalMs ?? 5000);
  }
  throw new Error('Backend job wait timed out; resume the same run directory to observe the existing native job');
}

export function preparedCandidateFromSnapshot(policy: BackendUpdatePolicy, sha: string, snapshot: BackendSnapshot): PreparedBackendCandidate {
  validateSha(sha);
  const candidate = snapshot.candidate as (PreparedBackendCandidate & { actual_image_id?: string; bot_desktop_label?: string; actual_stamp?: Record<string, unknown> }) | null;
  const expectedTag = `hermes-agent:main-${sha}${policy.botDesktop ? '-desktop' : ''}${policy.overlay ? '-overlay-' + policy.overlay.sha256 : ''}`;
  if (!candidate || candidate.status !== 'prepared' || candidate.sha !== sha || candidate.tag !== expectedTag || candidate.source_verified !== true || !imagePattern.test(candidate.image_id) || candidate.actual_image_id !== candidate.image_id || candidate.validation_key !== policy.helperSha256 || snapshot.helperSha256 !== policy.helperSha256 || (candidate.overlay_sha256 ?? null) !== (policy.overlay?.sha256 ?? null) || (policy.overlay && !shaPattern.test(candidate.overlay_tree_sha ?? ''))) throw new Error('Native candidate lacks matching immutable prepared evidence');
  if (policy.botDesktop && candidate.bot_desktop_label !== '1') throw new Error('Native candidate lacks the requested official bot desktop variant');
  if (policy.releaseTag && (candidate.actual_stamp?.commit !== sha || candidate.actual_stamp?.dirty !== false || candidate.actual_stamp?.displayVersion !== policy.releaseTag.slice(1))) throw new Error('Native candidate release version is not independently verified');
  for (const name of ['doctor', 'plugin']) {
    const evidence = candidate.validation_evidence?.[name];
    if (!evidence || !evidence.path.startsWith(policy.updaterRoot + '/reports/') || evidence.path.split('/').includes('..') || !digestPattern.test(evidence.sha256) || evidence.actualSha256 !== evidence.sha256) throw new Error('Native candidate diagnostic evidence changed or is missing');
  }
  return { ...candidate, helperSha256: snapshot.helperSha256 };
}
export async function prepareBackend(target: HermesTarget, policy: BackendUpdatePolicy, sha: string, runDir: string): Promise<PreparedBackendCandidate> {
  validatePolicy(target, policy); validateSha(sha);
  const snapshot = await inspectBackend(target, policy, runDir);
  if (snapshot.containers.every(c => c.running && c.health === 'healthy' && c.revision === sha && (c.overlaySha256 ?? null) === (policy.overlay?.sha256 ?? null) && (!policy.botDesktop || c.botDesktop === true) && (!policy.releaseTag || (c.stamp.dirty === false && c.stamp.displayVersion === policy.releaseTag.slice(1))))) {
    const current = snapshot.current;
    if (!current || current.sha !== sha || snapshot.containers.some(c => c.imageId !== current.image_id)) throw new Error('Running backend differs from native recorded state');
    return { ...current, status: 'already-current', source_verified: true, helperSha256: snapshot.helperSha256 };
  }
  const job = await startOrResumeJob(target, policy, 'prepare', sha, runDir);
  await awaitJob(target, policy, job, runDir);
  return { ...preparedCandidateFromSnapshot(policy, sha, await inspectBackend(target, policy, runDir)), job };
}

async function parkNonOwnerStandby(target: HermesTarget, policy: BackendUpdatePolicy, snapshot: BackendSnapshot, runDir: string): Promise<void> {
  if (!snapshot.gatewayOwner || snapshot.gatewayOwner.containerId !== snapshot.containers.find(c => c.service === target.gatewayService)?.containerId) throw new Error('Cannot park standby without the intended exact gateway owner');
  await remote(target, 'park-standby', pythonContext(target, policy) + `\nexpected=${literal(snapshot)}\n` + String.raw`
import pathlib,subprocess,os,re,stat
owner=expected['gatewayOwner'];p=pathlib.Path('/proc')/str(owner['hostPid']);lock=pathlib.Path(expected['containers'][0]['dataSource'])/'.local/state/hermes/gateway-locks/host-gateway.lock'
def guard():
 raw=(p/'stat').read_text();s=lock.lstat();assert stat.S_ISREG(s.st_mode),'Unsafe native lock'
 assert int(raw[raw.rfind(')')+2:].split()[19])==owner['startTicks'] and os.readlink(p/'ns/pid')==owner['namespace'],'Owner incarnation changed'
 assert (s.st_dev,s.st_ino)==(expected['gatewayLock']['device'],expected['gatewayLock']['inode']),'Native lock changed'
 assert any(f.stat().st_dev==s.st_dev and f.stat().st_ino==s.st_ino for f in (p/'fd').iterdir()),'Owner no longer holds native lock'
guard();changed=[]
for standby in expected['containers']:
 if standby['service']==target['gatewayService'] or not standby['rootSlot']['up']:continue
 assert standby['namespace']!=owner['namespace'],'Standby shares owner PID namespace'
 actual=json.loads(subprocess.check_output(['docker','inspect',standby['containerId']],text=True,timeout=5))[0]
 assert actual['State']['Running'] and actual['Image']==standby['imageId'] and os.readlink(pathlib.Path('/proc')/str(actual['State']['Pid'])/'ns/pid')==standby['namespace'],'Standby incarnation changed'
 slot='/run/service/'+target['gatewaySlot'];cid=standby['containerId']
 def control(args,timeout=5):return subprocess.check_output(['docker','exec','--user','0:0',cid,*args],text=True,timeout=timeout)
 identity=standby['rootSlot'].get('identity');assert identity and identity['matchingFdCount']==0 and identity['hostPid']!=owner['hostPid'],'Standby ownership not established'
 process=pathlib.Path('/proc')/str(identity['hostPid'])
 def standby_guard():
  current=control(['/command/s6-svstat',slot]);match=re.search(r'\bpid (\d+)\b',current)
  raw=(process/'stat').read_text()
  assert current.startswith('up ') and match and int(match.group(1))==identity['localPid'],'Standby slot PID changed'
  assert int(raw[raw.rfind(')')+2:].split()[19])==identity['startTicks'] and os.readlink(process/'ns/pid')==identity['namespace'],'Standby process incarnation changed'
  s=lock.stat();assert not any(f.stat().st_dev==s.st_dev and f.stat().st_ino==s.st_ino for f in (process/'fd').iterdir()),'Standby acquired native lock'
 # Native s6 control keeps shared gateway desired state untouched.
 guard();standby_guard();control(['/command/s6-svc','-d',slot]);control(['/command/s6-svwait','-d','-t','15000',slot],timeout=18)
 assert control(['/command/s6-svstat',slot]).startswith('down '),'Standby slot still up'
 guard();changed.append(cid)
print(json.dumps({'parkedContainers':changed,'ownerProcessPreserved':True,'lockInodePreserved':True,'sharedDesiredStateChanged':False}))
`, runDir);
}
export async function deployBackend(target: HermesTarget, policy: BackendUpdatePolicy, candidate: PreparedBackendCandidate, runDir: string, options: { authorization: string }): Promise<BackendSnapshot> {
  validatePolicy(target, policy); validateSha(candidate.sha);
  if (!options.authorization.trim()) throw new Error('Backend deployment requires explicit authorization');
  if (candidate.helperSha256 !== policy.helperSha256) throw new Error('Candidate updater identity differs from policy');
  const before = await inspectBackend(target, policy, runDir);
  const alreadyDeployed = before.containers.every(c => c.running && c.health === 'healthy' && c.imageId === candidate.image_id && c.revision === candidate.sha);
  if (candidate.status !== 'already-current' && !alreadyDeployed) {
    const recorded = preparedCandidateFromSnapshot(policy, candidate.sha, before);
    if (recorded.image_id !== candidate.image_id || recorded.tag !== candidate.tag) throw new Error('Reviewed backend candidate changed');
    const job = await startOrResumeJob(target, policy, 'deploy', candidate.sha, runDir, before);
    await awaitJob(target, policy, job, runDir);
  }
  let deployed = await inspectBackend(target, policy, runDir);
  if (deployed.containers.some(c => c.imageId !== candidate.image_id || c.revision !== candidate.sha || c.health !== 'healthy')) throw new Error('Backend readback differs from the completed native deployment');
  const handoff = planGatewayOwnerHandoff(target, policy, candidate, deployed);
  if (handoff) {
    await saveJson(join(runDir, `gateway-owner-handoff-${randomUUID()}.json`), { schemaVersion: 1, authority: options.authorization,
      observedAtUtc: deployed.observedAtUtc, plan: handoff, scope: 'Exact quiescent native service handoff; native desired state stop/start requests with final running readback; no image redeployment' });
    await remote(target, 'gateway-owner-handoff', pythonContext(target, policy) + `\nplan=${literal(handoff)}\n`
      + gatewayHandoffDriverPython + gatewayHandoffProtocolPython + String.raw`
try:
 owner=run_handoff(Driver());print(json.dumps({'status':'passed','events':events,'owner':owner,'imageId':plan['imageId'],'sourceSha':plan['sourceSha'],'nativeDesiredStateTransitionRequested':'running-stopped-running','finalDesiredStateVerified':'running','allExpectedProfilesVerified':True}))
except BaseException:
 print(json.dumps({'status':'failed','events':events,'imageRedeployed':False,'forcedSingletonBypass':False}),flush=True)
 raise
`, runDir);
    deployed = await inspectBackend(target, policy, runDir);
  }
  if (policy.parkStandby !== false) await parkNonOwnerStandby(target, policy, deployed, runDir);
  const verified = await verifyBackend(target, policy, candidate.sha, runDir);
  if (!verified.verification?.passed) throw new Error('Backend native serving verification failed; inspect saved verification checks');
  return verified;
}

export async function stageBackendOverlay(target: HermesTarget, policy: BackendUpdatePolicy, sha: string, localPatch: string, runDir: string): Promise<{ path: string; sha256: string }> {
  validatePolicy(target, policy); validateSha(sha);
  const bytes = await readFile(localPatch);
  if (!bytes.length || bytes.length > 1048576) throw new Error('Overlay must be nonempty and at most 1 MiB');
  const digest = createHash('sha256').update(bytes).digest('hex');
  const result = await remote<{ path: string; overlay_sha256: string; source_sha: string }>(target, 'stage-overlay', pythonContext(target, policy) + helperGuard(policy) + `\npatch=${JSON.stringify(bytes.toString('base64'))}\nargv=${literal([policy.helperPath, 'stage-overlay', '--sha', sha, '--overlay-sha256', digest])}\n` + String.raw`
import base64,subprocess
result=subprocess.run(argv,input=base64.b64decode(patch),capture_output=True,timeout=20)
assert result.returncode==0,'Native overlay staging failed: '+result.stderr.decode()[-1000:]
print(result.stdout.decode())
`, runDir);
  if (result.overlay_sha256 !== digest || result.source_sha !== sha || result.path !== `${policy.updaterRoot}/overlays/${digest}.patch`) throw new Error('Native overlay staging readback differs from the supplied patch');
  return { path: result.path, sha256: digest };
}
