import { expect, test } from 'bun:test';
import { planGatewayOwnerHandoff, gatewayHandoffDriverPython, gatewayHandoffProtocolPython } from '../src/adapters/gateway-owner-handoff';
import type { BackendSnapshot, BackendUpdatePolicy, PreparedBackendCandidate } from '../src/adapters/backend-update';
import type { HermesTarget } from '../src/adapters/ssh';
const sha='c'.repeat(40),overlay='d'.repeat(64),helper='e'.repeat(64),image='sha256:'+'f'.repeat(64);
const target={hostId:'host',project:'project',gatewayService:'hermes-serve',nativeUid:10000} as HermesTarget;
const policy={helperSha256:helper,services:['hermes-agent','hermes-serve'],overlay:{sha256:overlay}} as BackendUpdatePolicy;
const candidate={image_id:image,sha,overlay_sha256:overlay} as PreparedBackendCandidate;
function snapshot():BackendSnapshot {
 const owner={hostPid:1001,localPid:309,startTicks:9000,namespace:'pid:[1]',containerId:'a'.repeat(64),uid:10000,matchingFdCount:1};
 const contender={hostPid:1002,localPid:713,startTicks:9001,namespace:'pid:[2]',containerId:'b'.repeat(64),uid:10000,matchingFdCount:0};
 return {schemaVersion:1,hostId:'host',project:'project',helperSha256:helper,observedAtUtc:new Date().toISOString(),updaterLock:{device:1,inode:2,held:false},current:null,candidate:null,
  containers:[owner,contender].map((identity,i)=>({service:i?'hermes-serve':'hermes-agent',containerId:identity.containerId,imageId:image,revision:sha,overlaySha256:overlay,running:true,health:'healthy',namespace:identity.namespace,dataSource:'/data',rootSlot:{up:true,localPid:identity.localPid,raw:'up',identity},stamp:{commit:sha,overlaySha256:overlay},provenance:{revision:sha}})),
  gatewayLock:{device:1,inode:3,held:true,holders:[owner]},gatewayOwner:owner};
}
test('only exact deployed sibling owner yields a bounded handoff, intended owner remains a no-op',()=>{
 const s=snapshot(),p=planGatewayOwnerHandoff(target,policy,candidate,s)!;expect(p.from.service).toBe('hermes-agent');expect(p.intended.service).toBe('hermes-serve');
 const intended={...s.containers[1]!.rootSlot.identity!,matchingFdCount:1};s.containers[1]!.rootSlot.identity=intended;s.gatewayOwner=intended;s.gatewayLock.holders=[intended];
 expect(planGatewayOwnerHandoff(target,policy,candidate,s)).toBeNull();
});
test('image/source/overlay/mount/helper/namespace and native singleton mismatches refuse before effects',()=>{
 const changes:Array<(s:BackendSnapshot)=>void>=[s=>s.containers[1]!.imageId='sha256:'+'a'.repeat(64),s=>s.containers[0]!.revision='a'.repeat(40),s=>s.containers[0]!.overlaySha256=null,
  s=>s.containers[1]!.dataSource='/other',s=>s.helperSha256='a'.repeat(64),s=>s.containers[1]!.namespace=s.containers[0]!.namespace,
  s=>s.gatewayLock.holders.push({...s.gatewayOwner!,hostPid:999}),s=>s.gatewayOwner=null,s=>s.gatewayLock.held=false,
  s=>s.containers[0]!.rootSlot.localPid=999,s=>s.containers[1]!.rootSlot.identity={...s.containers[1]!.rootSlot.identity!,matchingFdCount:1}];
 for(const change of changes){const s=snapshot();change(s);expect(()=>planGatewayOwnerHandoff(target,policy,candidate,s)).toThrow();}
});
function python(source:string){const p=Bun.spawnSync(['python3','-c',source],{stdout:'pipe',stderr:'pipe'});if(p.exitCode)throw Error(p.stderr.toString());return JSON.parse(p.stdout.toString());}
test('actual handoff protocol keeps nonowner down before owner stop and lock release before native start',()=>{
 const result=python(gatewayHandoffProtocolPython+String.raw`
import json
class D:
 def __init__(self,mode):self.events=[];self.mode=mode;self.guards=0;self.parked=False;self.stopped=False;self.started=False
 def revalidate_owner(self):
  self.events.append('guard');self.guards+=1
  if self.mode=='pid-reuse' or self.mode=='owner-race' and self.guards==2:raise RuntimeError('identity changed')
 def assert_quiescent(self):
  self.events.append('quiescent')
  if self.mode=='busy' or self.mode=='new-work' and self.parked:raise RuntimeError('active work')
 def park_intended_nonowner(self):self.events.append('park');self.parked=True
 def stop_owner_and_wait(self):
  assert self.parked;self.events.append('stop');self.stopped=True
  if self.mode=='no-lock-release':raise RuntimeError('lock held')
 def assert_both_down_and_lock_free(self):
  assert self.stopped and self.parked;self.events.append('lock-free')
  if self.mode=='lock-race':raise RuntimeError('new owner')
 def start_intended(self):self.events.append('start');self.started=True
 def verify_intended_owner(self):self.events.append('verify');return 'intended'
out=[]
for mode in ['success','pid-reuse','busy','owner-race','new-work','no-lock-release','lock-race']:
 d=D(mode);error=None
 try:result=run_handoff(d)
 except RuntimeError as e:error=str(e)
 out.append({'mode':mode,'events':d.events,'error':error,'ownerStopped':d.stopped,'intendedStarted':d.started})
print(json.dumps(out))
`);
 expect(result[0].events).toEqual(['guard','quiescent','park','guard','quiescent','stop','lock-free','start','verify']);
 for(const row of result.slice(1))expect(row.intendedStarted).toBe(false);
 for(const row of result.slice(1,5))expect(row.ownerStopped).toBe(false);
 expect(result[5].events).not.toContain('lock-free');
});
test('actual native wrapper classifier detects CLI work without confusing gateway run or unrelated Python',()=>{
 const code=`import ast,json\ntree=ast.parse(${JSON.stringify(gatewayHandoffDriverPython)})\nnode=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='is_native_cli_work')\nexec(compile(ast.fix_missing_locations(ast.Module(body=[node],type_ignores=[])),'native-classifier','exec'))\n`+String.raw`
wrapper='import sys; from hermes_cli.main import main; main()'
rows=[['python','-P','-c',wrapper,'chat','--query','synthetic'],['python','-P','-c',wrapper,'chat','-c','native-session'],['python','-P','-c',wrapper,'gateway','run'],['python','-m','hermes_cli.main','--query=synthetic'],['python','/opt/hermes/run_agent.py'],['python','-c','print("unrelated")'],['python','-m','hermes_cli.main','-z','synthetic'],['/opt/hermes/hermes','--oneshot','synthetic'],['python','-P','-c',wrapper,'--oneshot=synthetic'],['python','-m','hermes_cli.main','--resume','session'],['python','/opt/hermes/cli.py','--resume=session']]
print(json.dumps([is_native_cli_work(a) for a in rows]))
`;
 expect(python(code)).toEqual([true,true,false,true,true,false,true,true,true,true,true]);
});
test('native completion requires restored desired state and every exact profile On under the new owner',()=>{
 const code=`import ast,json,copy\ntree=ast.parse(${JSON.stringify(gatewayHandoffDriverPython)})\nnodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('final_native_proof_checks','final_native_proof_matches')]\nexec(compile(ast.fix_missing_locations(ast.Module(body=nodes,type_ignores=[])),'native-completion','exec'))\n`+String.raw`
target={'expectedProfiles':['default','ontology-platform','dc-gang-a-dev','dc-context','dc-review','dc-contract-audit']};plan={'sourceSha':'pinned'};identity={'localPid':500,'startTicks':12345}
proof={'identity':{'pid':500,'start_time':12345,'code_sha':'pinned'},'desiredState':'running','gatewayState':'running','answeringPid':500,'profiles':[{'profile':name,'nativeOn':True,'liveServed':True,'ownerPid':500} for name in target['expectedProfiles']]}
out=[final_native_proof_matches(proof,identity)]
for kind in ['desired-stopped','wrong-pid','reused-pid','wrong-source','off-profile','duplicate-profile','wrong-live-owner']:
 p=copy.deepcopy(proof)
 if kind=='desired-stopped':p['desiredState']='stopped'
 if kind=='wrong-pid':p['answeringPid']=499
 if kind=='reused-pid':p['identity']['start_time']=99
 if kind=='wrong-source':p['identity']['code_sha']='different'
 if kind=='off-profile':p['profiles'][0]['nativeOn']=False
 if kind=='duplicate-profile':p['profiles'][0]['profile']='dc-review'
 if kind=='wrong-live-owner':p['profiles'][0]['ownerPid']=499
 out.append(final_native_proof_matches(p,identity))
print(json.dumps(out))
`;
 expect(python(code)).toEqual([true,false,false,false,false,false,false,false]);
});
test('all generated native probe and lifecycle programs compile without executing native effects',()=>{
 const source=`import ast,json\ntree=ast.parse(${JSON.stringify(gatewayHandoffDriverPython)})\ntarget={'nativeHome':'/opt/data','gatewaySlot':'gateway-default','expectedProfiles':['default','ontology-platform','dc-gang-a-dev','dc-context','dc-review','dc-contract-audit']}\nplan={'owner':{'localPid':309}}\ncount=0\nfor node in ast.walk(tree):\n if isinstance(node,ast.Assign) and any(isinstance(x,ast.Name) and x.id=='code' for x in node.targets):\n  value=eval(compile(ast.Expression(node.value),'native-builder','eval'),{'target':target,'plan':plan})\n  compile(value,'generated-native-program','exec');count+=1\nprint(json.dumps(count))\n`;
 expect(python(source)).toBe(3);
});


test('single intended owner is a no-op only after exact source, namespace, FD and incarnation guards', () => {
 const single={...policy,services:['hermes-serve']};
 const make=()=>{const s=snapshot();s.containers=[s.containers[1]!];const owner={...s.containers[0]!.rootSlot.identity!,matchingFdCount:1};s.containers[0]!.rootSlot.identity=owner;s.gatewayOwner=owner;s.gatewayLock.holders=[owner];return s;};
 expect(planGatewayOwnerHandoff(target,single,candidate,make())).toBeNull();
 const changes:Array<(s:BackendSnapshot)=>void>=[s=>s.containers[0]!.service='hermes-agent',s=>s.containers.push(structuredClone(s.containers[0]!)),s=>s.gatewayOwner=null,
  s=>s.gatewayOwner!.matchingFdCount=0,s=>s.gatewayOwner!.uid=0,s=>s.gatewayOwner!.containerId='foreign',s=>s.gatewayOwner!.startTicks++,s=>s.containers[0]!.rootSlot.identity={...s.gatewayOwner!,startTicks:999},
  s=>s.containers[0]!.namespace=null,s=>s.containers[0]!.provenance.revision='wrong',s=>s.gatewayLock.held=false,s=>s.containers[0]!.rootSlot.up=false];
 // Mutate an independent owner row so PID reuse cannot silently change both observations.
 for(const change of changes){const s=make();s.gatewayOwner=structuredClone(s.gatewayOwner);s.gatewayLock.holders=[s.gatewayOwner!];change(s);expect(()=>planGatewayOwnerHandoff(target,single,candidate,s)).toThrow();}
});

test('native final predicate diagnostics separate acquired ownership from incomplete profile readiness',()=>{
 const code=`import ast,json\ntree=ast.parse(${JSON.stringify(gatewayHandoffDriverPython)})\nnodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('final_native_proof_checks','final_native_proof_matches')]\nexec(compile(ast.fix_missing_locations(ast.Module(body=nodes,type_ignores=[])),'diagnostics','exec'))\n`+String.raw`
target={'expectedProfiles':['default']};plan={'sourceSha':'pinned'};identity={'localPid':500,'startTicks':12345}
proof={'identity':{'pid':500,'start_time':12345,'code_sha':'pinned'},'desiredState':'running','gatewayState':'running','answeringPid':500,'profiles':[{'profile':'default','nativeOn':False,'liveServed':False,'ownerPid':500}]}
checks=final_native_proof_checks(proof,identity)
print(json.dumps({'failed':[k for k,v in checks.items() if not v],'matches':final_native_proof_matches(proof,identity)}))
`;
 expect(python(code)).toEqual({failed:['allProfilesServing'],matches:false});
});
