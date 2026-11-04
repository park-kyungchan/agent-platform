
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
