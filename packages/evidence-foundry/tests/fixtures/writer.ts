import { Foundry, createObject } from "../owner-policy-fixture";
const f=await Foundry.open(Bun.argv[2]!);
const mode=Bun.argv[3];
const object=createObject("Project","multiprocess",{domainId:"research",name:"Writer "+Bun.argv[4]},{id:"worker-"+Bun.argv[4]});
if(mode==="crash-before")await f.store.transaction(()=>[object],{beforePublish:()=>process.exit(137)});
else if(mode==="crash-after")await f.store.transaction(()=>[object],{afterPublish:()=>process.exit(137)});
else await f.putObjects([object]);
f.close();
