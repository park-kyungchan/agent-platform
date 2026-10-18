import { expect, test } from "bun:test";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { generateHermesSourceFixture } from "./generate.ts";
import {
  applyHermesSourceDelivery,
  prepareHermesSourceDelivery,
  deliveryPaths,
} from "./delivery.ts";
import { fixture as workFixture } from "../../../../ci/quality/harness-scenarios/fixtures/work.ts";
import { hermes } from "../../../../ci/quality/harness-scenarios/fixtures/consumers.ts";

// This integration requires the finite owner QA installation used by the mandatory Source job.
// Ordinary CI-tool negative fixtures run directly in the checkout, without that owner qualification.
async function put(path: string, bytes: Uint8Array | string) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, bytes);
}

test("PT-QC-04 Hermes: partial completion and independent third-state bytes survive source drift and retry refusal", async () => {
  const f = await generateHermesSourceFixture();
  const staging: string[] = [];
  try {
    const target = join(f.fixtureRoot, "oracle-target");
    const prepared = await prepareHermesSourceDelivery(target);
    staging.push(prepared.stagingRoot);
    const partial = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent", 2);
    expect(partial.status).toBe("partial");
    expect(partial.completed.filter((p) => p.kind === "source").map((p) => p.path)).toHaveLength(2);
    const first = await readFile(join(target, deliveryPaths[0]!));
    const thirdPath = join(target, deliveryPaths[2]!);
    await put(thirdPath, "INDEPENDENT-THIRD-STATE");
    const retry = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
    expect(retry.status).toBe("refused");
    expect(retry.completed).toEqual([]);
    expect(await readFile(join(target, deliveryPaths[0]!))).toEqual(first);
    expect(await readFile(thirdPath, "utf8")).toBe("INDEPENDENT-THIRD-STATE");
    const validTarget = join(f.fixtureRoot, "valid-target");
    const validPlan = await prepareHermesSourceDelivery(validTarget);
    staging.push(validPlan.stagingRoot);
    const valid = await applyHermesSourceDelivery(validPlan, validPlan.planSha256, "absent");
    expect(valid.status).toBe("applied");
    expect(valid.nativePermissionGranted).toBe(false);
    const guard = await import(join(validTarget, "src/ontology-driven/provider-source.ts"));
    const proof = (await guard.observeHermesContextProvider(validTarget)) as {
      pins: unknown[];
      verify: () => Promise<void>;
    };
    await proof.verify();
    expect(proof.pins).toHaveLength(19);
    const source = join(
      f.providerRoot,
      "packages/native-hermes-adapter/src/ontology-driven/contracts.ts",
    );
    const consumer = await workFixture();
    try {
      expect(
        (await hermes(consumer.root, consumer.oid, "review", proof.verify).read()).status,
      ).toBe("source-qualified");
      let checks = 0;
      const late = await hermes(consumer.root, consumer.oid, "review", async () => {
        if (++checks === 2) await writeFile(source, "throw Error('MUST-NOT-EXECUTE');");
        await proof.verify();
      }).read();
      expect(checks).toBe(2);
      expect(late.status).toBe("held");
      expect(late.context).toBeNull();
      expect(JSON.stringify(late)).not.toContain("Exact answer");
    } finally {
      await consumer.cleanup();
    }
    expect(
      await proof.verify().then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
    expect(
      (await applyHermesSourceDelivery(validPlan, validPlan.planSha256, "already-applied")).status,
    ).toBe("refused");
    expect(await readFile(thirdPath, "utf8")).toBe("INDEPENDENT-THIRD-STATE");
  } finally {
    for (const path of staging) await rm(path, { recursive: true, force: true });
    await rm(f.fixtureRoot, { recursive: true, force: true });
  }
}, 60000);
