import { readJsonRequest } from "../platform-rearchitecture/input.ts";
import { observeHermesPlatformSource, readExactFile, sourceDigest } from "./provider.ts";
declare const HERMES_PLATFORM_REGISTRATION_SHA256: string;
declare const HERMES_PLATFORM_PROVIDER_ROOT: string;
declare const HERMES_PLATFORM_REGISTRATION_PATH: string;
declare const HERMES_CURRENT_WORK_BINDINGS_PATH: string | undefined;
declare const HERMES_CURRENT_WORK_BINDINGS_SHA256: string | undefined;

// Builtin-only bootstrap modules precede the implementation import. Data never chooses executable code.
try {
  const payload = await readJsonRequest();
  if (payload.hook_event_name !== "pre_llm_call") {
    console.log("{}");
  } else {
    if (
      typeof HERMES_PLATFORM_REGISTRATION_SHA256 !== "string" ||
      typeof HERMES_PLATFORM_PROVIDER_ROOT !== "string" ||
      typeof HERMES_PLATFORM_REGISTRATION_PATH !== "string" ||
      typeof HERMES_CURRENT_WORK_BINDINGS_PATH !== "string" ||
      typeof HERMES_CURRENT_WORK_BINDINGS_SHA256 !== "string" ||
      !/^[a-f0-9]{64}$/u.test(HERMES_CURRENT_WORK_BINDINGS_SHA256)
    )
      throw Error("NATIVE_CURRENT_WORK_NOT_REGISTERED");
    const source = await observeHermesPlatformSource(HERMES_PLATFORM_REGISTRATION_PATH, {
      providerRoot: HERMES_PLATFORM_PROVIDER_ROOT,
      registrationSha256: HERMES_PLATFORM_REGISTRATION_SHA256,
    });
    const bytes = await readExactFile(
      HERMES_CURRENT_WORK_BINDINGS_PATH,
      1048576,
      HERMES_CURRENT_WORK_BINDINGS_SHA256,
    );
    const bindings: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    const verify = async () => {
      await source.verify();
      const current = await readExactFile(
        HERMES_CURRENT_WORK_BINDINGS_PATH,
        1048576,
        HERMES_CURRENT_WORK_BINDINGS_SHA256,
      );
      if (sourceDigest(current) !== sourceDigest(bytes))
        throw Error("NATIVE_CURRENT_WORK_BINDINGS_CHANGED");
    };
    await verify();
    const { readHermesNativeWorkHook } = await import("./hook-context.ts");
    await verify();
    const response = await readHermesNativeWorkHook(payload, bindings, {
      verifySourceClosure: verify,
    });
    await verify();
    console.log(JSON.stringify(response));
  }
} catch {
  // Native loops/prompts/transcripts remain native-owned. Failure neither echoes payloads nor requests continuation.
  console.error("NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD");
  console.log("{}");
}
process.exitCode = 0;
