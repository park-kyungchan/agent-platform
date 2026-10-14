import {
  readCurrentWork,
  readRegisteredWorkContinuity,
  readExactFile,
  configuredNativeManifestPin,
  currentWorkContextMaxChars,
} from "./work-reader";
import {
  projectWorkContinuityData,
  recordWorkBoundary,
  type WorkBoundaryRequest,
} from "./work-continuity";
import { readReview } from "./review-reader";

import { InputError, readJsonRequest } from "./input.ts";

async function main() {
  const args = Bun.argv.slice(2),
    mode = args[0];
  const flag = (name: string) => {
    const index = args.indexOf(name);
    return index < 0 ? undefined : args[index + 1];
  };
  const manifest =
    flag("--manifest") ??
    "/opt/data/workspace/hermes-harness-audit/platform/current/current-work.json";
  const profile = flag("--profile") ?? process.env.HERMES_PROFILE;
  if (!profile) throw Error("Explicit native profile required");
  if (mode === "context")
    console.log(
      JSON.stringify(
        await readCurrentWork(
          manifest,
          profile,
          flag("--session"),
          currentWorkContextMaxChars,
          configuredNativeManifestPin(),
        ),
        null,
        2,
      ),
    );
  else if (mode === "continuity-read" || mode === "continuity-record") {
    // Reject invalid stdin before reading the registered owner. Valid requests still
    // recheck the exact manifest and use the existing canonical CAS implementation.
    const request =
      mode === "continuity-record"
        ? ((await readJsonRequest()) as unknown as WorkBoundaryRequest)
        : undefined;
    if (
      request &&
      (Object.keys(request).some(
        (k) => !["expected", "expectedDigest", "boundary", "core"].includes(k),
      ) ||
        !request.core)
    )
      throw Error("Continuity record requires expected, expectedDigest, boundary and core only");
    const owner = await readRegisteredWorkContinuity(
      manifest,
      profile,
      configuredNativeManifestPin(),
    );
    if (mode === "continuity-read")
      console.log(
        JSON.stringify(
          { manifestPin: owner.manifestPin, ...projectWorkContinuityData(owner.view) },
          null,
          2,
        ),
      );
    else {
      await readExactFile(manifest, 1048576, owner.manifestPin.sha256);
      const result = await recordWorkBoundary(owner.registration, request!);
      console.log(
        JSON.stringify(
          result.status === "effect-unknown-reconcile-before-retry"
            ? {
                ...result,
                authority: "none",
                nativePermissionGranted: false,
                executionReady: false,
              }
            : {
                status: result.status,
                manifestPin: owner.manifestPin,
                ...projectWorkContinuityData(result.view),
              },
          null,
          2,
        ),
      );
    }
  } else if (mode === "source-review" || mode === "ocp-review") {
    const input = await readJsonRequest();
    if (input.mode !== (mode === "source-review" ? "pinned-source" : "supplied-evidence"))
      throw Error("Review route/evidence mode mismatch");
    console.log(
      JSON.stringify(await readReview(manifest, profile, input, configuredNativeManifestPin())),
    );
  } else
    throw Error(
      "Use context|continuity-read|continuity-record|source-review|ocp-review --profile <native-handle>",
    );
}

// A lower-level owner/manifest failure may include caller-controlled bytes too.
// Do not print its message, stack or source excerpt. Unknown-effect results above
// remain successful structured receipts and must be reconciled before any retry.
try {
  await main();
} catch (error) {
  console.error(error instanceof InputError ? error.code : "CONTEXT_COMMAND_FAILED");
  process.exitCode = 1;
}
