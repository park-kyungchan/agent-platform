/** Explicit versioned API. Do not export through historical finite facades. */
export type {
  SourceContentBinding,
  SourceLocationBinding,
  SourceFilePin,
  SourceFileClass,
  SourceInstalledSelection,
  SourceIdentity,
  SourceProviderRegistration,
  SourceProviderId,
  OriginalSourceRecovery,
  SourceBindingInput,
} from "./source-contract.ts";
export {
  normalizeContent,
  normalizeLocation,
  normalizeProvider,
  normalizeRecovery,
  normalizeBinding,
  contentBytes,
  contentDigest,
  locationBytes,
  locationDigest,
  providerBytes,
  providerDigest,
  recoveryDigest,
  sourceHash,
  sourceLimits,
  sourceInstalledInventoryDigest,
} from "./source-contract.ts";
export {
  materializeSourceDistribution,
  type NativeSourceDistributionOwner,
  type SourceSnapshotLease,
} from "./source-distribution.ts";
export {
  withQualifiedSource,
  observeSourceProcess,
  type NativeSourceReadOwner,
  type NativeSourceReadLease,
  type NativeSourceSelection,
  type QualifiedSourceView,
} from "./source-runtime.ts";
export {
  prepareSourceProviderSuccessors,
  type ExistingProviderSource,
} from "./source-successors.ts";
