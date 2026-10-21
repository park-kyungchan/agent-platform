/** Explicit v3 public API. The historical git-work entry retains its finite legacy closure. */
export * from "./index.ts";
export { openGitSourceSnapshot } from "./source.ts";
export type { TaskRef } from "./generated/v3.ts";
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
export type { ExactV3SourceReader } from "./external-artifacts.ts";
