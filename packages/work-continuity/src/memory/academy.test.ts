import { test, expect } from "bun:test";
import { academyMemoryPolicy, academyChannels, assessAcademyMemoryAdmission, withAcademyMemoryAdmission, type AcademyContextControls } from "@palantirkc/operational-context/academy/context-admission";
const controls=():AcademyContextControls=>({policyGeneration:"current1",authenticatedReaderId:"synthetic-academy-reader",current:true,unauthorizedBodyObserved:false,quarantinedDerivedArtifacts:false,originalCustodyQualified:true,channels:Object.fromEntries(academyChannels.map(name=>[name,{state:"current-permission-enforced",policyGeneration:"current1",sourceDigest:"1".repeat(64)}])) as AcademyContextControls["channels"]});
test("Academy technical B2 adaptation retains all channel/source/quarantine and actual teacher authority gates",async()=>{
 const current=controls();const assessed=assessAcademyMemoryAdmission(current);
 expect(assessed.status).toBe("ready-for-separate-native-validation");expect(assessed.retrieval).toBe("all-currently-permitted-scopes");expect(assessed.workMeaning).toBe("relevance-context");expect(assessed.teacherAuthority).toBe("actual-teacher-owner-decision");expect(assessed.nativePermissionGranted).toBe(false);
 expect(academyMemoryPolicy.predecessorSha256).toBe("bd1f172e9b046b32fca4e479779021d6cca4975b5fdff0eeee5c0ec8a4b366fe");
 for(const name of academyChannels){const bad=controls();bad.channels[name].state="unverified";expect(assessAcademyMemoryAdmission(bad).holds).toContain("CHANNEL_CONTROL_REQUIRED:"+name);}
 for(const flag of ["unauthorizedBodyObserved","quarantinedDerivedArtifacts"] as const){const bad=controls();bad[flag]=true;expect(assessAcademyMemoryAdmission(bad).holds).toContain("INTERRUPT_QUARANTINE_AND_FRESH_CONVERSATION_REQUIRED");}
 const stale=controls();stale.channels.mcp.policyGeneration="old";expect(assessAcademyMemoryAdmission(stale).status).toBe("held");
 await expect(withAcademyMemoryAdmission("synthetic query",undefined!,value=>value)).rejects.toThrow("CURRENT_NATIVE_OWNER_REQUIRED");
 const result=await withAcademyMemoryAdmission("synthetic query",{async withCurrentControls(_purpose,consume){return consume(controls());}},value=>value);
 expect(result.humanAcceptance).toBe(false);
});
