import {z} from 'zod';
import {LeadProtocolError} from './lead-protocol.ts';
const Id=z.string().min(1).max(128).regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]*$/u).refine(x=>!/[\x00-\x20\x7f]/u.test(x));
const Text=z.string().min(1).max(1200).refine(x=>x.trim().length>0);
const unique=<T>(xs:T[])=>new Set(xs).size===xs.length;
const Ids=z.array(Id).max(16).refine(unique);
const EvidenceClass=z.enum(['user_decision','project_contract','observed_result','secondary_reference','proposal']);
const Effect=z.enum(['read','propose','owned_source_change','native_configuration','domain_operation','external_delivery','schedule']);
const Effects=z.array(Effect).max(7).refine(unique);
const SubjectBase=z.object({id:Id,owner:Id,kind:z.enum(['engineering_artifact','public_knowledge','domain_reference','native_capability']),meaning:Text,grain:Text});
const Subject=z.discriminatedUnion('state',[
 SubjectBase.extend({state:z.literal('known'),basis:z.object({evidenceClass:EvidenceClass,sourceRef:Text,revision:Text}).strict()}).strict(),
 SubjectBase.extend({state:z.literal('unknown'),reason:Text,resolution:z.enum(['source_lookup','owner_interview','defer'])}).strict(),
 SubjectBase.extend({state:z.literal('conflicting'),reason:Text,sourceRefs:z.array(Text).min(2).max(8).refine(unique),resolution:z.literal('owner_interview')}).strict(),
]);
const Logic=z.object({id:Id,dependsOn:Ids,inputs:z.array(z.object({subjectId:Id,acceptedEvidence:z.array(EvidenceClass).min(1).max(5).refine(unique)}).strict()).min(1).max(16),rule:Text}).strict();
export const OperatingContextSchema=z.object({
 schema:z.literal('backend.operating-context.v1'),id:Id,workId:Id,consumerId:Id,
 situation:z.object({beneficiary:Id,userRole:Text,activity:Text,desiredOutcome:Text,successCriterion:Text,failureToAvoid:Text,basisInterviewIds:Ids.min(1)}).strict(),
 subjects:z.array(Subject).min(1).max(16),logic:z.array(Logic).min(1).max(16),
 action:z.object({id:Id,alternativeId:Id,owner:Id,verb:Id,subjectIds:Ids.min(1),logicIds:Ids.min(1),requestedEffects:Effects.min(1),expectedResult:Text}).strict(),
 governance:z.object({owner:Id,allowedEffects:Effects,reservedEffects:Effects,
  reservedDecisionOwners:z.array(z.object({decision:Id,owner:Id}).strict()).max(16),basisInterviewIds:Ids.min(1),authority:z.literal('context_not_permission')}).strict(),
}).strict();
export const OperatingObservationSchema=z.object({schema:z.literal('backend.operating-context.observation.v1'),context:OperatingContextSchema}).strict();
export type OperatingContext=z.infer<typeof OperatingContextSchema>;
type ScopeNode={id:string;parent:string|null;kind:string;owner:string};
type Interview={id:string;subject:string;owner:string;status:string;answer?:string|undefined};
const fail=(code:string):never=>{throw new LeadProtocolError(code);};

/** Selected common meaning, not a native state mirror, scheduler or permission engine.
 * Snapshot integrity is checked by the public decision consumer; semantic source
 * interpretation, freshness and current effect admission stay with their owners.
 */
export function projectOperatingContext(input:unknown,scope:readonly ScopeNode[],consumerId:string,interviews:readonly Interview[],alternativeId:string){
 const parsed=OperatingContextSchema.safeParse(input);if(!parsed.success)fail('INVALID_OPERATING_CONTEXT');const c=parsed.data!;
 const work=scope.find(x=>x.id===c.workId),consumer=scope.find(x=>x.id===c.consumerId);
 if(work?.kind!=='work')fail('CONTEXT_WORK_MISMATCH');
 if(c.consumerId!==consumerId||consumer?.kind!=='consumer'||consumer.parent!==c.workId)fail('CONTEXT_CONSUMER_MISMATCH');
 if(c.action.alternativeId!==alternativeId)fail('CONTEXT_ALTERNATIVE_MISMATCH');
 if(c.action.owner!==consumer!.owner||c.governance.owner!==c.action.owner)fail('CONTEXT_OWNER_MISMATCH');
 const ancestors=new Set<string>();let cursor:ScopeNode|undefined=consumer;
 while(cursor){if(ancestors.has(cursor.id))fail('CONTEXT_SCOPE_CYCLE');ancestors.add(cursor.id);cursor=scope.find(x=>x.id===cursor!.parent);}
 for(const id of [...c.situation.basisInterviewIds,...c.governance.basisInterviewIds]){
  const answer=interviews.find(i=>i.id===id);
  if(!answer||answer.status!=='settled'||!answer.answer||!ancestors.has(answer.subject))fail('CONTEXT_BASIS_NOT_SETTLED');
 }
 const identifiers=[c.id,c.action.id,...c.subjects.map(s=>s.id),...c.logic.map(l=>l.id)];
 if(!unique(identifiers))fail('DUPLICATE_CONTEXT_ID');
 if(!unique(c.governance.reservedDecisionOwners.map(x=>x.decision)))fail('DUPLICATE_CONTEXT_DECISION');
 const subjects=new Map(c.subjects.map(s=>[s.id,s])),logic=new Map(c.logic.map(l=>[l.id,l]));
 if(c.action.subjectIds.some(id=>!subjects.has(id))||c.action.logicIds.some(id=>!logic.has(id)))fail('CONTEXT_REFERENCE_NOT_CLOSED');
 for(const rule of c.logic){
  if(rule.dependsOn.some(id=>!logic.has(id))||rule.inputs.some(i=>!subjects.has(i.subjectId)))fail('CONTEXT_REFERENCE_NOT_CLOSED');
  if(!unique(rule.inputs.map(i=>i.subjectId)))fail('DUPLICATE_CONTEXT_INPUT');
 }
 if(c.governance.allowedEffects.some(e=>c.governance.reservedEffects.includes(e)))fail('CONTRADICTORY_CONTEXT_EFFECTS');
 if(c.action.requestedEffects.some(e=>!c.governance.allowedEffects.includes(e)||c.governance.reservedEffects.includes(e)))fail('CONTEXT_EFFECT_NOT_ALLOWED');
 const writeEffects=['owned_source_change','native_configuration','domain_operation'];
 if(c.action.requestedEffects.some(e=>writeEffects.includes(e))&&c.action.subjectIds.some(id=>subjects.get(id)!.owner!==c.action.owner))fail('FOREIGN_CONTEXT_WRITE_OWNER');
 // Only dependsOn edges order logic. Subjects and reserved decisions are not
 // dependency edges, and an unrelated unknown subject does not block this Work.
 const visiting=new Set<string>(),complete=new Set<string>();
 const visit=(id:string,selected:Set<string>)=>{
  if(visiting.has(id))fail('CONTEXT_LOGIC_CYCLE');
  if(complete.has(id)){selected.add(id);return;}
  visiting.add(id);for(const dependency of logic.get(id)!.dependsOn)visit(dependency,selected);
  visiting.delete(id);complete.add(id);selected.add(id);
 };
 for(const id of logic.keys())visit(id,new Set());
 complete.clear();const selected=new Set<string>();for(const id of c.action.logicIds)visit(id,selected);
 const ordered=[...selected],selectedLogic=ordered.map(id=>logic.get(id)!);
 const needed=new Set([...c.action.subjectIds,...selectedLogic.flatMap(l=>l.inputs.map(i=>i.subjectId))]);
 const data=c.subjects.filter(s=>needed.has(s.id));
 type Gap={subjectId:string;owner:string;resolution:'source_lookup'|'owner_interview'|'defer';reason:'SUBJECT_UNKNOWN'|'SUBJECT_CONFLICT'|'EVIDENCE_CLASS_UNSUITABLE';detail:string;logicId?:string};
 const gaps:Gap[]=[];
 for(const s of data){
  if(s.state!=='known')gaps.push({subjectId:s.id,owner:s.owner,resolution:s.resolution,reason:s.state==='unknown'?'SUBJECT_UNKNOWN':'SUBJECT_CONFLICT',detail:s.reason});
 }
 for(const rule of selectedLogic)for(const requirement of rule.inputs){
  const s=subjects.get(requirement.subjectId)!;
  if(s.state==='known'&&!requirement.acceptedEvidence.includes(s.basis.evidenceClass))gaps.push({subjectId:s.id,owner:s.owner,resolution:'source_lookup',reason:'EVIDENCE_CLASS_UNSUITABLE',detail:`Required ${requirement.acceptedEvidence.join('|')}; supplied ${s.basis.evidenceClass}`,logicId:rule.id});
 }
 const status=gaps.some(g=>g.reason==='SUBJECT_CONFLICT')?'CONTEXT_CONFLICT':gaps.length?'CONTEXT_INPUT_REQUIRED':'USABLE_FOR_PROPOSAL';
 return structuredClone({schema:'backend.operating-context.projection.v1',contextId:c.id,workId:c.workId,consumerId:c.consumerId,
  scopeNodeIds:scope.filter(n=>ancestors.has(n.id)).map(n=>n.id),situation:c.situation,data,logic:selectedLogic,logicOrder:ordered,action:c.action,governance:c.governance,gaps,status,
  coverage:{selectedSubjectIds:data.map(s=>s.id),unselectedSubjectIds:c.subjects.filter(s=>!needed.has(s.id)).map(s=>s.id),unselectedLogicIds:c.logic.filter(l=>!selected.has(l.id)).map(l=>l.id),meaning:'selected_action_dependency_closure_not_whole_environment'},
  authority:'none',executionReady:false,limits:['Source references, evidence classes and effect envelopes are contextual declarations, not native authentication or permission.',
   'Typed connections and complete delivery do not establish semantic truth, agent comprehension or business acceptance.']});
}
