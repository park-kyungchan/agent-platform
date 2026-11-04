"""Maintainer utility: build literal fixtures from the immutable host oracle.
This file is not run by CI; candidate runs never regenerate expectations.
"""
import copy
import json
import tempfile
from pathlib import Path
from golden import oracle, ROOT

SID='01234567-89ab-cdef-0123-456789abcdef'
def p(id='wA:p1',**kw): return dict(pane_id=id,workspace_id='wA',tab_id='wA:t1',**kw)
def inf(shell=20,procs=None): return dict(shell_pid=shell,foreground_processes=procs or [])
def q(args,result): return dict(args=args,result=result)
def ls(ps): return q(['pane','list'],{'panes':ps})
def pi(p,i): return q(['pane','process-info','--pane',p['pane_id']],{'process_info':i})
def stat(parent=1,start=100): return '900 (name with ) spaces) S '+str(parent)+' '+' '.join(['0']*17)+f' {start}\n'
def base(name,args=None): return dict(name=name,args=args or [],proc={'900':stat()},queries=[])
def scenario(name,ps,infos,mode='ancestry',args=None,live=None,latest=None,**kwargs):
 c=base(name,args);c.update(kwargs);c['queries']=[ls(ps)]+[pi(p,i) for p,i in zip(ps,infos)]
 if mode not in ['none','ambiguous']:
  lp=live if live is not None else ps;c['queries'].append(ls(lp))
  chosen=ps[0]
  same=[p for p in lp if p['pane_id']==chosen['pane_id']]
  if len(same)==1 and all(same[0].get(k)==chosen.get(k) for k in ['workspace_id','tab_id']):
   c['queries'].append(pi(chosen,latest if latest is not None else infos[0]))
   if mode in ['resume','fresh','terminal']:
    c['queries'] += [pi(p,infos[ps.index(p)]) for p in lp if p['pane_id']!=chosen['pane_id']]
 return c
cases=[]
for arg in ['--help','-h']: cases.append(base('help-'+arg,[arg]))
cases += [dict(base('invalid-flag',['--bad'])),base('combined-flags',['--focused','--exports']),dict(base('missing-env'),herdr_env=False)]
for error in ['status','parse','missing','timeout','missing-result']:
 c=base('query-'+error);c['queries']=[dict(args=['pane','list'],error=error)];cases.append(c)
P=p();I=inf(900)
cases.append(dict(base('focused',['--focused']),queries=[ls([dict(P,focused=True),p('wA:p2')])]))
for n in [0,2]: cases.append(dict(base('focused-count-'+str(n),['--focused']),queries=[ls([dict(p('wA:p'+str(i)),focused=True) for i in range(n)])]))
cases.append(scenario('ancestry',[P],[I]))
cases.append(scenario('exports',[P],[I],args=['--exports']))
cases.append(scenario('parent-ancestry',[P],[inf(42)],proc={'900':stat(42),'42':stat(1)}))
cases.append(scenario('unresolved',[P],[inf()],mode='none'))
cases.append(scenario('broken-proc',[P],[inf()],mode='none',proc={'900':'broken'}))
cases.append(scenario('cycle-proc',[P],[inf()],mode='none',proc={'900':stat(901),'901':stat(900)}))
cases.append(scenario('ambiguous-ancestry',[P,p('wA:p2')],[I,I],mode='ambiguous'))
cases.append(scenario('pane-closed',[P],[I],live=[]))
cases.append(scenario('pane-moved',[P],[I],live=[dict(P,tab_id='wA:t2')]))
cases.append(scenario('association-changed',[P],[I],latest=inf(40)))
for field,value in [('workspace_id','bad'),('pane_id','wB:p1'),('tab_id','wB:t1')]:
 pp=dict(P,**{field:value});cases.append(scenario('exports-invalid-'+field,[pp],[I],args=['--exports']))
reported=dict(P,agent='codex',agent_session=dict(source='herdr:codex',kind='id',value=SID))
cases.append(scenario('reported-session',[reported],[inf()],mode='reported',session=SID))
cases.append(scenario('reported-session-changed',[reported],[inf()],mode='reported',session=SID,live=[P]))
cases.append(scenario('invalid-session',[reported],[inf()],mode='none',session='bad'))
resume=inf(procs=[dict(pid=55,argv=['/bin/codex','resume',SID])])
for args in [[],['--resume-candidate']]:
 cases.append(scenario('resume-'+str(args),[P],[resume],mode='resume',session=SID,args=args))
cases.append(scenario('resume-exports-refused',[P],[resume],mode='none',session=SID,args=['--exports']))
cases.append(scenario('resume-pid-changed',[P],[resume],mode='resume',session=SID,latest=inf(procs=[dict(pid=56,argv=['codex','resume',SID])])))
cases.append(scenario('resume-reported-refused',[reported],[resume],mode='none',session=SID,args=['--resume-candidate']))
cases.append(scenario('ambiguous-resume',[P,p('wA:p2')],[resume,resume],mode='ambiguous',session=SID,args=['--resume-candidate']))
for argv in [['codex','resume'],['codex','resume','other'],['not-codex','resume',SID],[1,'resume',SID],['/bin/codex/','resume',SID],['/bin/codex/.','resume',SID]]:
 cases.append(scenario('bad-resume-'+str(argv),[P],[inf(procs=[dict(pid=55,argv=argv)])],mode='none',session=SID))
cases.append(scenario('terminal',[P,p('wA:p2')],[inf(20),inf(30)],mode='terminal',ttys=[7],shell_ttys={'20':7,'30':8}))
cases.append(scenario('terminal-lost',[P],[inf(20)],mode='terminal',ttys=[7],shell_ttys={'20':7},latest=inf(30)))
cases.append(scenario('ambiguous-terminal',[P,p('wA:p2')],[inf(20),inf(30)],mode='ambiguous',ttys=[7],shell_ttys={'20':7,'30':7}))
fresh=inf(procs=[dict(pid=55,argv=['codex'],cwd='/workspace')])
for name,created,start,mode in [('new',1005000,100,'fresh'),('new-edge',1016000,100,'fresh'),('new-too-late',1016001,100,'none'),('new-before-start',1000999,100,'none')]:
 cases.append(scenario(name,[P],[fresh],mode=mode,session=SID,origin=[created,'/workspace'],proc={'900':stat(),'55':stat(start=start)}))
cases.append(scenario('new-pid-lost',[P],[fresh],mode='fresh',session=SID,origin=[1005000,'/workspace'],proc={'900':stat(),'55':stat()},latest=inf()))
cases.append(scenario('new-wrong-cwd',[P],[fresh],mode='none',session=SID,origin=[1005000,'/elsewhere'],proc={'900':stat(),'55':stat()}))
cases.append(scenario('priority-ancestry-over-terminal',[P,p('wA:p2')],[I,inf(20)],ttys=[7],shell_ttys={'20':7}))
for c in cases:
 with tempfile.TemporaryDirectory(dir=ROOT) as tmp:
  for pid,raw in c.get('proc',{}).items():
   path=Path(tmp)/pid/'stat';path.parent.mkdir(parents=True,exist_ok=True);path.write_text(raw)
  c['proc_root']=tmp
  c['expected']=oracle(c)
  del c['proc_root']
(ROOT/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
print(len(cases),'frozen fixtures')
