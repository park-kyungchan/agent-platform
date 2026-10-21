export type * from "./types.ts";
export {
  openGitWork,
  openGitSourceSnapshot,
  resolveGitWorkSelection,
  gitReadEnvironment,
  gitWorkPath,
  gitRecordPath,
  encodedId,
} from "./source.ts";
export {
  validateGitWorkRecords,
  validateGitWorkTransition,
  assertRecord,
  taskVerificationTarget,
  assertExternalArtifactPrerequisite,
  externalPrerequisiteFindings,
} from "./records.ts";
export {
  readGitWorkContext,
  readCompactGitWorkContext,
  compactGitWorkContext,
  decodeCompactGitWorkContext,
  qualifyGitWorkStart,
} from "./context.ts";
export { assertLogicalIdentity } from "../logical-identity.ts";
export { validateTaskOperationalState } from "../operational-state.ts";
export type * as GitWorkV3 from "./generated/v3.ts";
export { canonicalV3, digestV3, assertV3, encodeV3, decodeV3 } from "./v3-codec.ts";
export {
  validateGitWorkV3Graph,
  resolveEffectiveContract,
  readGitWorkV3Context,
  readSourceQualifiedGitWorkV3Context,
  expandGitWorkV3Context,
  v3ContractRef,
  v3ContractDigest,
  v3WorkKey,
  v3TaskKey,
} from "./effective-contract.ts";
export {
  allocateGitWorkIdentity,
  previewGitWorkV3Allocation,
  prepareV3Allocation,
  type NativeGitIdentityOwner,
  type CurrentAllocationAuthority,
  type GitAllocationTarget,
} from "./allocation.ts";

export {
  readLegacyGitWorkProjection,
  readVersionedLegacyGitWork,
  previewLegacyGitWorkMigration,
  admitLegacyGitWork,
  recoverLegacyCreateRequest,
  assertLegacyGitWriterAllowed,
  readMigratedGitWorkV3,
  type NativeLegacyMigrationOwner,
} from "./legacy-bridge.ts";
export { validateGitWorkAliases, resolveGitWorkAlias } from "./aliases.ts";
export {
  openExactV3SourceReader,
  qualifySelectedExternalArtifacts,
  consumerContractBinding,
} from "./external-artifacts.ts";
export type { NativeSourceGitHookPolicy } from "./git-owner-transaction.ts";
