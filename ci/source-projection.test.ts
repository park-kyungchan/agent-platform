import { expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import {
  assertSourceProjectionAgreement,
  contextProjectionPaths,
  verifyProjectedSourceQa,
  workMethodProjectionPath,
  type ProjectionContextVersion,
} from "./source-projection";

const common = resolve(import.meta.dir, ".."),
  policy = await Bun.file(common + "/architecture/runtime-filesystem-contract.json").json();
function registrations(version: ProjectionContextVersion = "v5") {
  const entry = policy.sourceProviderProjections[0];
  const shared = {
    consumerRoot: entry.canonicalConsumerRoot,
    providerRoot: entry.canonicalProviderRoot,
    sourceProjections: structuredClone(policy.sourceProviderProjections),
    authority: "none",
    nativePermissionGranted: false,
  };
  return {
    context: {
      ...structuredClone(shared),
      schema: "codex.context-provider-source-closure." + version,
      sourcePins: contextProjectionPaths(version).map((path) => ({ path, sha256: "0".repeat(64) })),
    },
    native: { ...structuredClone(shared), schema: "codex.native-adapter-source-closure.v2" },
  };
}
test("both registrations must independently agree with the exact current common source policy", () => {
  const { context, native } = registrations();
  expect(() => assertSourceProjectionAgreement(policy, context, native)).not.toThrow();
  for (const kind of ["context", "native"] as const) {
    const rows = registrations();
    rows[kind].sourceProjections[0]!.providerRoot = "/foreign/agent-platform";
    expect(() => assertSourceProjectionAgreement(policy, rows.context, rows.native)).toThrow();
    const owner = registrations();
    owner[kind].providerRoot = "/foreign/agent-platform";
    expect(() => assertSourceProjectionAgreement(policy, owner.context, owner.native)).toThrow();
    const authority = registrations();
    authority[kind].nativePermissionGranted = true;
    expect(() =>
      assertSourceProjectionAgreement(policy, authority.context, authority.native),
    ).toThrow();
  }
});
test("v6 agreement requires the exact additive source graph without relabeling v5", () => {
  const current = registrations("v6");
  expect(() =>
    assertSourceProjectionAgreement(policy, current.context, current.native),
  ).not.toThrow();
  expect(contextProjectionPaths("v5")).toHaveLength(25);
  expect(contextProjectionPaths("v6")).toHaveLength(26);
  for (const mutate of [
    (value: typeof current.context) => {
      value.sourcePins.pop();
    },
    (value: typeof current.context) => {
      value.sourcePins.at(-1)!.path = "packages/work-continuity/src/foreign.ts";
    },
    (value: typeof current.context) => {
      value.sourcePins.at(-1)!.path = value.sourcePins[0]!.path;
    },
    (value: typeof current.context) => {
      value.sourcePins.at(-1)!.sha256 = "unknown";
    },
    (value: typeof current.context) => {
      value.schema = "codex.context-provider-source-closure.v5";
    },
  ]) {
    const rows = registrations("v6");
    mutate(rows.context);
    expect(() => assertSourceProjectionAgreement(policy, rows.context, rows.native)).toThrow();
  }
});
const nativePaths = [
  "package.json",
  "bun.lock",
  "packages/native-codex-adapter/package.json",
  ...["hooks", "home-context", "evaluation-ledger"].map(
    (name) => `packages/native-codex-adapter/src/native/${name}.ts`,
  ),
];
const contextModules = [
  "index",
  "types",
  "validation",
  "source",
  "store",
  "context",
  "boundary",
  "sections",
];
test("version and finite source graph are explicit before any QA source copy", async () => {
  const base = {
    common,
    ownerFixture: common + "/owner-fixture/codex-harness-engineering",
    nativePaths,
    contextModules,
  };
  await expect(
    verifyProjectedSourceQa({ ...base, contextPaths: contextProjectionPaths("v6") }),
  ).rejects.toThrow("Exact versioned QA context source graph");
  await expect(
    verifyProjectedSourceQa({
      ...base,
      contextVersion: "v6",
      contextPaths: contextProjectionPaths("v5"),
    }),
  ).rejects.toThrow("Exact versioned QA context source graph");
  await expect(
    verifyProjectedSourceQa({
      ...base,
      contextVersion: "v6",
      contextPaths: contextProjectionPaths("v6"),
      contextModules: [...contextModules, "foreign"],
    }),
  ).rejects.toThrow("Exact finite QA context module graph");
  await expect(
    verifyProjectedSourceQa({
      ...base,
      contextVersion: "v6",
      contextPaths: contextProjectionPaths("v6"),
      nativePaths: [...nativePaths, "foreign.ts"],
    }),
  ).rejects.toThrow("Exact finite QA native source graph");
});
test("current finite source guards prove fictional canonical/physical separation without native admission", async () => {
  const before = await readFile(common + "/project.json");
  const proof = await verifyProjectedSourceQa({
    common,
    ownerFixture: common + "/owner-fixture/codex-harness-engineering",
    contextPaths: contextProjectionPaths("v6"),
    nativePaths,
    contextModules,
    contextVersion: "v6",
  });
  expect(proof.valid).toHaveLength(2);
  expect(proof.contextVersion).toBe("v6");
  expect(proof.rejectedCases).toBe(26);
  expect(proof.eagerV3ExportRegressionRejected).toBe(true);
  expect(
    proof.valid.every((value) => value.legacyPublicPackageAvailable && value.v3SourceAbsent),
  ).toBe(true);
  expect(
    proof.valid.every((value) => value.workMethodAvailable === true && value.contextPins === 35),
  ).toBe(true);
  expect(proof.canonicalOwnerDistinctFromProjectedPhysicalRoot).toBe(true);
  expect(proof.identicalSealedRegistrations).toBe(true);
  expect(proof.nativePermissionGranted).toBe(false);
  expect(proof.actualNativeTaskId).toBeNull();
  expect(proof.actualNativeRunId).toBeNull();
  expect(await readFile(common + "/project.json")).toEqual(before);
}, 15_000);
test("legacy v5 projection keeps its own source graph and guard semantics", async () => {
  const legacy = await mkdtemp(join(tmpdir(), "common-projection-legacy-source-"));
  try {
    for (const path of new Set([
      ...contextProjectionPaths("v5"),
      ...nativePaths,
      "architecture/runtime-filesystem-contract.json",
    ])) {
      const target = join(legacy, path);
      await mkdir(dirname(target), { recursive: true });
      await writeFile(target, await readFile(join(common, path)));
    }
    const index = join(legacy, "packages/work-continuity/src/context/index.ts"),
      current = await readFile(index, "utf8");
    const exports = [
      ...current.matchAll(/^export \* from ["']\.\.\/work-method(?:\.ts)?["'];\r?\n/gm),
    ];
    expect(exports).toHaveLength(1);
    // A named historical source graph fixture, never a native/current-source claim.
    await writeFile(index, current.replace(exports[0]![0], ""));
    await symlink(join(common, "node_modules"), join(legacy, "node_modules"));
    const proof = await verifyProjectedSourceQa({
      common: legacy,
      ownerFixture: common + "/owner-fixture/codex-harness-engineering",
      contextPaths: contextProjectionPaths("v5"),
      nativePaths,
      contextModules,
    });
    expect(proof.contextVersion).toBe("v5");
    expect(proof.rejectedCases).toBe(22);
    expect(proof.eagerV3ExportRegressionRejected).toBe(true);
    expect(
      proof.valid.every((value) => value.legacyPublicPackageAvailable && value.v3SourceAbsent),
    ).toBe(true);
    expect(
      proof.valid.every((value) => value.workMethodAvailable === false && value.contextPins === 34),
    ).toBe(true);
    expect(await Bun.file(join(legacy, workMethodProjectionPath)).exists()).toBe(false);
  } finally {
    await rm(legacy, { recursive: true, force: true });
  }
}, 15_000);
