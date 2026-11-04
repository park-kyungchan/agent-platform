
import subprocess
result=subprocess.run(argv,text=True,capture_output=True,timeout=90)
assert result.returncode==0,'Fresh font verification failed; do not accept metadata or replay an unknown writer'
assert len(result.stdout.encode())<=4096,'Font verification output exceeds bound'
print(json.dumps(json.loads(result.stdout)))
