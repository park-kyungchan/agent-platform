
owner=out['gatewayOwner'];assert owner is not None,'Exactly one native gateway owner required'
assert owner['uid']==target['nativeUid'],'Owner UID differs from the configured native UID'
intended=next(c for c in cs if c['Config']['Labels']['com.docker.compose.service']==target['gatewayService'])
assert owner['containerId']==intended['Id'],'Gateway owner is outside the intended service'
native=command(['docker','exec','--user',str(owner['uid'])+':'+str(target['nativeGid']),'--workdir',target['nativeRepository'],'--env','HOME='+target['nativeHome'],'--env','HERMES_HOME='+target['nativeHome'],'--env','PYTHONDONTWRITEBYTECODE=1',owner['containerId'],target['nativePython'],'-B','-c',native_code],timeout=20)
out['native']=json.loads(native)
after=process_identity(owner['hostPid']);after_lock=metadata(lock)
out['ownerPreserved']=after==owner and (after_lock['device'],after_lock['inode'],after_lock['held'])==(lock_meta['device'],lock_meta['inode'],True)
out['observedAtUtc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
print(json.dumps(out))
