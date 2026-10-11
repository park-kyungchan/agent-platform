import { z } from 'zod';
import { createHash } from 'node:crypto';
// A common semantic owner, never a native permission/provisioning surface.
const Id = z.string().min(1).max(128).regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]*$/u).refine(x=>!/[\x00-\x1f\x7f]/u.test(x));
const Text = z.string().trim().min(1).max(1200).refine(x=>!/[\x00-\x1f\x7f]/u.test(x));
const Sha = z.string().length(64).regex(/^[a-f0-9]{64}$/u);
const unique = <T>(xs:T[])=>new Set(xs).size===xs.length;
const ids = z.array(Id).max(32).refine(unique);
const layer = <I extends string,P extends string|null>(id:I,parent:P)=>z.object({id:z.literal(id),parent:z.literal(parent)}).strict();
export const LeadProtocolRecordSchema = z.object({
 type:z.literal('lead_protocol'),id:z.literal('lead-protocol'),protocolId:z.literal('ontology-platform.e2e-lead'),revision:z.literal(1),
 hierarchy:z.tuple([layer('north-star',null),layer('accountability','north-star'),layer('inception','accountability'),layer('ontology','inception'),layer('sdlc','ontology'),layer('task','sdlc')]),
 northStar:z.object({capability:z.literal('Palantir_AIP_Foundry_class_ontology_driven_system'),method:z.literal('AI_NATIVE_SDLC'),productIntegration:z.literal(false)}).strict(),
 accountability:z.object({accountableLead:z.literal('ontology-platform'),scope:z.literal('all_current_and_future_profile_ontology_layers'),
 centralOwns:z.tuple([z.literal('common_core'),z.literal('standard_contracts'),z.literal('standard_profile_configuration'),z.literal('integration_evaluation_adoption_recovery')]),
 domainOwns:z.tuple([z.literal('domain_implementation'),z.literal('private_data'),z.literal('final_business_judgment')]),
 delegation:z.literal('actual_domain_lead_with_platform_verify_rework_accept'),knowledgeOwner:z.literal('Wiki'),nativeOwner:z.literal('Hermes')}).strict(),
 inception:z.object({frontDoor:z.literal('ontology-platform_recommended_not_native_ui_restriction'),reuseSettledAnswers:z.literal(true),
 consequentialDecisions:z.tuple([z.literal('purpose'),z.literal('data'),z.literal('effects')]),ordinarySteps:z.literal('autonomous_inside_human_admitted_envelope'),
 profileNeed:z.literal('reuse_existing_role_before_provisioning')}).strict(),
 ontology:z.object({requiredDimensions:z.tuple([z.literal('identity'),z.literal('grain'),z.literal('evidence'),z.literal('relations'),z.literal('logic'),z.literal('actions'),z.literal('access'),z.literal('unknowns')]),extensions:z.literal('domain_owned_not_copied_common_policy')}).strict(),
 sdlc:z.object({stages:z.tuple([z.literal('admit'),z.literal('design'),z.literal('implement'),z.literal('verify'),z.literal('independent_evaluate'),z.literal('adopt'),z.literal('evolve'),z.literal('recover')]),
 evidenceClasses:z.tuple([z.literal('source_assertion'),z.literal('proposal'),z.literal('runtime_effect'),z.literal('local_check'),z.literal('independent_review'),z.literal('owner_acceptance')]),
 riskPolicy:z.literal('stronger_gates_by_risk_and_consumer'),recovery:z.literal('unchanged_still_owned_successors_only')}).strict(),
 task:z.object({actions:z.tuple([z.literal('design_contract'),z.literal('qualify_integration'),z.literal('select_capabilities'),z.literal('plan_profile_creation')]),
 executionAuthority:z.literal('none'),profileCreation:z.object({mode:z.literal('proposal_only'),requiredInspection:z.tuple([z.literal('layout_and_config'),z.literal('skills_and_identity'),z.literal('credentials_and_clone'),z.literal('gateway_service_registration'),z.literal('multiplexer_notification')])}).strict()}).strict(),
 governance:z.object({routes:z.object({mechanical:z.literal('deterministic_code'),session:z.literal('todo_list'),durable:z.literal('kanban_create_precise'),dependency_wait:z.literal('kanban_dependency'),time_wait:z.literal('native_cron_or_existing_monitor'),external_event:z.literal('native_webhook')}).strict(),
  effort:z.literal('complexity_selected_supported_not_fixed_high'),scheduledKanbanIsTimer:z.literal(false),maxRetries:z.literal('native_failure_limit_not_additional_retries'),
  nativeFields:ids.min(1),proceduralFields:ids.min(1),eligibility:z.literal('actual_role_route_and_restrictions_required'),customHarness:z.literal('thin_missing_control_only_after_native_parity_check'),
  amendment:z.object({path:Text,sha256:Sha}).strict()}).strict(),
 sources:z.tuple([z.object({id:z.literal('direction'),path:Text,sha256:Sha}).strict(),z.object({id:z.literal('feedback'),path:Text,sha256:Sha}).strict()]),
 principles:z.array(z.object({id:Id,sourceIds:ids.min(1),mechanism:z.enum(['system_scope','typed_consumer','purpose_first','answer_continuity','native_first','single_owner'])}).strict()).min(1).max(12)
}).strict();
export type LeadProtocol = z.infer<typeof LeadProtocolRecordSchema>;
export const LeadBindingSchema=z.object({path:Text,sha256:Sha,record:LeadProtocolRecordSchema,authority:z.literal('none')}).strict();
export type LeadBinding=z.infer<typeof LeadBindingSchema>;
export const GovernanceRequestSchema=z.object({schema:z.literal('backend.lead-governance.request.v1'),requestId:Id,
 protocol:z.object({id:Id,revision:z.number().int().positive(),roleSha256:Sha}).strict(),
 workload:z.object({kind:z.enum(['mechanical','session','durable','dependency_wait','time_wait','external_event']),complexity:z.enum(['low','moderate','complex'])}).strict(),
 route:z.object({tools:ids,lifecycleWorker:z.boolean(),restrictions:z.enum(['verified','unverified']),eventProducerAdmitted:z.boolean()}).strict(),
 controls:z.unknown().optional()}).strict();
const Effort=z.enum(['none','minimal','low','medium','high','xhigh','max','ultra']);
export const PreciseControlsSchema=z.object({role:z.enum(['implementation','review']),owner:Id,inputGenerationSha256:Sha,
 title:Text,initial_status:z.enum(['running','blocked']),
 model:Id,provider:Id,reasoning_effort:Effort,supportedEfforts:z.array(Effort).min(1).max(8).refine(unique),effortReason:Text,
 judgment:z.enum(['negligible','consequential']).optional(),
 max_runtime_seconds:z.number().int().positive().max(86400),max_retries:z.number().int().positive().max(10),
 workspace_kind:z.enum(['scratch','dir','worktree']),workspace_path:Text.optional(),idempotency_key:Id,
 parents:z.array(z.string().regex(/^t_[a-f0-9]{8}$/u).length(10)).max(16).refine(unique),completion_contract:Text,
 toolRestrictions:ids,effectScope:z.array(Text).min(1).max(16),verification:z.array(Text).min(1).max(16),recovery:Text}).strict();
export function assessGovernance(input:unknown,binding:LeadBinding){
 const parsed=GovernanceRequestSchema.safeParse(input);if(!parsed.success)fail('INVALID_GOVERNANCE_REQUEST');
 const q=parsed.data!,p=binding.record;
 if(q.protocol.id!==p.protocolId||q.protocol.revision!==p.revision||q.protocol.roleSha256!==binding.sha256)fail('STALE_PROTOCOL_BINDING');
 const common={schema:'backend.lead-governance.decision.v1',requestId:q.requestId,protocol:q.protocol,
  route:p.governance.routes[q.workload.kind],modelCallRequested:false,authority:'none',executionReady:false,appliesEffects:false,
  eligibility:'declared_not_authenticated',enforcementState:'NOT_DISPATCHED_SUPPORTED_FIELDS_ONLY',complexity:q.workload.complexity,
  scheduledKanbanIsTimer:p.governance.scheduledKanbanIsTimer};
 if(q.workload.kind==='external_event'&&!q.route.eventProducerAdmitted)fail('EVENT_PRODUCER_NOT_ADMITTED');
 if(q.workload.kind!=='durable')return common;
 const c=PreciseControlsSchema.safeParse(q.controls);if(!c.success)fail('NATIVE_CONTROLS_REQUIRED');
 const controls=c.data!;
 if(!q.route.tools.includes('kanban_create_precise'))fail('PRECISE_ROUTE_UNAVAILABLE');
 if(!q.route.lifecycleWorker)fail('ROLE_NOT_LIFECYCLE_ELIGIBLE');
 if(controls.role==='review'&&q.route.restrictions!=='verified')fail('REVIEW_RESTRICTIONS_UNQUALIFIED');
 if(!controls.supportedEfforts.includes(controls.reasoning_effort))fail('UNSUPPORTED_REASONING_EFFORT');
 // Local model policy at the actual proposal boundary, not native enforcement.
 // Low complexity alone does not establish negligible judgment. Review and
 // moderate/complex assignments remain consequential despite caller labels.
 if(controls.model==='gpt-6-luna'&&(controls.role==='review'||q.workload.complexity!=='low'||controls.judgment!=='negligible'))
  fail('MODEL_JUDGMENT_MISMATCH');
 if(controls.workspace_kind!=='scratch'&&(!controls.workspace_path||!controls.workspace_path.startsWith('/')||controls.workspace_path.split('/').some(x=>x==='..'||x==='.')))fail('EXPLICIT_WORKSPACE_REQUIRED');
 return {...common,requested:controls,
  nativeEnforced:{title:controls.title,assignee:controls.owner,initial_status:controls.initial_status,model:controls.model,provider:controls.provider,reasoning_effort:controls.reasoning_effort,max_runtime_seconds:controls.max_runtime_seconds,max_retries:controls.max_retries,
   workspace_kind:controls.workspace_kind,...(controls.workspace_path?{workspace_path:controls.workspace_path}:{}),idempotency_key:controls.idempotency_key,parents:controls.parents,completion_contract:controls.completion_contract},
  procedural:{role:controls.role,owner:controls.owner,inputGenerationSha256:controls.inputGenerationSha256,...(controls.judgment?{judgment:controls.judgment}:{}),toolRestrictions:controls.toolRestrictions,effectScope:controls.effectScope,verification:controls.verification,recovery:controls.recovery},
  requiredBeforeDispatch:['fresh_actual_profile_eligibility','native_control_readback','current_effect_admission','unchanged_generation_and_peer_scope'],
  maxRetriesSemantics:p.governance.maxRetries};
}
const DataClass=z.enum(['synthetic','public','private_domain']);
const Effect=z.enum(['local_contract','owned_code','native_configuration','profile_create','external_delivery']);
const Subject=z.object({profile:Id,revision:z.number().int().positive(),domainLead:Id}).strict();
const Source=z.object({id:Id,revision:z.number().int().positive(),kind:z.enum(['human_interview','source_evidence']),sha256:Sha,subject:Subject}).strict();
const purpose=z.object({status:z.literal('settled'),sourceId:Id,answer:Text}).strict();
const data=z.object({status:z.literal('settled'),sourceId:Id,classes:z.array(DataClass).min(1).max(3).refine(unique),owner:Id}).strict();
const effects=z.object({status:z.literal('settled'),sourceId:Id,allowed:z.array(Effect).max(5).refine(unique),excluded:z.array(Effect).max(5).refine(unique)}).strict();
export const InceptionRequestSchema=z.object({schema:z.literal('backend.profile-inception.request.v2'),requestId:Id,
 protocol:z.object({id:Id,revision:z.number().int().positive(),roleSha256:Sha}).strict(),
 sources:z.array(Source).min(1).max(16),originProfile:Id.optional(),
 subject:Subject,
 interview:z.object({purpose:purpose.optional(),data:data.optional(),effects:effects.optional()}).strict(),
 ontology:z.object({identity:ids.min(1),grain:Text,evidence:ids.min(1),relations:z.array(z.object({id:Id,from:Id,to:Id}).strict()).max(32),logic:z.array(Text).min(1).max(16),actions:z.array(Effect).max(5).refine(unique),
 access:z.object({dataOwner:Id,businessDecisionOwner:Id}).strict(),unknowns:ids}).strict(),
 work:z.object({action:Id,dataClasses:z.array(DataClass).min(1).max(3).refine(unique),effects:z.array(Effect).max(5).refine(unique),evaluator:z.literal('platform_and_domain'),recovery:Text}).strict(),
 nativePermission:z.literal('not_established')}).strict();
export class LeadProtocolError extends Error {constructor(readonly code:string){super(code);}}
const fail=(code:string):never=>{throw new LeadProtocolError(code);};
export function assessInception(input:unknown,binding:LeadBinding){
 const parsed=InceptionRequestSchema.safeParse(input);if(!parsed.success)fail('INVALID_INCEPTION_REQUEST');
 const q=parsed.data!,p=binding.record;
 if(q.protocol.id!==p.protocolId||q.protocol.revision!==p.revision||q.protocol.roleSha256!==binding.sha256)fail('STALE_PROTOCOL_BINDING');
 if(!unique(q.sources.map(s=>s.id)))fail('DUPLICATE_SOURCE_ID');
 if(q.sources.some(s=>s.revision!==q.subject.revision))fail('STALE_SOURCE_REVISION');
 if(q.sources.some(s=>s.subject.profile!==q.subject.profile||s.subject.domainLead!==q.subject.domainLead||s.subject.revision!==q.subject.revision))fail('SOURCE_SUBJECT_MISMATCH');
 const sources=new Map(q.sources.map(s=>[s.id,s]));
 const answered=Object.entries(q.interview) as [string,{sourceId:string}][];
 for(const [,a] of answered)if(sources.get(a.sourceId)?.kind!=='human_interview')fail('EVIDENCE_NOT_HUMAN_ADMISSION');
 if(q.ontology.evidence.some(id=>!sources.has(id)))fail('UNKNOWN_EVIDENCE');
 if(!unique(q.ontology.relations.map(r=>r.id)))fail('DUPLICATE_RELATION_ID');
 if(q.ontology.relations.some(r=>!q.ontology.identity.includes(r.from)||!q.ontology.identity.includes(r.to)))fail('RELATION_NOT_CLOSED');
 const owner=q.subject.domainLead;
 if(q.ontology.access.dataOwner!==owner||q.ontology.access.businessDecisionOwner!==owner||q.interview.data&&q.interview.data.owner!==owner)fail('DOMAIN_AUTHORITY_OVERREACH');
 if(!p.task.actions.includes(q.work.action as typeof p.task.actions[number]))fail('UNSUPPORTED_ACTION');
 const missing=p.inception.consequentialDecisions.filter(id=>!q.interview[id]);
 const common={schema:'backend.profile-inception.decision.v2',authority:'none',requestId:q.requestId,subject:q.subject,
  inputGenerationSha256:createHash('sha256').update(JSON.stringify(q)).digest('hex'),sources:q.sources,
  admission:'declaration_only_not_authenticated',
  protocol:{id:p.protocolId,revision:p.revision,roleSha256:binding.sha256},originProfile:q.originProfile??'ontology-platform',
  settled:answered.map(([decision,a])=>({decision,sourceId:a.sourceId,revision:sources.get(a.sourceId)!.revision,sha256:sources.get(a.sourceId)!.sha256})),
  nativePermission:'not_established',appliesEffects:false,executionReady:false};
 if(missing.length)return {...common,status:'INTERVIEW_REQUIRED',missing};
 const d=q.interview.data!,e=q.interview.effects!;
 if(e.allowed.some(x=>e.excluded.includes(x)))fail('CONTRADICTORY_EFFECT_ENVELOPE');
 if(q.work.dataClasses.some(x=>!d.classes.includes(x)))fail('UNAPPROVED_DATA');
 if(q.work.effects.some(x=>!e.allowed.includes(x)||e.excluded.includes(x))||q.ontology.actions.some(x=>!e.allowed.includes(x)||e.excluded.includes(x)))fail('UNAPPROVED_EFFECT');
 return {...common,status:q.ontology.unknowns.length?'CONTRACT_UNKNOWN':'DESIGN_READY',missing:[],action:q.work.action,
  accountability:p.accountability,stages:p.sdlc.stages,
  requiredInspection:q.work.action==='plan_profile_creation'?p.task.profileCreation.requiredInspection:[],unknowns:q.ontology.unknowns,
  evaluation:{local:'required',independent:'risk_proportional',adoption:'platform_and_domain_separate'},recovery:p.sdlc.recovery};
}

