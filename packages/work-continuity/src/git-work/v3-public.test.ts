import { test, expect } from "bun:test";
import { join } from "node:path";

test("fresh public package imports keep legacy finite and expose v3 through one explicit canonical facade", async () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const base = join(root, "packages/work-continuity/src/git-work");
  const script = `import { strict as assert } from 'node:assert';
import * as legacy from '@palantirkc/work-continuity/git-work';
import * as v3 from '@palantirkc/work-continuity/git-work/v3';
assert.equal(Bun.resolveSync('@palantirkc/work-continuity/git-work',${JSON.stringify(join(root, "package.json"))}),${JSON.stringify(join(base, "index.ts"))});
assert.equal(Bun.resolveSync('@palantirkc/work-continuity/git-work/v3',${JSON.stringify(join(root, "package.json"))}),${JSON.stringify(join(base, "v3.ts"))});
for(const name of ['canonicalV3','resolveEffectiveContract','allocateGitWorkIdentity','readMigratedGitWorkV3','openGitSourceSnapshot'])assert.equal(name in legacy,false,name);
for(const [name,api] of Object.entries(legacy))assert.equal(v3[name],api,name);
for(const [file,names] of [
 ['v3-codec.ts',['assertV3','canonicalV3','digestV3','encodeV3','decodeV3']],
 ['effective-contract.ts',['resolveEffectiveContract','readGitWorkV3Context','readSourceQualifiedGitWorkV3Context','expandGitWorkV3Context']],
 ['allocation.ts',['allocateGitWorkIdentity','previewGitWorkV3Allocation','prepareV3Allocation']],
 ['legacy-bridge.ts',['readLegacyGitWorkProjection','readVersionedLegacyGitWork','admitLegacyGitWork','recoverLegacyCreateRequest','readMigratedGitWorkV3']],
 ['external-artifacts.ts',['openExactV3SourceReader','qualifySelectedExternalArtifacts','consumerContractBinding']],
 ['source.ts',['openGitSourceSnapshot']],
]){
 const canonical=await import(${JSON.stringify(base)}+'/'+file);
 for(const name of names)assert.equal(v3[name],canonical[name],name);
}
const {fixtureGraph,fixtureTask,fixtureTaskRequest}=await import(${JSON.stringify(join(base, "fixtures/v3-owner.ts"))});
const graph=fixtureGraph(),effective=v3.resolveEffectiveContract(graph,fixtureTask);
assert.deepEqual(v3.decodeV3('Graph',v3.encodeV3('Graph',graph)),graph);
for(const mode of ['full','compact','dispatch','resume']){
 const context=v3.expandGitWorkV3Context(v3.readGitWorkV3Context({graph,task:fixtureTask,mode,expectedDigest:effective.digest}));
 assert.deepEqual(context.effective,effective);assert.equal(context.nativePermissionGranted,false);
}
await assert.rejects(()=>v3.allocateGitWorkIdentity(fixtureTaskRequest(),{},undefined),/NATIVE_OWNER_ADAPTER_REQUIRED/);
console.log(JSON.stringify({legacyPublicImport:true,explicitV3PackageImport:true,singleCanonicalImplementation:true,contextModes:4,nativePermissionGranted:false}));`;
  const child = Bun.spawn(
    [process.execPath, "--no-env-file", "--no-install", "--no-macros", "--no-addons", "-e", script],
    {
      cwd: root,
      env: { ...process.env, BUN_OPTIONS: "", NODE_OPTIONS: "" },
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const stdout = await new Response(child.stdout).text();
  const stderr = await new Response(child.stderr).text();
  expect(await child.exited, stderr).toBe(0);
  expect(JSON.parse(stdout)).toEqual({
    legacyPublicImport: true,
    explicitV3PackageImport: true,
    singleCanonicalImplementation: true,
    contextModes: 4,
    nativePermissionGranted: false,
  });
}, 15_000);
