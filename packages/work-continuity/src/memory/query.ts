import type { Scope, SourceRef } from "../git-work/generated/v3.ts";
import { readGitWorkV3Context } from "../git-work/effective-contract.ts";
import type { MemoryQuery, MemoryDocument, GoverningDocument } from "./generated/memory-v1.ts";
import { assertMemory, canonicalMemory, decodeMemory, memoryDigest } from "./codec.ts";
import { rankMemory, retrievalVersion } from "./ranking.ts";
import { MemoryAcquisitionGap, type MemoryAddress, type NativeMemoryReader, type NativeMemoryReadLease, type NativeMemoryUse } from "./reader.ts";

type Acquired = { key: string; source: SourceRef; currentness: MemoryAddress["currentness"] } & (
  | { kind: "memory"; document: MemoryDocument; use?: NativeMemoryUse }
  | { kind: "governing"; document: GoverningDocument; governs: boolean }
);
export interface MemoryQueryResult {
  schema: "common.memory-query-result.v1";
  mode: MemoryQuery["mode"];
  readerId: string;
  policyGeneration: string;
  ownerPolicyGenerations: Record<string, string>;
  observedAtUtc: string;
  asOf: string;
  hits: Array<Omit<Extract<Acquired, { kind: "memory" }>, "use"> & { usage: "advisory-evidence"; currentInstructionEligible: boolean; adoptionSource?: SourceRef; timeCoverage: "known" | "unknown"; temporalStatus: string }>;
  governing: Array<Extract<Acquired, { kind: "governing" }>>;
  conflicts: Array<{ claimKey: string; conflictingEvidence: boolean; governingConflict: boolean; selectedGoverningValue: string | null }>;
  taskContext?: ReturnType<typeof readGitWorkV3Context>;
  counts: { discovered: number; acquired: number; denied: number; statusExcluded: number; timeExcluded: number; scopeExcluded: number; matched: number; governingClosure: number };
  omissions: { sourceLimit: number; hitLimit: number; hitChars: number; governing: number; conflicts: number };
  acquisitionGaps: Array<{ code: string }>;
  sourceCoverage: "complete" | "partial";
  index: { version: string; lifetime: "query"; disposed: true; persistentBodies: false; rebuildWatermark: string };
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
const scopeKey = (scope: Scope) => canonicalMemory(scope);
export function memorySourceKey(address: MemoryAddress): string {
  return memoryDigest(canonicalMemory([address.source, address.header.owner, address.header.id, address.header.revision]));
}
function instant(value: string): number {
  const time = Date.parse(value);
  if (!Number.isFinite(time) || new Date(time).toISOString().replace('.000Z','Z') !== value.replace('.000Z','Z')) throw Error("MEMORY_EXACT_UTC_TIME_REQUIRED");
  return time;
}
function validAt(header: { validFrom?: string; validUntil?: string }, at: number): boolean {
  const from = header.validFrom === undefined ? -Infinity : instant(header.validFrom);
  const until = header.validUntil === undefined ? Infinity : instant(header.validUntil);
  if (from >= until) throw Error("MEMORY_INVALID_VALIDITY_INTERVAL");
  return from <= at && at < until;
}
function validateHeader(address: MemoryAddress): void {
  if (!address || !["current","historical","unknown"].includes(address.currentness) || !["memory", "governing"].includes(address.kind) || !address.source || Object.hasOwn(address.header, "text")) throw Error("MEMORY_BODY_IN_DISCOVERY_METADATA");
  if (address.kind === "memory") assertMemory("MemoryRecord", { ...address.header, text: "metadata validation" });
  else {
    if (Object.hasOwn(address.header,"claim") || Object.hasOwn(address.header,"schema")) throw Error("MEMORY_BODY_IN_DISCOVERY_METADATA");
    const { claimKey, ...header } = address.header;
    assertMemory("GoverningDocument", { ...header, schema: "common.memory-governing.v1", claim: { key: claimKey, value: "metadata validation" }, text: "metadata validation" });
  }
  if (Object.keys(address.source).sort().join(",")!=="digest,owner,revision,uri" || !address.source.owner?.trim() || !address.source.uri?.trim() || !address.source.revision?.trim() || !/^[a-f0-9]{64}$/.test(address.source.digest)) throw Error("MEMORY_EXACT_SOURCE_REQUIRED");
}
/** Production query boundary. Native reader authentication/policy is separate
 * from all JSON requests/source flags. No persistent text/result/embedding cache
 * exists. Every query and view use reauthorizes current originals and policy.
 * Native consumers retain custody/invalidation of copies already delivered.
 */
export async function withMemoryQuery<T>(input: unknown, owner: NativeMemoryReader, consume: (result: MemoryQueryResult) => T | Promise<T>): Promise<T> {
  assertMemory("MemoryQuery", input);
  const request = structuredClone(input);
  if (!owner || typeof owner.withCurrentRead !== "function") throw Error("MEMORY_CURRENT_NATIVE_READER_REQUIRED");
  if (request.task && request.work && (request.task.projectId !== request.work.projectId || request.task.workId !== request.work.workId)) throw Error("MEMORY_CONTEXT_SCOPE_MISMATCH");
  if (["dispatch", "resume"].includes(request.transport ?? "full") && (!request.task || !request.effectiveDigest)) throw Error("MEMORY_EXACT_TASK_CONTEXT_REQUIRED");
  return owner.withCurrentRead(request, async (lease) => {
    if (!lease.readerId?.trim() || !lease.policyGeneration?.trim() || !Object.keys(lease.ownerPolicyGenerations).length || Object.values(lease.ownerPolicyGenerations).some(value => !value.trim())) throw Error("MEMORY_CURRENT_AUTHENTICATED_POLICY_REQUIRED");
    const policy = canonicalMemory(lease.ownerPolicyGenerations), generation = lease.policyGeneration;
    const at = instant(request.asOf ?? lease.nowUtc), now = instant(lease.nowUtc), started = performance.now();
    if (request.mode === "current" && request.asOf !== undefined && at !== now) throw Error("MEMORY_HISTORY_MODE_REQUIRED_FOR_ASOF");
    const current = async () => {
      await lease.assertCurrent();
      if (lease.policyGeneration !== generation || canonicalMemory(lease.ownerPolicyGenerations) !== policy || instant(lease.expiresAtUtc) <= now + (performance.now() - started)) throw Error("MEMORY_POLICY_CHANGED_OR_EXPIRED");
    };
    await current();
    let taskContext: ReturnType<typeof readGitWorkV3Context> | undefined;
    if (request.task) {
      if (!lease.readTaskGraph) throw Error("MEMORY_EXACT_TASK_GRAPH_READER_REQUIRED");
      const graph = await lease.readTaskGraph(request.task); await current();
      taskContext = readGitWorkV3Context({ graph, task: request.task, mode: request.transport ?? "full", ...(request.effectiveDigest ? {expectedDigest:request.effectiveDigest} : {}) });
    }
    const inventory = await lease.discover({ mode: request.mode, purpose: request.purpose, ...(request.scopes ? {scopes:request.scopes} : {}) }); await current();
    const counts = { discovered: inventory.entries.length, acquired: 0, denied: 0, statusExcluded: 0, timeExcluded: 0, scopeExcluded: 0, matched: 0, governingClosure: 0 };
    const omissions = { sourceLimit: Math.max(0,inventory.entries.length-1000),hitLimit:0,hitChars:0,governing:0,conflicts:0 };
    const gaps: Array<{code:string}> = inventory.gaps.map(gap => ({code:gap.code}));
    if (inventory.truncated) gaps.push({code:"owner-inventory-truncated"});
    const acquired: Acquired[] = [], seen = new Set<string>();
    let acquiredBytes = 0, originalBytesTotal = 0;
    try {
      for (const original of inventory.entries.slice(0,1000)) {
        await current();
        validateHeader(original);
        const address = structuredClone(original), header = address.header;
        const identity = canonicalMemory([address.source.owner,address.source.uri,address.source.revision]);
        if (seen.has(identity)) throw Error("MEMORY_DUPLICATE_SOURCE_IDENTITY"); seen.add(identity);
        if (![address.source,...header.sourceRefs].every(source=>Object.hasOwn(lease.ownerPolicyGenerations,source.owner)) || !header.allowedReaders.includes(lease.readerId)) { counts.denied++; continue; }
        if (request.scopes && !request.scopes.some(scope => scopeKey(scope)===scopeKey(header.owner) || (address.kind === "memory" && address.header.subjects.some(subject=>scopeKey(subject)===scopeKey(scope))))) { counts.scopeExcluded++; continue; }
        if (request.mode === "current" && (header.status !== "active" || address.currentness !== "current")) { counts.statusExcluded++; continue; }
        const inTime = validAt(header,at);
        if (request.mode === "current" && !inTime) { counts.timeExcluded++; continue; }
        const permit = await lease.authorize(address,{mode:request.mode,purpose:request.purpose}); await current();
        if (!permit || typeof permit !== "object") {counts.denied++;continue;}
        if(acquiredBytes>=2_097_152||originalBytesTotal>=16_777_216){omissions.sourceLimit++;continue;}
        let bytes: Uint8Array;
        try {
          if(typeof lease.readOriginal!=="function")throw Error("MEMORY_EXACT_ORIGINAL_READER_REQUIRED");
          let originalBudgetOmitted=false;
          for(const source of address.header.sourceRefs){
            if(originalBytesTotal>=16_777_216){originalBudgetOmitted=true;break;}
            const originalBytes=await lease.readOriginal(source,permit);await current();
            if(originalBytes.byteLength>16_777_216||memoryDigest(originalBytes)!==source.digest)throw Error("MEMORY_ORIGINAL_SOURCE_HASH_MISMATCH");
            originalBytesTotal+=originalBytes.byteLength;
            if(originalBytesTotal>16_777_216){originalBudgetOmitted=true;break;}
          }
          if(originalBudgetOmitted){omissions.sourceLimit++;continue;}
          bytes=await lease.acquire(address,permit);
        }
        catch(error) { await current(); if (!(error instanceof MemoryAcquisitionGap)) throw error; gaps.push({code:error.code});continue; }
        await current();
        if (bytes.byteLength>1_048_576 || memoryDigest(bytes)!==address.source.digest) throw Error("MEMORY_SOURCE_BYTES_MISMATCH");
        counts.acquired++;
        // Bound the complete canonical envelope, including legacy metadata and
        // claim values, before parsing/retaining it. Text-only accounting allows
        // tiny summaries to carry unbounded aggregate source payloads.
        acquiredBytes+=bytes.byteLength;
        if(acquiredBytes>2_097_152){omissions.sourceLimit++;continue;}
        const key = memorySourceKey(address);
        if (address.kind === "memory") {
          const document=decodeMemory("MemoryDocument",bytes), {text,...actual}=document.record;
          if (canonicalMemory(actual)!==canonicalMemory(address.header) || (document.claim?.key ?? undefined)!==address.claimKey) throw Error("MEMORY_AUTHENTICATED_METADATA_MISMATCH");
          let use:NativeMemoryUse|null=null;
          if(address.currentness==="current"&&document.record.status==="active"&&document.record.authority==="owner-adopted"&&validAt(document.record,now)&&lease.qualifyMemoryUse){
            use=await lease.qualifyMemoryUse(address,document,permit);await current();
            if(use){
              if(!document.record.sourceRefs.some(source=>canonicalMemory(source)===canonicalMemory(use!.source)))throw Error("MEMORY_ADOPTION_SOURCE_NOT_ACQUIRED");
              if(use.kind==="governing-decision"){
                if(document.record.kind!=="decision"||!use.decision||!lease.qualifyGoverning)throw Error("MEMORY_ACTUAL_GOVERNING_DECISION_REQUIRED");
                assertMemory("GoverningDocument",use.decision);
                if(canonicalMemory(use.decision.owner)!==canonicalMemory(document.record.owner)||!use.decision.sourceRefs.every(source=>document.record.sourceRefs.some(acquiredSource=>canonicalMemory(source)===canonicalMemory(acquiredSource)))||(document.claim&&document.claim.key!==use.decision.claim.key))throw Error("MEMORY_ACTUAL_GOVERNING_SOURCE_SCOPE_MISMATCH");
                if(!use.decision.allowedReaders.includes(lease.readerId)||use.decision.status!=="active"||!validAt(use.decision,now)||!await lease.qualifyGoverning(address,use.decision,permit))use=null;
              }else if(!["preference","procedure","knowledge"].includes(document.record.kind)||!((use.kind==="standing-preference"&&document.record.kind==="preference")||use.kind==="standing-procedure"))throw Error("MEMORY_ADOPTION_CLASS_MISMATCH");
              await current();
            }
          }
          acquired.push({kind:"memory",source:address.source,currentness:address.currentness,key,document,...(use?{use}: {})});
        } else {
          const document=decodeMemory("GoverningDocument",bytes), {schema:_,text,claim,...actual}=document;
          const {claimKey,...expected}=address.header;
          if (canonicalMemory(actual)!==canonicalMemory(expected) || claim.key!==claimKey) throw Error("MEMORY_AUTHENTICATED_METADATA_MISMATCH");
          const governs=address.currentness==="current" && document.status==="active" && validAt(document,now) && !!lease.qualifyGoverning && await lease.qualifyGoverning(address,document,permit);
          await current(); acquired.push({kind:"governing",source:address.source,currentness:address.currentness,key,document,governs});
        }
      }
      await current();
      const ranked=rankMemory(acquired.map(row=>row.kind==="memory"?row.document.record.text:row.document.text),request.query);
      counts.matched=ranked.indices.length;
      // Work affects relevance among matching rows; it never filters permitted scopes.
      const contextWork=request.task??request.work;
      const relevant=(row:Acquired) => contextWork && [row.kind==="memory"?row.document.record.owner:row.document.owner,...(row.kind==="memory"?row.document.record.subjects:[])].some(scope=>"workId" in scope && scope.workId===contextWork.workId && scope.projectId===contextWork.projectId)?1:0;
      // Lexical recall of a summary must also surface the actual applicable
      // governing original for that claim, even when its wording differs. This
      // closes over already permitted/acquired sources, never new body reads.
      const claimOf=(row:Acquired)=>row.kind==="memory"?row.document.claim?.key:row.document.claim.key;
      const claimScores=new Map<string,number>();
      for(const index of ranked.indices){const claim=claimOf(acquired[index]!);if(claim)claimScores.set(claim,Math.max(claimScores.get(claim)??0,ranked.scores.get(index)!));}
      const candidateScores=new Map(ranked.scores);
      for(const [index,row] of acquired.entries())if(row.kind==="memory"&&row.use?.decision&&claimScores.has(row.use.decision.claim.key))candidateScores.set(index,claimScores.get(row.use.decision.claim.key)!);
      counts.governingClosure=candidateScores.size-ranked.indices.length;
      const selected=[...candidateScores.keys()].sort((a,b)=>{
        const left=acquired[a]!,right=acquired[b]!;
        const score=(row:Acquired,index:number)=>Math.max(candidateScores.get(index)!,claimScores.get(claimOf(row)??"")??0);
        return score(right,b)-score(left,a)||Number(right.kind==="memory"&&!!right.use?.decision)-Number(left.kind==="memory"&&!!left.use?.decision)||relevant(right)-relevant(left)||a-b;
      }).map(index=>acquired[index]!);
      const result:MemoryQueryResult={schema:"common.memory-query-result.v1",mode:request.mode,readerId:lease.readerId,policyGeneration:generation,ownerPolicyGenerations:JSON.parse(policy) as Record<string,string>,observedAtUtc:lease.nowUtc,asOf:request.asOf??lease.nowUtc,hits:[],governing:[],conflicts:[],...(taskContext?{taskContext}:{}),counts,omissions,acquisitionGaps:gaps,sourceCoverage:inventory.truncated||gaps.length||omissions.sourceLimit?"partial":"complete",index:{version:retrievalVersion,lifetime:"query",disposed:true,persistentBodies:false,rebuildWatermark:memoryDigest(canonicalMemory([generation,policy,acquired.map(row=>row.source)]))},authority:"none",nativePermissionGranted:false,semanticAcceptanceEstablished:false};
      const budget=request.maxChars??16000, limit=request.limit??10;
      const fits=()=>JSON.stringify(result).length<=budget;
      if(!fits())throw Error("MEMORY_METADATA_BUDGET_EXCEEDED");
      const governingRows:Array<Extract<Acquired,{kind:"governing"}>>=acquired.flatMap(row=>row.kind==="governing"?[row]:row.use?.decision?[{kind:"governing" as const,source:row.use.source,currentness:row.currentness,key:row.key+"/actual-governing",document:row.use.decision,governs:true}]:[]);
      const matchedClaims=new Set(selected.flatMap(row=>row.kind==="memory"?[...(row.document.claim?[row.document.claim.key]:[]),...(row.use?.decision?[row.use.decision.claim.key]:[])]:[row.document.claim.key]));
      for(const row of governingRows) if(matchedClaims.has(row.document.claim.key)) {
        result.governing.push(row);
        if(result.governing.length>20||!fits()){result.governing.pop();omissions.governing++;}
      }
      for(const row of selected) if(row.kind==="memory") {
        if(result.hits.length>=limit){omissions.hitLimit++;continue;}
        const {use,...visibleRow}=row;
        const decisionValues=use?.decision?[...new Set(governingRows.filter(g=>g.governs&&g.document.claim.key===use.decision!.claim.key).map(g=>g.document.claim.value))]:[];
        result.hits.push({...visibleRow,usage:"advisory-evidence",currentInstructionEligible:!!use&&(!use.decision||(decisionValues.length===1&&result.governing.some(g=>g.key===row.key+"/actual-governing"))),...(use?{adoptionSource:use.source}:{}),timeCoverage:row.document.record.validFrom||row.document.record.validUntil?"known":"unknown",temporalStatus:row.document.record.status==="candidate"?"candidate":row.document.record.status==="superseded"?"historical-superseded":row.document.record.status==="revoked"?"historical-revoked":row.document.record.validUntil&&instant(row.document.record.validUntil)<=at?"historical-expired":row.document.record.validFrom&&instant(row.document.record.validFrom)>at?"future":row.currentness!=="current"?"historical-revision":"current"});
        if(!fits()){result.hits.pop();omissions.hitChars++;}
      }
      for(const claimKey of matchedClaims) {
        const values=acquired.flatMap(row=>row.kind==="memory"?(row.document.claim?.key===claimKey?[row.document.claim.value]:[]):row.document.claim.key===claimKey?[row.document.claim.value]:[]);
        const authoritative=governingRows.filter(row=>row.governs&&row.document.claim.key===claimKey);
        const activeValues=[...new Set(authoritative.map(row=>row.document.claim.value))];
        const visible=authoritative.every(row=>result.governing.some(shown=>shown.key===row.key));
        result.conflicts.push({claimKey,conflictingEvidence:new Set(values).size>1,governingConflict:activeValues.length>1,selectedGoverningValue:activeValues.length===1&&visible?activeValues[0]!:null});
        if(!fits()){result.conflicts.pop();omissions.conflicts++;}
      }
      if(omissions.conflicts) for(const conflict of result.conflicts) conflict.selectedGoverningValue=null;
      if(!fits())throw Error("MEMORY_RESULT_BUDGET_EXCEEDED");
      await current(); const returned=await consume(structuredClone(result)); await current(); return returned;
    } finally { acquired.splice(0); seen.clear(); }
  });
}
/** A reusable view retains only query metadata, never acquired bodies or ranks.
 * Every use re-enters all current native leases. Closing refuses future uses. */
export function createMemoryQueryView(input: unknown, owner: NativeMemoryReader) {
  assertMemory("MemoryQuery",input); const query=structuredClone(input); let closed=false;
  return Object.freeze({withCurrent<T>(consume:(result:MemoryQueryResult)=>T|Promise<T>):Promise<T>{if(closed)throw Error("MEMORY_VIEW_CLOSED");return withMemoryQuery(query,owner,result=>{if(closed)throw Error("MEMORY_VIEW_CLOSED");return consume(result);});},close(){closed=true;},toJSON(){return {schema:"common.memory-view.v1",persistentBodies:false};}});
}
