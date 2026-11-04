
import json,pathlib,subprocess,os,fcntl,time,datetime,re
request=json.loads(__REQUEST__); config=json.loads(__CONFIG__); P=pathlib.Path('/proc')
cid=request['target']['containerId']; info=json.loads(subprocess.check_output(['docker','inspect',cid],text=True))[0]
assert info['Id']==cid and info['State']['Running']
assert info['Config']['Labels'].get('com.docker.compose.project')==config['project']
assert info['Config']['Labels'].get('com.docker.compose.service')==request['target']['service']==config['gatewayService']
def incarnation(pid):
 p=P/str(pid); raw=(p/'stat').read_text(); fields=raw[raw.rfind(')')+2:].split(); status=(p/'status').read_text().splitlines()
 return {'hostPid':pid,'localPid':int(next(x for x in status if x.startswith('NSpid:')).split()[-1]),'startTicks':int(fields[19]),'pidNamespace':os.readlink(p/'ns/pid'),'containerId':cid,'uid':int(next(x for x in status if x.startswith('Uid:')).split()[1])}
for identity in request['preserveProcesses']:
 actual=incarnation(identity['hostPid'])
 assert all(actual[k]==identity[k] for k in ['localPid','startTicks','pidNamespace','uid'])
source=next(m['Source'] for m in info['Mounts'] if m['Destination']==config['nativeHome'])
lock_path=pathlib.Path(source)/'.local/state/hermes/gateway-locks/host-gateway.lock'
s=lock_path.lstat(); expected_lock=request['target']['lockIdentity']
if expected_lock: assert s.st_dev==expected_lock['device'] and s.st_ino==expected_lock['inode']
def lock_free():
 fd=os.open(lock_path,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB); fcntl.flock(fd,fcntl.LOCK_UN); return True
 except BlockingIOError: return False
 finally: os.close(fd)
identity=request['target']['processIdentity']; kind=request['kind']; events=[]
def native(code):
 r=subprocess.run(['docker','exec','--user','0:0','--workdir',config['nativeRepository'],'--env','HOME='+config['nativeHome'],'--env','HERMES_HOME='+config['nativeHome'],cid,config['nativePython'],'-c',code],capture_output=True,text=True,timeout=25)
 events.append({'nativeExitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
 if r.returncode!=0:print(json.dumps({'partialNativeEvents':events}),flush=True)
 assert r.returncode==0
if kind in ['native-gateway-stop','native-gateway-restart']:
 assert incarnation(identity['hostPid'])==identity
 assert identity['pidNamespace']==os.readlink(P/str(info['State']['Pid'])/'ns/pid')
 assert any(f.stat().st_dev==s.st_dev and f.stat().st_ino==s.st_ino for f in (P/str(identity['hostPid'])/'fd').iterdir())
 # Stop and start are distinct native steps: a clean exit can park an s6 longrun.
 code="import pathlib,subprocess;from gateway.status import write_planned_stop_marker;from hermes_cli.service_manager import S6ServiceManager;sv=subprocess.check_output(['/command/s6-svstat','/run/service/"+config['gatewaySlot']+"'],text=True);assert int(sv.split('pid ',1)[1].split()[0])=="+str(identity['localPid'])+";raw=pathlib.Path('/proc/"+str(identity['localPid'])+"/stat').read_text();fields=raw[raw.rfind(')')+2:].split();assert int(fields[19])=="+str(identity['startTicks'])+";write_planned_stop_marker("+str(identity['localPid'])+");S6ServiceManager().stop('"+config['gatewaySlot']+"')"
 native(code)
 native("import subprocess;subprocess.run(['/command/s6-svwait','-d','-t','20000','/run/service/"+config['gatewaySlot']+"'],check=True,timeout=22)")
 deadline=time.monotonic()+20
 while not lock_free() and time.monotonic()<deadline:time.sleep(.1)
 assert lock_free(), 'Lock not released; no forced action performed'
elif kind=='native-gateway-start': assert lock_free(), 'Existing owner blocks start'
elif kind!='inspect': raise RuntimeError('Unsupported operation')
if kind in ['native-gateway-start','native-gateway-restart']:
 assert lock_path.stat().st_dev==s.st_dev and lock_path.stat().st_ino==s.st_ino
 native("from hermes_cli.gateway_multiplex_s6 import bring_root_slot_up;print(bring_root_slot_up())")
preserved=[]
for identity in request['preserveProcesses']:
 actual=incarnation(identity['hostPid']); preserved.append(actual)
 assert all(actual[k]==identity[k] for k in ['localPid','startTicks','pidNamespace','uid'])
print(json.dumps({'observedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'operation':kind,'containerId':cid,'nativeEvents':events,'unchangedLockDevice':lock_path.stat().st_dev==s.st_dev,'unchangedLockInode':lock_path.stat().st_ino==s.st_ino,'preservedProcesses':preserved},indent=2))
