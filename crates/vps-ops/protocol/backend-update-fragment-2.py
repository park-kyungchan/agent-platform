
import subprocess,re,datetime
result=subprocess.run(['git','ls-remote',policy['upstream'],'refs/heads/main'],text=True,capture_output=True,timeout=20)
assert result.returncode==0,'Main lookup failed'
rows=[line.split() for line in result.stdout.splitlines()]
assert len(rows)==1 and len(rows[0])==2 and rows[0][1]=='refs/heads/main' and re.fullmatch('[0-9a-f]{40}',rows[0][0]),'Invalid main response'
print(json.dumps({'sha':rows[0][0],'resolvedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'ref':'refs/heads/main','upstream':policy['upstream']}))
