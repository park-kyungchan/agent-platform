import {test,expect} from 'bun:test';
import {assessReview} from '../../src/backend/review/review-evidence.ts';
import {SourceCoverage} from '../../src/backend/review/source-coverage.ts';
import {reviewFixture} from './review-fixture.ts';
function discovered(name:string,value:any,args:any){const f=reviewFixture();f.observation.messages.splice(1,0,
 {id:2,session_id:f.expected.sessionId,role:'assistant',tool_calls:[{id:'discovery',function:{name,arguments:args}}]},
 {id:3,session_id:f.expected.sessionId,role:'tool',tool_call_id:'discovery',tool_name:name,content:value});
 f.observation.messages.forEach((row:any,i:number)=>row.id=i+1);f.observation.session.message_count=6;f.observation.session.tool_call_count=2;
 return assessReview(f.packet,f.expected,f.observation);}
const index=()=>{const f=reviewFixture();return {review_id:f.page.review_id,packet_sha256:f.page.packet_sha256,documents:[{id:f.page.document_id,source:f.page.source,sha256:f.page.source_sha256,total_chars:3}]};};
for(const delta of [{native_restriction:'DENIED',unknown_native_annotation:{status:'unrecognized'}},{native_restriction:'DENIED'},{unknown_native_annotation:{}},{error:'REVIEW_INPUT_REJECTED'},{error:false},{status:'DENIED'}]){
 test(`unsupported result annotations fail before page coverage: ${JSON.stringify(delta)}`,()=>{
  const f=reviewFixture();f.observation.messages[2].content=JSON.stringify({...f.page,...delta});
  const r=assessReview(f.packet,f.expected,f.observation);expect(r.status).toBe('INSUFFICIENT');expect(r.binding).toBeNull();expect(r.pageReads).toBe(0);expect(r.issues).toContain('UNSUPPORTED_RESULT_ANNOTATION');
  const issues:string[]=[];const s=new SourceCoverage(f.packet,f.expected,(ok,code)=>{if(!ok)issues.push(code);});s.read(f.args,{...f.page,...delta});s.finish();expect(s.pageReads).toBe(0);expect(issues).toContain('INCOMPLETE_SOURCE_COVERAGE');
 });
 test(`unsupported index annotation rejects: ${JSON.stringify(delta)}`,()=>{const f=reviewFixture();const issues:string[]=[];const s=new SourceCoverage(f.packet,f.expected,(ok,code)=>{if(!ok)issues.push(code);});s.read({},{...index(),...delta});expect(issues).toContain('UNSUPPORTED_RESULT_ANNOTATION');});
}
test('supported unannotated page and index retain benign source labels and totals',()=>{const f=reviewFixture();const issues:string[]=[];const s=new SourceCoverage(f.packet,f.expected,(ok,code)=>{if(!ok)issues.push(code);});s.read({},index());s.read(f.args,f.page);s.finish();expect(issues).toEqual([]);expect(assessReview(f.packet,f.expected,f.observation).status).toBe('COMPLETE_READER_EVIDENCE');});
const search={queries:['review read'],total_available:1,results:[{query:'review read',matches:['ocp_review_read']}],tools:{ocp_review_read:{source:'local',source_name:'Review',description:'Synthetic reader',required:[]}}};
const parameters={type:'object',additionalProperties:false,properties:{document_id:{type:'string',enum:['synthetic-document']},offset:{type:'integer',minimum:0},limit:{type:'integer',minimum:1,maximum:8000}}};
const describe={tools:{ocp_review_read:{description:'Synthetic reader',parameters}}};
test('supported native schema sanitization required empty array retains success',()=>{expect(discovered('tool_describe',{tools:{ocp_review_read:{description:'Synthetic reader',parameters:{...parameters,required:[]}}}},{names:['ocp_review_read']}).status).toBe('COMPLETE_READER_EVIDENCE');});
test('unknown nested schema annotations and malformed descriptions reject',()=>{for(const p of [{},{...parameters,native_restriction:'DENIED'},{...parameters,required:['document_id']},{...parameters,properties:{...parameters.properties,offset:{...parameters.properties.offset,unknown_native_annotation:{}}}}])expect(discovered('tool_describe',{tools:{ocp_review_read:{description:'Synthetic reader',parameters:p}}},{names:['ocp_review_read']}).binding).toBeNull();});
for(const [name,value,args] of [['tool_search',search,{queries:['review read']}],['tool_describe',describe,{names:['ocp_review_read']}]] as const){
 test(`supported ${name} discovery metadata preserved`,()=>{expect(discovered(name,value,args).status).toBe('COMPLETE_READER_EVIDENCE');});
 for(const delta of [{unknown_native_annotation:{}},{native_restriction:'DENIED'},{error:'denied'},{error:false}])test(`annotated ${name} rejects ${JSON.stringify(delta)}`,()=>{const r=discovered(name,{...value,...delta},args);expect(r.status).toBe('INSUFFICIENT');expect(r.binding).toBeNull();expect(r.issues).toContain('UNSUPPORTED_RESULT_ANNOTATION');});
}
test('supported search no-match hints and available source summaries retain shape',()=>{const value={...search,results:[{query:'review read',matches:[],available_sources:[{name:'Review',tool_count:1}],hint:'Retry a concrete query.'}],tools:{}};expect(discovered('tool_search',value,{queries:['review read']}).status).toBe('COMPLETE_READER_EVIDENCE');});
test('explicit describe error and not_found never complete',()=>{for(const delta of [{errors:{ocp_review_read:'denied'}},{not_found:['ocp_review_read'],hint:'Unavailable'}])expect(discovered('tool_describe',{...describe,...delta},{names:['ocp_review_read']}).binding).toBeNull();});
test('JSON final lists serialize holes to explicit null and reject them',()=>{for(const field of ['findings','limits']){const f=reviewFixture();const output:any={review_id:f.page.review_id,verdict:'pass',findings:[],limits:[]};output[field]=new Array(1);f.observation.messages[3].content=JSON.stringify(output);expect(assessReview(f.packet,f.expected,f.observation).binding).toBeNull();}});
