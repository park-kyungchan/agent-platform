import { readJsonRequest } from "../platform-rearchitecture/input.ts";
import { observeHermesPlatformSource } from "./provider.ts";
import { currentWorkDiagnostic, type CurrentWorkDiagnosticStage } from "./diagnostics.ts";

declare const HERMES_PLATFORM_REGISTRATION_SHA256: string;
declare const HERMES_PLATFORM_PROVIDER_ROOT: string;
declare const HERMES_PLATFORM_REGISTRATION_PATH: string;

// Static compiled owner constants only. Request data never chooses a module, root or registration.
if (import.meta.main) {
  let stage: CurrentWorkDiagnosticStage = "input";
  try {
    const request = await readJsonRequest();
    stage = "registration";
    if (
      typeof HERMES_PLATFORM_REGISTRATION_SHA256 !== "string" ||
      !/^[a-f0-9]{64}$/u.test(HERMES_PLATFORM_REGISTRATION_SHA256) ||
      typeof HERMES_PLATFORM_PROVIDER_ROOT !== "string" ||
      typeof HERMES_PLATFORM_REGISTRATION_PATH !== "string"
    )
      throw Error("WORK_METHOD_SOURCE_NOT_REGISTERED");
    stage = "source";
    const source = await observeHermesPlatformSource(HERMES_PLATFORM_REGISTRATION_PATH, {
      providerRoot: HERMES_PLATFORM_PROVIDER_ROOT,
      registrationSha256: HERMES_PLATFORM_REGISTRATION_SHA256,
    });
    await source.verify();
    stage = "implementation";
    const { readHermesWorkMethod } = await import("./work-method.ts");
    stage = "source-readback";
    await source.verify();
    stage = "context";
    const response = await readHermesWorkMethod(request, {
      verifySourceClosure: async () => {
        stage = "source-readback";
        await source.verify();
        stage = "context";
      },
    });
    stage = "source-readback";
    await source.verify();
    console.log(JSON.stringify(response));
    process.exitCode = response.status === "held" ? 2 : 0;
  } catch {
    // Existing closed diagnostics: no exception, request, native identity or source bytes escape.
    console.error(
      JSON.stringify(currentWorkDiagnostic("CURRENT_WORK_INPUT_OR_SOURCE_FAILED", stage)),
    );
    console.log(
      JSON.stringify({
        schema: "hermes.work-method-owner-reader-held.v1",
        status: "held",
        holdReasons: ["WORK_METHOD_INPUT_OR_SOURCE_HELD"],
        nativeAuthenticationEstablished: false,
        nativePermissionGranted: false,
        executionAuthorityGranted: false,
      }),
    );
    process.exitCode = 1;
  }
}
