import { sha256 } from '../context/source';

/** Local reference contracts, not generated Ontology schemas or runtime adapters. */
export const qualificationBasis = {
  upperOntologyVersion: '0.2.0', localExtension: true,
  generatedProductionContractValidationClaimed: false,
  proofClass: 'finite-local-reference-contract', nativePermissionGranted: false,
} as const;

export interface PortableSource {
  repositoryId: string;
  revision: string;
  relativePath: string;
  sha256: string;
  byteLength: number;
}
/** Observations come from the existing source owner. This module performs no reads. */
export type SourceObservation = Pick<PortableSource, 'repositoryId' | 'revision' | 'relativePath'> & (
  | { status: 'available'; bytes: Uint8Array; canonicalPathVerified: boolean; access: 'original' | 'derived-only' }
  | { status: 'missing' | 'denied' | 'unreachable' }
);
export type SourceVerdict = 'verified' | 'unsafe' | 'denied' | 'missing' | 'unreachable' | 'ambiguous' | 'revision-mismatch' | 'hash-mismatch' | 'length-mismatch';
export const sourceKey = (source: PortableSource): string => JSON.stringify([source.repositoryId, source.revision, source.relativePath, source.sha256]);
const locatorKey = (source: Pick<PortableSource, 'repositoryId' | 'relativePath'>): string => JSON.stringify([source.repositoryId, source.relativePath]);
const hashValid = (hash: string): boolean => /^[a-f0-9]{64}$/.test(hash);
const sourceValid = (source: PortableSource): boolean => Boolean(source.repositoryId && source.revision)
  && hashValid(source.sha256) && Number.isSafeInteger(source.byteLength) && source.byteLength >= 0
  && !source.relativePath.includes('\\') && !/[\u0000-\u001f]/.test(source.relativePath) && !/^[A-Za-z]:/.test(source.relativePath)
  && source.relativePath.split('/').every(part => Boolean(part) && part !== '.' && part !== '..');

export function qualifySourceClosure(required: readonly PortableSource[], observations: readonly SourceObservation[], admittedRepositories: ReadonlySet<string>) {
  const checks = required.map(source => {
    const result = (status: SourceVerdict) => ({ key: sourceKey(source), source: { ...source }, status });
    if (!sourceValid(source)) return result('unsafe');
    if (!admittedRepositories.has(source.repositoryId)) return result('denied');
    const contradictory = required.some(other => locatorKey(other) === locatorKey(source)
      && other.revision === source.revision && (other.sha256 !== source.sha256 || other.byteLength !== source.byteLength));
    if (contradictory) return result('ambiguous');
    const located = observations.filter(row => locatorKey(row) === locatorKey(source));
    const rows = located.filter(row => row.revision === source.revision);
    if (rows.length !== 1) return result(rows.length ? 'ambiguous' : located.length ? 'revision-mismatch' : 'missing');
    const row = rows[0]!;
    if (row.revision !== source.revision) return result('revision-mismatch');
    if (row.status !== 'available') return result(row.status);
    if (row.access !== 'original') return result('denied');
    if (!row.canonicalPathVerified) return result('unsafe');
    if (sha256(row.bytes) !== source.sha256) return result('hash-mismatch');
    if (row.bytes.byteLength !== source.byteLength) return result('length-mismatch');
    return result('verified');
  });
  return {
    status: checks.every(check => check.status === 'verified') ? 'verified' as const : 'incomplete' as const,
    requiredCount: checks.length, verifiedCount: checks.filter(check => check.status === 'verified').length,
    checks, nativePermissionGranted: false as const,
  };
}

export interface SavedHandoff {
  issue: { id: string; source: PortableSource };
  spec: { id: string; status: 'adopted'; source: PortableSource };
  plan: { id: string; source: PortableSource };
  decisions: Array<{ id: string; status: 'accepted' | 'superseded' | 'proposed'; source: PortableSource }>;
  effects: Array<{ id: string; action: string; status: 'completed' | 'uncertain'; verification: 'passed' | 'failed' | 'unexecuted' }>;
  checks: Array<{ id: string; status: 'passed' | 'failed' | 'unexecuted'; evidence: PortableSource[] }>;
  pending: Array<{ id: string; action: string; dependsOn: string[] }>;
  nextAction: { taskId: string; action: string; effectId?: string; purpose: 'continue' | 'observe-uncertain-effect' } | null;
  originals: PortableSource[];
}
export function qualifyHandoff(saved: SavedHandoff, observations: readonly SourceObservation[], admittedRepositories: ReadonlySet<string>, environment: { receiverAvailable: boolean; sourceRuntimeAvailable: boolean; memoryAvailable: boolean }) {
  const required = [saved.issue.source, saved.spec.source, saved.plan.source, ...saved.decisions.map(row => row.source), ...saved.originals, ...saved.checks.flatMap(row => row.evidence)];
  const unique = [...new Map(required.map(source => [JSON.stringify(source), source])).values()];
  const closure = qualifySourceClosure(unique, observations, admittedRepositories);
  const problems: string[] = [];
  const ids = [...saved.effects, ...saved.pending].map(row => row.id);
  if (!saved.issue.id || !saved.spec.id || !saved.plan.id || saved.spec.status !== 'adopted') problems.push('Issue, adopted Spec and Plan identity are required.');
  if (new Set(ids).size !== ids.length) problems.push('Effect/task identities are ambiguous or a completed effect is queued for replay.');
  if (new Set(saved.checks.map(row => row.id)).size !== saved.checks.length || new Set(saved.decisions.map(row => row.id)).size !== saved.decisions.length) problems.push('Check/decision identities are ambiguous.');
  if (saved.checks.some(row => row.status === 'passed' && !row.evidence.length)) problems.push('Passed checks require pinned evidence.');
  if (saved.pending.some(row => row.dependsOn.some(id => !ids.includes(id)))) problems.push('Pending dependency is absent from saved work.');
  const visiting = new Set<string>(), visited = new Set<string>();
  const cycle = (id: string): boolean => {
    if (visiting.has(id)) return true;
    if (visited.has(id)) return false;
    visiting.add(id);
    if (saved.pending.find(row => row.id === id)?.dependsOn.some(cycle)) return true;
    visiting.delete(id); visited.add(id); return false;
  };
  if (saved.pending.some(row => cycle(row.id))) problems.push('Saved task dependencies are cyclic.');
  const next = saved.nextAction;
  const completedIds = new Set(saved.effects.filter(row => row.status === 'completed').map(row => row.id));
  if (next) {
    const task = saved.pending.find(row => row.id === next.taskId);
    if (!task) problems.push('Next action must identify saved pending work.');
    if (completedIds.has(next.taskId) || next.effectId && completedIds.has(next.effectId)) problems.push('Completed effects cannot be replayed.');
    if (next.effectId && !saved.effects.some(row => row.id === next.effectId)) problems.push('Next action effect is unknown.');
    if (next.effectId && saved.effects.some(row => row.id === next.effectId && row.status === 'uncertain') && next.purpose !== 'observe-uncertain-effect') problems.push('Observe uncertain effects before attempting execution.');
    if (task?.dependsOn.some(id => !completedIds.has(id))) problems.push('Next action has unresolved dependencies.');
  }
  if (!environment.receiverAvailable) problems.push('Receiving development environment is unavailable.');
  if (closure.status !== 'verified') problems.push('Required original/source closure is incomplete.');
  const disclosures = [
    ...(!environment.sourceRuntimeAvailable ? ['Source runtime unavailable; only already observed/shared original bytes support this continuation.'] : []),
    ...(!environment.memoryAvailable ? ['Memory unavailable; Recall/Reflect and fresh extraction are unavailable. Continue from verified required originals only.'] : []),
    'Local source-state qualification grants no native permission, model budget, implementation acceptance or runtime readiness.',
  ];
  return {
    status: problems.length ? 'blocked' as const : 'source-continuation-qualified' as const,
    closure, problems, disclosures, savedState: structuredClone(saved),
    nextAction: problems.length ? null : structuredClone(saved.nextAction),
    completedEffectIds: [...completedIds], nativePermissionGranted: false as const, executionReady: false as const,
  };
}

export interface Episode {
  id: string;
  provenance: 'source-linked' | 'unknown';
  sources: Array<{ source: PortableSource; fragmentIds: string[] }>;
}
export interface DerivedMemory {
  id: string;
  episodeId: string;
  generationId: string;
  policyRevision: string;
  text: string;
  contentHash: string;
  sourceKeys: string[];
  scope: { kind: 'repository'; repositoryId: string } | { kind: 'common' };
  claim: { key: string; value: string };
}
export interface MemoryPolicy {
  revision: string;
  excludedSourceHashes: string[];
  excludedFragmentIds: string[];
  aliases: Array<{ from: string; to: string }>;
  commonAdmissions: Array<{ memoryId: string; episodeId: string; contentHash: string; claim: DerivedMemory['claim']; applicability: string; status: 'accepted' | 'proposed' | 'withdrawn' }>;
}
/** Trusted source-owner receipt is a reference input, not an implemented attestation. */
export interface DerivationReceipt {
  memoryId: string; episodeId: string; generationId: string; policyRevision: string;
  contentHash: string; sourceKeys: string[]; provenance: 'owner-verified' | 'unknown';
  claim: DerivedMemory['claim']; scope: DerivedMemory['scope']; episodeFingerprint: string;
}
export const episodeFingerprint = (episode: Episode): string => sha256(JSON.stringify({
  id: episode.id, provenance: episode.provenance,
  sources: episode.sources.map(row => ({ key: sourceKey(row.source), byteLength: row.source.byteLength, fragmentIds: [...row.fragmentIds].sort() })).sort((a, b) => a.key < b.key ? -1 : a.key > b.key ? 1 : 0),
}));
export interface RecallContext {
  repositoryId: string;
  explicitlySharedRepositoryIds: ReadonlySet<string>;
  commonEnabled: boolean;
  applicability: string;
  currentPolicyRevision: string;
  policyHead: 'current' | 'unknown' | 'publishing';
  activeGenerationId: string;
  readableMemoryIds: ReadonlySet<string>;
}
function canonicalFragments(policy: MemoryPolicy): { canonical: (id: string) => string | null; valid: boolean } {
  const aliases = new Map<string, string>();
  let valid = true;
  for (const alias of policy.aliases) {
    if (!alias.from || !alias.to || aliases.has(alias.from) && aliases.get(alias.from) !== alias.to) valid = false;
    aliases.set(alias.from, alias.to);
  }
  const canonical = (id: string): string | null => {
    const seen = new Set<string>();
    while (aliases.has(id)) {
      if (seen.has(id)) return null;
      seen.add(id); id = aliases.get(id)!;
    }
    return id || null;
  };
  if ([...aliases.keys()].some(id => canonical(id) === null)) valid = false;
  return { canonical, valid };
}
/** Policy/lineage are explicit inputs, not remote authorization or model attestation. */
export function qualifyMemory(memory: DerivedMemory, episode: Episode | undefined, policy: MemoryPolicy, context: RecallContext, receipts: readonly DerivationReceipt[]) {
  const deny = (reason: string) => ({ status: 'withheld' as const, reason, memoryId: memory.id });
  if (!policy.revision || context.policyHead !== 'current' || policy.revision !== context.currentPolicyRevision || memory.policyRevision !== policy.revision) return deny('Current stable policy is absent or the memory is stale.');
  if (!context.activeGenerationId || memory.generationId !== context.activeGenerationId) return deny('Memory generation is not the active generation.');
  if (!episode || !episode.id || episode.id !== memory.episodeId || episode.provenance !== 'source-linked' || !episode.sources.length) return deny('Meaningful Episode/source lineage is incomplete.');
  if (!memory.id || !memory.claim.key || !memory.claim.value || !hashValid(memory.contentHash) || sha256(memory.text) !== memory.contentHash) return deny('Memory identity/content binding is invalid.');
  if (!context.readableMemoryIds.has(memory.id)) return deny('Derived content access is not granted.');
  const episodeKeys = episode.sources.map(row => sourceKey(row.source));
  if (!memory.sourceKeys.length || new Set(memory.sourceKeys).size !== memory.sourceKeys.length || memory.sourceKeys.some(key => !episodeKeys.includes(key))) return deny('Memory source references do not bind to the Episode.');
  // Require full Episode lineage: dropping a denied/excluded source cannot launder a derived summary.
  if (episodeKeys.some(key => !memory.sourceKeys.includes(key))) return deny('Memory omitted part of its Episode source lineage.');
  const receiptsForMemory = receipts.filter(row => row.memoryId === memory.id);
  const receipt = receiptsForMemory[0];
  if (receiptsForMemory.length !== 1 || !receipt || receipt.provenance !== 'owner-verified' || receipt.episodeId !== memory.episodeId
    || receipt.generationId !== memory.generationId || receipt.policyRevision !== memory.policyRevision || receipt.contentHash !== memory.contentHash
    || receipt.claim.key !== memory.claim.key || receipt.claim.value !== memory.claim.value || JSON.stringify(receipt.scope) !== JSON.stringify(memory.scope)
    || receipt.episodeFingerprint !== episodeFingerprint(episode)
    || receipt.sourceKeys.length !== memory.sourceKeys.length || ![...receipt.sourceKeys].sort().every((key, index) => key === [...memory.sourceKeys].sort()[index])) return deny('Source-owner derivation receipt is absent, ambiguous or mismatched.');
  const { canonical, valid } = canonicalFragments(policy);
  if (!valid) return deny('Fragment alias policy is ambiguous or cyclic.');
  const excluded = new Set(policy.excludedFragmentIds.map(canonical));
  for (const row of episode.sources) {
    if (!sourceValid(row.source) || !row.fragmentIds.length || row.fragmentIds.some(id => canonical(id) === null)) return deny('Stable source fragment lineage is incomplete.');
    if (policy.excludedSourceHashes.includes(row.source.sha256) || row.fragmentIds.some(id => excluded.has(canonical(id)))) return deny('Memory depends on persistently excluded source content.');
  }
  if (memory.scope.kind === 'common') {
    const admissions = policy.commonAdmissions.filter(row => row.memoryId === memory.id && row.episodeId === episode.id && row.contentHash === memory.contentHash && row.applicability === context.applicability);
    if (!context.commonEnabled || admissions.length !== 1 || admissions[0]!.status !== 'accepted'
      || admissions[0]!.claim.key !== memory.claim.key || admissions[0]!.claim.value !== memory.claim.value) return deny('Common content lacks unambiguous actual current applicable admission.');
  } else {
    const repositoryId = memory.scope.repositoryId;
    if (episode.sources.some(row => row.source.repositoryId !== repositoryId)) return deny('Repository memory ownership differs from its Episode source.');
    if (repositoryId !== context.repositoryId && !context.explicitlySharedRepositoryIds.has(repositoryId)) return deny('Other repository scope is not explicitly shared.');
  }
  return { status: 'eligible' as const, memoryId: memory.id, claimClass: 'unconfirmed-source-attributed-inference' as const, originalAccessGranted: false as const };
}

export interface AuthoritativeClaim {
  key: string;
  value: string;
  recordClass: 'decision' | 'spec';
  status: 'adopted-current' | 'proposed' | 'superseded';
  source: PortableSource;
}
/** Supplied by the source record owner; authenticity and semantic extraction are not implemented here. */
export interface AuthoritativeClaimReceipt {
  sourceKey: string; recordClass: AuthoritativeClaim['recordClass']; status: AuthoritativeClaim['status'];
  key: string; value: string; provenance: 'owner-verified' | 'unknown';
}
export function qualifyRecall(input: {
  memories: DerivedMemory[]; episodes: Episode[]; policy: MemoryPolicy; context: RecallContext;
  observations: SourceObservation[]; originalRepositoryGrants: ReadonlySet<string>;
  receipts: DerivationReceipt[];
  authoritativeClaims: AuthoritativeClaim[]; authoritativeReceipts: AuthoritativeClaimReceipt[]; claimKey: string;
}) {
  const evaluated = input.memories.map(memory => ({ memory, result: qualifyMemory(memory, input.episodes.find(episode => episode.id === memory.episodeId), input.policy, input.context, input.receipts) }));
  const eligible = evaluated.filter(row => row.result.status === 'eligible' && row.memory.claim.key === input.claimKey).map(row => ({ ...structuredClone(row.memory), claimClass: 'unconfirmed-source-attributed-inference' as const }));
  const authoritativeEvaluation = input.authoritativeClaims.filter(row => row.key === input.claimKey).map(claim => {
    const receipts = input.authoritativeReceipts.filter(receipt => receipt.sourceKey === sourceKey(claim.source) && receipt.key === claim.key);
    const receipt = receipts[0];
    let reason: string | null = null;
    if (claim.status !== 'adopted-current') reason = 'Record is proposed or superseded.';
    else if (receipts.length !== 1 || !receipt || receipt.provenance !== 'owner-verified' || receipt.value !== claim.value || receipt.recordClass !== claim.recordClass || receipt.status !== claim.status) reason = 'Current authoritative claim lacks an unambiguous matching source-owner receipt.';
    else if (claim.source.repositoryId !== input.context.repositoryId && !input.context.explicitlySharedRepositoryIds.has(claim.source.repositoryId)) reason = 'Authoritative record repository is not admitted to this Recall scope.';
    else if (qualifySourceClosure([claim.source], input.observations, input.originalRepositoryGrants).status !== 'verified') reason = 'Authoritative original/source closure is incomplete.';
    return { claim, reason };
  });
  const authoritative = authoritativeEvaluation.filter(row => row.reason === null).map(row => row.claim);
  const values = new Set([...eligible.map(row => row.claim.value), ...authoritative.map(row => row.value)]);
  const authoritativeValues = new Set(authoritative.map(row => row.value));
  return {
    eligible, withheld: evaluated.filter(row => row.result.status === 'withheld').map(row => row.result), authoritative: structuredClone(authoritative),
    withheldAuthoritative: structuredClone(authoritativeEvaluation.filter(row => row.reason !== null)),
    conflict: values.size > 1, authoritativeConflict: authoritativeValues.size > 1,
    selectedValue: authoritativeValues.size === 1 ? authoritative[0]!.value : null,
    selectionClass: authoritativeValues.size === 1 ? 'current-authoritative-record' as const : 'unresolved' as const,
    influence: [...eligible.map(row => ({ memoryId: row.id, episodeId: row.episodeId, sourceKeys: row.sourceKeys, status: row.claimClass })), ...authoritative.map(row => ({ sourceKeys: [sourceKey(row.source)], status: 'current-authoritative-record' as const }))],
    nativePermissionGranted: false as const,
  };
}

export interface CoordinatorGeneration { issueId: string; agentId: string; epoch: number; checkpointSha256: string }
/** A local equality check only. No lease acquisition, writer fencing or dispatch occurs. */
export function qualifyCoordinator(current: CoordinatorGeneration, candidate: CoordinatorGeneration) {
  const valid = (row: CoordinatorGeneration) => Boolean(row.issueId && row.agentId) && Number.isSafeInteger(row.epoch) && row.epoch > 0 && hashValid(row.checkpointSha256);
  return { status: valid(current) && valid(candidate) && current.issueId === candidate.issueId && current.agentId === candidate.agentId && current.epoch === candidate.epoch && current.checkpointSha256 === candidate.checkpointSha256 ? 'reference-match' as const : 'stale-or-invalid' as const,
    distributedFencingVerified: false as const, nativePermissionGranted: false as const };
}
