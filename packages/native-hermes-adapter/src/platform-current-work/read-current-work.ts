import { readJsonRequest } from "../platform-rearchitecture/input.ts";
import { observeHermesPlatformSource } from "./provider.ts";
declare const HERMES_PLATFORM_REGISTRATION_SHA256: string;
declare const HERMES_PLATFORM_PROVIDER_ROOT: string;
declare const HERMES_PLATFORM_REGISTRATION_PATH: string;
try {
  const request = await readJsonRequest();
  if (Object.keys(request).some((key) => !["event", "selection", "runtimeIndex"].includes(key)))
    throw Error("INVALID_CURRENT_WORK_REQUEST");
  if (
    typeof HERMES_PLATFORM_REGISTRATION_SHA256 !== "string" ||
    typeof HERMES_PLATFORM_PROVIDER_ROOT !== "string" ||
    typeof HERMES_PLATFORM_REGISTRATION_PATH !== "string"
  )
    throw Error("CURRENT_WORK_SOURCE_NOT_REGISTERED");
  const source = await observeHermesPlatformSource(HERMES_PLATFORM_REGISTRATION_PATH, {
    providerRoot: HERMES_PLATFORM_PROVIDER_ROOT,
    registrationSha256: HERMES_PLATFORM_REGISTRATION_SHA256,
  });
  const { readHermesCurrentWorkContext } = await import("./context.ts");
  console.log(
    JSON.stringify(
      await readHermesCurrentWorkContext(request.event, request.selection, request.runtimeIndex, {
        verifySourceClosure: source.verify,
      }),
    ),
  );
} catch {
  console.error("CURRENT_WORK_INPUT_OR_SOURCE_FAILED");
  process.exitCode = 1;
}
