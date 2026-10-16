import { readJsonRequest } from "../platform-rearchitecture/input.ts";
import { observeHermesPlatformSource } from "./provider.ts";
import { currentWorkDiagnostic, type CurrentWorkDiagnosticStage } from "./diagnostics.ts";
declare const HERMES_PLATFORM_REGISTRATION_SHA256: string;
declare const HERMES_PLATFORM_PROVIDER_ROOT: string;
declare const HERMES_PLATFORM_REGISTRATION_PATH: string;
let stage: CurrentWorkDiagnosticStage = "input";
try {
  const request = await readJsonRequest();
  if (
    Object.keys(request).some(
      (key) => !["event", "selection", "runtimeIndex", "outputVersion", "maxChars"].includes(key),
    )
  )
    throw Error("INVALID_CURRENT_WORK_REQUEST");
  if (
    request.outputVersion !== undefined &&
    request.outputVersion !== "v1" &&
    request.outputVersion !== "v2"
  )
    throw Error("EXPLICIT_CURRENT_WORK_OUTPUT_VERSION_REQUIRED");
  if (request.maxChars !== undefined && typeof request.maxChars !== "number")
    throw Error("EXPLICIT_CURRENT_WORK_OUTPUT_BUDGET_REQUIRED");
  stage = "registration";
  if (
    typeof HERMES_PLATFORM_REGISTRATION_SHA256 !== "string" ||
    typeof HERMES_PLATFORM_PROVIDER_ROOT !== "string" ||
    typeof HERMES_PLATFORM_REGISTRATION_PATH !== "string"
  )
    throw Error("CURRENT_WORK_SOURCE_NOT_REGISTERED");
  stage = "source";
  const source = await observeHermesPlatformSource(HERMES_PLATFORM_REGISTRATION_PATH, {
    providerRoot: HERMES_PLATFORM_PROVIDER_ROOT,
    registrationSha256: HERMES_PLATFORM_REGISTRATION_SHA256,
  });
  stage = "implementation";
  const { readHermesCurrentWorkContext, readHermesCurrentWorkContextV2 } =
    await import("./context.ts");
  const read =
    request.outputVersion === "v2" ? readHermesCurrentWorkContextV2 : readHermesCurrentWorkContext;
  stage = "context";
  console.log(
    JSON.stringify(
      await read(request.event, request.selection, request.runtimeIndex, {
        maxChars: request.maxChars as number | undefined,
        verifySourceClosure: async () => {
          const previous = stage;
          stage = "source-readback";
          await source.verify();
          stage = previous;
        },
      }),
    ),
  );
} catch {
  console.error(
    JSON.stringify(currentWorkDiagnostic("CURRENT_WORK_INPUT_OR_SOURCE_FAILED", stage)),
  );
  process.exitCode = 1;
}
