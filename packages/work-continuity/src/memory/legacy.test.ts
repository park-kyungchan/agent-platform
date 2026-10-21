import { test, expect } from "bun:test";
import { projectQualifiedLegacyMemory, memoryDigest } from "./v1.ts";
import { episodeFingerprint, sourceKey, type DerivedMemory, type Episode, type MemoryPolicy, type RecallContext, type DerivationReceipt } from "../workspace/qualification.ts";
function fixture(){
 const original={repositoryId:"synthetic-repo",revision:"r1",relativePath:"notes/source.md",sha256:memoryDigest("synthetic-original"),byteLength:18};
 const episode:Episode={id:"episode-old",provenance:"source-linked",sources:[{source:original,fragmentIds:["fragment-old"]}]};
 const memory:DerivedMemory={id:"memory-old",episodeId:episode.id,generationId:"generation-old",policyRevision:"policy-old",text:"언어 한국어",contentHash:memoryDigest("언어 한국어"),sourceKeys:[sourceKey(original)],scope:{kind:"repository",repositoryId:original.repositoryId},claim:{key:"claim-old",value:"Korean"}};
 const policy:MemoryPolicy={revision:"policy-old",excludedSourceHashes:[],excludedFragmentIds:[],aliases:[],commonAdmissions:[]};
 const context:RecallContext={repositoryId:original.repositoryId,explicitlySharedRepositoryIds:new Set(),commonEnabled:false,applicability:"synthetic",currentPolicyRevision:"policy-old",policyHead:"current",activeGenerationId:"generation-old",readableMemoryIds:new Set([memory.id])};
 const receipt:DerivationReceipt={memoryId:memory.id,episodeId:episode.id,generationId:memory.generationId,policyRevision:memory.policyRevision,contentHash:memory.contentHash,sourceKeys:memory.sourceKeys,provenance:"owner-verified",claim:memory.claim,scope:memory.scope,episodeFingerprint:episodeFingerprint(episode)};
 return {memory,episode,policy,context,receipts:[receipt],owner:{kind:"project" as const,projectId:original.repositoryId},readers:["synthetic-reader"],source:{owner:"synthetic-owner",uri:"fixture://legacy-record",revision:"legacy-r1",digest:memoryDigest(JSON.stringify(memory))},revision:1,sourceBindings:[{legacyKey:sourceKey(original),source:{owner:"synthetic-owner",uri:"fixture://legacy-original",revision:original.revision,digest:original.sha256}}]};
}
test("versioned legacy bridge retains old identity/generation/claim and full original lineage without granting adoption",()=>{
 const f=fixture(),projected=projectQualifiedLegacyMemory(f);
 expect(projected.record.id).toBe(f.memory.id);expect(projected.legacy!.metadata).toEqual(f.memory);expect(projected.legacy!.generationId).toBe(f.memory.generationId);expect(projected.claim).toEqual(f.memory.claim);expect(projected.record.status).toBe("candidate");expect(projected.record.authority).toBe("model-inference");expect(projected.record.sourceRefs).toEqual([f.source,f.sourceBindings[0]!.source]);expect("episodeId" in projected.record).toBe(false);
});
test("legacy exclusion/current generation/receipt and complete source binding failures remain effective",()=>{
 const excluded=fixture();excluded.policy.excludedFragmentIds=["fragment-old"];expect(()=>projectQualifiedLegacyMemory(excluded)).toThrow("QUALIFICATION_REQUIRED");
 const stale=fixture();stale.context.activeGenerationId="new-generation";expect(()=>projectQualifiedLegacyMemory(stale)).toThrow("QUALIFICATION_REQUIRED");
 const noReceipt=fixture();noReceipt.receipts=[];expect(()=>projectQualifiedLegacyMemory(noReceipt)).toThrow("QUALIFICATION_REQUIRED");
 const missing=fixture();missing.sourceBindings=[];expect(()=>projectQualifiedLegacyMemory(missing)).toThrow("EXACT_SOURCE_BINDING_REQUIRED");
 const tampered=fixture();tampered.sourceBindings[0]!.source.digest="a".repeat(64);expect(()=>projectQualifiedLegacyMemory(tampered)).toThrow("EXACT_SOURCE_BINDING_REQUIRED");
});
