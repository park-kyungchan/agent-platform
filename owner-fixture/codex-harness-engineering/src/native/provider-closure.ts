import {lstat,readFile,realpath} from 'node:fs/promises';
import {resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
const root=resolve(import.meta.dir,'../..'),common=resolve(root,'../agent-platform'),name='@palantirkc/native-codex-adapter';
const modules=['hooks','home-context','evaluation-ledger'] as const;
const required=['package.json','bun.lock','packages/native-codex-adapter/package.json',...modules.map(module=>'packages/native-codex-adapter/src/native/'+module+'.ts')];
const hash=(bytes:Uint8Array)=>createHash('sha256').update(bytes).digest('hex');
async function source(path:string){if(await realpath(path)!==path)throw Error('Native adapter code source must be exact canonical path');const stat=await lstat(path);if(!stat.isFile()||stat.size>2097152)throw Error('Native adapter code source must be bounded regular file');const bytes=await readFile(path);if(bytes.length!==stat.size||await realpath(path)!==path||bytes.length>2097152)throw Error('Native adapter code source changed during read');return {bytes,pin:{path,sha256:hash(bytes)}};}
/** Code-source evidence only. Native /hooks owns trust; these pins grant no execution authority. */
export async function readNativeAdapterSourceClosure(){
 const owner=await Promise.all(modules.map(module=>source(resolve(root,'src/native/'+module+'.ts')))),hook=owner[0]!.bytes.toString();
 const marker=/^\/\/ codex-native-provider-registration-sha256: ([a-f0-9]{64})\n/.exec(hook);
 if(!marker){let registered=false;try{await lstat(resolve(root,'architecture/native-codex-provider.json'));registered=true;}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}if(registered||owner.some(s=>s.bytes.toString().includes(name)))throw Error('Registered native adapter cannot downgrade its source closure');return {pins:owner.map(s=>s.pin),providerRoot:null};}
 const record=await source(resolve(root,'architecture/native-codex-provider.json'));if(record.pin.sha256!==marker[1])throw Error('Native adapter registration changed');const value=JSON.parse(record.bytes.toString());
 if(value.schema!=='codex.native-adapter-source-closure.v1'||value.consumerRoot!==root||value.providerRoot!==common||value.packageName!==name||value.authority!=='none'||value.nativePermissionGranted!==false||Object.keys(value).sort().join('|')!=='authority|consumerRoot|nativePermissionGranted|packageName|providerRoot|schema|sourcePins')throw Error('Native adapter source owner/scope differs');
 if(!Array.isArray(value.sourcePins)||value.sourcePins.length!==required.length||new Set(value.sourcePins.map((p:any)=>p.path)).size!==required.length||required.some(path=>!value.sourcePins.some((p:any)=>p.path===path))||value.sourcePins.some((p:any)=>!p||Object.keys(p).sort().join('|')!=='path|sha256'||!/^[a-f0-9]{64}$/.test(p.sha256)))throw Error('Exact native adapter provider source set required');
 const pins=[...owner.map(s=>s.pin),record.pin],provider=new Map<string,Uint8Array>();for(const p of value.sourcePins){const observed=await source(resolve(common,p.path));if(observed.pin.sha256!==p.sha256)throw Error('Native adapter provider source changed');pins.push(observed.pin);provider.set(p.path,observed.bytes);}
 const pkg=JSON.parse(provider.get('packages/native-codex-adapter/package.json')!.toString()),consumer=await source(resolve(root,'package.json')),lock=await source(resolve(root,'bun.lock'));pins.push(consumer.pin,lock.pin);
 if(pkg.name!==name||pkg.dependencies?.['@palantirkc/work-continuity']!=='file:../work-continuity'||JSON.parse(consumer.bytes.toString()).dependencies?.[name]!=='file:../agent-platform/packages/native-codex-adapter')throw Error('Native adapter package binding differs');
 for(const module of modules){const expected=resolve(common,'packages/native-codex-adapter/src/native/'+module+'.ts');if(pkg.exports?.['./'+module]!=='./src/native/'+module+'.ts'||Bun.resolveSync(name+'/'+module,dirname(resolve(root,'src/native/'+module+'.ts')))!==expected)throw Error('Native adapter package resolves outside canonical source');}
 for(const path of ['src/legacy/harness/owner-context-loader.ts','src/legacy/harness/provider-closure.ts','src/native/provider-closure.ts'])pins.push((await source(resolve(root,path))).pin);
 for(const pin of pins)if((await source(pin.path)).pin.sha256!==pin.sha256)throw Error('Native adapter source changed during observation');
 return {pins,providerRoot:common};
}
