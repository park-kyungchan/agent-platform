import type { InfrastructureObservation } from '../domain';
import type { HermesTarget } from './ssh';
import { executeRemotePython } from './ssh';

export async function observeHermes(target: HermesTarget, operation: string, runDirectory: string): Promise<InfrastructureObservation> {
  const source = String.raw`
import os,json,pathlib,subprocess,datetime,fcntl,re,stat
P=pathlib.Path('/proc'); PROFILE=__PROFILE__; TARGET=__TARGET__; SESSION=None; EXPECTED=json.loads(__EXPECTED__)
ids=subprocess.check_output(['docker','ps','-q'],text=True).split()
containers=json.loads(subprocess.check_output(['docker','inspect',*ids],text=True)) if ids else []
all_containers=containers
containers=[c for c in containers if c['Config']['Labels'].get('com.docker.compose.project')=='hermes-agent-2q3y' and c['Config']['Labels'].get('com.docker.compose.service') in ['hermes-agent','hermes-serve']]
sources={m['Source'] for c in containers for m in c['Mounts'] if m['Destination']=='/opt/data'}
assert len(sources)==1, 'Expected exactly one shared data source'
lock=pathlib.Path(next(iter(sources)))/'.local/state/hermes/gateway-locks/host-gateway.lock'
s=lock.lstat(); assert stat.S_ISREG(s.st_mode), 'Lock must be a regular non-symlink file'
key=f'{os.major(s.st_dev):02x}:{os.minor(s.st_dev):02x}:{s.st_ino}'
locks=[x for x in (P/'locks').read_text().splitlines() if key in x.split() and '->' not in x.split()]
fd=os.open(lock,os.O_RDONLY|os.O_NOFOLLOW)
try:
 fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB); held=False; fcntl.flock(fd,fcntl.LOCK_UN)
except BlockingIOError: held=True
finally: os.close(fd)

def process_identity(pid,container_id):
 p=P/str(pid); raw=(p/'stat').read_text(); fields=raw[raw.rfind(')')+2:].split(); status=(p/'status').read_text().splitlines()
 return {'hostPid':pid,'localPid':int(next(x for x in status if x.startswith('NSpid:')).split()[-1]),'startTicks':int(fields[19]),'pidNamespace':os.readlink(p/'ns/pid'),'containerId':container_id,'uid':int(next(x for x in status if x.startswith('Uid:')).split()[1])}
def inventory(cs):
 rows=[]
 for c in cs:
  label=c['Config']['Labels']; service=label.get('com.docker.compose.service','unknown'); pid=c['State']['Pid']
  published=[]
  for key,bindings in (c['NetworkSettings']['Ports'] or {}).items():
   for binding in bindings or []:
    port,protocol=key.split('/'); published.append({'containerPort':int(port),'protocol':protocol,'hostAddress':binding['HostIp'],'hostPort':int(binding['HostPort'])})
  rows.append({'containerId':c['Id'],'name':c['Name'].lstrip('/'),'service':service,'imageId':c['Image'],'imageRevision':label.get('org.opencontainers.image.revision'),'state':c['State']['Status'],'dockerHealth':c['State'].get('Health',{}).get('Status','none'),'initPid':pid or None,'pidNamespace':os.readlink(P/str(pid)/'ns/pid') if pid else None,'role':'gateway-owner' if service==TARGET else 'backend' if service.startswith('hermes-') else 'ingress' if service=='traefik' else 'other','mounts':[{'type':m['Type'],'source':m.get('Source',''),'destination':m['Destination'],'readWrite':m['RW']} for m in c['Mounts']],'publishedPorts':published,'restartCount':c['RestartCount'],'restartPolicy':c['HostConfig']['RestartPolicy']['Name']})
 return {'hostId':'srv1873899','observedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'containers':rows}
def ports(cs):
 import socket
 rows=[]
 for c in cs:
  pid=c['State']['Pid']; ns=os.readlink(P/str(pid)/'ns/pid'); candidates={}
  for family,name in [(socket.AF_INET,'tcp'),(socket.AF_INET6,'tcp6')]:
   for line in (P/str(pid)/'net'/name).read_text().splitlines()[1:]:
    fields=line.split()
    if fields[3]!='0A':continue
    address,port=fields[1].split(':'); port=int(port,16)
    if family==socket.AF_INET: address=socket.inet_ntop(family,bytes.fromhex(address)[::-1])
    else: address=socket.inet_ntop(family,b''.join(bytes.fromhex(address)[i:i+4][::-1] for i in range(0,16,4)))
    purpose={9119:'dashboard',9120:'native-backend',8642:'native-backend',443:'ingress',80:'ingress',8443:'ingress',22:'ssh'}.get(port,'other')
    row={'protocol':'tcp','address':address,'port':port,'pidNamespace':ns,'owner':None,'containerId':c['Id'],'purpose':purpose,'exposure':'loopback' if address in ['127.0.0.1','::1'] else 'container-network'}
    candidates[fields[9]]=row
  for p in P.iterdir():
   if not p.name.isdigit():continue
   try:
    if os.readlink(p/'ns/pid')!=ns:continue
    for f in (p/'fd').iterdir():
     try:
      link=os.readlink(f)
      if link.startswith('socket:['):
       key=link[8:-1]
       if key in candidates:candidates[key]['owner']=process_identity(int(p.name),c['Id'])
     except FileNotFoundError:pass
   except (FileNotFoundError,ProcessLookupError,PermissionError):pass
  rows.extend(candidates.values())
 return rows

out={'schemaVersion':1,'observedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'targetService':TARGET,'profile':PROFILE,'expectedProfiles':EXPECTED,'lock':{'device':s.st_dev,'inode':s.st_ino,'held':held,'holders':[]},'runtimes':[]}
for line in locks:
 pid=int(line.split()[4]); p=P/str(pid); raw=(p/'stat').read_text(); fields=raw[raw.rfind(')')+2:].split()
 ns=os.readlink(p/'ns/pid'); localpid=int(next(x for x in (p/'status').read_text().splitlines() if x.startswith('NSpid:')).split()[-1])
 count=0
 for f in (p/'fd').iterdir():
  try:
   fs=f.stat(); count+=int(fs.st_dev==s.st_dev and fs.st_ino==s.st_ino)
  except FileNotFoundError: pass
 owner=next((c['Id'] for c in containers if os.readlink(P/str(c['State']['Pid'])/'ns/pid')==ns),None)
 cmd=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode()
 # Only expected Hermes command metadata is retained; an unrelated owner's argv stays private.
 if 'gateway run' not in cmd: cmd='[unrelated command omitted]'
 out['lock']['holders'].append({'pid':pid,'localPid':localpid,'startTicks':int(fields[19]),'namespace':ns,'containerId':owner,'command':cmd,'matchingFdCount':count})
native_code=r'''
import json,urllib.request
from pathlib import Path
from gateway.control_socket import identify_gateway,query_gateway_control
from gateway.host_attach import host_gateway_serving,invalidate_host_gateway_cache
from hermes_cli.profiles import list_profiles
from gateway.run import _profile_runtime_scope
from gateway.config import load_gateway_config
profile=__PROFILE__
invalidate_host_gateway_cache()
identity=identify_gateway(Path('/opt/data'),timeout=3)
owner=host_gateway_serving(profile,wait_for_channel=2)
status=query_gateway_control(Path('/opt/data'),'status',timeout=3) or {}
infos=list_profiles(lazy_skill_count=True)
info=next((p for p in infos if p.name==profile),None)
profiles=[]
for p in infos:
 h=host_gateway_serving(p.name,wait_for_channel=0)
 profiles.append({'profile':p.name,'nativeOn':p.gateway_running,'liveServed':h is not None,'nativeOwnerPid':h.pid if h else None})
adapters=[]
configured={}
adapter_profiles={'default'}|{name.rsplit(':',1)[0] for name in (status.get('platforms') or {}) if ':' in name}
for p in infos:
 if p.name not in adapter_profiles:continue
 with _profile_runtime_scope(p.path,hydrate_secrets=False):
  config=load_gateway_config()
  for platform,setting in config.platforms.items():
   key=platform.value if p.name=='default' else p.name+':'+platform.value
   configured[key]=setting.enabled
for name,details in (status.get('platforms') or {}).items():
 if not isinstance(details,dict): continue
 message=' '.join(str(details.get(k,'')) for k in ['error_code','error_message','error','message','last_error']).lower()
 adapters.append({'name':name,'state':details.get('state'),'errorCode':details.get('error_code'),'configured':configured.get(name,True),'writerPid':details.get('writer_pid'),'credentialRejected':any(x in message for x in ['auth_error','token rejected','token is invalid','invalid token','invalid credentials','unauthorized','revoked'])})
out={'identify':identity,'nativeServing':owner is not None,'nativeOwnerPid':owner.pid if owner else None,'profileOn':bool(info and info.gateway_running),'profiles':profiles,'status':{k:status[k] for k in ['gateway_state','desired_state','answering_pid','answered_at'] if k in status},'adapters':adapters}
try:
 with urllib.request.urlopen('http://127.0.0.1:9120/api/status?profile='+profile,timeout=5) as response:
  body=json.load(response)
  out['http']={'code':response.status,**{k:body[k] for k in ['gateway_running','gateway_state','gateway_mode'] if k in body}}
except Exception as exc: out['http']={'code':getattr(exc,'code',0)}
print(json.dumps(out))
'''.replace('__PROFILE__',json.dumps(PROFILE))
for c in containers:
 root=P/str(c['State']['Pid'])/'root'; service=c['Config']['Labels']['com.docker.compose.service']
 p=subprocess.run(['docker','exec','--user','10000:10000','--workdir','/opt/hermes','--env','HOME=/opt/data','--env','HERMES_HOME=/opt/data',c['Id'],'/opt/hermes/.venv/bin/python','-c',native_code],capture_output=True,text=True,timeout=25)
 native=json.loads(p.stdout) if p.returncode==0 else {'identify':None,'nativeServing':False,'profileOn':False,'status':{},'adapters':[],'error':'Native probe exited '+str(p.returncode)}
 sv=subprocess.run(['docker','exec','--user','0:0',c['Id'],'/command/s6-svstat','/run/service/gateway-default'],capture_output=True,text=True,timeout=5)
 raw=sv.stdout.strip(); match=re.search(r'\bpid (\d+)\b',raw)
 out['runtimes'].append({'service':service,'containerId':c['Id'],'initPid':c['State']['Pid'],'namespace':os.readlink(P/str(c['State']['Pid'])/'ns/pid'),'dataSource':next(m['Source'] for m in c['Mounts'] if m['Destination']=='/opt/data'),'dockerHealth':c['State'].get('Health',{}).get('Status'),'rootSlot':{'up':raw.startswith('up '),'localPid':int(match.group(1)) if match else None,'raw':raw,'downFileExists':(root/'run/service/gateway-default/down').exists()},'native':native})
if SESSION:
 session=P/str(SESSION['hostPid'])
 try:
  raw=(session/'stat').read_text(); fields=raw[raw.rfind(')')+2:].split()
  ticks=int(fields[19]); ns=os.readlink(session/'ns/pid')
  out['preservedSession']={'pid':SESSION['hostPid'],'startTicks':ticks,'namespace':ns,'alive':ticks==SESSION['startTicks'] and ns==SESSION['namespace']}
 except FileNotFoundError: out['preservedSession']={'pid':SESSION['hostPid'],'startTicks':SESSION['startTicks'],'namespace':SESSION['namespace'],'alive':False}
out['inventory']=inventory(all_containers)
out['ports']=ports(all_containers)
out['observedAtUtc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
print(json.dumps(out,indent=2))
`.replaceAll('__PROFILE__', JSON.stringify(target.profile)).replaceAll('__TARGET__', JSON.stringify(target.gatewayService)).replace('__EXPECTED__', JSON.stringify(JSON.stringify(target.expectedProfiles)));
  // Two native probes can each take 25 seconds, in addition to host inventory.
  const result = await executeRemotePython(target, operation, source, runDirectory, { timeoutMs: 180_000 });
  if (result.exitCode !== 0) throw new Error('Remote native observation failed; inspect saved execution receipt: ' + result.stderr);
  return JSON.parse(result.stdout);
}
