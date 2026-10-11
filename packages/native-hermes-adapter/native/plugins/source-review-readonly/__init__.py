"""Thin native adapter for approved common pinned-source review sets."""
import hashlib,json,os,re,stat
from pathlib import Path
MAX_BYTES=262144
MAX_CHARS=8000
_ID=re.compile(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,63}')
_SHA=re.compile(r'[0-9a-f]{64}')
class Refused(ValueError):pass
def _object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Refused('duplicate_key')
        value[key] = item
    return value

def _keys(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise Refused('invalid_shape')
    return value

def _sha(value):
    if not isinstance(value, str) or not _SHA.fullmatch(value):
        raise Refused('invalid_pin')
    return value

def _path(value):
    if not isinstance(value, str) or not value.startswith('/') or '\\' in value or '\x00' in value:
        raise Refused('invalid_path')
    if any(x in ('.','..','') for x in value.split('/')[1:]):
        raise Refused('invalid_path')
    return Path(value)

def _read(path, expected, maximum=MAX_BYTES):
    """Open every path component relative to verified directory FDs; follow no symlinks."""
    path = _path(str(path)); _sha(expected)
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for segment in path.parts[1:-1]:
            nxt = os.open(segment, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd); fd = nxt
        leaf = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        with os.fdopen(leaf, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > maximum:
                raise Refused('nonregular_or_oversized_source')
            raw = stream.read(maximum + 1)
            after = os.fstat(stream.fileno())
            if (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns) != (after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns):
                raise Refused('source_changed_during_read')
    finally:
        os.close(fd)
    if len(raw) > maximum or hashlib.sha256(raw).hexdigest() != expected:
        raise Refused('source_pin_changed')
    return raw

def _json(path, expected):
    return json.loads(_read(path, expected).decode('utf-8'), object_pairs_hook=_object)


# Native Python registration seam; no alternate runtime, scheduler or credentials.
import subprocess
_POLICY_ROOT='/opt/data/workspace/hermes-harness-audit/platform/current/'
_BUN_PATHS={'/opt/data/toolchains/bun/1.4.1/bin/bun','/opt/data/toolchains/bun/1.4.2/bin/bun'}

def _native_profile():
    from hermes_constants import get_hermes_home
    home=Path(get_hermes_home())
    if str(home)=='/opt/data':return 'default'
    if home.parent==Path('/opt/data/profiles') and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,63}',home.name):return home.name
    raise ValueError('NATIVE_PROFILE_UNQUALIFIED')

def _verified_bun_fd(bun):
    expected={'/opt/data/toolchains/bun/1.4.1/bin/bun':'69293d3be4f0d6d624ca8581af4574435fa39209ab67e89eb03912866f3e14cb','/opt/data/toolchains/bun/1.4.2/bin/bun':'a83d263767d839e4d2649ca8e35d07159c7afc99afdc96d731ced29e056dda0c'}[bun]
    path=Path(bun);directory=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            child=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=directory);os.close(directory);directory=child
        fd=os.open(path.name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=directory)
    finally:os.close(directory)
    try:
        before=os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size>268435456:raise ValueError('POLICY_BUN_INVALID')
        digest=hashlib.sha256()
        while True:
            chunk=os.read(fd,1048576)
            if not chunk:break
            digest.update(chunk)
        after=os.fstat(fd)
        if digest.hexdigest()!=expected or (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns)!=(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns):raise ValueError('POLICY_BUN_DRIFT')
        return fd
    except BaseException:
        os.close(fd);raise

def _bundle_call(ctx,bundle,operation,payload):
    bun=ctx.get_config('policy_bun_path');expected=ctx.get_config(bundle.replace('.js','').replace('-','_')+'_sha256')
    if bun not in _BUN_PATHS or not isinstance(expected,str) or not re.fullmatch(r'[0-9a-f]{64}',expected):raise ValueError('POLICY_BRIDGE_UNQUALIFIED')
    path=Path(_POLICY_ROOT+bundle)
    raw=_read(path,expected,1048576) if '_read' in globals() else _bridge_read(path,expected)
    request=json.dumps(payload,separators=(',',':')).encode()
    if len(request)>1048576:raise ValueError('POLICY_INPUT_OVERSIZED')
    if len(raw)>100000:raise ValueError('POLICY_CODE_ARGUMENT_OVERSIZED')
    fd=_verified_bun_fd(bun)
    try:
        # Only hash-verified promoted code enters --eval. Payload stays JSON stdin; no shell.
        argv=['/proc/self/fd/'+str(fd),'--no-env-file','--eval',raw.decode('utf-8'),'hermes-pinned-policy',operation]
        if bundle=='read-context.js':argv+=['--profile',_native_profile()]
        result=subprocess.run(argv,input=request,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=20,check=False,pass_fds=(fd,))
    finally:os.close(fd)
    if result.returncode or len(result.stdout)>1048576:raise ValueError('POLICY_BRIDGE_REFUSED')
    response=json.loads(result.stdout)
    if not isinstance(response,dict) or response.get('ok') is False or response.get('error'):raise ValueError('POLICY_BRIDGE_REFUSED')
    return response

def _bridge_read(path,expected):
    # No symlink components, bounded regular source, exact byte pin.
    descriptor=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            nxt=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=descriptor);os.close(descriptor);descriptor=nxt
        leaf=os.open(path.name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=descriptor)
        with os.fdopen(leaf,'rb') as stream:
            info=os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_size>1048576:raise ValueError('POLICY_BUNDLE_INVALID')
            raw=stream.read(1048577)
    finally:os.close(descriptor)
    if len(raw)>1048576 or hashlib.sha256(raw).hexdigest()!=expected:raise ValueError('POLICY_BUNDLE_DRIFT')
    return raw


def register(ctx):
    profile=_native_profile()
    def handler(args,**kwargs):
        try:
            if _native_profile()!=profile or not isinstance(args,dict) or set(args)-{'action','document_id','offset','limit'}:raise ValueError('SOURCE_REVIEW_REFUSED')
            action=args.get('action','list');source_set=ctx.get_config('source_set_id')
            if not isinstance(source_set,str) or not _ID.fullmatch(source_set) or action not in ('list','read'):raise ValueError('SOURCE_REVIEW_REFUSED')
            if action=='list' and set(args)-{'action'}:raise ValueError('SOURCE_REVIEW_REFUSED')
            document=args.get('document_id')
            if action=='read' and (not isinstance(document,str) or not document):raise ValueError('SOURCE_REVIEW_REFUSED')
            offset,limit=args.get('offset',0),args.get('limit',MAX_CHARS)
            if type(offset) is not int or offset<0 or type(limit) is not int or not 1<=limit<=MAX_CHARS:raise ValueError('SOURCE_REVIEW_REFUSED')
            result=_bundle_call(ctx,'read-context.js','source-review',{'mode':'pinned-source','sourceSetId':source_set,'paths':[] if action=='list' else [document],'offset':offset,'limit':limit})
            if result.get('mode')!='pinned-source' or result.get('sourceSetId')!=source_set or result.get('authority')!='none':raise ValueError('SOURCE_REVIEW_REFUSED')
            return json.dumps(result,ensure_ascii=False)
        except (ValueError,OSError,subprocess.SubprocessError):return json.dumps({'error':'SOURCE_REVIEW_REFUSED','authority':'none','executionReady':False})
    return ctx.register_tool(name='source_review_read',toolset='source_review_readonly',schema={'name':'source_review_read','description':'List reviewed pinned source paths or read one listed path from the current Work source set. Real file bytes are verified by the common reader; no arbitrary path, write, network or supplied-evidence substitution.','parameters':{'type':'object','additionalProperties':False,'properties':{'action':{'type':'string','enum':['list','read']},'document_id':{'type':'string'},'offset':{'type':'integer','minimum':0},'limit':{'type':'integer','minimum':1,'maximum':MAX_CHARS}}}},handler=handler)

