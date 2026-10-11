import {test,expect} from 'bun:test';
import {createHash} from 'node:crypto';
const hashText=(text:string)=>createHash('sha256').update(text).digest('hex');
import {resolveContext} from '../../src/backend/intent-context/resolve.ts';
import {parseLosslessJson} from '../../src/backend/intent-context/json-ingress.ts';
import {contextFixture} from './context-fixture.ts';
test('negative: missing or changed bytes do not resolve',()=>{
 for(const mode of ['missing','changed']){const f=contextFixture();if(mode==='missing')f.rows=[];else f.rows[0]!.content+=' altered';
 const r=resolveContext(f.candidate,f.request,f.rows);expect(r.status).toBe('NEEDS_EVIDENCE');expect(r.decisions).toEqual([]);}
});
test('negative: another owner scope, proposals and historical grants are not current direction',()=>{
 for(const mode of ['scope','proposal','historical-authorization']){const f=contextFixture();if(mode==='scope')f.request.profile='foreign-profile';else f.candidate.decisions[0]!.kind=mode;
 const r=resolveContext(f.candidate,f.request,f.rows);expect(r.status).toBe('SCOPE_UNRESOLVED');expect(r.authority).toBe('NO_EXECUTION_AUTHORITY');}
});
test('negative: unresolved revision conflict is not silently won by newest date',()=>{
 const f=contextFixture();f.candidate.decisions.push({...f.candidate.decisions[0]!,revision:2,statement:'Synthetic competing interpretation.'});
 const r=resolveContext(f.candidate,f.request,f.rows);expect(r.status).toBe('NEEDS_DECISION');expect(r.conflicts).toHaveLength(1);expect(r.decisions).toEqual([]);
});
test('negative: replayed source cannot support supersession',()=>{
 const f=contextFixture();f.candidate.decisions.push({...f.candidate.decisions[0]!,revision:2,supersedes:[1]});
 const r=resolveContext(f.candidate,f.request,f.rows);expect(r.status).toBe('REJECTED');expect(r.gaps[0]?.code).toBe('SUPERSESSION_NEEDS_NEW_SOURCE');
});
test('negative: duplicate JSON keys and rounded numeric spellings refuse',()=>{
 for(const text of ['{"x":1,"x":2}','{"x":9007199254740993}','{"x":1e0}'])expect(()=>parseLosslessJson(text)).toThrow();
});
import {SourceCoverage} from '../../src/backend/review/source-coverage.ts';
import {readHandoff,validSummary} from '../../src/backend/review/handoff-contract.ts';
for(const field of ['findings','limits'] as const){
 const input=(list:unknown)=>({verdict:'PASS',findings:[],limits:[],[field]:list});
 test(`R3: ${field} prototype-only indices refuse`,()=>{
  const list=new Array(1);Object.setPrototypeOf(list,Object.assign(Object.create(Array.prototype),{0:'Synthetic prototype-only finding'}));
  expect(Object.hasOwn(list,0)).toBe(false);expect(readHandoff(input(list),true)).toBeNull();
 });
 test(`R3: ${field} iterator cannot fill a hole`,()=>{
  const list=new Array(1);list[Symbol.iterator]=function*(){yield 'Synthetic iterator-only finding';return undefined;};
  expect(readHandoff(input(list),true)).toBeNull();
 });
 test(`R3: ${field} iterator cannot replace validated output`,()=>{
  const list=['Synthetic own entry'];list[Symbol.iterator]=function*(){yield 'Synthetic injected entry';return undefined;};
  expect(readHandoff(input(list),true)).toBeNull(); // R4 strict decoration refusal retains the R3 substitution case.
 });
 test(`R3: ${field} iterator accessor is never invoked`,()=>{
  const list=['Synthetic own entry'];let calls=0;Object.defineProperty(list,Symbol.iterator,{get(){calls++;throw new Error('Synthetic forbidden iterator read');}});
  expect(readHandoff(input(list),true)).toBeNull();expect(calls).toBe(0);
 });
 test(`R3: ${field} own indexed accessor refuses without invocation`,()=>{
  const list=new Array(1);let calls=0;Object.defineProperty(list,'0',{get(){calls++;return 'Synthetic accessor entry';}});
  expect(readHandoff(input(list),true)).toBeNull();expect(calls).toBe(0);
 });
}
test('R3: required top-level accessors refuse without invocation',()=>{
 for(const field of ['verdict','findings','limits']){let calls=0;const value={verdict:'PASS',findings:[],limits:[]};
  Object.defineProperty(value,field,{get(){calls++;return field==='verdict'?'PASS':[];}});
  expect(readHandoff(value,true)).toBeNull();expect(calls).toBe(0);}
});
test('R3: plain JSON lists retain verdicts Unicode BOM budgets and detached copies',()=>{
 for(const verdict of ['PASS','FAIL','NO_VERDICT']){const value=JSON.parse(JSON.stringify({verdict,findings:['😀'.repeat(1500),'\ufeff'],limits:['Synthetic limit']}));
  const output=readHandoff(value,true)!;expect(output).toEqual(value);output.findings.push('Synthetic mutation');expect(value.findings).toHaveLength(2);}
 expect(readHandoff({verdict:'PASS',findings:Array(12).fill(''),limits:[]},true)).not.toBeNull();
 expect(readHandoff({verdict:'PASS',findings:Array(13).fill(''),limits:[]},true)).toBeNull();
});
import {reviewFixture} from './review-fixture.ts';
test('negative: wrong pin, text, total, pagination and incomplete coverage cannot pass',()=>{
 const f=reviewFixture();expect(()=>new SourceCoverage(f.packet,{...f.expected,packetSha256:'0'.repeat(64)},()=>{})).toThrow();
 for(const delta of [{content:'tampered'},{total_chars:4},{has_more:true},{packet_sha256:'0'.repeat(64)}]){
 const issues:string[]=[];const source=new SourceCoverage(f.packet,f.expected,(ok,code)=>{if(!ok)issues.push(code);});source.read(f.args,{...f.page,...delta});source.finish();expect(issues.length).toBeGreaterThan(0);expect(issues).toContain('INCOMPLETE_SOURCE_COVERAGE');}
});
test('negative: missing, inherited, sparse, unknown or over-budget handoff fields refuse',()=>{
 for(const value of [{verdict:'PASS',findings:[]},Object.create({verdict:'PASS',findings:[],limits:[]}),{verdict:'PASS',findings:new Array(1),limits:[]},{verdict:'PASS',findings:[],limits:[],authority:'granted'},{verdict:'PASS',findings:['x'.repeat(1501)],limits:[]}])expect(readHandoff(value,true)).toBeNull();
 expect(validSummary('😀'.repeat(6001))).toBe(false);expect(validSummary('   ')).toBe(false);
});
import {assessReview} from '../../src/backend/review/review-evidence.ts';
test('negative: forbidden tools, conflicting results, false totals and premature completion refuse',()=>{
 for(const mode of ['tool','result','total','premature','foreign']){
 const f=reviewFixture();if(mode==='tool'){const calls=JSON.parse(f.observation.messages[1].tool_calls);calls[0].function.name='write_file';f.observation.messages[1].tool_calls=JSON.stringify(calls);}
 if(mode==='result')f.observation.messages[2].tool_call_id='unmatched';
 if(mode==='total')f.observation.session.tool_call_count=2;
 if(mode==='premature'){f.observation.messages.splice(2,1);f.observation.session.message_count=3;}
 if(mode==='foreign')f.observation.session.id='synthetic-foreign';
 expect(assessReview(f.packet,f.expected,f.observation).status).toBe('INSUFFICIENT');}
});
