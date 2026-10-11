"""Native-required Python seam; additive task controls, no scheduler or actor override.

Reuse the exact installed native handler (including its guards, provenance,
workspace, subscription and idempotency paths). The only AST edits are a distinct
registered tool name and two keyword arguments to its one native create call.
No model-supplied code is evaluated and no native module is patched.
"""
import ast
import copy
import hashlib
import inspect
import os
import re
import stat
from pathlib import Path
import json
from tools import kanban_tools as native
from hermes_cli import kanban_db as kb
from tools.registry import registry
from hermes_constants import VALID_REASONING_EFFORTS

TOOL = 'kanban_create_precise'
SOURCE_SHA256 = 'e158d22494e1fb77bd95b3f7084c9a06f4f5ef4e77e8dd1f303a563a654283d0'
CONTROLS = {'model':'model_override', 'provider':'provider_override', 'reasoning_effort':'reasoning_effort',
            'max_retries':'max_retries', 'max_runtime_seconds':'max_runtime_seconds',
            'completion_contract':'completion_contract'}
REQUIRED = ('model','provider','reasoning_effort','max_retries','max_runtime_seconds','idempotency_key',
            'completion_contract','initial_status','workspace_kind')

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


def build_handler(ctx):
    source = inspect.getsource(native._handle_create)
    if hashlib.sha256(source.encode()).hexdigest() != SOURCE_SHA256:
        raise ValueError('NATIVE_CREATE_SOURCE_DRIFT: requalify this adapter before use')
    tree = ast.parse(source)
    function = tree.body[0]
    if len(tree.body) != 1 or not isinstance(function, ast.FunctionDef) or len(function.decorator_list) != 1:
        raise ValueError('NATIVE_CREATE_SHAPE_DRIFT')
    decorator = function.decorator_list[0]
    if ast.unparse(decorator) != "_kanban_handler('kanban_create')":
        raise ValueError('NATIVE_CREATE_DECORATOR_DRIFT')
    decorator.args[0] = ast.Constant(TOOL)
    calls = [n for n in ast.walk(function) if isinstance(n,ast.Call) and ast.unparse(n.func)=='kb.create_task']
    if len(calls) != 1 or any(k.arg in ('reasoning_effort','max_retries') for k in calls[0].keywords):
        raise ValueError('NATIVE_CREATE_CALL_DRIFT')
    for key in ('reasoning_effort','max_retries'):
        calls[0].keywords.append(ast.keyword(arg=key,value=ast.Subscript(value=ast.Name(id='args',ctx=ast.Load()),slice=ast.Constant(key),ctx=ast.Load())))
    namespace = dict(vars(native))
    namespace['_CREATED_FIELDS'] = tuple(dict.fromkeys((*native._CREATED_FIELDS,*CONTROLS.values())))
    exec(compile(ast.fix_missing_locations(tree),'<owned-native-task-controls>','exec'),namespace)
    original_path = namespace['_handle_create']
    def handler(args, **kw):
        try:
            native._reject_delegated_child_mutation(TOOL)
            if kb._explicit_board_intent_pinned():
                raise ValueError('NATIVE_BOARD_FENCE_DISALLOWS_SHARED_ROOT_ADAPTER')
            if not native._check_kanban_mode():
                raise ValueError('NATIVE_CALLER_NOT_ELIGIBLE')
            if not isinstance(args,dict) or any(k not in args for k in REQUIRED):
                raise ValueError('EXPLICIT_CONTROLS_REQUIRED')
            allowed = registry.get_schema(TOOL)['parameters']['properties']
            if set(args)-set(allowed):
                raise ValueError('UNKNOWN_ARGUMENT_OR_PROVENANCE_OVERRIDE')
            for key in ('model','provider','idempotency_key','assignee'):
                value=args.get(key)
                if not isinstance(value,str) or not value or value != value.strip():
                    raise ValueError('INVALID_LITERAL_CONTROL')
            if args['reasoning_effort'] not in ('none',*VALID_REASONING_EFFORTS):
                raise ValueError('INVALID_REASONING_EFFORT')
            for key in ('max_retries','max_runtime_seconds'):
                if type(args[key]) is not int or args[key] < 1:
                    raise ValueError('POSITIVE_INTEGER_CONTROL_REQUIRED')
            governance=args.get('governance')
            original_criterion=args['completion_contract']
            luna_scope=args.get('lunaScope')
            args={k:v for k,v in args.items() if k not in ('governance','lunaScope')} | {'completion_contract':json.dumps({'schema':'hermes.controlled-task-completion.v1','governance':governance,'semanticAcceptance':False,'originalCriterion':original_criterion},separators=(',',':'),sort_keys=True)}
            _bundle_call(ctx,'task-policy.js','validate-task',{'callerProfile':_native_profile(),'args':{k:v for k,v in args.items() if k not in ('governance','lunaScope')},'governance':governance,**({'lunaScope':luna_scope} if luna_scope is not None else {})})
            args={k:v for k,v in args.items() if k not in ('governance','lunaScope')}
            result=json.loads(original_path(dict(args),**kw))
            if result.get('ok') and any(result.get(target)!=args[key] for key,target in CONTROLS.items()):
                return json.dumps({'ok':False,'error':'NATIVE_CONTROL_READBACK_MISMATCH','task_id':result['task_id'],
                                   'recovery':'Native task retained. Inspect the exact task; no automatic update, replacement or retry.'})
            return json.dumps(result)
        except native._Reject as error:
            return error.args[0]
        except (ValueError,OSError,subprocess.SubprocessError) as error:
            return json.dumps({'ok':False,'error':str(error)})
    return handler

def register(ctx):
    handler = build_handler(ctx)
    schema=copy.deepcopy(registry.get_schema('kanban_create'))
    schema['name']=TOOL
    schema['description']='Create a native Kanban task with explicit model/provider/effort/runtime/retry controls. Same native caller, child fence and lifecycle as kanban_create; no dispatch, permissions or scheduler added. Readback mismatch retains the task and requires inspection, never a blind retry.'
    properties=schema['parameters']['properties']
    properties['reasoning_effort']={'type':'string','enum':['none',*VALID_REASONING_EFFORTS]}
    properties['max_retries']={'type':'integer','minimum':1,'description':'Native task failure limit; explicit, not a promise of additional retries.'}
    schema['parameters']['required']=list(dict.fromkeys((*schema['parameters']['required'],*REQUIRED)))
    properties['governance']={'type':'object','additionalProperties':False,'required':['projectId','workId','issueIds','specRevision','planRef','owner','allowedPaths','allowedEffects','dependencies','toolsets','maxIterations','completionCriterion'],'properties':{key:({'type':'array','items':{'type':'string'}} if key in ('issueIds','allowedPaths','allowedEffects','dependencies','toolsets') else {'type':'integer','minimum':1} if key=='maxIterations' else {'type':'string'}) for key in ('projectId','workId','issueIds','specRevision','planRef','owner','allowedPaths','allowedEffects','dependencies','toolsets','maxIterations','completionCriterion')}}
    properties['lunaScope']={'type':'object'}
    schema['parameters']['required'].append('governance')
    schema['parameters']['additionalProperties']=False
    ctx.register_tool(name=TOOL,toolset='native_task_controls',schema=schema,handler=handler,check_fn=native._check_kanban_mode,
                      description='Additive precision controls for the existing native Kanban create path.',emoji='📋')

