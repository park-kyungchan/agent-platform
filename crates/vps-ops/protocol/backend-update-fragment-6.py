
import pathlib,hashlib,os,fcntl
helper=pathlib.Path(policy['helperPath']);assert helper.is_file() and not helper.is_symlink(),'Unsafe updater path'
assert hashlib.sha256(helper.read_bytes()).hexdigest()==policy['helperSha256'],'Updater identity changed'
lock=pathlib.Path(policy['updaterRoot'])/'lock';assert not lock.is_symlink(),'Unsafe updater lock'
identity=lock.stat()
expected=policy.get('updaterLock')
if expected:assert (identity.st_dev,identity.st_ino)==(expected['device'],expected['inode']),'Updater lock identity changed'
fd=os.open(lock,os.O_RDONLY|os.O_NOFOLLOW)
try:
 fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB);fcntl.flock(fd,fcntl.LOCK_UN)
except BlockingIOError:raise RuntimeError('Native updater already active; resume its existing run before starting another job')
finally:os.close(fd)
