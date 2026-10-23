import { afterEach, expect, test } from 'bun:test';
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, writeFile, lstat, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const source = await readFile(new URL('../scripts/hermes-main-update', import.meta.url), 'utf8');
const SHA = '818c13be1dc4fd28987e1e881a9408224afd4535';
const ID = 'sha256:' + 'a'.repeat(64);
const BASE = 'hermes-agent:main-' + SHA;
const DESKTOP = BASE + '-desktop';
const shellQuote = (value: string): string => "'" + value.replaceAll("'", "'\\''") + "'";
const roots: string[] = [];
afterEach(async () => { await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true }))); });

interface Fixture {
  root: string; script: string; config: string; log: string; env: Record<string, string | undefined>;
  run(args: string[]): Promise<{ code: number; stdout: string; stderr: string }>;
  functions(body: string): Promise<{ code: number; stdout: string; stderr: string }>;
  calls(): Promise<string[][]>;
  configure(values: Record<string, unknown>): Promise<void>;
}
async function fixture(options: Record<string, unknown> = {}, overrides = ''): Promise<Fixture> {
  const root = await mkdtemp(join(tmpdir(), 'native-hermes-desktop-')); roots.push(root);
  const bin = join(root, 'bin'); await mkdir(bin);
  const script = join(root, 'hermes-main-update');
  const rewritten = source.replace('ROOT="/docker/hermes-agent-2q3y"', 'ROOT=' + shellQuote(root));
  const rewrittenBoundary=rewritten.indexOf('COMMAND="${1:-help}"');
  await writeFile(script, rewritten.slice(0, rewrittenBoundary) + overrides + '\n' + rewritten.slice(rewrittenBoundary), { mode: 0o700 });
  const config = join(root, 'fake.json'), log = join(root, 'calls.jsonl');
  const defaults = { sha: SHA, id: ID, desktopLabel: '1', binaries: true, images: { [DESKTOP]: ID }, remoteTagSha: SHA, localTagSha: SHA };
  await writeFile(config, JSON.stringify({ ...defaults, ...options })); await writeFile(log, '');
  const fake = `#!${process.execPath}
import {readFileSync,writeFileSync,appendFileSync} from 'node:fs';
const a=process.argv.slice(2), command=process.argv[1].split('/').pop();
const path=process.env.FAKE_CONFIG, c=JSON.parse(readFileSync(path,'utf8'));
appendFileSync(process.env.FAKE_LOG,JSON.stringify([command,...a])+'\\n');
const out=v=>{console.log(v);process.exit(0)}, fail=()=>process.exit(1);
if(command==='systemd-run')process.exit(0);
if(command==='git'){
 if(a.includes('ls-remote')){out('b'.repeat(40)+'\\trefs/tags/v0.21.6\\n'+c.remoteTagSha+'\\trefs/tags/v0.21.6^{}');}
 if(a.includes('fetch'))process.exit(0);
 if(a.includes('rev-parse'))out(a.at(-1).includes('refs/tags/')?c.localTagSha:c.sha);
 fail();
}
if(command==='docker'){
 if(a[0]==='image'&&a[1]==='inspect'){
  if(!c.images[a.at(-1)])fail();
  const f=a[a.indexOf('--format')+1];
  if(!a.includes('--format'))out('{}');
  if(f==='{{.Id}}')out(c.images[a.at(-1)]);
  if(f.includes('org.opencontainers.image.revision'))out(c.sha);
  if(f.includes('bot-desktop'))out(c.desktopLabel);
  if(f.includes('overlay.sha256'))out(c.overlayLabel||'<no value>');fail();
 }
 if(a[0]==='image'&&a[1]==='tag'){if(c.images[a[3]])fail();c.images[a[3]]=c.images[a[2]];writeFileSync(path,JSON.stringify(c));process.exit(0);}
 if(a[0]==='buildx'&&a[1]==='build'){c.images[a[a.indexOf('--tag')+1]]=c.id;writeFileSync(path,JSON.stringify(c));process.exit(0);}
 if(a[0]==='run'){
  const entry=a[a.indexOf('--entrypoint')+1];
  if(entry==='/bin/sh')process.exit(c.binaries?0:1);
  if(entry==='/opt/hermes/.venv/bin/python'&&a.at(-1).includes('Font probe must')){
   if(c.fontMode!==420)fail();
   out(JSON.stringify({schema:1,check:'font-readability',result:'passed',uid:10000,files:10,fresh_container:true,live_data_mounted:false}));
  }
  if(a.at(-1)==='/etc/hermes/image-provenance.json')out(JSON.stringify({revision:c.sha}));
  if(a.at(-1)==='/opt/hermes/install-stamp.json')out(JSON.stringify({commit:c.sha,baseVersion:'0.21.6',displayVersion:'0.21.6',distance:0,dirty:false,...c.stamp}));fail();
 }
 if(a[0]==='compose')process.exit(0);fail();
}
fail();
`;
  for (const name of ['docker','git','systemd-run']) { await writeFile(join(bin,name),fake); await chmod(join(bin,name),0o700); }
  const env = { ...process.env, PATH: bin + ':' + process.env.PATH, FAKE_CONFIG: config, FAKE_LOG: log };
  const execute = async (args: string[]) => {
    const child = Bun.spawn(args, { env, stdout: 'pipe', stderr: 'pipe' });
    const [stdout,stderr,code] = await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
    return { code, stdout, stderr };
  };
  return { root,script,config,log,env,run:args=>execute(['bash',script,...args]),
    functions:async body=>{const file=join(root,'functions.sh');await writeFile(file,rewritten.slice(0,rewrittenBoundary)+'\n'+body);return execute(['bash',file]);},
    calls:async()=> (await readFile(log,'utf8')).trim().split('\n').filter(Boolean).map(line=>JSON.parse(line) as string[]),
    configure:async values=>{const current=JSON.parse(await readFile(config,'utf8'));await writeFile(config,JSON.stringify({...current,...values}));}
  };
}
const variables = `BOT_DESKTOP=1; RELEASE_TAG=v0.21.6; OVERLAY_MODE=0; OVERLAY_SHA256=''; OVERLAY_TREE_SHA=''; CANDIDATE_SHA=${SHA}; CANDIDATE_TAG=${DESKTOP}; FORCE_REBUILD=0; ADOPT_IMAGE_ID=''; RUN_ID=test; export BOT_DESKTOP RELEASE_TAG;`;

test('CLI rejects mixed overlays and inappropriate source-variant actions before native commands', async () => {
  const f=await fixture();
  const mixed=await f.run(['prepare','--sha',SHA,'--bot-desktop','--overlay-file','/unread/patch','--overlay-sha256','b'.repeat(64)]);
  expect(mixed.code).toBe(1); expect(mixed.stderr).toContain('pristine official source'); expect(await f.calls()).toEqual([]);
  const action=await f.run(['start','--action','status','--bot-desktop']);
  expect(action.code).toBe(1); expect(action.stderr).toContain('prepare, deploy, or update job');
  const tag=await f.run(['prepare','--release-tag','v00.21.6','--sha',SHA]);
  expect(tag.code).toBe(1); expect(tag.stderr).toContain('canonical');
});

test('background native job forwards the exact variant and release pin and records both', async () => {
  const f=await fixture(); const result=await f.run(['start','--action','prepare','--sha',SHA,'--bot-desktop','--release-tag','v0.21.6']);
  expect(result.code).toBe(0);
  const call=(await f.calls()).find(row=>row[0]==='systemd-run')!;
  expect(call.slice(call.indexOf('--')+1)).toEqual([f.script,'prepare','--sha',SHA,'--bot-desktop','--release-tag','v0.21.6']);
  const files=await readdir(join(f.root,'.main-updater/jobs'));
  const record=JSON.parse(await readFile(join(f.root,'.main-updater/jobs',files[0]!), 'utf8'));
  expect(record).toMatchObject({bot_desktop:true,release_tag:'v0.21.6',requested_sha:SHA});
});

test('candidate verification rejects label, binary and release-stamp mismatches without live data mounts', async () => {
  const f=await fixture();
  expect((await f.functions(variables+'\ncandidate_image_valid')).code).toBe(0);
  await f.configure({desktopLabel:'<no value>'}); expect((await f.functions(variables+'\ncandidate_image_valid')).code).toBe(1);
  await f.configure({desktopLabel:'1',binaries:false}); expect((await f.functions(variables+'\ncandidate_image_valid')).code).toBe(1);
  await f.configure({binaries:true,stamp:{displayVersion:'git.818c13b'}}); expect((await f.functions(variables+'\ncandidate_image_valid')).code).toBe(1);
  const runs=(await f.calls()).filter(row=>row[1]==='run');
  expect(runs.every(row=>row.includes('--network')&&row.includes('none')&&!row.includes('-v')&&!row.includes('--mount'))).toBe(true);
});

test('existing desktop tags cannot be force-rebuilt, silently adopted or repointed', async () => {
  const f=await fixture(); await mkdir(join(f.root,'.main-updater'),{recursive:true});
  await writeFile(join(f.root,'.main-updater/state.json'),JSON.stringify({candidate:{tag:DESKTOP,image_id:ID}}));
  const forced=await f.functions(variables+'\nFORCE_REBUILD=1; build_candidate');
  expect(forced.code).toBe(1); expect(forced.stderr).toContain('cannot repoint');
  await writeFile(join(f.root,'.main-updater/state.json'),'{}');
  const orphan=await f.functions(variables+'\nbuild_candidate');
  expect(orphan.code).toBe(1); expect(orphan.stderr).toContain('no saved image ID');
  expect((await f.calls()).some(row=>row[1]==='buildx'||row[2]==='tag')).toBe(false);
});

test('new desktop build uses the official opt-in and only promotes a verified temporary variant', async () => {
  const f=await fixture({images:{[BASE]:ID}}); const checkout=join(f.root,'checkout');
  await mkdir(join(checkout,'scripts/releases'),{recursive:true});
  await writeFile(join(checkout,'scripts/releases/distance.py'),"print('0.21.6')\n");
  await writeFile(join(checkout,'scripts/write_install_stamp.py'),"import sys,pathlib\npathlib.Path(sys.argv[sys.argv.index('--output')+1]).write_text('{}')\n");
  const result=await f.functions(variables+`\nprepare_worktree(){ WORKTREE=${shellQuote(checkout)}; }; build_candidate`);
  expect(result.code).toBe(0);
  const calls=await f.calls(),build=calls.find(row=>row[1]==='buildx')!,tag=calls.find(row=>row[2]==='tag')!;
  expect(build).toContain('HERMES_BOT_DESKTOP=1'); expect(build).toContain('io.nousresearch.hermes.bot-desktop=1');
  expect(build[build.indexOf('--tag')+1]).not.toBe(BASE); expect(tag[4]).toBe(DESKTOP);
  expect(calls.findIndex(row=>row[1]==='run'&&row.includes('/bin/sh'))).toBeLessThan(calls.indexOf(tag));
  const config=JSON.parse(await readFile(f.config,'utf8')); expect(config.images[BASE]).toBe(ID); expect(config.images[DESKTOP]).toBe(ID);
});

test('release pin checks remote and local peeled identities and fetches only the non-forced exact tag', async () => {
  const f=await fixture();
  expect((await f.functions(variables+'\nverify_release_tag')).code).toBe(0);
  const fetch=(await f.calls()).find(row=>row.includes('fetch'))!;
  expect(fetch.at(-1)).toBe('refs/tags/v0.21.6:refs/tags/v0.21.6'); expect(fetch).not.toContain('--force');
  await writeFile(f.log,''); await f.configure({remoteTagSha:'b'.repeat(40)});
  const wrong=await f.functions(variables+'\nverify_release_tag'); expect(wrong.code).toBe(1); expect(wrong.stderr).toContain('requested source SHA');
  expect((await f.calls()).some(row=>row.includes('fetch'))).toBe(false);
  await f.configure({remoteTagSha:SHA,localTagSha:'b'.repeat(40)});
  expect((await f.functions(variables+'\nverify_release_tag')).code).toBe(1);
});

test('Compose admits only full desktop identity and replaces it without touching unrelated services', async () => {
  const f=await fixture(); const compose=join(f.root,'docker-compose.yml');
  await writeFile(compose,`services:\n  hermes-serve:\n    image: ${DESKTOP}\n  unrelated:\n    image: nginx:stable\n`);
  expect((await f.functions('compose_image_ref')).stdout.trim()).toBe(DESKTOP);
  const result=await f.functions(variables+`\nCANDIDATE_TAG=${BASE}; save_and_write_compose`);
  expect(result.code).toBe(0); expect(await readFile(compose,'utf8')).toContain('image: nginx:stable');
  await writeFile(compose,`image: hermes-agent:main-${SHA.slice(0,12)}-desktop\n`);
  expect((await f.functions('compose_image_ref')).code).not.toBe(0);
});

test('update exec retains variant and release choices when invoking deployment', async () => {
  const f=await fixture({},`
prepare_latest(){ CANDIDATE_SHA="$REQUESTED_SHA"; set_candidate_tag; CURRENT_TAG=other; }
remove_worktree(){ :; }
deploy_candidate(){ printf 'DEPLOY:%s:%s:%s\\n' "$BOT_DESKTOP" "$RELEASE_TAG" "$REQUESTED_SHA"; }
`);
  const result=await f.run(['update','--sha',SHA,'--bot-desktop','--release-tag','v0.21.6','--approve-deploy']);
  expect(result.code).toBe(0); expect(result.stdout).toContain('DEPLOY:1:v0.21.6:'+SHA);
});

test('rollback infers saved desktop variant, verifies it and retains release metadata', async () => {
  const f=await fixture(); await mkdir(join(f.root,'.main-updater'),{recursive:true});
  await writeFile(join(f.root,'.main-updater/state.json'),JSON.stringify({previous:{sha:SHA,tag:DESKTOP,image_id:ID,bot_desktop:true,release_tag:'v0.21.6'}}));
  const result=await f.functions(variables+`
APPROVE_DEPLOY=1; BOT_DESKTOP=0; RELEASE_TAG=''; CURRENT_OVERLAY_SHA256=''; CURRENT_OVERLAY_TREE_SHA=''; CURRENT_BOT_DESKTOP=0; CURRENT_RELEASE_TAG=''; export CURRENT_BOT_DESKTOP CURRENT_RELEASE_TAG;
acquire_lock(){ :; }; current_facts(){ CURRENT_SHA=${SHA}; CURRENT_TAG=${BASE}; CURRENT_IMAGE_ID=${ID}; }
record_phase(){ :; }; save_and_write_compose(){ :; }; wait_for_candidate(){ :; }; gateway_canary(){ :; }
rollback_command
`);
  expect(result.code).toBe(0);
  const state=JSON.parse(await readFile(join(f.root,'.main-updater/state.json'),'utf8'));
  expect(state.current).toMatchObject({sha:SHA,tag:DESKTOP,bot_desktop:true,release_tag:'v0.21.6'});
  expect(state.previous).toMatchObject({tag:BASE,bot_desktop:false}); expect(state.candidate).toBeNull();
});


const FONT_SHA='d923a5eb487a17c0a68801bba9ef0f56c81040a8';
const FONT_PATCH='d9d7d4bb8f446f80ffd7402d9ad685b829f454c3fd1d91c5d1589616ae8d2ee2';
const FONT_TAG=`hermes-agent:main-${FONT_SHA}-desktop-overlay-${FONT_PATCH}`;
const TREE='b'.repeat(40);
const fontVariables=`FONT_READABILITY=1; BOT_DESKTOP=1; RELEASE_TAG=v0.22.2; OVERLAY_MODE=1; OVERLAY_SHA256=${FONT_PATCH}; OVERLAY_TREE_SHA=${TREE}; CANDIDATE_SHA=${FONT_SHA}; CANDIDATE_TAG=${FONT_TAG}; FORCE_REBUILD=0; ADOPT_IMAGE_ID=''; RUN_ID=font; export FONT_READABILITY BOT_DESKTOP RELEASE_TAG;`;
const fontOptions={sha:FONT_SHA,overlayLabel:FONT_PATCH,images:{[FONT_TAG]:ID},stamp:{dirty:true,displayVersion:'0.22.2',baseVersion:'0.22.2',overlaySha256:FONT_PATCH,overlayTreeSha:TREE},fontMode:0o644};

test('font admission rejects near-miss source, release, overlay, feature and unselected routes before native commands',async()=>{
 const f=await fixture();
 const good=['prepare','--sha',FONT_SHA,'--bot-desktop','--release-tag','v0.22.2','--font-readability','--overlay-file',join(f.root,'.main-updater/overlays',FONT_PATCH+'.patch'),'--overlay-sha256',FONT_PATCH];
 for(const [field,value] of [['--sha',SHA],['--release-tag','v0.22.1'],['--overlay-sha256','e'.repeat(64)]]){
  const args=[...good];args[args.indexOf(field!)+1]=value!;const out=await f.run(args);expect(out.code).toBe(1);expect(out.stderr).toContain('exact reviewed');
 }
 for(const omit of ['--bot-desktop','--font-readability']){const out=await f.run(good.filter(x=>x!==omit));expect(out.code).toBe(1);}
 expect(await f.calls()).toEqual([]);
 const tag=await f.functions(fontVariables+'\nset_candidate_tag; echo "$CANDIDATE_TAG"');expect(tag.code).toBe(0);expect(tag.stdout.trim()).toBe(FONT_TAG);
 const unselected=await f.functions(fontVariables+'\nFONT_READABILITY=0; set_candidate_tag');expect(unselected.code).toBe(1);
});

test('font candidate requires dirty derived stamp and exact patch/tree while preserving pristine tag',async()=>{
 const f=await fixture(fontOptions);
 expect((await f.functions(fontVariables+'\ncandidate_image_valid')).code).toBe(0);
 for(const stamp of [{dirty:false},{overlaySha256:'f'.repeat(64)},{overlayTreeSha:'f'.repeat(40)},{displayVersion:'0.22.1'}]){
  await f.configure({stamp:{...fontOptions.stamp,...stamp}});expect((await f.functions(fontVariables+'\ncandidate_image_valid')).code).toBe(1);
 }
});

test('same image and bytes with fresh unreadable font modes fails before a diagnostic cache hit',async()=>{
 const f=await fixture(fontOptions);
 const body=fontVariables+`\nvalidation_cache_valid(){ touch "$ROOT/cache-used"; }; validate_runtime_image predeploy`;
 const good=await f.functions(body);expect(good.code).toBe(0);
 await rm(join(f.root,'cache-used'));await f.configure({fontMode:0o600});
 const bad=await f.functions(body);expect(bad.code).toBe(1);expect(await Bun.file(join(f.root,'cache-used')).exists()).toBe(false);
 const runs=(await f.calls()).filter(x=>x[1]==='run');expect(runs.length).toBe(2);
 for(const run of runs){expect(run).toContain(ID);expect(run).toContain('--read-only');expect(run).toContain('10000:10000');expect(run).toContain('none');expect(run).not.toContain('--mount');expect(run).not.toContain('-v');}
 expect((await f.calls()).some(x=>x[1]==='exec')).toBe(false);
});

test('font-derived builder retains Desktop build opt-in, dirty stamp and refuses underlying build failure',async()=>{
 const f=await fixture({...fontOptions,images:{}});const checkout=join(f.root,'checkout');await mkdir(join(checkout,'scripts/releases'),{recursive:true});
 await writeFile(join(checkout,'scripts/releases/distance.py'),"print('0.22.2')\n");
 await writeFile(join(checkout,'scripts/write_install_stamp.py'),"import sys,pathlib,json\npathlib.Path(sys.argv[sys.argv.index('--output')+1]).write_text(json.dumps({'commit':'"+FONT_SHA+"','dirty':True,'baseVersion':'0.22.2','displayVersion':'0.22.2','distance':0}))\n");
 const body=fontVariables+`\nWORKTREE=${shellQuote(checkout)}; build_overlay_candidate`;
 const out=await f.functions(body);expect(out.code).toBe(0);
 const calls=await f.calls(),build=calls.find(x=>x[1]==='buildx')!;expect(build).toContain('HERMES_BOT_DESKTOP=1');expect(build).toContain('io.nousresearch.hermes.bot-desktop=1');expect(build).toContain('io.nousresearch.hermes.overlay.sha256='+FONT_PATCH);
 expect(calls.find(x=>x[2]==='tag')!.at(-1)).toBe(FONT_TAG);
 await f.configure({images:{},binaries:false});const failed=await f.functions(body);expect(failed.code).toBe(1);expect(failed.stderr).toContain('failed label');
});

const fontPatch=await readFile(new URL('../deployments/hermes-font-readability.patch',import.meta.url),'utf8');
test('font prepare emits deploy options that select the exact prepared candidate',async()=>{
 const f=await fixture(fontOptions,`
acquire_lock(){ :; }
prepare_exit_handler(){ :; }
current_facts(){ CURRENT_SHA=${SHA}; CURRENT_TAG=other; }
resolve_candidate_once(){ CANDIDATE_SHA="$REQUESTED_SHA"; CANDIDATE_SOURCE=synthetic; set_candidate_tag; }
start_log(){ :; }
record_phase(){ :; }
check_free_space(){ :; }
resume_recorded_candidate(){ return 0; }
smoke_candidate(){ :; }
validate_runtime_image(){ :; }
write_candidate_state(){ :; }
deploy_candidate(){ CANDIDATE_SHA="$REQUESTED_SHA"; set_candidate_tag; printf 'SELECTED:%s\\n' "$CANDIDATE_TAG"; }
`);
 const overlays=join(f.root,'.main-updater/overlays');await mkdir(overlays,{recursive:true});
 const patchPath=join(overlays,FONT_PATCH+'.patch');await writeFile(patchPath,fontPatch);
 const prepared=await f.run(['prepare','--sha',FONT_SHA,'--bot-desktop','--font-readability','--release-tag','v0.22.2','--overlay-file',patchPath,'--overlay-sha256',FONT_PATCH]);
 expect({code:prepared.code,stderr:prepared.stderr}).toEqual({code:0,stderr:''});
 expect(prepared.stdout).toContain('Prepared candidate '+FONT_TAG);
 const hint=prepared.stdout.split('\n').find(line=>line.startsWith('Deploy with: '));expect(hint).toBeDefined();
 const argv=hint!.slice('Deploy with: '.length).split(/\s+/);
 expect(argv[0]).toBe(f.script);expect(argv[1]).toBe('deploy');
 expect(argv).toContain('--bot-desktop');expect(argv).toContain('--font-readability');
 expect(argv[argv.indexOf('--release-tag')+1]).toBe('v0.22.2');expect(argv).toContain('--approve-deploy');
 const selected=await f.run(argv.slice(1));
 expect({code:selected.code,stderr:selected.stderr,stdout:selected.stdout}).toEqual({code:0,stderr:'',stdout:'SELECTED:'+FONT_TAG+'\n'});
 expect(await f.calls()).toEqual([]);
});

const normalizer=fontPatch.split('\n').filter(x=>x.startsWith('+')&&!x.startsWith('+++')).map(x=>x.slice(1)).join('\n').split('RUN ')[1]!.replaceAll('\\\n','\n');
test('exact font patch changes direct regular assets only and propagates first-directory failure',async()=>{
 expect(new Bun.CryptoHasher('sha256').update(fontPatch).digest('hex')).toBe(FONT_PATCH);
 const f=await fixture();const web=join(f.root,'web');await mkdir(join(web,'fonts/nested'),{recursive:true});await mkdir(join(web,'fonts-terminal'),{recursive:true});
 const names=['fonts/a.woff2','fonts-terminal/b.woff2','fonts/private.txt','fonts/nested/c.woff2','outside.woff2'];
 for(const name of names){await writeFile(join(web,name),'font '+name);await chmod(join(web,name),0o600);}
 await symlink(join(web,'outside.woff2'),join(web,'fonts/link.woff2'));
 const cmd=normalizer.replaceAll('/opt/hermes/hermes_cli/web_dist',web);
 const child=Bun.spawn(['/bin/sh','-c',cmd],{stdout:'pipe',stderr:'pipe'});expect(await child.exited).toBe(0);
 for(const name of names){expect((await lstat(join(web,name))).mode&0o777).toBe(['fonts/a.woff2','fonts-terminal/b.woff2'].includes(name)?0o644:0o600);expect(await readFile(join(web,name),'utf8')).toBe('font '+name);}
 await chmod(join(web,'fonts-terminal/b.woff2'),0o600);const bin=join(f.root,'fail-bin');await mkdir(bin);await writeFile(join(bin,'find'),'#!/bin/sh\nexit 13\n',{mode:0o755});
 const failure=Bun.spawn(['/bin/sh','-c',cmd],{env:{...process.env,PATH:bin+':'+process.env.PATH},stdout:'pipe',stderr:'pipe'});expect(await failure.exited).toBe(13);expect((await lstat(join(web,'fonts-terminal/b.woff2'))).mode&0o777).toBe(0o600);
});


test('font update and background job retain explicit admission and exact source choices',async()=>{
 const f=await fixture(fontOptions,`
prepare_latest(){ CANDIDATE_SHA="$REQUESTED_SHA"; set_candidate_tag; CURRENT_TAG=other; }
remove_worktree(){ :; }
deploy_candidate(){ printf 'FONT_DEPLOY:%s:%s:%s:%s\\n' "$FONT_READABILITY" "$BOT_DESKTOP" "$RELEASE_TAG" "$OVERLAY_SHA256"; }
`);
 const patchPath=join(f.root,'.main-updater/overlays',FONT_PATCH+'.patch');await mkdir(join(f.root,'.main-updater/overlays'),{recursive:true});await writeFile(patchPath,fontPatch);
 const args=['--sha',FONT_SHA,'--bot-desktop','--font-readability','--release-tag','v0.22.2','--overlay-file',patchPath,'--overlay-sha256',FONT_PATCH];
 const job=await f.run(['start','--action','prepare',...args]);expect(job.code).toBe(0);const launch=(await f.calls()).find(x=>x[0]==='systemd-run')!;expect(launch).toContain('--font-readability');expect(launch).toContain('--bot-desktop');expect(launch).toContain(FONT_PATCH);
 const update=await f.run(['update',...args,'--approve-deploy']);expect(update.code).toBe(0);expect(update.stdout).toContain('FONT_DEPLOY:1:1:v0.22.2:'+FONT_PATCH);
 await writeFile(patchPath,fontPatch+'\n');const drift=await f.run(['start','--action','prepare',...args]);expect(drift.code).toBe(1);expect(drift.stderr).toContain('SHA256');
});


test('readonly font verification joins exact image/tree and creates no updater directories or receipts',async()=>{
 const f=await fixture({...fontOptions,images:{...fontOptions.images,[ID]:ID}});const overlays=join(f.root,'.main-updater/overlays');await mkdir(overlays,{recursive:true});const patchPath=join(overlays,FONT_PATCH+'.patch');await writeFile(patchPath,fontPatch);
 const args=['verify-font-readability','--sha',FONT_SHA,'--bot-desktop','--font-readability','--release-tag','v0.22.2','--overlay-file',patchPath,'--overlay-sha256',FONT_PATCH,'--probe-image-id',ID,'--probe-overlay-tree',TREE];
 const good=await f.run(args);expect({code:good.code,stderr:good.stderr}).toMatchObject({code:0});expect(JSON.parse(good.stdout)).toMatchObject({imageId:ID,overlaySha256:FONT_PATCH,overlayTreeSha:TREE,fresh_container:true,uid:10000});
 expect(await readdir(join(f.root,'.main-updater'))).toEqual(['overlays']);
 await f.configure({fontMode:0o600});const bad=await f.run(args);expect(bad.code).toBe(1);expect(bad.stderr).toContain('fresh UID10000');
 const writer=await f.run([...args,'--approve-deploy']);expect(writer.code).toBe(1);expect(writer.stderr).toContain('no writer options');
 expect(await readdir(join(f.root,'.main-updater'))).toEqual(['overlays']);
});
