import { resolve, join } from "node:path";
import { readCurrentEngineeringIdentity } from "./work-identity.ts";
import { loadWorkSourceManifest, currentEngineeringRecordAdmission } from "./work-records.ts";

/** Original owner facades bind their existing root here; canonical code never infers data ownership from its own location. */
export function createLegacyHermesOwnerReaders(ownerRoot: string) {
  if (!ownerRoot || resolve(ownerRoot) !== ownerRoot)
    throw Error("EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED");
  return {
    readCurrentEngineeringIdentity: () => readCurrentEngineeringIdentity(ownerRoot),
    loadWorkSourceManifest: () =>
      loadWorkSourceManifest(
        join(ownerRoot, "architecture/platform-rearchitecture/work-source-manifest.json"),
      ),
    currentEngineeringRecordAdmission: () => currentEngineeringRecordAdmission(ownerRoot),
    sourceOwnerRoot: ownerRoot,
    authority: "none" as const,
    nativePermissionGranted: false as const,
  };
}
