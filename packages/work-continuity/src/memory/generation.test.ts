import { test, expect } from "bun:test";
import ts from "typescript";
import { join } from "node:path";

test("Memory single generator reproduces exact types/validators from Memory and canonical S01 scope schemas",async()=>{
 const root=new URL("../../../../",import.meta.url).pathname;
 const child=Bun.spawn([process.execPath,"--no-env-file","--no-install","--no-macros","--no-addons","packages/ontology/scripts/generate.ts","--memory-v1","--check"],{cwd:root,stdout:"pipe",stderr:"pipe"});
 const out=await new Response(child.stdout).text(),err=await new Response(child.stderr).text();
 expect(await child.exited,err).toBe(0);expect(JSON.parse(out).status).toBe("pass");
},30_000);
test("Memory production, native-owner consumers and Academy source satisfy strict TypeScript",()=>{
 const root=new URL("../../../../",import.meta.url).pathname;
 const config=ts.readConfigFile(join(root,"ci/quality/tsconfig.json"),ts.sys.readFile),parsed=ts.parseJsonConfigFileContent(config.config,ts.sys,root);
 const owned=ts.sys.readDirectory(join(root,"packages/work-continuity/src/memory"),[".ts"],[],["**/*.ts"]);
 const program=ts.createProgram([...owned,join(root,"packages/operational-context/src/academy/context-admission.ts"),join(root,"scripts/harness.ts")],parsed.options);
 const diagnostics=ts.getPreEmitDiagnostics(program).filter(row=>row.category===ts.DiagnosticCategory.Error);
 for(const row of diagnostics.slice(0,32)){const pos=row.file&&row.start!==undefined?row.file.getLineAndCharacterOfPosition(row.start):undefined;console.error(`MEMORY_TYPECHECK_SOURCE ${row.file?.fileName??"unknown"}:${(pos?.line??0)+1}:${(pos?.character??0)+1} TS${row.code}`);}
 expect(diagnostics.length).toBe(0);
},30_000);
