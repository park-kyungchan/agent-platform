import {dirname, isAbsolute, join, resolve} from 'node:path';
import {isDeepStrictEqual as same} from 'node:util';
import {pathToFileURL} from 'node:url';
import {projectSemanticContext, semanticContextRequest, type SemanticSourceSnapshot} from '../adoption/semantic-context';
import {adaptAuditSession, type LinkAssessment} from './audit-adapter';
import {readCurrentIntent} from './current-intent';
import {bindDomainContext} from './domain-context';
import {profileRoleRegistry, selectProfileRole} from './role-registry';
import {SEMANTIC_OWNER, UPPER_ONTOLOGY, exactKeys, hash, identity, object, parsedJson, pin, requireValue, strings, text, timestamp,
  type EffectScope, type Pin, type ReadPinned, type WorkBinding, type NativeObservation} from './contracts';

export const REVIEWED_PACK_MANIFEST = '00b25d58951272a61894da4dac4c4a773b6055b4238cce7d88109db536fe2a9b';
export interface WorkContextRequest {
  schema: 'hermes.work-context.request.v1'; programId: string; intent: Pin; binding: Pin;
  pack: {manifest: Pin; session: Pin; links: LinkAssessment[]};
  nativeSources: {role: Pin; context: Pin}; nativeObservation?: Pin;
  auditRecordIds?: string[]; domainAtomIds?: string[]; roleRegistry?: Pin; atUtc: string; maxChars?: number;
}
export function nativeObservation(input: unknown): NativeObservation {
  object(input, 'native observation'); exactKeys(input, ['schema','binding','actor','workId','observedAt','validUntil','evidence','owner','capabilities','deniedCapabilities','effectScope'], 'native observation');
  requireValue(input.schema === 'hermes.native-observation.v1', 'NATIVE_OBSERVATION_SCHEMA', 'producer');
  pin(input.binding); pin(input.evidence); identity(input.workId, 'workId'); timestamp(input.observedAt, 'observedAt'); timestamp(input.validUntil, 'validUntil');
  requireValue(Date.parse(input.validUntil) > Date.parse(input.observedAt), 'EMPTY_NATIVE_WINDOW', input.workId);
  object(input.actor, 'actor'); exactKeys(input.actor, ['runtime','profile'], 'actor'); identity(input.actor.profile, 'profile');
  object(input.owner, 'owner'); requireValue(input.owner.runtime === input.actor.runtime, 'OWNER_RUNTIME_CONFLICT', input.workId);
  if (input.owner.runtime === 'native-hermes') {
    exactKeys(input.owner, ['runtime','containerId','hostPid','localPid','startTicks','pidNamespace','sourceCommit'], 'Hermes owner');
    requireValue(typeof input.owner.containerId === 'string' && /^[a-f0-9]{64}$/.test(input.owner.containerId)
      && typeof input.owner.sourceCommit === 'string' && /^[a-f0-9]{40}$/.test(input.owner.sourceCommit)
      && typeof input.owner.pidNamespace === 'string' && /^pid:\[\d+\]$/.test(input.owner.pidNamespace)
      && ['hostPid','localPid','startTicks'].every(k => Number.isSafeInteger(input.owner[k]) && input.owner[k] > 0), 'INVALID_NATIVE_OWNER', input.workId);
  } else {requireValue(input.owner.runtime === 'native-codex', 'NATIVE_RUNTIME_REQUIRED', input.workId); exactKeys(input.owner, ['runtime','threadId'], 'Codex owner'); identity(input.owner.threadId, 'threadId');}
  strings(input.deniedCapabilities, 'deniedCapabilities');
  requireValue(Array.isArray(input.capabilities) && Array.isArray(input.effectScope) && input.capabilities.length <= 128 && input.effectScope.length <= 128, 'INVALID_NATIVE_SCOPE', input.workId);
  const seen = new Set<string>();
  for (const claim of input.capabilities) {
    object(claim, 'capability'); exactKeys(claim, ['name','stage','evidence','observedAt','validUntil'], 'capability'); identity(claim.name, 'capability'); pin(claim.evidence);
    timestamp(claim.observedAt, 'capability observedAt'); timestamp(claim.validUntil, 'capability validUntil');
    requireValue(['configured','exposed','authenticated','behaviorallyVerified'].includes(claim.stage) && Date.parse(claim.validUntil) > Date.parse(claim.observedAt)
      && !seen.has(claim.name + ':' + claim.stage), 'INVALID_CAPABILITY', claim.name); seen.add(claim.name + ':' + claim.stage);
  }
  for (const effect of input.effectScope) {object(effect, 'effect'); exactKeys(effect, ['kind','target'], 'effect'); identity(effect.kind, 'effect kind'); text(effect.target, 'effect target');}
  return structuredClone(input) as NativeObservation;
}
export function workBinding(input: unknown): WorkBinding {
  object(input, 'work binding'); exactKeys(input, ['schema','id','revision','programId','actor','work','intent','intentId','intentRevision','statementIds','repositories','acceptingOwner','domainContext'], 'work binding');
  requireValue(input.schema === 'hermes.work-binding.v1', 'WORK_BINDING_SCHEMA', 'Unknown producer schema.');
  for (const key of ['id','programId','intentId','acceptingOwner']) identity(input[key], key);
  for (const key of ['revision','intentRevision']) requireValue(Number.isSafeInteger(input[key]) && input[key] > 0, 'INVALID_REVISION', key);
  pin(input.intent); strings(input.statementIds, 'statementIds');
  object(input.actor, 'actor'); exactKeys(input.actor, ['runtime','profile'], 'actor');
  requireValue(['native-hermes','native-codex'].includes(input.actor.runtime), 'NATIVE_RUNTIME_REQUIRED', 'actor'); identity(input.actor.profile, 'profile');
  object(input.work, 'work'); exactKeys(input.work, ['id','owner','domainOwner'], 'work');
  for (const key of ['id','owner','domainOwner']) identity(input.work[key], key);
  requireValue(Array.isArray(input.repositories) && input.repositories.length <= 128, 'INVALID_LIST', 'repositories');
  const repositories = new Set<string>();
  for (const repository of input.repositories) {
    object(repository, 'repository'); exactKeys(repository, ['repositoryId','root','version','evidence','evidenceScope'], 'repository');
    identity(repository.repositoryId, 'repositoryId'); text(repository.root, 'root'); pin(repository.evidence);
    requireValue(isAbsolute(repository.root) && resolve(repository.root) === repository.root, 'INVALID_REPOSITORY_BINDING', repository.repositoryId);
    requireValue(['metadata-and-selected-files','complete-source-tree'].includes(repository.evidenceScope), 'REPOSITORY_EVIDENCE_SCOPE', repository.repositoryId);
    object(repository.version, 'repository version'); const v = repository.version;
    if (v.kind === 'git-checkout') {
      exactKeys(v, ['kind','commit','dirtyState','dirtySnapshot'], 'git version');
      requireValue(typeof v.commit === 'string' && /^[a-f0-9]{40}$/.test(v.commit) && ['clean','dirty'].includes(v.dirtyState), 'INVALID_REPOSITORY_BINDING', repository.repositoryId);
      if (v.dirtyState === 'dirty') pin(v.dirtySnapshot);
      else requireValue(v.dirtySnapshot === undefined, 'UNEXPECTED_DIRTY_SNAPSHOT', repository.repositoryId);
    } else {
      exactKeys(v, ['kind','sourceRevision','archiveManifest'], 'archive version');
      requireValue(v.kind === 'source-archive' && (v.sourceRevision === null || typeof v.sourceRevision === 'string' && /^[a-f0-9]{40}$/.test(v.sourceRevision)), 'INVALID_ARCHIVE_BINDING', repository.repositoryId);
      pin(v.archiveManifest);
    }
    requireValue(!repositories.has(repository.repositoryId), 'DUPLICATE_REPOSITORY_BINDING', repository.repositoryId); repositories.add(repository.repositoryId);
  }
  if (input.domainContext !== undefined) {
    object(input.domainContext, 'domain context'); const d = input.domainContext;
    exactKeys(d, ['repositoryId','projection','expectedRevision','workUnit','workState','relation','sourcePins','liveSources'], 'domain context');
    identity(d.repositoryId, 'domain repository'); identity(d.workUnit, 'domain Work'); pin(d.projection);
    requireValue(repositories.has(d.repositoryId) && typeof d.expectedRevision === 'string' && /^[a-f0-9]{64}$/.test(d.expectedRevision)
      && ['in_progress','complete'].includes(d.workState) && ['current-work','reference'].includes(d.relation), 'INVALID_DOMAIN_CONTEXT_BINDING', d.repositoryId);
    requireValue(Array.isArray(d.sourcePins) && d.sourcePins.length > 0 && new Set(d.sourcePins.map((p: any) => p.id)).size === d.sourcePins.length, 'DOMAIN_SOURCE_DENOMINATOR', d.repositoryId);
    for (const source of d.sourcePins) {object(source, 'domain source'); exactKeys(source, ['id','path','sha256'], 'domain source'); identity(source.id, 'domain source ID'); pin({path: source.path, sha256: source.sha256});}
    if (d.liveSources !== undefined) {
      requireValue(Array.isArray(d.liveSources) && d.liveSources.length === d.sourcePins.length
        && new Set(d.liveSources.map((p: any) => p.id)).size === d.liveSources.length
        && new Set(d.liveSources.map((p: any) => p.path)).size === d.liveSources.length, 'DOMAIN_LIVE_SOURCE_DENOMINATOR', d.repositoryId);
      for (const source of d.liveSources) {
        object(source, 'live domain source'); exactKeys(source, ['id','path','sha256'], 'live domain source'); identity(source.id, 'live domain source ID'); pin({path: source.path, sha256: source.sha256});
        requireValue(isAbsolute(source.path) && resolve(source.path) === source.path
          && d.sourcePins.some((p: any) => p.id === source.id && p.sha256 === source.sha256), 'DOMAIN_LIVE_SOURCE_CONFLICT', source.id);
      }
    }
  }
  return structuredClone(input) as WorkBinding;
}

/** Executes only the exact immutable local pack's reviewed validators. All its
 * distributed bytes are checked first; no target application imports occur. */
async function readPack(request: WorkContextRequest['pack'], read: ReadPinned) {
  requireValue(request.manifest.sha256 === REVIEWED_PACK_MANIFEST, 'UNREVIEWED_PACK', 'A new pack version needs an explicit reviewed adapter revision.');
  const manifest: any = parsedJson(await read(request.manifest), request.manifest), root = dirname(read.resolvePath?.(request.manifest.path) ?? resolve(request.manifest.path));
  const files: Pin[] = manifest.files.map((f: any) => {
    requireValue(typeof f.path === 'string' && !isAbsolute(f.path) && !f.path.split('/').includes('..'), 'INVALID_PACK_PATH', 'manifest');
    return {path: join(root, f.path), sha256: f.sha256};
  });
  for (const file of files) requireValue(hash(await read(file)) === file.sha256, 'SOURCE_PIN_CHANGED', file.path);
  const original = await import(pathToFileURL(join(root, 'tooling/validate.ts')).href);
  const {pack} = original.validatePack(root);
  const session = parsedJson(await read(request.session), request.session);
  original.assertDocument(session); original.assertSession(session, pack);
  const projection = adaptAuditSession(session, pack.context, request.links);
  // Detect ordinary concurrent edits during validation; this is not an OS sandbox.
  for (const file of files) requireValue(hash(await read(file)) === file.sha256, 'SOURCE_PIN_CHANGED', file.path);
  return projection;
}

/** Actual consumer: read exact current intent, audit and native context inputs,
 * select this Work's records, then feed the existing native context projector. */
export async function readWorkContext(request: WorkContextRequest, read: ReadPinned) {
  object(request, 'context request'); exactKeys(request, ['schema','programId','intent','binding','pack','nativeSources','nativeObservation','auditRecordIds','domainAtomIds','roleRegistry','atUtc','maxChars'], 'context request');
  requireValue(request.schema === 'hermes.work-context.request.v1', 'WORK_CONTEXT_SCHEMA', 'request'); timestamp(request.atUtc, 'atUtc');
  const intent = await readCurrentIntent(request.intent, read), binding = workBinding(parsedJson(await read(request.binding), request.binding));
  requireValue(intent.programId === request.programId && binding.programId === request.programId && intent.id === binding.intentId
    && intent.revision === binding.intentRevision && binding.intent.path === request.intent.path && binding.intent.sha256 === request.intent.sha256, 'STALE_INTENT_BINDING', binding.id);
  const statements = binding.statementIds.map(id => intent.statements.find(s => s.id === id));
  requireValue(statements.length > 0 && statements.every(s => s?.status === 'CURRENT'
    && (!s.scope.profiles.length || s.scope.profiles.includes(binding.actor.profile) || s.scope.profiles.includes(binding.work.owner))), 'UNBOUND_CURRENT_INTENT', binding.id);
  for (const repository of binding.repositories) {
    const owner = intent.repositoryResponsibilities.find(r => r.repositoryId === repository.repositoryId);
    requireValue(owner?.ownerProfile === binding.work.owner && owner.domainOwner === binding.work.domainOwner && binding.statementIds.includes(owner.statementId), 'REPOSITORY_WORK_OWNER_CONFLICT', repository.repositoryId);
  }
  const observedPins = binding.repositories.flatMap(r => [r.evidence, ...(r.version.kind === 'source-archive' ? [r.version.archiveManifest] : r.version.dirtySnapshot ? [r.version.dirtySnapshot] : [])]);
  for (const evidence of observedPins) requireValue(hash(await read(evidence)) === evidence.sha256, 'SOURCE_PIN_CHANGED', evidence.path);
  const fullAudit = await readPack(request.pack, read), snapshots: SemanticSourceSnapshot[] = [];
  const selectedIds = request.auditRecordIds ?? []; strings(selectedIds, 'audit record selection');
  const knownIds = new Set([...fullAudit.evidence, ...fullAudit.links, ...fullAudit.roles].map((r: any) => r.id));
  requireValue(selectedIds.every(id => knownIds.has(id)), 'UNKNOWN_AUDIT_SELECTION', 'auditRecordIds');
  const audit = {...fullAudit, evidence: fullAudit.evidence.filter(e => selectedIds.includes(e.id)), links: fullAudit.links.filter((l: any) => selectedIds.includes(l.id)), roles: fullAudit.roles.filter((r: any) => selectedIds.includes(r.id))};
  const domain = binding.domainContext ? bindDomainContext(parsedJson(await read(binding.domainContext.projection), binding.domainContext.projection), binding.domainContext, binding) : null;
  let omittedDomainAtomIds: string[] = [];
  if (request.domainAtomIds !== undefined) {
    strings(request.domainAtomIds, 'domain atom selection'); requireValue(domain, 'MISSING_DOMAIN_CONTEXT', 'domainAtomIds');
    const selected = new Set(request.domainAtomIds);
    requireValue([...selected].every(id => domain.atoms.some((a: any) => a.id === id)), 'UNKNOWN_DOMAIN_SELECTION', 'domainAtomIds');
    // Keep the owner's Work, selector and effect boundary visible in every view.
    for (const atom of domain.atoms) if (['work_intent','selection_logic','effect_boundary'].includes(atom.kind)) selected.add(atom.id);
    omittedDomainAtomIds = domain.atoms.filter((a: any) => !selected.has(a.id)).map((a: any) => a.id);
    domain.atoms = domain.atoms.filter((a: any) => selected.has(a.id));
    domain.relations = domain.relations.filter((r: any) => selected.has(r.from) && selected.has(r.to));
    domain.dependencyOrder = domain.dependencyOrder.filter(id => selected.has(id));
  }
  let native: {status: 'not-observed' | 'fresh' | 'stale' | 'binding-mismatch' | 'unavailable'; observation: NativeObservation | null; errorCode?: string} = {status: 'not-observed', observation: null};
  if (request.nativeObservation) try {
    const observation = nativeObservation(parsedJson(await read(request.nativeObservation), request.nativeObservation));
    for (const p of [observation.evidence, ...observation.capabilities.map(c => c.evidence)]) requireValue(hash(await read(p)) === p.sha256, 'SOURCE_PIN_CHANGED', p.path);
    const bound = same(observation.binding, request.binding) && same(observation.actor, binding.actor) && observation.workId === binding.work.id;
    native = {status: !bound ? 'binding-mismatch' : Date.parse(observation.observedAt) <= Date.parse(request.atUtc) && Date.parse(request.atUtc) < Date.parse(observation.validUntil) ? 'fresh' : 'stale', observation};
  } catch (error) {native = {status: 'unavailable', observation: null, errorCode: error instanceof Error && 'code' in error ? String(error.code) : 'NATIVE_OBSERVATION_UNAVAILABLE'};}
  for (const name of ['role','context'] as const) {
    const source = request.nativeSources[name]; snapshots.push({name, path: source.path, sha256: source.sha256, value: parsedJson(await read(source), source), unchanged: true});
  }
  const query = semanticContextRequest({schema: 'backend.common-context.request.v1', actor: binding.actor, work: binding.work,
    needs: ['purpose','definitions','dependencies','authority','evidence'], expectedSourcePins: Object.fromEntries(snapshots.map(s => [s.name, s.sha256])), maxChars: 12000});
  const common = projectSemanticContext(query, snapshots);
  let selectedRole: ReturnType<typeof selectProfileRole> | null = null;
  if (request.roleRegistry) {
    const registry = profileRoleRegistry(parsedJson(await read(request.roleRegistry), request.roleRegistry));
    requireValue(same(registry.intent, request.intent), 'STALE_ROLE_INTENT_BINDING', registry.revision);
    selectedRole = selectProfileRole(registry, binding.actor.profile);
  }
  const liveChecks: Array<{id: string; path: string; sha256: string; status: 'matched' | 'changed' | 'unavailable'; reasonCode?: string}> = [];
  for (const source of binding.domainContext?.liveSources ?? []) {
    try {
      const raw = await read({path: source.path, sha256: source.sha256});
      liveChecks.push({...source, status: hash(raw) === source.sha256 ? 'matched' : 'changed'});
    } catch (error) {
      const reasonCode = error instanceof Error && 'code' in error ? String(error.code) : 'SOURCE_UNAVAILABLE';
      liveChecks.push({...source, status: reasonCode === 'SOURCE_PIN_CHANGED' ? 'changed' : 'unavailable', reasonCode});
    }
  }
  const domainCurrentness = {status: !domain ? 'not-applicable' as const : !liveChecks.length ? 'unverified' as const
    : liveChecks.some(p => p.status === 'changed') ? 'stale' as const : liveChecks.some(p => p.status === 'unavailable') ? 'unverified' as const : 'current' as const,
    capturedRelation: binding.domainContext?.relation ?? null,
    meaning: 'Source bytes checked for this read only; copied projections are references until all declared live owner inputs match. Refresh through the existing domain workflow:context, not a new interview.'};
  if (domain && domainCurrentness.status !== 'current') domain.relation = 'reference';
  const value = {schema: 'hermes.work-context.projection.v1' as const, programId: request.programId, semanticOwner: SEMANTIC_OWNER,
    upperOntology: UPPER_ONTOLOGY, actor: binding.actor, work: binding.work, intent: {id: intent.id, revision: intent.revision, pin: request.intent, statements},
    repositories: binding.repositories, selectedRole, common: {records: common.records, status: common.status}, audit, domain, domainCurrentness,
    authority: 'none' as const, executionReady: false as const, nativePermissionGranted: false as const};
  const maxChars = request.maxChars ?? 24000, requiredChars = [...JSON.stringify(value)].length;
  requireValue(Number.isSafeInteger(maxChars) && maxChars > 0 && maxChars <= 64000, 'INVALID_CONTEXT_BUDGET', 'maxChars');
  // Re-read all roots after resolving dependencies; never emit partially stale context.
  for (const p of [request.intent, request.binding, request.pack.session, ...Object.values(request.nativeSources), ...(binding.domainContext ? [binding.domainContext.projection] : []), ...(request.roleRegistry ? [request.roleRegistry] : [])]) requireValue(hash(await read(p)) === p.sha256, 'SOURCE_PIN_CHANGED', p.path);
  const ready = common.status === 'CONTEXT_READY' && requiredChars <= maxChars;
  return {status: common.status !== 'CONTEXT_READY' ? common.status : ready ? 'CONTEXT_READY' : 'CONTEXT_BUDGET_EXCEEDED',
    requiredChars, maxChars, value: ready ? value : null, binding, bindingPin: request.binding, native,
    provenance: {nativeCommonSources: common.sources, omittedDomainAtomIds, domainLiveSourceChecks: liveChecks, sourcePins: [request.intent, request.binding, request.pack.manifest, request.pack.session, ...Object.values(request.nativeSources), ...observedPins,
      ...(binding.domainContext ? [binding.domainContext.projection, ...(binding.domainContext.liveSources ?? []).map(({path, sha256}) => ({path, sha256}))] : []), ...(request.nativeObservation ? [request.nativeObservation] : []), ...(request.roleRegistry ? [request.roleRegistry] : [])],
      meaning: 'Durable intent/source context is distinct from ephemeral native admission. Pinned snapshots do not authenticate callers or grant permission.'}, acceptanceDecision: null};
}
export type WorkContext = Awaited<ReturnType<typeof readWorkContext>>;

/** Context precondition only. Native approvals, concrete tool/effect checks and
 * model reservations remain mandatory in their existing owners. */
export function checkActionContext(context: WorkContext, action: {workId: string; profile: string; intentSha256: string; bindingSha256: string;
  sourcePins: Pin[]; repositoryIds: string[]; effects: EffectScope[]; capability?: {name: string; stage: 'exposed' | 'authenticated' | 'behaviorallyVerified'}}, atUtc: string) {
  const reasons: string[] = [], value = context.value; timestamp(atUtc, 'atUtc');
  if (context.status !== 'CONTEXT_READY' || !value) reasons.push('CONTEXT_NOT_READY');
  else {
    const b = context.binding, n = context.native.observation;
    if (action.workId !== b.work.id || action.profile !== b.actor.profile) reasons.push('ACTOR_WORK_MISMATCH');
    if (action.intentSha256 !== value.intent.pin.sha256 || action.bindingSha256 !== context.bindingPin.sha256) reasons.push('STALE_ACTION_CONTEXT');
    const selectedPins = (pins: Pin[]) => pins.map(p => p.path + '\0' + p.sha256).sort();
    if (!Array.isArray(action.sourcePins) || !same(selectedPins(action.sourcePins), selectedPins(context.provenance.sourcePins))) reasons.push('ACTION_SOURCE_PINS_CHANGED');
    if (context.native.status !== 'fresh' || !n || Date.parse(atUtc) < Date.parse(n.observedAt) || Date.parse(atUtc) >= Date.parse(n.validUntil)) reasons.push('FRESH_NATIVE_ADMISSION_REQUIRED');
    if (b.domainContext?.relation === 'current-work' && value.domainCurrentness.status !== 'current') reasons.push('CURRENT_DOMAIN_SOURCE_REQUIRED');
    if (action.repositoryIds.some(id => !b.repositories.some(r => r.repositoryId === id))) reasons.push('REPOSITORY_NOT_BOUND');
    if (value.selectedRole?.role.delivery === 'supplied-evidence-only' && (action.capability || action.effects.length > 0)) reasons.push('ROLE_SUPPLIED_EVIDENCE_ONLY');
    if (action.effects.some(e => !n?.effectScope.some(a => a.kind === e.kind && a.target === e.target))) reasons.push('EFFECT_OUTSIDE_SCOPE');
    if (b.domainContext?.workUnit === b.work.id && b.domainContext.workState === 'complete' && action.effects.some(e => !['read','repo_read'].includes(e.kind))) reasons.push('DOMAIN_WORK_ALREADY_COMPLETE');
    if (action.capability) {
      if (n?.deniedCapabilities.includes(action.capability.name)) reasons.push('NATIVE_CAPABILITY_DENIED');
      const claim = n?.capabilities.find(c => c.name === action.capability!.name && c.stage === action.capability!.stage);
      if (!claim || Date.parse(atUtc) < Date.parse(claim.observedAt) || Date.parse(atUtc) >= Date.parse(claim.validUntil)) reasons.push('CAPABILITY_STAGE_NOT_OBSERVED');
    }
  }
  return {status: reasons.length ? 'context-rejected' as const : 'context-admitted' as const, reasons,
    nativePermissionGranted: false as const, executionReady: false as const, requiresNativeAdmission: true as const};
}
