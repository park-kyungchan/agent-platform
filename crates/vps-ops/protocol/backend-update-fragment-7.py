
import pathlib,subprocess,datetime
matches=[]
for path in pathlib.Path(policy['updaterRoot']).joinpath('jobs').glob('*.json'):
 if path.name in checkpoint['baselineJobs']:continue
 job=json.loads(path.read_text())
 requested_at=datetime.datetime.fromisoformat(checkpoint['requestedAtUtc'].replace('Z','+00:00')).timestamp()
 launched_at=datetime.datetime.fromisoformat(job['launched_at'].replace('Z','+00:00')).timestamp()
 if job.get('action')==checkpoint['action'] and job.get('requested_sha')==checkpoint['sha'] and (job.get('overlay_sha256') or None)==checkpoint['overlaySha256'] and bool(job.get('bot_desktop',False))==bool(checkpoint.get('botDesktop',False)) and bool(job.get('font_readability',False))==bool(checkpoint.get('fontReadability',False)) and (job.get('release_tag') or None)==checkpoint.get('releaseTag') and launched_at>=requested_at-5:
  job['mainPid']=int(subprocess.check_output(['systemctl','show',job['unit'],'--property=ExecMainPID','--value'],text=True,timeout=5).strip() or '0');matches.append(job)
assert len(matches)<=1,'Ambiguous recovered job'
if matches:assert matches[0]['mainPid']>0,'Recovered job already collected; reconcile transaction receipts before continuing'
print(json.dumps(matches[0] if matches else None))
