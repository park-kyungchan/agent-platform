import {test,expect} from 'bun:test';
import {createHash} from 'node:crypto';
const hashText=(text:string)=>createHash('sha256').update(text).digest('hex');
import {resolveContext} from '../../src/backend/intent-context/resolve.ts';
import {contextFixture} from './context-fixture.ts';
test('positive: scoped source-bound direction is attributed but not authorized',()=>{
 const f=contextFixture();const result=resolveContext(f.candidate,f.request,f.rows);
 expect(result.status).toBe('RESOLVED');expect(result.decisions[0]?.sourceIds).toEqual(['synthetic-source']);
 expect(result.authority).toBe('NO_EXECUTION_AUTHORITY');expect(result.requestedSelectionComplete).toBe(true);
});
test('positive: explicit correction uses new evidence and retires only the exact predecessor',()=>{
 const f=contextFixture();const content='Synthetic correction: retain the typed boundary.';
 const row={...f.rows[0]!,id:2,content};f.rows.push(row);
 const source={...f.candidate.sources[0]!,id:'new-synthetic-source',messageId:2,contentSha256:hashText(content)};f.candidate.sources.push(source);
 f.candidate.decisions.push({...f.candidate.decisions[0]!,revision:2,sourceIds:[source.id],supersedes:[1]});
 const result=resolveContext(f.candidate,f.request,f.rows);expect(result.status).toBe('RESOLVED');expect(result.decisions.map(d=>d.revision)).toEqual([2]);
});
import {SourceCoverage} from '../../src/backend/review/source-coverage.ts';
import {readHandoff,validSummary} from '../../src/backend/review/handoff-contract.ts';
import {reviewFixture} from './review-fixture.ts';
test('positive: exact Unicode page coverage and complete handoff are structurally consumable',()=>{
 const f=reviewFixture();const issues:string[]=[];const coverage=new SourceCoverage(f.packet,f.expected,(ok,code)=>{if(!ok)issues.push(code);});
 coverage.read(f.args,f.page);expect(coverage.finish()).toEqual([{documentId:'synthetic-document',coveredChars:3,totalChars:3}]);expect(issues).toEqual([]);
 const input={verdict:'NO_VERDICT',findings:['Synthetic source requires an owner decision.'],limits:['Fixture only.']};const output=readHandoff(input,true)!;expect(output.verdict).toBe('NO_VERDICT');output.findings.push('caller mutation');expect(input.findings).toHaveLength(1);expect(validSummary('😀'.repeat(6000))).toBe(true);
});
import {assessReview} from '../../src/backend/review/review-evidence.ts';
test('positive: exact synthetic trace establishes reader evidence, not semantic acceptance',()=>{
 const f=reviewFixture();const r=assessReview(f.packet,f.expected,f.observation);expect(r.status).toBe('COMPLETE_READER_EVIDENCE');expect(r.verdict).toBe('revise');expect(r.pageReads).toBe(1);expect(r.issues).toEqual([]);
});
