
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
