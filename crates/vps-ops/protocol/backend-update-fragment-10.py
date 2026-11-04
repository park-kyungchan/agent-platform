
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
