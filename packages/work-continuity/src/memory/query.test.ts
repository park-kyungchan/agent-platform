import { test, expect } from "bun:test";
import { withMemoryQuery, createMemoryQueryView, encodeMemory, decodeMemory, memoryDigest } from "./v1.ts";
import type { MemoryQuery, MemoryRecord } from "./generated/memory-v1.ts";
import { memory, governing, nativeFixture, syntheticReaderId } from "./fixtures/native-reader.ts";
import { fixtureGraph, fixtureTask } from "../git-work/fixtures/v3-owner.ts";
import { resolveEffectiveContract, expandGitWorkV3Context } from "../git-work/v3.ts";
import { MemoryAcquisitionGap, type NativeMemoryReader } from "./reader.ts";
const query=(extra:Partial<MemoryQuery>={}):MemoryQuery=>({schema:"common.memory-query.v1",purpose:"source-only synthetic verification",query:"언어",mode:"current",limit:20,...extra});
const result=(q:MemoryQuery, f:ReturnType<typeof nativeFixture>)=>withMemoryQuery(q,f.owner,value=>value);

test("generated User Memory and public query need no fabricated Work or Episode; legacy fields remain outside record",()=>{
  const row=memory("personal-language","언어 한국어 설명",{kind:"preference"});
  expect(decodeMemory("MemoryDocument",encodeMemory("MemoryDocument",row))).toEqual(row);
  expect("workId" in row.record.owner).toBe(false);expect("episodeId" in row.record).toBe(false);
  expect(()=>encodeMemory("MemoryRecord",{...row.record,episodeId:"invented"} as MemoryRecord)).toThrow("INVALID_MEMORY");
  expect(()=>decodeMemory("MemoryDocument",JSON.stringify(row))).toThrow("CANONICAL_TRANSPORT");
});
test("all permitted User/Project/Work sources are useful without Work; selected Work only supplies relevance",async()=>{
  const f=nativeFixture([
    memory("user","언어 개인 설정",{kind:"preference"}),
    memory("p1","언어 첫 프로젝트",{owner:{kind:"project",projectId:"p1"}}),
    memory("w2","언어 다른 Work",{owner:{kind:"work",projectId:"p2",workId:"w2"}}),
  ]);
  const broad=await result(query(),f);expect(broad.hits).toHaveLength(3);expect(f.state.contextReads).toBe(0);
  const scoped=await result(query({work:{projectId:"p2",workId:"w2"}}),f);
  expect(scoped.hits.map(row=>row.document.record.id).sort()).toEqual(["p1","user","w2"]);
  expect(scoped.hits[0]!.document.record.id).toBe("w2");
  expect((await result(query({scopes:[{kind:"project",projectId:"p1"}]}),f)).hits.map(row=>row.document.record.id)).toEqual(["p1"]);
  expect((await result(query({scopes:[]}),f)).hits).toEqual([]);
});
test.each(["readerId","allowedReaders","policyGeneration","authorityRef","nativeOwner"])("request JSON cannot grant %s",async key=>{
  const f=nativeFixture([memory("one","언어")]);
  await expect(withMemoryQuery({...query(),[key]:"forged"},f.owner,value=>value)).rejects.toThrow("INVALID_MEMORY");
  expect(f.events).toEqual([]);
});
test("current native reader is mandatory before source access",async()=>{
  await expect(withMemoryQuery(query(),undefined!,value=>value)).rejects.toThrow("CURRENT_NATIVE_READER_REQUIRED");
  const f=nativeFixture([memory("one","언어")]);f.state.active=false;
  await expect(result(query(),f)).rejects.toThrow("POLICY_REVOKED");expect(f.reads.size).toBe(0);
});
test("current original permission gates body, ranking, context and every reusable view representation",async()=>{
  const secret="언어 PROTECTED_SYNTHETIC_MARKER";
  const f=nativeFixture([memory("secret",secret),memory("public","언어 공개")]);
  const view=createMemoryQueryView(query(),f.owner);
  expect((await view.withCurrent(value=>value)).hits).toHaveLength(2);
  f.state.generation++;f.denied.add(f.addresses[0]!.header.sourceRefs[0]!.uri);f.reads.clear();f.events.splice(0);
  const after=await view.withCurrent(value=>value);
  expect(after.hits.map(row=>row.document.record.id)).toEqual(["public"]);
  expect(JSON.stringify(after)).not.toContain("PROTECTED_SYNTHETIC_MARKER");
  expect(f.reads.has(f.addresses[0]!.source.uri)).toBe(false);
  expect(JSON.stringify(view)).not.toContain("PROTECTED_SYNTHETIC_MARKER");
  expect(after.index.persistentBodies).toBe(false);expect(after.index.disposed).toBe(true);
  expect(f.events.indexOf("authorize:"+f.addresses[1]!.source.uri)).toBeLessThan(f.events.indexOf("acquire:"+f.addresses[1]!.source.uri));
  view.close();expect(()=>view.withCurrent(value=>value)).toThrow("VIEW_CLOSED");
});
test("allowedReaders and authenticated owner scope deny before acquisition",async()=>{
  const f=nativeFixture([memory("no-reader","언어",{allowedReaders:["another-reader"]})]);
  const out=await result(query(),f);expect(out.hits).toEqual([]);expect(out.counts.denied).toBe(1);expect(f.reads.size).toBe(0);
  f.addresses[0]!.header.allowedReaders=[syntheticReaderId];f.addresses[0]!.source.owner="unqualified-owner";
  expect((await result(query(),f)).counts.denied).toBe(1);expect(f.reads.size).toBe(0);
});
test.each(["beforeAcquire","afterAcquire"] as const)("policy revocation %s prevents derived/consumer output",async hook=>{
  const f=nativeFixture([memory("secret","언어 PROTECTED_SYNTHETIC_MARKER")]);
  f.state[hook]=()=>{f.state.generation++;};let consumed=false;
  await expect(withMemoryQuery(query(),f.owner,()=>{consumed=true;})).rejects.toThrow("POLICY_REVOKED");expect(consumed).toBe(false);
});
test("current/history modes separate status from present access revocation and retain temporal uncertainty",async()=>{
  const f=nativeFixture([
    ...(["active","candidate","superseded","revoked"] as const).map(status=>memory(status,"언어 "+status,{status})),
    memory("expired","언어 이전",{validFrom:"2000-01-01T00:00:00Z",validUntil:"2001-01-01T00:00:00Z"}),
    memory("future","언어 이후",{validFrom:"2099-01-01T00:00:00Z"}),
  ]);
  expect((await result(query(),f)).hits.map(row=>row.document.record.id)).toEqual(["active"]);
  const history=await result(query({mode:"history"}),f);expect(history.hits).toHaveLength(6);
  expect(history.hits.find(row=>row.document.record.id==="revoked")!.document.record.status).toBe("revoked");
  expect(history.hits.find(row=>row.document.record.id==="active")!.timeCoverage).toBe("unknown");
  f.denied.add(f.addresses[3]!.source.uri);expect((await result(query({mode:"history"}),f)).hits.some(row=>row.document.record.id==="revoked")).toBe(false);
  f.state.historyAllowed=false;f.reads.clear();expect((await result(query({mode:"history"}),f)).hits).toEqual([]);expect(f.reads.size).toBe(0);
  await expect(result(query({asOf:"2000-01-01T00:00:00Z"}),f)).rejects.toThrow("HISTORY_MODE_REQUIRED");
});
test("actual current governing Decision prevails over decision-kind/owner-adopted Memory, conflicts stay explicit",async()=>{
  const f=nativeFixture([memory("summary","언어 영어",{kind:"decision",authority:"owner-adopted"},{key:"language",value:"English"}),governing("actual","Korean")]);
  const out=await result(query(),f);expect(out.hits[0]!.usage).toBe("advisory-evidence");
  expect(out.conflicts[0]).toEqual({claimKey:"language",conflictingEvidence:true,governingConflict:false,selectedGoverningValue:"Korean"});
  expect(out.nativePermissionGranted).toBe(false);expect(out.semanticAcceptanceEstablished).toBe(false);
  f.state.governingQualified=false;expect((await result(query(),f)).conflicts[0]!.selectedGoverningValue).toBeNull();
  const conflict=nativeFixture([governing("one","Korean"),governing("two","English")]);
  expect((await result(query(),conflict)).conflicts[0]!.governingConflict).toBe(true);
  expect((await result(query(),conflict)).conflicts[0]!.selectedGoverningValue).toBeNull();
});
test.each(["bytes","header","duplicate"])("source %s contradiction never yields partial qualified output",async mutation=>{
  const f=nativeFixture([memory("one","언어")]);
  if(mutation==="bytes")f.bodies.set(f.addresses[0]!.source.uri,new TextEncoder().encode("{}"));
  if(mutation==="header")f.addresses[0]!.header.status="superseded";
  if(mutation==="duplicate")f.addresses.push(structuredClone(f.addresses[0]!));
  await expect(result(query({mode:"history"}),f)).rejects.toThrow();
});
test("source-scoped identity retains equal legacy IDs in distinct owners/projects",async()=>{
  const f=nativeFixture([memory("old-id","언어",{owner:{kind:"project",projectId:"one"}}),memory("old-id","언어",{owner:{kind:"project",projectId:"two"}})]);
  const out=await result(query(),f);expect(out.hits).toHaveLength(2);expect(new Set(out.hits.map(row=>row.key)).size).toBe(2);
});
test("whole-row budgets and bounded gaps remain visible without exception/body leakage",async()=>{
  const f=nativeFixture([memory("one","언어 "+"x".repeat(1000)),memory("two","언어 공개")]);
  const out=await result(query({limit:1}),f);expect(out.hits).toHaveLength(1);expect(out.omissions.hitLimit).toBe(1);
  f.state.gaps=[{code:"unreachable"}];f.state.truncated=true;
  const gaps=await result(query(),f);expect(gaps.sourceCoverage).toBe("partial");expect(gaps.acquisitionGaps).toEqual([{code:"unreachable"},{code:"owner-inventory-truncated"}]);
  const owner:NativeMemoryReader={async withCurrentRead(q,consume){return f.owner.withCurrentRead(q,lease=>consume({...lease,acquire:async()=>{throw new MemoryAcquisitionGap("missing");}}));}};
  expect((await withMemoryQuery(query(),owner,value=>value)).acquisitionGaps.some(gap=>gap.code==="missing")).toBe(true);
  await expect(result(query({maxChars:512}),f)).rejects.toThrow("BUDGET");
});
test("all four transports compose the same S01 effective contract while broad Memory and unrelated Task independence remain",async()=>{
  const graph=fixtureGraph();graph.tasks.push({...structuredClone(graph.tasks[0]!),taskId:"independent-held",status:"blocked"});
  const effective=resolveEffectiveContract(graph,fixtureTask),f=nativeFixture([memory("cross-work","언어",{owner:{kind:"work",projectId:"another",workId:"another"}})],graph);
  for(const transport of ["full","compact","dispatch","resume"] as const){
    const out=await result(query({task:fixtureTask,effectiveDigest:effective.digest,transport}),f);
    expect(expandGitWorkV3Context(out.taskContext!).effective).toEqual(effective);expect(out.hits).toHaveLength(1);
  }
  f.reads.clear();await expect(result(query({task:fixtureTask,effectiveDigest:"0".repeat(64),transport:"resume"}),f)).rejects.toThrow("STALE_EFFECTIVE");expect(f.reads.size).toBe(0);
});

test("historical immutable revision cannot masquerade as current merely because its old body says active",async()=>{
 const f=nativeFixture([memory("past","언어 과거")]);f.addresses[0]!.currentness="historical";
 expect((await result(query(),f)).hits).toEqual([]);expect(f.reads.size).toBe(0);
 const history=await result(query({mode:"history"}),f);expect(history.hits[0]!.currentness).toBe("historical");expect(history.hits[0]!.document.record.status).toBe("active");
});

test("every original owner must be in the current lease before native original/body acquisition",async()=>{
 const f=nativeFixture([memory("cross-owner","언어",{sourceRefs:[{owner:"absent-original-owner",uri:"fixture://unavailable-owner/one",revision:"1",digest:"a".repeat(64)}]})]);
 const out=await result(query(),f);expect(out.hits).toEqual([]);expect(out.counts.denied).toBe(1);expect(f.events.some(event=>event.startsWith("original:")||event.startsWith("acquire:"))).toBe(false);
});
test("missing or corrupt exact originals reject before derived body even when metadata and labels are plausible",async()=>{
 const f=nativeFixture([memory("one","언어")]);
 for(const readOriginal of [undefined,async()=>new TextEncoder().encode("corrupted original")]){
  const owner:NativeMemoryReader={async withCurrentRead(q,consume){return f.owner.withCurrentRead(q,lease=>consume({...lease,readOriginal:readOriginal!}));}};
  await expect(withMemoryQuery(query(),owner,value=>value)).rejects.toThrow();expect(f.reads.size).toBe(0);
 }
});
test("current actual governing original closes a recalled summary claim even without matching its wording",async()=>{
 const summary=memory("summary","API_Z9",{kind:"knowledge",authority:"model-inference"},{key:"language",value:"English"});
 const adopted=memory("adopted","전혀 다른 한국어 결정",{kind:"decision",authority:"owner-adopted"},{key:"language",value:"Korean"});
 const actual=governing("actual-source","Korean");
 // Native owner qualifies actual governing bytes from the already acquired
 // original. Neither kind nor authority alone supplies this proof.
 const f=nativeFixture([summary,adopted]);
 const decision={...actual,sourceRefs:adopted.record.sourceRefs};
 const owner:NativeMemoryReader={async withCurrentRead(q,consume){return f.owner.withCurrentRead(q,lease=>consume({...lease,qualifyMemoryUse:async(address)=>address.header.id==="adopted"?{kind:"governing-decision",source:adopted.record.sourceRefs[0]!,decision}:null}));}};
 const out=await withMemoryQuery(query({query:"API_Z9"}),owner,value=>value);
 expect(out.hits.map(hit=>hit.document.record.id)).toEqual(["adopted","summary"]);expect(out.counts.governingClosure).toBe(1);expect(out.conflicts[0]!.selectedGoverningValue).toBe("Korean");
 expect(out.hits[0]!.currentInstructionEligible).toBe(true);
 decision.allowedReaders=["another-reader"];
 const excludedDecision=await withMemoryQuery(query({query:"API_Z9"}),owner,value=>value);expect(excludedDecision.hits.map(hit=>hit.document.record.id)).toEqual(["summary"]);expect(excludedDecision.governing).toEqual([]);
 decision.allowedReaders=adopted.record.allowedReaders;
 f.state.governingQualified=false;
 const unqualified=await withMemoryQuery(query({query:"API_Z9"}),owner,value=>value);expect(unqualified.hits.map(hit=>hit.document.record.id)).toEqual(["summary"]);
});
test("current-policy requalification rebuilds views after a denial; closing during acquisition prevents consumer use",async()=>{
 const f=nativeFixture([memory("one","언어")]);const view=createMemoryQueryView(query(),f.owner);
 f.denied.add(f.addresses[0]!.source.uri);expect((await view.withCurrent(value=>value)).hits).toHaveLength(0);
 f.state.generation++;f.denied.clear();expect((await view.withCurrent(value=>value)).hits).toHaveLength(1);
 f.state.afterAcquire=()=>view.close();let consumed=false;
 await expect(view.withCurrent(()=>{consumed=true;})).rejects.toThrow("VIEW_CLOSED");expect(consumed).toBe(false);
});

test("whole canonical document budget includes legacy metadata even when summary text is tiny",async()=>{
 const docs=[0,1,2,3].map(i=>{const doc=memory("large-legacy-"+i,"언어");doc.legacy={protocol:"synthetic-legacy",identityKey:doc.record.id,original:doc.record.sourceRefs[0]!,metadata:Array.from({length:15},()=>"x".repeat(60_000))};return doc;});
 const f=nativeFixture(docs),out=await result(query({maxChars:64000}),f);
 expect(out.counts.acquired).toBe(3);expect(f.reads.size).toBe(3);expect(out.counts.matched).toBe(2);expect(out.omissions.sourceLimit).toBe(2);expect(out.sourceCoverage).toBe("partial");expect(out.hits).toEqual([]);expect(out.omissions.hitChars).toBe(2);
});
test("aggregate original acquisition budget omits a source before derived body acquisition and reports partial coverage",async()=>{
 const bytes=new TextEncoder().encode("x".repeat(6_000_000)),digest=memoryDigest(bytes);
 const docs=[0,1,2].map(i=>memory("original-budget-"+i,"언어",{sourceRefs:[{owner:"synthetic-owner",uri:"fixture://large-original/"+i,revision:"1",digest}]}));
 const f=nativeFixture(docs);
 const owner:NativeMemoryReader={async withCurrentRead(q,consume){return f.owner.withCurrentRead(q,lease=>consume({...lease,readOriginal:async()=>bytes}));}};
 const out=await withMemoryQuery(query(),owner,value=>value);expect(out.hits).toHaveLength(2);expect(f.reads.size).toBe(2);expect(out.omissions.sourceLimit).toBe(1);expect(out.sourceCoverage).toBe("partial");
});
