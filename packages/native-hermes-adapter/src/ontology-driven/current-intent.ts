import {SEMANTIC_OWNER, exactKeys, hash, identity, object, pin, parsedJson, requireValue, strings, text, timestamp,
  type CurrentIntentRecord, type IntentOrigin, type Pin, type ReadPinned} from './contracts';

const origins: IntentOrigin[] = ['AUTHORED_USER','NATIVE_INTERVIEW_ANSWER','PRIOR_CONTEXT','RELAY','COMPRESSION','ASSISTANT_INFERENCE','REMOTE_METADATA'];
export function currentIntentRecord(input: unknown): CurrentIntentRecord {
  object(input, 'current intent'); exactKeys(input, ['schema','id','programId','revision','recordedAt','semanticOwner','sources','statements','repositoryResponsibilities','preserveDataAndHistory'], 'current intent');
  requireValue(input.schema === 'hermes.current-intent.v1' && input.semanticOwner === SEMANTIC_OWNER && input.preserveDataAndHistory === true, 'INTENT_BOUNDARY', 'Schema, semantic ownership and preservation must remain explicit.');
  identity(input.id, 'intent.id'); identity(input.programId, 'programId'); timestamp(input.recordedAt, 'recordedAt');
  requireValue(Number.isSafeInteger(input.revision) && input.revision > 0, 'INVALID_REVISION', 'intent revision');
  for (const key of ['sources','statements','repositoryResponsibilities']) requireValue(Array.isArray(input[key]) && input[key].length <= 256, 'INVALID_LIST', key);
  const sources = new Map<string, any>();
  for (const source of input.sources) {
    object(source, 'intent source'); exactKeys(source, ['id','origin','pin','selector','fragmentSha256','observedAt','authorRole'], 'intent source');
    identity(source.id, 'source.id'); pin(source.pin); text(source.selector, 'selector'); timestamp(source.observedAt, 'source observedAt');
    requireValue(source.selector.startsWith('/') && typeof source.fragmentSha256 === 'string' && /^[a-f0-9]{64}$/.test(source.fragmentSha256), 'INVALID_INTENT_FRAGMENT', source.id);
    requireValue(origins.includes(source.origin) && ['user','assistant','system','unknown'].includes(source.authorRole), 'INVALID_PROVENANCE', source.id);
    requireValue(!sources.has(source.id), 'DUPLICATE_SOURCE', source.id); sources.set(source.id, source);
  }
  const statements = new Map<string, any>();
  for (const statement of input.statements) {
    object(statement, 'intent statement'); exactKeys(statement, ['id','topic','statement','status','sourceIds','supersedes','scope'], 'intent statement');
    identity(statement.id, 'statement.id'); identity(statement.topic, 'topic'); text(statement.statement, 'statement');
    requireValue(['CURRENT','SUPERSEDED','UNRESOLVED'].includes(statement.status), 'INVALID_INTENT_STATUS', statement.id);
    strings(statement.sourceIds, 'sourceIds'); strings(statement.supersedes, 'supersedes');
    object(statement.scope, 'scope'); exactKeys(statement.scope, ['profiles','repositories'], 'scope');
    strings(statement.scope.profiles, 'profiles'); strings(statement.scope.repositories, 'repositories');
    requireValue(statement.sourceIds.every((id: string) => sources.has(id)), 'MISSING_INTENT_SOURCE', statement.id);
    if (statement.status === 'CURRENT') requireValue(statement.sourceIds.length > 0 && statement.sourceIds.every((id: string) => {
      const source = sources.get(id); return source.authorRole === 'user' && ['AUTHORED_USER','NATIVE_INTERVIEW_ANSWER'].includes(source.origin);
    }), 'NON_AUTHORITATIVE_CURRENT_INTENT', statement.id);
    requireValue(!statements.has(statement.id), 'DUPLICATE_STATEMENT', statement.id); statements.set(statement.id, statement);
  }
  for (const statement of statements.values()) for (const prior of statement.supersedes) {
    requireValue(prior !== statement.id && statements.get(prior)?.status === 'SUPERSEDED', 'INVALID_SUPERSESSION', statement.id);
    requireValue(statements.get(prior).supersedes.length === 0 || !statements.get(prior).supersedes.includes(statement.id), 'INVALID_SUPERSESSION', statement.id);
  }
  const active = new Set<string>(), done = new Set<string>();
  function visit(id: string) {requireValue(!active.has(id), 'INVALID_SUPERSESSION', id); if (done.has(id)) return; active.add(id); for (const prior of statements.get(id).supersedes) visit(prior); active.delete(id); done.add(id);}
  for (const id of statements.keys()) visit(id);
  const current = [...statements.values()].filter(s => s.status === 'CURRENT');
  for (let i = 0; i < current.length; i++) for (const other of current.slice(i + 1)) {
    const a = current[i];
    const overlap = (x: string[], y: string[]) => !x.length || !y.length || x.some(v => y.includes(v));
    requireValue(a.topic !== other.topic || !overlap(a.scope.profiles, other.scope.profiles) || !overlap(a.scope.repositories, other.scope.repositories), 'AMBIGUOUS_CURRENT_INTENT', a.topic);
  }
  const repositories = new Set<string>();
  for (const binding of input.repositoryResponsibilities) {
    object(binding, 'repository responsibility'); exactKeys(binding, ['repositoryId','ownerProfile','domainOwner','statementId'], 'repository responsibility');
    for (const key of ['repositoryId','ownerProfile','domainOwner','statementId']) identity(binding[key], key);
    const statement = statements.get(binding.statementId);
    requireValue(statement?.status === 'CURRENT' && statement.scope.profiles.includes(binding.ownerProfile) && statement.scope.repositories.includes(binding.repositoryId), 'UNBOUND_REPOSITORY_RESPONSIBILITY', binding.repositoryId);
    requireValue(!repositories.has(binding.repositoryId), 'AMBIGUOUS_REPOSITORY_OWNER', binding.repositoryId); repositories.add(binding.repositoryId);
  }
  return structuredClone(input) as CurrentIntentRecord;
}

/** Content verification does not authenticate an author. The source producer is
 * accountable for authored/interview classification and its exact record selector. */
export async function readCurrentIntent(expected: Pin, read: ReadPinned): Promise<CurrentIntentRecord> {
  const value = currentIntentRecord(parsedJson(await read(expected), expected));
  for (const source of value.sources) {
    const raw = await read(source.pin); requireValue(raw.byteLength > 0, 'EMPTY_INTENT_SOURCE', source.id);
    requireValue(hash(raw) === source.pin.sha256, 'SOURCE_PIN_CHANGED', source.pin.path);
    let selected = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(raw));
    for (const token of source.selector.slice(1).split('/')) {
      const key = token.replaceAll('~1', '/').replaceAll('~0', '~');
      requireValue(selected !== null && typeof selected === 'object' && Object.hasOwn(selected, key), 'INTENT_FRAGMENT_MISSING', source.id);
      selected = selected[key];
    }
    requireValue(typeof selected === 'string' && hash(selected) === source.fragmentSha256, 'INTENT_FRAGMENT_CHANGED', source.id);
  }
  return value;
}
