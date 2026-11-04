
import base64,subprocess
result=subprocess.run(argv,input=base64.b64decode(patch),capture_output=True,timeout=20)
assert result.returncode==0,'Native overlay staging failed: '+result.stderr.decode()[-1000:]
print(result.stdout.decode())
