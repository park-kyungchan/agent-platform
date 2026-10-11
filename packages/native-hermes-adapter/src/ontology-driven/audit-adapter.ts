import {assertArtifact, type EvidenceArtifact} from '@palantirkc/ontology';
import {SEMANTIC_OWNER, UPPER_ONTOLOGY, object, requireValue, strings, type Pin} from './contracts';

export interface LinkAssessment {linkId: string; basis: 'OBSERVED' | 'PROPOSED' | 'UNKNOWN'; evidenceIds: string[]}
const observed = new Set(['DECLARED','SOURCE_OBSERVED','RUNTIME_OBSERVED','OUTCOME_CONFIRMED']);
const statuses = new Set(['OFFICIAL','CONTEXT','DECLARED','SOURCE_OBSERVED','RUNTIME_OBSERVED','OUTCOME_CONFIRMED','INFERRED','PROPOSED','UNKNOWN']);

/** Additional target-side checks, after the original pack's schema/reference
 * validators. Original pack files and producer identities are never rewritten. */
export function adaptAuditSession(session: unknown, context: unknown, linkAssessments: readonly LinkAssessment[]) {
  object(session, 'audit session'); object(context, 'pack context');
  requireValue(session.documentKind === 'AUDIT_SESSION' && session.schemaVersion === '1.0.0', 'PACK_SESSION_SCHEMA', 'Expected original audit session producer.');
  for (const key of ['sessionContext','evidence','decisions','options','linkCandidates','objectCandidates','classifiedUnits'])
    requireValue(Array.isArray(session[key]), 'PACK_SESSION_SCHEMA', key);
  requireValue(Array.isArray(context.contextRules), 'PACK_CONTEXT_SCHEMA', 'contextRules');
  const origins = new Map<string, {user: boolean; metadata: boolean}>();
  for (const rule of context.contextRules) {
    object(rule, 'context rule'); requireValue(!origins.has(rule.id), 'CONTEXT_ID_COLLISION', rule.id);
    origins.set(rule.id, {user: rule.origin === 'CURRENT_USER_DIRECTIVE' && rule.verificationStatus === 'USER_STATED', metadata: false});
  }
  for (const item of session.sessionContext) {
    object(item, 'session context'); requireValue(!origins.has(item.id), 'CONTEXT_ID_COLLISION', item.id);
    origins.set(item.id, {user: item.origin === 'USER_MESSAGE', metadata: item.origin === 'REMOTE_METADATA'});
  }
  const evidence = new Map<string, any>();
  for (const item of session.evidence) {
    object(item, 'pack evidence'); requireValue(!evidence.has(item.id) && statuses.has(item.status), 'INVALID_PACK_EVIDENCE', item.id);
    if (item.status === 'CONTEXT') {
      const origin = origins.get(item.directiveId); requireValue(origin, 'MISSING_CONTEXT_ORIGIN', item.id);
      if (item.contextNature === 'USER_DIRECTIVE') requireValue(origin.user, 'PACK_CONTEXT_NOT_USER_AUTHORITY', item.directiveId);
      if (item.contextNature === 'REMOTE_METADATA') requireValue(origin.metadata, 'PACK_CONTEXT_NOT_METADATA', item.directiveId);
    }
    evidence.set(item.id, item);
  }
  for (const decision of session.decisions) if (decision.status === 'DECIDED') {
    strings(decision.userInstructionEvidenceRefs, 'user instruction evidence');
    requireValue(decision.userInstructionEvidenceRefs.some((id: string) => {
      const e = evidence.get(id); return e?.status === 'CONTEXT' && e.contextNature === 'USER_DIRECTIVE' && origins.get(e.directiveId)?.user;
    }), 'PACK_DECISION_NO_USER_BASIS', decision.id);
  }
  for (const unit of session.classifiedUnits) if (unit.classification?.status === 'CLASSIFIED') {
    requireValue(Array.isArray(unit.classification.roles) && unit.classification.roles.length > 0
      && unit.classification.roles.every((r: unknown) => ['DATA','LOGIC','ACTION','SECURITY'].includes(String(r))), 'INVALID_SEMANTIC_ROLE', unit.id);
  }
  requireValue(linkAssessments.length === session.linkCandidates.length && new Set(linkAssessments.map(l => l.linkId)).size === linkAssessments.length, 'LINK_BASIS_DENOMINATOR', 'Every actual link candidate needs one explicit assessment.');
  const links = session.linkCandidates.map((link: any) => {
    const assessment = linkAssessments.find(a => a.linkId === link.id);
    requireValue(assessment && ['OBSERVED','PROPOSED','UNKNOWN'].includes(assessment.basis), 'MISSING_LINK_BASIS', link.id);
    strings(assessment.evidenceIds, 'link evidence');
    requireValue(assessment.evidenceIds.every(id => evidence.has(id) && link.evidenceRefs.includes(id)), 'UNBOUND_LINK_EVIDENCE', link.id);
    if (assessment.basis !== 'UNKNOWN') requireValue(assessment.evidenceIds.length > 0 && assessment.evidenceIds.some(id =>
      assessment.basis === 'OBSERVED' ? observed.has(evidence.get(id).status) : evidence.get(id).status === 'PROPOSED'), 'LINK_BASIS_MISMATCH', link.id);
    return {id: link.id, from: link.fromObjectId, to: link.toObjectId, basis: assessment.basis, evidenceIds: [...assessment.evidenceIds]};
  });
  return {
    schema: 'hermes.audit-projection.v1' as const, semanticOwner: SEMANTIC_OWNER, upperOntology: UPPER_ONTOLOGY,
    sourceProducer: {documentKind: session.documentKind as 'AUDIT_SESSION', schemaVersion: '1.0.0', sessionId: session.sessionId as string | null},
    auditState: session.auditState as string, evidence: [...evidence.values()].map(e => ({id: e.id as string, status: e.status as string})),
    links, roles: session.classifiedUnits.map((u: any) => ({id: u.id as string, classification: structuredClone(u.classification), implementationLayers: [...u.implementationLayers], effects: [...u.effects]})),
    authority: 'none' as const, executionReady: false as const, acceptanceDecision: null,
    validationScope: 'pack-schema-references-plus-target-provenance; not criterion satisfaction or runtime acceptance' as const,
  };
}
export type AuditProjection = ReturnType<typeof adaptAuditSession>;

/** A truthful upper wrapper about the adapter check, not an assertion that the
 * projected audit's claims or business outcome were independently verified. */
export function auditProjectionEvidence(projection: AuditProjection, source: Pin, options: {id: string; projectId: string; createdAt: string}): EvidenceArtifact {
  const value: EvidenceArtifact = {
    schemaVersion: '1.0.0', kind: 'evidence', ...options, domainId: 'hermes-harness',
    payload: {claim: 'A source-pinned audit session passed the additional target provenance and link-basis checks.',
      method: 'Original producer schema remains distinct; explicit target-side projection checks only.', sourceUrl: 'local://' + source.path,
      fetchedAt: options.createdAt, result: {outcome: 'success', processExitCode: null, contentMatched: true}, attempts: [{attempt: 1, outcome: 'success'}],
      limitations: ['Not target runtime verification, business acceptance, or a native permission grant.', 'Source authorship remains the responsibility of the evidence producer.'],
      details: {verificationClass: 'contract-and-provenance-only', source: {...source}, producer: projection.sourceProducer,
        semanticOwner: projection.semanticOwner, upperOntology: {...UPPER_ONTOLOGY}, nativeProducerSchemaPreserved: true}},
  };
  assertArtifact(value); return value;
}
