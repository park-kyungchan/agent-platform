import { qualifyMemory, sourceKey, type DerivedMemory, type Episode, type MemoryPolicy, type RecallContext, type DerivationReceipt } from "../workspace/qualification.ts";
import type { MemoryDocument, Scope, SourceRef } from "./generated/memory-v1.ts";
import { assertMemory } from "./codec.ts";

/** Versioned projection after the existing legacy semantic qualifier. This does
 * not authenticate a native reader or adopt legacy inference. Native discovery
 * and acquisition must still pass the current lease and original-source gate. */
export function projectQualifiedLegacyMemory(input: {
  memory: DerivedMemory; episode: Episode | undefined; policy: MemoryPolicy;
  context: RecallContext; receipts: readonly DerivationReceipt[];
  owner: Scope; readers: string[]; source: SourceRef; revision: number;
  /** Native source owner maps every old Episode source to its exact immutable
   * reader address. This projection grants no access to any of those sources. */
  sourceBindings: readonly { legacyKey: string; source: SourceRef }[];
}): MemoryDocument {
  const {memory,episode,policy,context,receipts}=input;
  if(qualifyMemory(memory,episode,policy,context,receipts).status!=="eligible")throw Error("MEMORY_LEGACY_QUALIFICATION_REQUIRED");
  const originals=episode!.sources.map(row=>{
    const matches=input.sourceBindings.filter(binding=>binding.legacyKey===sourceKey(row.source));
    if(matches.length!==1||matches[0]!.source.digest!==row.source.sha256||matches[0]!.source.revision!==row.source.revision)throw Error("MEMORY_LEGACY_EXACT_SOURCE_BINDING_REQUIRED");
    return matches[0]!.source;
  });
  if(input.sourceBindings.length!==originals.length)throw Error("MEMORY_LEGACY_EXACT_SOURCE_BINDING_REQUIRED");
  const sourceRefs=[...new Map([input.source,...originals].map(source=>[JSON.stringify(source),source])).values()];
  const document:MemoryDocument={schema:"common.memory-document.v1",record:{
    id:memory.id,revision:input.revision,owner:input.owner,subjects:[input.owner],kind:"knowledge",status:"candidate",authority:"model-inference",sourceRefs:sourceRefs as MemoryDocument["record"]["sourceRefs"],text:memory.text,allowedReaders:[...input.readers],
  },claim:{...memory.claim},legacy:{protocol:"workspace-architecture.derived-memory.v1",identityKey:memory.id,generationId:memory.generationId,claimKey:memory.claim.key,original:input.source,metadata:JSON.parse(JSON.stringify(memory)) as NonNullable<MemoryDocument["legacy"]>["metadata"]}};
  assertMemory("MemoryDocument",document);return document;
}
