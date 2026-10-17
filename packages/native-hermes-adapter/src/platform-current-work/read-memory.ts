import { readJsonRequest } from "../platform-rearchitecture/input.ts";
import { observeHermesPlatformSource } from "./provider.ts";
import { currentWorkDiagnostic, type CurrentWorkDiagnosticStage } from "./diagnostics.ts";
declare const HERMES_PLATFORM_REGISTRATION_SHA256: string;
declare const HERMES_PLATFORM_PROVIDER_ROOT: string;
declare const HERMES_PLATFORM_REGISTRATION_PATH: string;

let stage: CurrentWorkDiagnosticStage = "input";
try {
  const request = await readJsonRequest();
  if (Object.keys(request).sort().join("|") !== "ownerSelection|requestRef|snapshot")
    throw Error("EXPLICIT_MEMORY_REQUEST_REQUIRED");
  stage = "registration";
  if (
    typeof HERMES_PLATFORM_REGISTRATION_SHA256 !== "string" ||
    typeof HERMES_PLATFORM_PROVIDER_ROOT !== "string" ||
    typeof HERMES_PLATFORM_REGISTRATION_PATH !== "string"
  )
    throw Error("MEMORY_SOURCE_NOT_REGISTERED");
  stage = "source";
  const source = await observeHermesPlatformSource(HERMES_PLATFORM_REGISTRATION_PATH, {
    providerRoot: HERMES_PLATFORM_PROVIDER_ROOT,
    registrationSha256: HERMES_PLATFORM_REGISTRATION_SHA256,
  });
  stage = "implementation";
  const { openGitWork, resolveGitWorkSelection } =
    await import("@palantirkc/work-continuity/git-work");
  const { readGitWorkMemoryRequest } =
    await import("@palantirkc/work-continuity/memory/recall-request");
  stage = "memory";
  const reader = await openGitWork(await resolveGitWorkSelection(request.snapshot));
  const output = await readGitWorkMemoryRequest(
    reader,
    request.requestRef as Parameters<typeof readGitWorkMemoryRequest>[1],
    request.ownerSelection as Parameters<typeof readGitWorkMemoryRequest>[2],
  );
  stage = "source-readback";
  await source.verify();
  console.log(JSON.stringify(output));
} catch {
  console.error(JSON.stringify(currentWorkDiagnostic("MEMORY_INPUT_OR_SOURCE_FAILED", stage)));
  process.exitCode = 1;
}
