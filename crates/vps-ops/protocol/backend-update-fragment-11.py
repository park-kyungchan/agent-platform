
try:
 owner=run_handoff(Driver());print(json.dumps({'status':'passed','events':events,'owner':owner,'imageId':plan['imageId'],'sourceSha':plan['sourceSha'],'nativeDesiredStateTransitionRequested':'running-stopped-running','finalDesiredStateVerified':'running','allExpectedProfilesVerified':True}))
except BaseException:
 print(json.dumps({'status':'failed','events':events,'imageRedeployed':False,'forcedSingletonBypass':False}),flush=True)
 raise
