/** Frozen public synthetic native-owner adapter. Expected query outputs never
 * participate in discovery, authorization, source acquisition or ranking. */
import records from "./public/corpus.public.records.json" with { type:"json" };
import type { Scope, SourceRef, MemoryDocument, GoverningDocument, MemoryQuery } from "../generated/memory-v1.ts";
import type { MemoryAddress, NativeMemoryReader, NativeMemoryReadLease, NativeMemoryUse } from "../reader.ts";
import { MemoryAcquisitionGap } from "../reader.ts";
import { canonicalMemory, memoryDigest, assertMemory } from "../codec.ts";
export type PublicRow=typeof records.rows[number];
export interface PublicQueryInput {text:string;mode:string;asOf:string;limit:number;ownerFilter:unknown;selectedWork:unknown;applicableScopes:unknown[];}
const equal=(a:unknown,b:unknown)=>canonicalMemory(a)===canonicalMemory(b);
export function publicCorpusReader(q:PublicQueryInput,initialGeneration=8){
 const rows=records.rows;
 const state={generation:initialGeneration};const trace:Array<{phase:string;sourceId:string;generation:number}>=[];
 const bodies=new Map<string,Uint8Array>(),originals=new Map<string,Uint8Array>(),byUri=new Map<string,PublicRow>();
 const addresses:MemoryAddress[]=rows.map(row=>{
  const candidate:unknown=row.memory;
  assertMemory("MemoryRecord",candidate);
  const record=candidate;
  const semantics=row.fixtureSemantics as {claimKey?:string};
  const doc:MemoryDocument={schema:"common.memory-document.v1",record,...(semantics.claimKey?{claim:{key:semantics.claimKey,value:record.text}}:{})};
  assertMemory("MemoryDocument",doc);
  const original=new TextEncoder().encode(canonicalMemory(row.sourceDocument)+"\n");
  if(memoryDigest(original)!==record.sourceRefs[0]!.digest)throw Error("PUBLIC_ORIGINAL_FIXTURE_DIGEST_MISMATCH");
  originals.set(record.sourceRefs[0]!.uri,original);byUri.set(record.sourceRefs[0]!.uri,row);
  const bytes=new TextEncoder().encode(canonicalMemory(doc));const source={owner:record.sourceRefs[0]!.owner,uri:"fixture://canonical-memory/"+row.sourceId,revision:String(record.revision),digest:memoryDigest(bytes)};
  bodies.set(source.uri,bytes);byUri.set(source.uri,row);
  const {text:_,...header}=record;
  return {kind:"memory",currentness:"current",source,header,...(doc.claim?{claimKey:doc.claim.key}:{})};
 });
 const permits=new WeakMap<object,{sourceId:string;generation:number}>();
 const owner:NativeMemoryReader={async withCurrentRead(request,consume){
  const generation=state.generation;
  const current=()=>{if(state.generation!==generation)throw Error("PUBLIC_POLICY_GENERATION_CHANGED");};
  const policy=records.policy.generations.find(row=>row.generation===generation)!;
  const assertPermitted=(sourceId:string)=>{current();if(!policy.grantIds.includes(sourceId)||policy.excludedIds.includes(sourceId))throw Error("PUBLIC_FORBIDDEN_BODY_ACQUISITION");};
  const applies=(scope:Scope)=>q.applicableScopes.some(candidate=>equal(candidate,scope));
  const checkPermit=(uri:string,permit:object)=>{const row=byUri.get(uri)!;const p=permits.get(permit);if(!row||!p||p.sourceId!==row.sourceId||p.generation!==generation)throw Error("PUBLIC_OPAQUE_PERMIT_REQUIRED");assertPermitted(row.sourceId);return row;};
  const lease:NativeMemoryReadLease={readerId:records.policy.readerId,policyGeneration:String(generation),ownerPolicyGenerations:Object.fromEntries([...new Set(rows.map(row=>row.sourceDocument.issuer))].map(owner=>[owner,String(generation)])),nowUtc:q.asOf,expiresAtUtc:"2099-01-01T00:00:00Z",assertCurrent:current,
   async discover(input){current();return {entries:addresses.filter(address=>!input.scopes||input.scopes.some(scope=>equal(scope,address.header.owner))),gaps:[],truncated:false};},
   async authorize(address){current();const row=byUri.get(address.source.uri)!;trace.push({phase:"authorize",sourceId:row.sourceId,generation});if(!policy.grantIds.includes(row.sourceId)||policy.excludedIds.includes(row.sourceId))return null;const permit={};permits.set(permit,{sourceId:row.sourceId,generation});return permit;},
   async readOriginal(source,permit){const row=checkPermit(source.uri,permit);if(records.policy.permittedUnavailableSourceIds.includes(row.sourceId))throw new MemoryAcquisitionGap("unreachable");trace.push({phase:"original",sourceId:row.sourceId,generation});return originals.get(source.uri)!.slice();},
   async acquire(address,permit){const row=checkPermit(address.source.uri,permit);trace.push({phase:"body",sourceId:row.sourceId,generation});return bodies.get(address.source.uri)!.slice();},
   async qualifyMemoryUse(address,doc,permit):Promise<NativeMemoryUse|null>{
    const row=checkPermit(address.source.uri,permit),source=row.sourceDocument as {adoptionEvidence?:{id:string;issuer:string;scope:Scope}};
    const evidence=source.adoptionEvidence;
    if(!evidence||evidence.issuer!==row.sourceDocument.issuer||!equal(evidence.scope,doc.record.owner)||!applies(evidence.scope))return null;
    const proof:SourceRef=doc.record.sourceRefs[0]!;
    if(doc.record.kind!=="decision")return {kind:doc.record.kind==="preference"?"standing-preference":"standing-procedure",source:proof};
    const semantics=row.fixtureSemantics as {decisionId?:string;claimKey?:string};
    const decision:GoverningDocument={schema:"common.memory-governing.v1",id:semantics.decisionId??evidence.id,revision:doc.record.revision,owner:doc.record.owner,recordClass:"decision",status:doc.record.status==="active"?"active":doc.record.status==="superseded"?"superseded":"revoked",sourceRefs:doc.record.sourceRefs,allowedReaders:doc.record.allowedReaders,claim:{key:semantics.claimKey??evidence.id,value:doc.record.text},text:doc.record.text,...(doc.record.validFrom?{validFrom:doc.record.validFrom}:{}),...(doc.record.validUntil?{validUntil:doc.record.validUntil}:{})};
    return {kind:"governing-decision",source:proof,decision};
   },
   async qualifyGoverning(address,decision,permit){const row=checkPermit(address.source.uri,permit),source=row.sourceDocument as {adoptionEvidence?:{id:string;scope:Scope}};return !!source.adoptionEvidence&&applies(decision.owner)&&equal(source.adoptionEvidence.scope,decision.owner);},
  };
  const out=await consume(lease);current();return out;
 }};
 const request:MemoryQuery={schema:"common.memory-query.v1",purpose:"fixed independent public S02 source check",query:q.text,mode:q.mode as MemoryQuery["mode"],asOf:q.asOf,limit:q.limit,maxChars:64000,...(q.ownerFilter?{scopes:[q.ownerFilter as Scope]}:{}),...(q.selectedWork?{work:{projectId:(q.selectedWork as {projectId:string}).projectId,workId:(q.selectedWork as {workId:string}).workId}}:{})};
 return {owner,request,state,trace,sourceId:(source:SourceRef)=>byUri.get(source.uri)?.sourceId??"unknown",rows};
}
