
import pathlib,subprocess,os,json,time,stat,fcntl,re,hashlib
P=pathlib.Path('/proc');events=[];slot='/run/service/'+target['gatewaySlot']
lock=pathlib.Path(plan['from']['dataSource'])/'.local/state/hermes/gateway-locks/host-gateway.lock'
def command(args,timeout=10):return subprocess.check_output(args,text=True,timeout=timeout).strip()
def control(container,args,timeout=10):return command(['docker','exec','--user','0:0',container['containerId'],*args],timeout)
def native(container,code,uid=None,timeout=25):
 return command(['docker','exec','--user',str(target['nativeUid'] if uid is None else uid)+':'+str(target['nativeGid'] if uid is None else 0),
  '--workdir',target['nativeRepository'],'--env','HOME='+target['nativeHome'],'--env','HERMES_HOME='+target['nativeHome'],
  '--env','PYTHONDONTWRITEBYTECODE=1',container['containerId'],target['nativePython'],'-B','-c',code],timeout)
def lock_identity():
 s=lock.lstat();assert stat.S_ISREG(s.st_mode) and (s.st_dev,s.st_ino)==(plan['lock']['device'],plan['lock']['inode']),'Native lock identity changed';return s
def holds(pid):
 s=lock_identity()
 try:
  for fd in (P/str(pid)/'fd').iterdir():
   try:
    a=fd.stat()
    if (a.st_dev,a.st_ino)==(s.st_dev,s.st_ino):return True
   except FileNotFoundError:pass
 except FileNotFoundError:pass
 return False
def incarnation(identity):
 p=P/str(identity['hostPid']);raw=(p/'stat').read_text();fields=raw.rsplit(')',1)[1].split()
 assert fields[0] not in ('Z','X') and int(fields[19])==identity['startTicks'] and os.readlink(p/'ns/pid')==identity['namespace'],'Gateway process incarnation changed'
 lines=(p/'status').read_text().splitlines()
 assert int(next(x for x in lines if x.startswith('NSpid:')).split()[-1])==identity['localPid'],'Gateway local PID changed'
 assert int(next(x for x in lines if x.startswith('Uid:')).split()[1])==target['nativeUid'],'Gateway UID changed'
def slot_pid(container):
 text=control(container,['/command/s6-svstat',slot]);match=re.search(r'\bpid (\d+)\b',text)
 return int(match.group(1)) if text.startswith('up ') and match else None
def same_containers():
 assert hashlib.sha256(pathlib.Path(policy['helperPath']).read_bytes()).hexdigest()==policy['helperSha256'],'Native helper changed'
 for expected in [plan['from'],plan['intended']]:
  row=json.loads(command(['docker','inspect',expected['containerId']]))[0]
  assert row['State']['Running'] and row['Image']==plan['imageId'] and row['Config']['Labels'].get('com.docker.compose.project')==target['project'] and row['Config']['Labels'].get('com.docker.compose.service')==expected['service'],'Target container changed'
  assert os.readlink(P/str(row['State']['Pid'])/'ns/pid')==expected['namespace'],'Target namespace changed'
  assert next(m['Source'] for m in row['Mounts'] if m['Destination']==target['nativeHome'])==expected['dataSource'],'Shared data mount changed'
 lock_identity()
def lock_free():
 lock_identity();fd=os.open(lock,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB);fcntl.flock(fd,fcntl.LOCK_UN);return True
  except BlockingIOError:return False
 finally:os.close(fd)
def owner_pid():
 s=lock_identity();key=f'{os.major(s.st_dev):02x}:{os.minor(s.st_dev):02x}:{s.st_ino}'
 rows=[int(line.split()[4]) for line in (P/'locks').read_text().splitlines() if key in line.split() and '->' not in line.split()]
 return rows[0] if len(rows)==1 else None
def current_slot_identity(container,pid):
 for p in P.iterdir():
  if not p.name.isdigit():continue
  try:
   if os.readlink(p/'ns/pid')!=container['namespace']:continue
   lines=(p/'status').read_text().splitlines()
   if int(next(x for x in lines if x.startswith('NSpid:')).split()[-1])!=pid:continue
   raw=(p/'stat').read_text();identity={'hostPid':int(p.name),'localPid':pid,'startTicks':int(raw.rsplit(')',1)[1].split()[19]),'namespace':container['namespace']}
   incarnation(identity);return identity
  except (FileNotFoundError,ProcessLookupError):continue
 raise RuntimeError('Current non-owner native slot identity unavailable')
def is_native_cli_work(args):
 wrapped=any(x=='-c' and i+1<len(args) and 'from hermes_cli.main import main' in args[i+1] for i,x in enumerate(args))
 direct=any(x=='hermes_cli.main' or x.endswith(('/hermes','/cli.py','/run_agent.py')) for x in args)
 work=any(x in ['chat','--query','-q','-z','--oneshot','--resume'] or x.startswith(('--query=','--oneshot=','--resume=')) for x in args) or any(x.endswith('/run_agent.py') for x in args)
 return (wrapped or direct) and work
def final_native_proof_checks(proof,identity):
 native_identity=proof.get('identity') or {};rows=proof.get('profiles') or []
 return {'identityPid':native_identity.get('pid')==identity['localPid'],'identityStart':native_identity.get('start_time')==identity['startTicks'],'sourceSha':native_identity.get('code_sha')==plan['sourceSha'],'desiredStateRunning':proof.get('desiredState')=='running','gatewayStateRunning':proof.get('gatewayState')=='running','answeringPid':proof.get('answeringPid')==identity['localPid'],'exactProfiles':len(rows)==len(target['expectedProfiles']) and {r.get('profile') for r in rows}==set(target['expectedProfiles']),'allProfilesServing':all(r.get('nativeOn') is True and r.get('liveServed') is True and r.get('ownerPid')==identity['localPid'] for r in rows)}
def final_native_proof_matches(proof,identity):
 return all(final_native_proof_checks(proof,identity).values())
class Driver:
 def revalidate_owner(self):
  same_containers();incarnation(plan['owner']);assert owner_pid()==plan['owner']['hostPid'] and holds(plan['owner']['hostPid']),'Sole native owner changed'
  assert slot_pid(plan['from'])==plan['owner']['localPid'],'Owner native service changed'
 def assert_quiescent(self):
  self.revalidate_owner()
  code="import json,pathlib,sqlite3\nfrom gateway.control_socket import identify_gateway,query_gateway_control\nfrom tools.bot_desktop import runtime,lease\nfrom hermes_constants import set_hermes_home_override,reset_hermes_home_override\nh=pathlib.Path("+repr(target['nativeHome'])+");i=identify_gateway(h,timeout=3);s=query_gateway_control(h,'status',timeout=3) or {};screens=[];cron=0;workers=0\nfor profile in "+repr(target['expectedProfiles'])+":\n home=h if profile=='default' else h/'profiles'/profile;token=set_hermes_home_override(str(home))\n try:\n  screen=runtime.status(profile);l=lease.get(home);screens.append(not screen.running and l.holder=='agent')\n finally:reset_hermes_home_override(token)\n jobs=home/'cron/jobs.json'\n if jobs.exists():\n  value=json.loads(jobs.read_text());rows=value.get('jobs',[]) if isinstance(value,dict) else value;cron+=sum(bool(row.get('enabled')) for row in rows)\nfor path in [h/'kanban.db',*sorted((h/'kanban/boards').glob('*/kanban.db'))]:\n if not path.exists():continue\n with sqlite3.connect(path.as_uri()+'?mode=ro',uri=True) as c:\n  c.execute('PRAGMA query_only=ON');columns={r[1] for r in c.execute('PRAGMA table_info(tasks)')}\n  if 'worker_pid' in columns:workers+=c.execute('SELECT count(*) FROM tasks WHERE worker_pid IS NOT NULL').fetchone()[0]\nprint(json.dumps({'identity':i,'active_agents':s.get('active_agents'),'answering_pid':s.get('answering_pid'),'gateway_state':s.get('gateway_state'),'desired_state':s.get('desired_state'),'screensIdle':len(screens)==len("+repr(target['expectedProfiles'])+") and all(screens),'enabledCron':cron,'boardWorkers':workers}))"
  status=json.loads(native(plan['from'],code));identity=status.get('identity') or {}
  assert identity.get('pid')==plan['owner']['localPid'] and identity.get('start_time')==plan['owner']['startTicks'] and identity.get('code_sha')==plan['sourceSha'],'Fresh native owner identity differs'
  assert status.get('answering_pid')==plan['owner']['localPid'] and status.get('active_agents')==0 and status.get('gateway_state')=='running' and status.get('desired_state')=='running','Native gateway is not quiescent and running'
  assert status.get('screensIdle') is True and status.get('enabledCron')==0 and status.get('boardWorkers')==0,'Native profile effects are not quiescent'
  namespaces={plan['from']['namespace'],plan['intended']['namespace']}
  for p in P.iterdir():
   if not p.name.isdigit():continue
   try:
    if os.readlink(p/'ns/pid') not in namespaces:continue
    args=[x.decode(errors='replace') for x in (p/'cmdline').read_bytes().split(b'\0') if x]
    assert not is_native_cli_work(args),'Native CLI work still active in target containers'
   except (FileNotFoundError,ProcessLookupError):continue
  events.append({'step':'fresh-native-quiescence','identity':identity,'activeAgents':0})
 def park_intended_nonowner(self):
  self.revalidate_owner();pid=slot_pid(plan['intended'])
  if pid is not None:
   identity=current_slot_identity(plan['intended'],pid)
   incarnation(identity);assert not holds(identity['hostPid']) and slot_pid(plan['intended'])==pid,'Non-owner slot incarnation or lock changed'
   control(plan['intended'],['/command/s6-svc','-d',slot])
   control(plan['intended'],['/command/s6-svwait','-d','-t','20000',slot],timeout=23)
  else:control(plan['intended'],['/command/s6-svc','-d',slot])
  assert slot_pid(plan['intended']) is None,'Intended contender not down';self.revalidate_owner();events.append({'step':'intended-contender-down'})
 def stop_owner_and_wait(self):
  self.revalidate_owner();assert slot_pid(plan['intended']) is None,'Intended contender restarted'
  code="from gateway.status import write_planned_stop_marker;from hermes_cli.service_manager import S6ServiceManager;write_planned_stop_marker("+str(plan['owner']['localPid'])+");S6ServiceManager().stop("+repr(target['gatewaySlot'])+")"
  events.append({'step':'old-owner-native-stop-requested','desiredStateTransition':'running-to-stopped'})
  native(plan['from'],code,uid=0);control(plan['from'],['/command/s6-svwait','-d','-t','20000',slot],timeout=23)
  deadline=time.monotonic()+20
  while not lock_free() and time.monotonic()<deadline:time.sleep(.1)
  assert lock_free() and not holds(plan['owner']['hostPid']),'Native lock not released; no forced action performed'
  events.append({'step':'old-owner-down-and-lock-released'})
 def assert_both_down_and_lock_free(self):
  same_containers();assert slot_pid(plan['from']) is None and slot_pid(plan['intended']) is None and lock_free(),'Start refused: both slots must be down and original lock free'
 def start_intended(self):
  self.assert_both_down_and_lock_free()
  events.append({'step':'intended-native-start-requested','desiredStateTransition':'stopped-to-running'})
  native(plan['intended'],"from hermes_cli.gateway_multiplex_s6 import bring_root_slot_up;print(bring_root_slot_up())",uid=0)
  events.append({'step':'intended-native-root-start-requested'})
 def verify_intended_owner(self):
  deadline=time.monotonic()+25;last={'ownerAcquired':False,'failedPredicates':['ownerAcquired']}
  while time.monotonic()<deadline:
   same_containers();pid=owner_pid()
   if pid is not None:
    assert os.readlink(P/str(pid)/'ns/pid')==plan['intended']['namespace'],'Unexpected native owner acquired lock'
    assert slot_pid(plan['from']) is None,'Old owner slot resumed unexpectedly'
    lines=(P/str(pid)/'status').read_text().splitlines();local=int(next(x for x in lines if x.startswith('NSpid:')).split()[-1]);raw=(P/str(pid)/'stat').read_text();start=int(raw.rsplit(')',1)[1].split()[19])
    identity={'hostPid':pid,'localPid':local,'startTicks':start,'namespace':plan['intended']['namespace'],'containerId':plan['intended']['containerId']};assert holds(pid) and slot_pid(plan['intended'])==local
    code="import json,pathlib\nfrom gateway.control_socket import identify_gateway,query_gateway_control\nfrom gateway.host_attach import host_gateway_serving,invalidate_host_gateway_cache\nfrom hermes_cli.profiles import list_profiles\ninvalidate_host_gateway_cache();h=pathlib.Path("+repr(target['nativeHome'])+");i=identify_gateway(h,timeout=2);s=query_gateway_control(h,'status',timeout=2) or {};infos=list_profiles(lazy_skill_count=True);rows=[]\nfor name in "+repr(target['expectedProfiles'])+":\n info=next((p for p in infos if p.name==name),None);live=host_gateway_serving(name,wait_for_channel=0);rows.append({'profile':name,'nativeOn':bool(info and info.gateway_running),'liveServed':live is not None,'ownerPid':live.pid if live else None})\nprint(json.dumps({'identity':i,'desiredState':s.get('desired_state'),'gatewayState':s.get('gateway_state'),'answeringPid':s.get('answering_pid'),'profiles':rows}))"
    proof=json.loads(native(plan['intended'],code))
    checks=final_native_proof_checks(proof,identity);last={'ownerAcquired':True,'identity':identity,'native':proof,'checks':checks,'failedPredicates':[name for name,value in checks.items() if not value]}
    if final_native_proof_matches(proof,identity):
     events.append({'step':'intended-native-owner-and-profiles-verified','identity':identity,'native':proof});return identity
   time.sleep(.1)
  events.append({'step':'intended-native-verification-timeout','lastObservation':last})
  raise RuntimeError('Intended native completion timed out; inspect last ownership and profile predicates; no forced fallback performed')
