import { randomBytes, timingSafeEqual } from "node:crypto";
import { readFile, open, link, unlink } from "node:fs/promises";
import { join } from "node:path";
import type { Foundry, ActionRequest, CaptureInput, QueryOptions } from "./service";
export async function ownerCredential(root:string) {
  const path=join(root,"owner-credential");
  const temporary=path+".pending-"+crypto.randomUUID();
  const fd=await open(temporary,"wx",0o600);
  try{await fd.writeFile(randomBytes(32).toString("hex"));await fd.sync();}finally{await fd.close();}
  try{await link(temporary,path);}catch(error){if((error as NodeJS.ErrnoException).code!=="EEXIST")throw error;}finally{await unlink(temporary);}
  const credential=(await readFile(path,"utf8")).trim();
  if(!/^[a-f0-9]{64}$/.test(credential))throw new Error("Invalid owner credential; remove damaged owner-credential to regenerate");
  return credential;
}
export async function serve(foundry:Foundry,port=0) {
  const credential=await ownerCredential(foundry.store.root);
  const routes:Record<string,(body:any)=>Promise<unknown>>={
    query:body=>foundry.queryEvidence(body as QueryOptions),
    ingestCapture:body=>foundry.ingestCapture(body as CaptureInput),
    traverseLinks:body=>foundry.traverseLinks(body.id,body.depth??1),
    getProvenance:body=>foundry.getProvenance(body.id),
    runAction:body=>foundry.runAction(body.request as ActionRequest,body.options),
    runSuite:body=>foundry.runSuite(body.id,body.options),
    verifyClaim:body=>foundry.verifyClaim(body.id,body.options),
  };
  const server=Bun.serve({hostname:"127.0.0.1",port,async fetch(request) {
    const supplied=request.headers.get("authorization")?.replace(/^Bearer /,"")??"";
    if(Buffer.byteLength(supplied)!==Buffer.byteLength(credential)||!timingSafeEqual(Buffer.from(supplied),Buffer.from(credential)))return Response.json({error:"Unauthorized"},{status:401});
    const route=new URL(request.url).pathname.slice(1);if(request.method!=="POST"||!routes[route])return Response.json({error:"Unknown operation"},{status:404});
    try {return Response.json(await routes[route]!(await request.json()));}catch(error){return Response.json({error:String(error)},{status:400});}
  }});return server;
}
