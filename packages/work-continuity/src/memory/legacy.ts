import { qualifyMemory, type DerivedMemory, type Episode, type MemoryPolicy, type RecallContext, type DerivationReceipt } from "../workspace/qualification.ts";
import type { MemoryDocument, Scope, SourceRef } from "./generated/memory-v1.ts";
import { assertMemory } from "./codec.ts";

/** Versioned projection after the existing legacy semantic qualifier. This does
 * not authenticate a native reader or adopt legacy inference. Native discovery
 * and acquisition must still pass the current lease and original-source gate. */
export function projectQualifiedLegacyMemory(input: {
  memory: DerivedMemory; episode: Episode | undefined; policy: MemoryPolicy;
  context: RecallContext; receipts: readonly DerivationReceipt[];
  owner: Scope; readers: string[]; source: SourceRef; revision: number;
}): MemoryDocument {
  const {memory,episode,policy,context,receipts}=input;
  if(qualifyMemory(memory,episode,policy,context,receipts).status!=="eligible")throw Error("MEMORY_LEGACY_QUALIFICATION_REQUIRED");
  const document:MemoryDocument={schema:"common.memory-document.v1",record:{
    id:memory.id,revision:input.revision,owner:input.owner,subjects:[input.owner],kind:"knowledge",status:"candidate",authority:"model-inference",sourceRefs:[input.source],text:memory.text,allowedReaders:[...input.readers],
  },claim:{...memory.claim},legacy:{protocol:"workspace-architecture.derived-memory.v1",identityKey:memory.id,generationId:memory.generationId,claimKey:memory.claim.key,original:input.source,metadata:JSON.parse(JSON.stringify(memory)) as NonNullable<MemoryDocument["legacy"]>["metadata"]}};
  assertMemory("MemoryDocument",document);return document;
}
