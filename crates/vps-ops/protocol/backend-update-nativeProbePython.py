
import json,urllib.request
from pathlib import Path
from gateway.control_socket import identify_gateway,query_gateway_control
from gateway.host_attach import host_gateway_serving,invalidate_host_gateway_cache
from hermes_cli.profiles import list_profiles
from gateway.run import _profile_runtime_scope
from gateway.config import load_gateway_config
invalidate_host_gateway_cache()
identity=identify_gateway(Path(request['nativeHome']),timeout=2)
owner=host_gateway_serving(request['profile'],wait_for_channel=0)
status=query_gateway_control(Path(request['nativeHome']),'status',timeout=2) or {}
infos=list_profiles(lazy_skill_count=True);profiles=[]
for profile in request['expectedProfiles']:
 info=next((p for p in infos if p.name==profile),None);live=host_gateway_serving(profile,wait_for_channel=0)
 profiles.append({'profile':profile,'nativeOn':bool(info and info.gateway_running),'liveServed':live is not None,'nativeOwnerPid':live.pid if live else None})
configured={}
platforms=status.get('platforms') or {}
adapter_profiles={'default'}|{name.rsplit(':',1)[0] for name in platforms if ':' in name}
for profile in infos:
 if profile.name not in adapter_profiles:continue
 with _profile_runtime_scope(profile.path,hydrate_secrets=False):
  config=load_gateway_config()
  for platform,setting in config.platforms.items():
   name=platform.value if profile.name=='default' else profile.name+':'+platform.value
   configured[name]=bool(setting.enabled)
# Retain machine codes only; error_message/error/message/last_error are never read or emitted.
adapters=[{'name':name,'state':details.get('state'),'errorCode':details.get('error_code'),'configured':configured.get(name,True),'writerPid':details.get('writer_pid')} for name,details in platforms.items() if isinstance(details,dict)]
try:
 with urllib.request.urlopen('http://127.0.0.1:9120/api/health',timeout=3) as response:
  body=json.load(response);health={'code':response.status,'commit':body.get('commit')}
except Exception as exc:health={'code':getattr(exc,'code',0),'commit':None}
print(json.dumps({'identify':identity,'nativeServing':owner is not None,'nativeOwnerPid':owner.pid if owner else None,'profiles':profiles,'status':{k:status[k] for k in ['gateway_state','desired_state','answering_pid'] if k in status},'health':health,'adapters':adapters,'configuredActiveAdapters':sorted(name for name,enabled in configured.items() if enabled)}))
