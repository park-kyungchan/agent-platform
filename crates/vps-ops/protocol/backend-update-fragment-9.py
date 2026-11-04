
import pathlib,subprocess,datetime
rows=[]
for path in pathlib.Path(policy['updaterRoot']).joinpath('transactions').glob('*.json'):
 if path.stat().st_mtime<datetime.datetime.fromisoformat(job['launched_at']).timestamp()-2:continue
 tx=json.loads(path.read_text())
 if tx.get('pid')==job['mainPid']:rows.append(tx)
state=subprocess.run(['systemctl','show',job['unit'],'--property=ActiveState','--value'],capture_output=True,text=True,timeout=5).stdout.strip()
print(json.dumps({'transactions':rows,'unitActive':state in ['active','activating','deactivating']}))
