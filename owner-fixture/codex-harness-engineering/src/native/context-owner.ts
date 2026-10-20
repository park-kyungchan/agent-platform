import {lstat,realpath,open} from 'node:fs/promises';
import {constants} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {readRegisteredContextProvider,type ProviderPin} from './context-provider';

/** Code ownership is this module's immutable location, never a Work/binding data root. */
export const codexContextCodeOwnerRoot=resolve(import.meta.dir,'../..');
const ownerPaths=['index','types','validation','source','store','context','boundary','sections'].map(name=>resolve(codexContextCodeOwnerRoot,'src/continuity/'+name+'.ts'));
const hash=(bytes:Uint8Array)=>createHash('sha256').update(bytes).digest('hex');
async function readExact(path:string){
 if(resolve(path)!==path||await realpath(path)!==path)throw Error('Context code source must be exact canonical path');
 const before=await lstat(path);if(!before.isFile()||before.size>2097152)throw Error('Context code source exceeds regular bounded file scope');
 const h=await open(path,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
 try{const stat=await h.stat();if(!stat.isFile()||stat.dev!==before.dev||stat.ino!==before.ino||stat.size>2097152)throw Error('Context code source descriptor changed');
  const buffer=Buffer.alloc(stat.size+1);let count=0;
  while(count<buffer.length){const next=await h.read(buffer,count,buffer.length-count,null);if(!next.bytesRead)break;count+=next.bytesRead;}
  const bytes=buffer.subarray(0,count),after=await h.stat();if(bytes.length!==stat.size||after.size!==stat.size||after.mtimeMs!==stat.mtimeMs||await realpath(path)!==path)throw Error('Context code source changed during read');
  return {bytes,pin:{path,sha256:hash(bytes)}};
 }finally{await h.close();}
}
async function observe(){
 const ownerSources=await Promise.all(ownerPaths.map(readExact)),module=ownerPaths[0]!;
 const provider=await readRegisteredContextProvider(codexContextCodeOwnerRoot,module,ownerSources[0]!.bytes,readExact);
 // A registered/migrated owner cannot silently downgrade to standalone proof.
 // A genuine legacy standalone owner has neither a registration nor provider forwarders.
 if(provider.pins.length===0){
  let registered=false;try{await lstat(resolve(codexContextCodeOwnerRoot,'architecture/context-provider.json'));registered=true;}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
  const forwarded=ownerSources.some(source=>Buffer.from(source.bytes).includes('@palantirkc/work-continuity/context'));
  if(registered||forwarded)throw Error('Registered context provider requires its exact source-owned index forwarder; standalone downgrade refused');
 }
 const pins=[...ownerSources.map(s=>s.pin),...provider.pins].sort((a,b)=>a.path.localeCompare(b.path));
 for(const pin of pins)if((await readExact(pin.path)).pin.sha256!==pin.sha256)throw Error('Context provider changed during observation');
 return {pins,gitWorkModulePath:provider.gitWorkModulePath,workMethodAvailable:provider.workMethodAvailable,signature:hash(Buffer.from(JSON.stringify(pins)))};
}
type IndexApi=typeof import('../continuity/index');
type SectionsApi=typeof import('../continuity/sections');
type GitWorkApi=typeof import('@palantirkc/work-continuity/git-work');
// The common index remains the single typed semantic owner. This narrow bridge
// accepts unknown boundary data; it neither duplicates schemas nor imports an
// unregistered package subpath before Root publishes the reviewed v6 successor.
const workMethodFunctions=['validateWorkMethodStanding','validateWorkMethodPacket','validateWorkMethodReceipt','workMethodPacketSha256','workMethodReceiptSha256','workMethodRequiredSources','decideWorkMethodAdmission','evaluateWorkMethodCoverage'] as const;
export type WorkMethodOwnerApi=Readonly<Record<typeof workMethodFunctions[number],(...args:unknown[])=>unknown>>;
export interface LoadedCodexOwnerContext {index:IndexApi;sections:SectionsApi;gitWork?:GitWorkApi;workMethod?:WorkMethodOwnerApi;sourcePins:ProviderPin[];verifyCurrentSourceClosure():Promise<void>}
let loaded:LoadedCodexOwnerContext|undefined,signature:string|undefined,pending:Promise<LoadedCodexOwnerContext>|undefined;
export async function loadCodexOwnerContext():Promise<LoadedCodexOwnerContext>{
 if(pending){const result=await pending;await result.verifyCurrentSourceClosure();return result;}
 pending=(async()=>{
  const observation=await observe();
  if(loaded){if(signature!==observation.signature)throw Error('Codex context code successor requires a fresh process');await loaded.verifyCurrentSourceClosure();return loaded;}
  const cache=require.cache;
  if(!cache||typeof cache!=='object'||observation.pins.filter(p=>p.path.endsWith('.ts')).some(p=>Object.hasOwn(cache,p.path)))throw Error('Codex context code was imported before its observed source closure; use a fresh process');
  const index:IndexApi=await import(pathToFileURL(ownerPaths[0]!).href),sections:SectionsApi=await import(pathToFileURL(ownerPaths[7]!).href);
  const gitWork:GitWorkApi|undefined=observation.gitWorkModulePath?await import(pathToFileURL(observation.gitWorkModulePath).href):undefined;
  if(gitWork&&['resolveGitWorkSelection','openGitWork','readGitWorkContext'].some(name=>typeof gitWork[name as keyof GitWorkApi]!=='function'))throw Error('Registered Git Work provider API differs');
  let workMethod:WorkMethodOwnerApi|undefined;
  if(observation.workMethodAvailable){
   const exports=index as unknown as Record<string,unknown>;
   if(workMethodFunctions.some(name=>typeof exports[name]!=='function'))throw Error('Registered Work method provider API differs');
   workMethod=Object.freeze(Object.fromEntries(workMethodFunctions.map(name=>[name,exports[name]]))) as WorkMethodOwnerApi;
  }
  if((await observe()).signature!==observation.signature)throw Error('Codex context provider changed during import; use a fresh process');
  signature=observation.signature;
  loaded={index,sections,...(gitWork?{gitWork}:{}),...(workMethod?{workMethod}:{}),sourcePins:observation.pins,verifyCurrentSourceClosure:async()=>{if((await observe()).signature!==signature)throw Error('Codex context code successor requires a fresh process');}};
  return loaded;
 })();
 try{return await pending;}finally{pending=undefined;}
}
