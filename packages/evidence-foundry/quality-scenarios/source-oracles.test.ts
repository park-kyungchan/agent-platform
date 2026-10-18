import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { generateHermesSourceFixture } from "../../native-hermes-adapter/src/source-provider/generate.ts";
import {
  applyHermesSourceDelivery,
  prepareHermesSourceDelivery,
  deliveryPaths,
} from "../../native-hermes-adapter/src/source-provider/delivery.ts";

import { fixture as workFixture } from "./fixtures/work.ts";
import { hermes } from "./fixtures/consumers.ts";

const repository = resolve(import.meta.dir, "../../..");
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
async function put(path: string, bytes: Uint8Array | string) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, bytes);
}

// Reuse the exact reviewed owner facade/guard bytes. Only the bounded implementation payload is fictional.
async function codexClosureFixture() {
  const root = await mkdtemp(join(tmpdir(), "pt10-codex-source-oracle-"));
  const owner = join(root, "codex-harness-engineering"),
    common = join(root, "agent-platform"),
    pkg = join(common, "packages/native-codex-adapter");
  const names = ["hooks", "home-context", "evaluation-ledger"];
  for (const name of ["provider-closure", "hook-cli", ...names])
    await put(
      join(owner, `src/native/${name}.ts`),
      await readFile(
        join(repository, `owner-fixture/codex-harness-engineering/src/native/${name}.ts`),
      ),
    );
  await put(
    join(owner, "src/legacy/harness/owner-context-loader.ts"),
    "export const loadCodexOwnerContext=()=>({});",
  );
  await put(join(owner, "src/legacy/harness/provider-closure.ts"), "export const guard = true;");
  await put(
    join(owner, "package.json"),
    JSON.stringify({
      type: "module",
      dependencies: {
        "@palantirkc/native-codex-adapter": "file:../agent-platform/packages/native-codex-adapter",
      },
    }),
  );
  await put(join(owner, "bun.lock"), "fictional-lock");
  await put(join(common, "package.json"), "{}");
  await put(join(common, "bun.lock"), "fictional-lock");
  await put(
    join(pkg, "package.json"),
    JSON.stringify({
      name: "@palantirkc/native-codex-adapter",
      type: "module",
      exports: Object.fromEntries(names.map((n) => [`./${n}`, `./src/native/${n}.ts`])),
      dependencies: { "@palantirkc/work-continuity": "file:../work-continuity" },
    }),
  );
  await mkdir(join(owner, "node_modules/@palantirkc"), { recursive: true });
  await symlink(pkg, join(owner, "node_modules/@palantirkc/native-codex-adapter"));
  const marker = join(root, "executed"),
    canary = join(root, "canary"),
    late = join(root, "inject-late-drift");
  const watched = join(pkg, "src/native/home-context.ts");
  await put(watched, "export const home=true;");
  await put(join(pkg, "src/native/evaluation-ledger.ts"), "export const ledger=true;");
  await put(
    join(pkg, "src/native/hooks.ts"),
    `import {existsSync,writeFileSync} from 'node:fs';
writeFileSync(${JSON.stringify(marker)},'executed');
export const readHookBindings=()=>({});
export async function handleHook(){await Promise.resolve();if(existsSync(${JSON.stringify(late)}))writeFileSync(${JSON.stringify(watched)},'late drift');return {stdout:'PRIVATE-COMPLETED-PAYLOAD'};}`,
  );
  const paths = [
    "package.json",
    "bun.lock",
    "packages/native-codex-adapter/package.json",
    ...names.map((n) => `packages/native-codex-adapter/src/native/${n}.ts`),
  ];
  const registration =
    JSON.stringify({
      schema: "codex.native-adapter-source-closure.v1",
      consumerRoot: owner,
      providerRoot: common,
      packageName: "@palantirkc/native-codex-adapter",
      sourcePins: await Promise.all(
        paths.map(async (path) => ({ path, sha256: sha(await readFile(join(common, path))) })),
      ),
      authority: "none",
      nativePermissionGranted: false,
    }) + "\n";
  await put(join(owner, "architecture/native-codex-provider.json"), registration);
  const wrapper = join(owner, "src/native/hooks.ts");
  await put(
    wrapper,
    (await readFile(wrapper, "utf8")).replace(
      /codex-native-provider-registration-sha256: [a-f0-9]{64}/,
      `codex-native-provider-registration-sha256: ${sha(registration)}`,
    ),
  );
  const bindings = join(owner, "bindings.json");
  await put(bindings, "{}");
  const invoke = async () => {
    const child = Bun.spawn(
      [process.execPath, join(owner, "src/native/hook-cli.ts"), "--bindings", bindings],
      {
        stdin: new Blob(['{"hook_event_name":"SessionStart","private":"PRIVATE-EVENT"}']),
        stdout: "pipe",
        stderr: "pipe",
        env: { ...process.env, BUN_OPTIONS: "" },
      },
    );
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect({ code, stderr }).toEqual({ code: 0, stderr: "" });
    return stdout;
  };
  return { root, marker, canary, late, watched, invoke };
}

test("PT-QC-04 Codex: valid output, late drift suppression and first-import canary refusal use the reviewed owner facade", async () => {
  const f = await codexClosureFixture();
  try {
    const valid = await f.invoke();
    expect(valid).toContain("PRIVATE-COMPLETED-PAYLOAD");
    await writeFile(f.late, "inject");
    const late = await f.invoke();
    expect(late).toContain("context held");
    expect(late).not.toContain("PRIVATE-COMPLETED-PAYLOAD");
    expect(late).not.toContain("PRIVATE-EVENT");
    expect(late).not.toContain(f.root);
    await rm(f.marker);
    await writeFile(
      f.watched,
      `import {writeFileSync} from 'node:fs';writeFileSync(${JSON.stringify(f.canary)},'unsafe');`,
    );
    expect(await f.invoke()).toContain("context held");
    expect(await Bun.file(f.marker).exists()).toBe(false);
    expect(await Bun.file(f.canary).exists()).toBe(false);
  } finally {
    await rm(f.root, { recursive: true, force: true });
  }
}, 30000);

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
