/** Operational defaults, scaled once from b308033. Never scale a caller's supplied budget. */
export const operationalTimeouts = {
  nativeCommandMs: 450_000, // 90_000
  networkMs: 300_000, // 60_000
  processMs: 600_000, // 120_000
  sourceRuntimeMs: 600_000, // 120_000; preparation only, not the continued source job
  sourceTestMs: 600_000, // 120_000 per test
  // Old 360_000 was below a 600_000 test; now 3_000_000 + 60_000 completion drain.
  qualityFixturesMs: 3_060_000,
  reviewPublicationMs: 300_000, // 60_000; protected-main absence only
  frozenInstallMs: 3_000_000, // 600_000
  sourceFetchMs: 900_000, // 180_000
  workerMs: 4_500_000, // 900_000 aggregate for one worker occurrence
  scenarioCommandMs: 50_000, // 10_000
  behaviorCallMs: 75_000, // 15_000
  secretScanSeconds: 300, // 60; the outer nativeCommandMs budget remains larger
} as const;
