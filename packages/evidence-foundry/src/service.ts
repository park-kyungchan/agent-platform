import { join } from "node:path";
import { readFile } from "node:fs/promises";
import { assertActionRequest } from "@palantirkc/ontology/operational";
import type { ActionRequest as CanonicalActionRequest, ObjectRecord, LinkRecord } from "@palantirkc/ontology/operational";
import type { JsonObject } from "@palantirkc/ontology";
import { Store, canonical, hash } from "./modules/storage/store";
import type { AuditRecord } from "./generated/storage";
import { redactCaptureMetadata } from "./adapters/capture-policy";
import { Registry } from "./modules/ontology/registry";
import { runSuite } from "./modules/evals/runner";
import { collectFingerprints, evaluateUnit } from "./modules/evals";
import type { VerificationUnitProperties, EvaluatorProperties } from "./modules/evals";
import { verifyClaim } from "./modules/claims/verification";

export function createObject(objectType:string,projectId:string,properties:JsonObject,options:{id?:string;scope?:"project"|"shared"}={}):ObjectRecord {
  const now=new Date().toISOString();
  return {schemaVersion:"1.0.0",kind:"object",id:options.id??`${projectId}:${objectType}:${crypto.randomUUID()}`,objectType,objectTypeVersion:"1.0.0",version:1,projectId,scope:options.scope??"project",createdAt:now,updatedAt:now,properties};
}
export function createLink(linkType:string,from:ObjectRecord,to:ObjectRecord,options:{id?:string;projectId?:string}={}):LinkRecord {
  return {schemaVersion:"1.0.0",kind:"link",id:options.id??crypto.randomUUID(),linkType,linkTypeVersion:"1.0.0",version:1,from:from.id,to:to.id,projectId:options.projectId??from.projectId,scope:"project",createdAt:new Date().toISOString()};
}
export interface QueryOptions {projectId:string;objectType?:string;search?:string;includeShared?:boolean}
export interface ActionRequest {actionType:string;actionTypeVersion?:string;projectId:string;idempotencyKey:string;input:JsonObject;expectedVersions?:Record<string,number>|CanonicalActionRequest["expectedVersions"];simulate?:boolean;id?:string;createdAt?:string}
export interface ActionResult {status:"success"|"failure"|"simulated";auditId:string;objects:ObjectRecord[];error?:string}
export interface CaptureInput {projectId:string;requestedUrl:string;retrievedAt:string;outcome:string;artifacts:{kind:string;path?:string;bytes?:Uint8Array|string;sha256?:string;byteLength?:number;[key:string]:unknown}[];[key:string]:unknown}
export type ActionHandler=(request:ActionRequest,objects:Map<string,ObjectRecord>,links:Map<string,LinkRecord>)=>Promise<{objects:ObjectRecord[];links:LinkRecord[]}>|{objects:ObjectRecord[];links:LinkRecord[]};
export interface FoundryPolicy { readonly excludedDomains: readonly string[] }
function requirePolicy(policy: FoundryPolicy | undefined): asserts policy is FoundryPolicy {
  if (!policy || !Array.isArray(policy.excludedDomains) || policy.excludedDomains.some(value => typeof value !== "string")) throw new Error("Explicit semantic-owner Foundry policy required");
}
export class Foundry {
  private readonly policy: FoundryPolicy;
  readonly registry=new Registry();readonly handlers=new Map<string,ActionHandler>();
  constructor(readonly store:Store, policy?: FoundryPolicy) {
    requirePolicy(policy); this.policy = policy;
    this.handlers.set("put-object",req=>({objects:[req.input.object as unknown as ObjectRecord],links:[]}));
    this.handlers.set("link-objects",req=>({objects:[],links:[req.input.link as unknown as LinkRecord]}));
    this.handlers.set("accept-decision",req=>({objects:[req.input.decision as unknown as ObjectRecord],links:[]}));
    this.handlers.set("evidence.putObject",this.handlers.get("put-object")!);
    this.handlers.set("evidence.linkObjects",this.handlers.get("link-objects")!);
    this.handlers.set("evidence.createClaim",req=>({objects:[createObject("Claim",req.projectId,{verificationUnitIds:[],temporal:"current-state",...req.input})],links:[]}));
    this.handlers.set("evidence.createVerificationUnit",req=>({objects:[createObject("VerificationUnit",req.projectId,req.input.unit as JsonObject)],links:[]}));
    this.handlers.set("evidence.recordDecision",req=>({objects:[createObject("Decision",req.projectId,req.input)],links:[]}));
    this.handlers.set("evidence.ingestCapture",async req=>{
      const artifacts:JsonObject[]=[];
      try {return {objects:[await this.prepareCapture({...req.input.capture as unknown as CaptureInput,projectId:req.projectId},artifacts,!req.simulate)],links:[]};}
      catch(error){throw Object.assign(new Error(String(error)),{artifacts});}
    });
    this.handlers.set("evidence.runSuite",async(req,objects,links)=>{
      const stagedObjects=new Map(objects);const stagedLinks=new Map(links);const changedObjects=new Map<string,ObjectRecord>();const changedLinks=new Map<string,LinkRecord>();
      const staging={
        store:this.store,
        getObject:async(id:string)=>stagedObjects.get(id),
        queryObjects:async(options:QueryOptions)=>[...stagedObjects.values()].filter(o=>(o.projectId===options.projectId||options.includeShared!==false&&o.scope==="shared")&&(!options.objectType||o.objectType===options.objectType)),
        traverseLinks:async(id:string)=>({objects:[...stagedObjects.values()],links:[...stagedLinks.values()].filter(l=>l.from===id||l.to===id)}),
        putObjects:async(records:ObjectRecord[],newLinks:LinkRecord[]=[])=>{for(const object of records){stagedObjects.set(object.id,object);changedObjects.set(object.id,{...object,version:(objects.get(object.id)?.version??0)+1});}for(const link of newLinks){stagedLinks.set(link.id,link);changedLinks.set(link.id,link);}return {staged:true};},
        runAction:async(nested:ActionRequest,options?:{simulate?:boolean})=>{
          if(!options?.simulate)throw new Error("Staged suites only allow simulated local Actions");
          if(nested.actionType==="runSuite"||nested.actionType==="ingestCapture")throw new Error("Recursive or acquisition Actions cannot run in simulation");
          const type=this.registry.actionType(nested.actionType,nested.actionTypeVersion??"1.0.0");this.registry.validateActionInput(type,nested.input);
          if(type.handler.version!=="1.0.0")throw new Error("Unavailable Action handler version");
        const handler=this.handlers.get(type.handler.id);if(!handler)throw new Error("Unavailable simulation handler");
          const effect=await handler(nested,new Map(stagedObjects),new Map(stagedLinks));this.enforce(effect.objects,effect.links,{objects:stagedObjects,links:stagedLinks});
          return {status:"simulated",auditId:crypto.randomUUID(),objects:effect.objects};
        }
      };
      const suite=stagedObjects.get(String(req.input.suiteId));if(!suite||suite.projectId!==req.projectId)throw new Error("Unknown suite or project mismatch");
      // Native external execution is captured by the direct runner before local persistence.
      if((suite.properties.target as JsonObject).kind==="native-codex")throw new Error("Use runSuite API for budgeted native execution; local Actions manage persisted evidence");
      await runSuite(staging,suite.id,{runDirectory:join(this.store.root,"runs")});
      return {objects:[...changedObjects.values()],links:[]};
    });
    this.handlers.set("evidence.verifyClaim",async(req,objects)=>{
      const claim=objects.get(String(req.input.claimId));if(!claim||claim.objectType!=="Claim"||claim.projectId!==req.projectId)throw new Error("Unknown Claim or project mismatch");
      const runRef=req.input.runRef;if(typeof runRef!=="string"||objects.get(runRef)?.objectType!=="EvalRun")throw new Error("verifyClaim Action requires an existing EvalRun reference");
      const results:ObjectRecord[]=[];
      for(const unitId of claim.properties.verificationUnitIds as string[]) {
        const unit=objects.get(unitId);if(!unit)throw new Error("Unknown VerificationUnit");
        const evaluatorRef=req.input.evaluatorRef;const evaluator=typeof evaluatorRef==="string"?objects.get(evaluatorRef):undefined;if(!evaluator||evaluator.objectType!=="Evaluator"||evaluator.properties.method!=="deterministic")throw new Error("verifyClaim local Action requires a deterministic Evaluator");
        const result=evaluateUnit(unit.properties as unknown as VerificationUnitProperties,unit.properties.inputSnapshot,evaluator.properties as unknown as EvaluatorProperties);
        const fingerprints=await collectFingerprints({getObject:async id=>objects.get(id),queryObjects:async()=>[...objects.values()],putObjects:async()=>{},runAction:async()=>{}},unit,evaluator);
        results.push(createObject("CaseResult",req.projectId,{runRef,unitRef:unitId,evaluatorRef:evaluator.id,fingerprints,...JSON.parse(canonical(result)),verifiedAt:new Date().toISOString()}));
      }return {objects:results,links:[]};
    });
  }
  static async open(root=process.env.EVIDENCE_FOUNDRY_HOME??join(process.env.XDG_DATA_HOME??join(process.env.HOME!,".local/share"),"evidence-foundry"), policy?: FoundryPolicy) {requirePolicy(policy); return new Foundry(await new Store(root).initialize(), policy);}
  private enforce(records:ObjectRecord[],links:LinkRecord[],context:{objects:Map<string,ObjectRecord>;links:Map<string,LinkRecord>}) {
    requirePolicy(this.policy); const excluded=new Set(this.policy.excludedDomains);
    for(const object of records)if(excluded.has(object.projectId)||typeof object.properties.domainId==="string"&&excluded.has(object.properties.domainId))throw new Error("Excluded project or domain");
    const allObjects=new Map(context.objects);const allLinks=new Map(context.links);
    for(const object of records) {const previous=allObjects.get(object.id);if(object.version!==(previous?.version??0)+1)throw new Error("Version conflict: "+object.id);allObjects.set(object.id,object);}
    for(const link of links) {const previous=allLinks.get(link.id);if(link.version!==(previous?.version??0)+1)throw new Error("Version conflict: "+link.id);if(previous&&(previous.from!==link.from||previous.to!==link.to||previous.linkType!==link.linkType||previous.projectId!==link.projectId||previous.scope!==link.scope||previous.createdAt!==link.createdAt))throw new Error("Immutable link identity conflict");allLinks.set(link.id,link);}
    this.registry.validateObjects(allObjects,context.objects);this.registry.validateGraph(allObjects,allLinks);
    for(const object of allObjects.values())if(object.objectType==="CaseResult"&&object.properties.verificationClass==="deterministic-verified") {
      const evaluator=typeof object.properties.evaluatorRef==="string"?allObjects.get(object.properties.evaluatorRef):undefined;
      if(object.properties.nativeResult||evaluator&&["model-judged","native-codex"].includes(String(evaluator.properties.method)))throw new Error("Model judgment cannot be promoted to deterministic verification");
    }
    for(const object of records)if(object.objectType==="Decision") {
      const subject=allObjects.get(String(object.properties.subjectRef));if(!subject)throw new Error("Dangling decision subject");if(subject.projectId!==object.projectId&&subject.scope!=="shared")throw new Error("Decision subject project scope mismatch");
      if(object.properties.status!=="accepted")continue;
      const refs=object.properties.verificationRefs;if(!Array.isArray(refs)||!refs.length||!object.properties.authority)throw new Error("Accepted decision requires verification references and authority");
      if(!allObjects.has(String(object.properties.subjectRef)))throw new Error("Dangling decision subject");
      for(const id of refs)if(typeof id!=="string"||allObjects.get(id)?.objectType!=="CaseResult")throw new Error("Acceptance requires a CaseResult reference");
    }
    return {allObjects,allLinks};
  }
  async putObjects(objects:ObjectRecord[],links:LinkRecord[]=[]) {
    try{return await this.store.transaction(context=>{this.enforce(objects,links,context);return [...objects,...links];});}
    catch(error){await this.store.audit("putObjects","failure",{error:String(error),objectIds:objects.map(o=>o.id),linkIds:links.map(l=>l.id),records:JSON.parse(canonical(redactCaptureMetadata([...objects,...links])))});throw error;}
  }
  async getObject(id:string) {return (await this.store.snapshot()).objects.get(id);}
  async queryObjects(options:QueryOptions) {return this.store.query(options);}
  async traverseLinks(id:string,depth=1) {
    if(!Number.isInteger(depth)||depth<0||depth>64)throw new Error("Traversal depth must be 0..64");
    const snapshot=await this.store.snapshot();if(!snapshot.objects.has(id))throw new Error("Unknown object");
    const visited=new Set([id]);const links=new Map<string,LinkRecord>();let frontier=[id];
    for(let level=0;level<depth;level++){const next:string[]=[];for(const link of snapshot.links.values())if(frontier.includes(link.from)||frontier.includes(link.to)){links.set(link.id,link);for(const endpoint of [link.from,link.to])if(!visited.has(endpoint)){visited.add(endpoint);next.push(endpoint);}}frontier=next;}
    return {objects:[...visited].map(id=>snapshot.objects.get(id)!),links:[...links.values()]};
  }
  async getProvenance(id:string) {
    const snapshot=await this.store.snapshot();if(!snapshot.objects.has(id))throw new Error("Unknown object");
    const references:{from:string;property:string;to:string}[]=[];
    for(const object of snapshot.objects.values()) {
      const properties=this.registry.objectType(object.objectType,object.objectTypeVersion).properties.filter(p=>p.reference).map(p=>p.name);
      if(object.objectType==="Decision")properties.push("subjectRef");
      for(const property of properties) {const value=object.properties[property];for(const target of Array.isArray(value)?value:[value])if(typeof target==="string"&&snapshot.objects.has(target))references.push({from:object.id,property,to:target});}
    }
    const visited=new Set([id]);let frontier=[id];const includedLinks=new Map<string,LinkRecord>();const includedRefs=new Map<string,typeof references[number]>();
    for(let level=0;level<64&&frontier.length;level++) {
      const next:string[]=[];
      for(const edge of [...snapshot.links.values(),...references])if(frontier.includes(edge.from)||frontier.includes(edge.to)) {
        if("linkType" in edge)includedLinks.set(edge.id,edge);else includedRefs.set(edge.from+":"+edge.property+":"+edge.to,edge);
        for(const endpoint of [edge.from,edge.to])if(!visited.has(endpoint)){visited.add(endpoint);next.push(endpoint);}
      }frontier=next;
    }
    const objects=[...visited].map(id=>snapshot.objects.get(id)!);
    const histories=await Promise.all(objects.map(async object=>({id:object.id,versions:await Promise.all((await this.store.history(object.id)).map(async h=>({...h,record:await this.store.readRecord(h.hash)})))})));
    return {subject:id,objects,links:[...includedLinks.values()],references:[...includedRefs.values()],histories,truncated:frontier.length>0,authority:"Raw observations, verification classes and acceptance decisions remain distinct"};
  }
  async queryEvidence(options:QueryOptions) {
    const objects=await this.queryObjects(options);const verifications=await Promise.all(objects.filter(o=>o.objectType==="Claim").map(o=>this.verifyClaim(o.id)));
    return {objects,verifications};
  }
  async runAction(rawRequest:ActionRequest,options:{simulate?:boolean}={}):Promise<ActionResult> {
    const request=rawRequest&&typeof rawRequest==="object"&&!Array.isArray(rawRequest)?rawRequest:{} as ActionRequest;
    const auditKey=hash(canonical([request.projectId??"",request.actionType??"",request.idempotencyKey??""]));
    let result:ActionResult|undefined;
    await this.store.transaction(async context=>{
      const auditId=crypto.randomUUID();const requestHash=hash(canonical(request));
      let records:ObjectRecord[]=[];let links:LinkRecord[]=[];let status:ActionResult["status"]="success";let error:string|undefined;let artifacts:JsonObject[]=[];let canonicalRequest:CanonicalActionRequest|undefined;
      try {
        if(!request.idempotencyKey||!request.projectId)throw new Error("Action requires project and idempotency key");
        const key=auditKey;
        if(context.audits.some(a=>a.operation==="runAction"&&a.idempotencyKey===key))throw new Error("Duplicate Action idempotency key");
        const type=this.registry.actionType(request.actionType,request.actionTypeVersion??"1.0.0");this.registry.validateActionInput(type,request.input);
        const expected=Array.isArray(request.expectedVersions)?Object.fromEntries(request.expectedVersions.map(p=>[p.objectId,p.version])):request.expectedVersions??{};
        canonicalRequest={schemaVersion:"1.0.0",kind:"action-request",id:request.id??auditId,actionType:request.actionType,actionTypeVersion:type.version,projectId:request.projectId,idempotencyKey:request.idempotencyKey,input:request.input,expectedVersions:Object.entries(expected).map(([objectId,version])=>({objectId,version})),simulate:options.simulate??request.simulate??false,createdAt:request.createdAt??new Date().toISOString()};assertActionRequest(canonicalRequest,type);
        for(const [id,version] of Object.entries(expected))if((context.objects.get(id)??context.links.get(id))?.version!==version)throw new Error("Version precondition conflict: "+id);
        if(type.handler.version!=="1.0.0")throw new Error("Unavailable Action handler version");
        const handler=this.handlers.get(type.handler.id);if(!handler)throw new Error("Unavailable local Action handler: "+type.handler.id);
        for(const precondition of type.preconditions) {
          if(precondition.kind==="custom")throw new Error("Unavailable custom precondition handler");
          const raw=precondition.inputPath?.split(".").reduce<unknown>((v,p)=>v&&typeof v==="object"?(v as Record<string,unknown>)[p]:undefined,request.input);
          const id=typeof raw==="string"?raw:raw&&typeof raw==="object"?String((raw as {id?:string}).id):undefined;
          if(precondition.kind==="object-exists"&&(!id||!context.objects.has(id)))throw new Error("Object precondition failed");
          if(precondition.kind==="version-match"&&id&&context.objects.has(id)&&expected[id]!==context.objects.get(id)!.version)throw new Error("Explicit version precondition required: "+id);
        }
        const effect=await handler({...request,simulate:options.simulate??request.simulate??false},new Map([...context.objects].map(([id,obj])=>[id,JSON.parse(canonical(obj))])),new Map([...context.links].map(([id,link])=>[id,JSON.parse(canonical(link))])));records=effect.objects;links=effect.links;
        for(const object of records) {if(object.projectId!==request.projectId)throw new Error("Action project mismatch");if(!type.effects.objectTypes.includes(object.objectType))throw new Error("Undeclared Action object effect");if(context.objects.has(object.id)&&expected[object.id]!==context.objects.get(object.id)!.version)throw new Error("Version precondition required for mutation");}
        for(const link of links) {if(link.projectId!==request.projectId||!type.effects.linkTypes.includes(link.linkType))throw new Error("Undeclared Action link effect");if(context.links.has(link.id)&&expected[link.id]!==context.links.get(link.id)!.version)throw new Error("Version precondition required for link mutation");}
        this.enforce(records,links,context);if(options.simulate??request.simulate)status="simulated";
      } catch(caught){status="failure";error=String(caught);if(!(options.simulate??request.simulate)&&caught&&typeof caught==="object"&&"artifacts" in caught)artifacts=(caught as {artifacts:JsonObject[]}).artifacts;records=[];links=[];}
      const audit:AuditRecord={schemaVersion:"1.0.0",kind:"audit",id:auditId,createdAt:new Date().toISOString(),operation:"runAction",status,idempotencyKey:auditKey,requestHash,details:{request:JSON.parse(canonical(redactCaptureMetadata(canonicalRequest??request))),objects:records.map(r=>r.id),links:links.map(r=>r.id),artifacts,...(error?{error}:{})}};
      result={status,auditId,objects:records,...(error?{error}:{})};return status==="success"?[...records,...links,audit]:[audit];
    });return result!;
  }
  private async prepareCapture(input:CaptureInput,artifacts:JsonObject[],persist=true) {
    for(const artifact of input.artifacts) {
      const bytes=artifact.bytes??(artifact.path?await readFile(artifact.path):undefined);if(bytes===undefined)throw new Error("Capture artifact bytes unavailable");
      const data=typeof bytes==="string"?Buffer.from(bytes):bytes;const stored=persist?await this.store.putBlob(data):{sha256:hash(data),byteLength:data.byteLength};
      artifacts.push({...JSON.parse(canonical(redactCaptureMetadata(Object.fromEntries(Object.entries(artifact).filter(([key])=>key!=="bytes"&&key!=="path"))))),kind:artifact.kind,...stored,...(artifact.path?{originalPath:artifact.path}:{}),...(artifact.sha256?{declaredSha256:artifact.sha256}:{})});
      if(artifact.sha256&&artifact.sha256!==stored.sha256)throw new Error("Capture artifact hash mismatch");
      if(artifact.byteLength!==undefined&&artifact.byteLength!==stored.byteLength)throw new Error("Capture artifact byte length mismatch");
    }
    const metadata=JSON.parse(canonical(redactCaptureMetadata(Object.fromEntries(Object.entries(input).filter(([key])=>key!=="artifacts")))));
    return createObject("SourceCapture",input.projectId,{url:input.requestedUrl,retrievedAt:input.retrievedAt,outcome:input.outcome,artifacts,metadata},{scope:input.scope==="shared"?"shared":"project"});
  }
  async ingestCapture(input:CaptureInput) {
    let object:ObjectRecord|undefined;let failure:unknown;
    await this.store.transaction(async context=>{
      const artifacts:JsonObject[]=[];
      try {object=await this.prepareCapture(input,artifacts);this.enforce([object],[],context);}
      catch(error){failure=error;}
      const audit:AuditRecord={schemaVersion:"1.0.0",kind:"audit",id:crypto.randomUUID(),createdAt:new Date().toISOString(),operation:"ingestCapture",status:failure?"failure":"success",details:{input:JSON.parse(canonical(redactCaptureMetadata(Object.fromEntries(Object.entries(input).filter(([key])=>key!=="artifacts"))))),artifacts,...(failure?{error:String(failure)}:{objectId:object!.id})}};
      return failure?[audit]:[object!,audit];
    });
    if(failure)throw failure;return object!;
  }
  runSuite(id:string,options?:Parameters<typeof runSuite>[2]) {return runSuite(this,id,{runDirectory:join(this.store.root,"runs",crypto.randomUUID()),...options});}
  verifyClaim(id:string,options?:Parameters<typeof verifyClaim>[2]) {return verifyClaim(this,id,options);}
  close(){this.store.close();}
}
