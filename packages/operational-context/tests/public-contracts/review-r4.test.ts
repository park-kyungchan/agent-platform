import {test,expect} from 'bun:test';
import {assessReview} from '../../src/backend/review/review-evidence.ts';
import {readerResult,discoveryResult} from '../../src/backend/review/result-contract.ts';
import {SourceCoverage} from '../../src/backend/review/source-coverage.ts';
import {reviewFixture} from './review-fixture.ts';
function search(matches:any=['ocp_review_read']) {return {queries:['review read'],total_available:1,results:[{query:'review read',matches}],tools:{ocp_review_read:{source:'local',source_name:'Review',description:'Synthetic reader',required:[]}}};}
function discovered(value:any,args:any={queries:['review read']}) {
 const f=reviewFixture();f.observation.messages.splice(1,0,
 {id:2,session_id:f.expected.sessionId,role:'assistant',tool_calls:[{id:'discovery',function:{name:'tool_search',arguments:args}}]},
 {id:3,session_id:f.expected.sessionId,role:'tool',tool_call_id:'discovery',tool_name:'tool_search',content:value});
 f.observation.messages.forEach((r:any,i:number)=>r.id=i+1);f.observation.session.message_count=6;f.observation.session.tool_call_count=2;
 return assessReview(f.packet,f.expected,f.observation);
}
function refused(f:ReturnType<typeof reviewFixture>) {const r=assessReview(f.packet,f.expected,f.observation);expect(r.status).toBe('INSUFFICIENT');expect(r.binding).toBeNull();}
test('R4 reproduced forbidden indexed call cannot be replaced by iterator',()=>{
 const f=reviewFixture(),allowed=JSON.parse(f.observation.messages[1].tool_calls)[0];let runs=0;
 const entries=[{...allowed,function:{...allowed.function,name:'write_file'}}];entries[Symbol.iterator]=function*(){runs++;yield allowed;return undefined;};
 f.observation.messages[1].tool_calls=entries;refused(f);expect(runs).toBe(0);
});
test('R4 reproduced indexed discovery reader cannot be replaced by write_file iterator',()=>{
 let runs=0;const matches=['ocp_review_read'];matches[Symbol.iterator]=function*(){runs++;yield 'write_file';return undefined;};
 const value=search(matches);value.tools={write_file:{source:'local',source_name:'Review',description:'Synthetic unsupported discovery',required:[]}} as any;
 const r=discovered(value);expect(r.status).toBe('INSUFFICIENT');expect(r.binding).toBeNull();expect(runs).toBe(0);
});
const seams=['messages','tool_calls','queries','results','matches','required','bridge.calls','documents'] as const;
for(const seam of seams)for(const mode of ['hole','own-getter','inherited-getter','iterator','map','every','some','decoration'])test(`R4 ${seam}/${mode} refuses before caller code`,()=>{
 const f=reviewFixture();let runs=0;let value:any;
 if(seam==='messages')value=f.observation.messages;
 else if(seam==='tool_calls'){value=JSON.parse(f.observation.messages[1].tool_calls);f.observation.messages[1].tool_calls=value;}
 else if(seam==='bridge.calls'){const call=JSON.parse(f.observation.messages[1].tool_calls)[0];value=[{name:'ocp_review_read',arguments:f.args}];call.function={name:'tool_call',arguments:{calls:value}};f.observation.messages[1].tool_calls=[call];}
 else if(seam==='documents')value=[{id:f.page.document_id,source:f.page.source,sha256:f.page.source_sha256,total_chars:3}];
 else {const v=search();value=seam==='queries'?v.queries:seam==='results'?v.results:seam==='matches'?v.results[0].matches:v.tools.ocp_review_read.required;(f as any).discovery=v;}
 if(value.length===0)value.push('synthetic');
 if(mode==='hole')delete value[0];
 else if(mode==='own-getter')Object.defineProperty(value,'0',{get(){runs++;return 'synthetic';}});
 else if(mode==='inherited-getter'){delete value[0];const p=Object.create(Array.prototype);Object.defineProperty(p,'0',{get(){runs++;return 'synthetic';}});Object.setPrototypeOf(value,p);}
 else if(mode==='iterator')Object.defineProperty(value,Symbol.iterator,{get(){runs++;throw Error('caller iterator');}});
 else if(mode==='decoration')value.extra='synthetic';
 else Object.defineProperty(value,mode,{get(){runs++;throw Error('caller method');}});
 if(seam==='documents'){expect(readerResult({}, {review_id:f.page.review_id,packet_sha256:f.page.packet_sha256,documents:value})).toBe(false);}
 else if((f as any).discovery){const r=discovered((f as any).discovery);expect(r.status).toBe('INSUFFICIENT');expect(r.binding).toBeNull();}
 else refused(f);
 expect(runs).toBe(0);
});
for(const seam of ['observation.messages','row.tool_calls','call.function','function.arguments','args.document_id','page.content','page.error','discovery.matches'])test(`R4 ${seam} object accessor refuses without reading`,()=>{
 const f=reviewFixture();let runs=0;const call=JSON.parse(f.observation.messages[1].tool_calls)[0];call.function.arguments=f.args;f.observation.messages[1].tool_calls=[call];f.observation.messages[2].content=f.page;
 const targets:any={ 'observation.messages':[f.observation,'messages'],'row.tool_calls':[f.observation.messages[1],'tool_calls'],'call.function':[call,'function'],'function.arguments':[call.function,'arguments'],'args.document_id':[f.args,'document_id'],'page.content':[f.page,'content'],'page.error':[f.page,'error']};
 if(seam==='discovery.matches'){const v=search();Object.defineProperty(v.results[0],'matches',{get(){runs++;return ['ocp_review_read'];}});expect(discovered(v).binding).toBeNull();}
 else {const [target,key]=targets[seam];Object.defineProperty(target,key,{get(){runs++;throw Error('caller field');}});refused(f);}
 expect(runs).toBe(0);
});
test('R4 direct plain arrays and JSON strings preserve complete reader evidence',()=>{
 for(const direct of [false,true]){const f=reviewFixture();if(direct){const calls=JSON.parse(f.observation.messages[1].tool_calls);calls[0].function.arguments=f.args;f.observation.messages[1].tool_calls=calls;f.observation.messages[2].content=f.page;}expect(assessReview(f.packet,f.expected,f.observation).status).toBe('COMPLETE_READER_EVIDENCE');}
 expect(discovered(search()).status).toBe('COMPLETE_READER_EVIDENCE');
});
test('R4 public result predicate and coverage entrypoints reject caller methods',()=>{
 let runs=0;const v=search();Object.defineProperty(v.queries,'map',{get(){runs++;throw Error('caller map');}});
 expect(discoveryResult('tool_search',{queries:['review read']},v,'ocp_review_read')).toBe(false);expect(runs).toBe(0);
 const f=reviewFixture();Object.defineProperty(f.page,'content',{get(){runs++;throw Error('caller page');}});
 const s=new SourceCoverage(f.packet,f.expected,()=>{});expect(()=>s.read(f.args,f.page)).toThrow();expect(s.pageReads).toBe(0);expect(runs).toBe(0);
});
