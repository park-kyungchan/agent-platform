import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { assertSourceProjectionAgreement, verifyProjectedSourceQa } from "./source-projection";
import {
  contextProviderRelativeSources,
  gitWorkProviderRelativeSources,
  memoryProviderRelativeSources,
} from "../owner-fixture/codex-harness-engineering/src/native/context-provider";

const common = resolve(import.meta.dir, ".."),
  policy = await Bun.file(common + "/architecture/runtime-filesystem-contract.json").json();
function registrations() {
  const entry = policy.sourceProviderProjections[0];
  const shared = {
    consumerRoot: entry.canonicalConsumerRoot,
    providerRoot: entry.canonicalProviderRoot,
    sourceProjections: structuredClone(policy.sourceProviderProjections),
    authority: "none",
    nativePermissionGranted: false,
  };
  return {
    context: { ...structuredClone(shared), schema: "codex.context-provider-source-closure.v5" },
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
test("current finite source guards prove fictional canonical/physical separation without native admission", async () => {
  const before = await readFile(common + "/project.json");
  const proof = await verifyProjectedSourceQa({
    common,
    ownerFixture: common + "/owner-fixture/codex-harness-engineering",
    contextPaths: [
      ...contextProviderRelativeSources,
      ...gitWorkProviderRelativeSources,
      ...memoryProviderRelativeSources,
    ],
    nativePaths: [
      "package.json",
      "bun.lock",
      "packages/native-codex-adapter/package.json",
      ...["hooks", "home-context", "evaluation-ledger"].map(
        (name) => `packages/native-codex-adapter/src/native/${name}.ts`,
      ),
    ],
    contextModules: [
      "index",
      "types",
      "validation",
      "source",
      "store",
      "context",
      "boundary",
      "sections",
    ],
  });
  expect(proof.valid).toHaveLength(2);
  expect(proof.rejectedCases).toBe(22);
  expect(proof.canonicalOwnerDistinctFromProjectedPhysicalRoot).toBe(true);
  expect(proof.identicalSealedRegistrations).toBe(true);
  expect(proof.nativePermissionGranted).toBe(false);
  expect(proof.actualNativeTaskId).toBeNull();
  expect(proof.actualNativeRunId).toBeNull();
  expect(await readFile(common + "/project.json")).toEqual(before);
}, 15_000);
