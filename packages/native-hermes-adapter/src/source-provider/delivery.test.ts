import { afterEach, expect, test } from "bun:test";
import { readFile, writeFile, rm, mkdir, symlink, lstat, readdir } from "node:fs/promises";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { generateHermesSourceFixture, modules, sourcePaths, generationPaths } from "./generate.ts";
import {
  applyHermesSourceDelivery,
  prepareHermesSourceDelivery,
  deliveryPaths,
} from "./delivery.ts";
const owned: string[] = [];
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
afterEach(async () => {
  for (const path of owned.splice(0)) await rm(path, { recursive: true, force: true });
});
async function setup() {
  const f = await generateHermesSourceFixture();
  owned.push(f.fixtureRoot);
  const target = join(f.fixtureRoot, "hermes-new-source-owner");
  const prepared = await prepareHermesSourceDelivery(target);
  owned.push(prepared.stagingRoot);
  return { f, target, prepared };
}
test("prepare projects final target identities; absence-only delivery proves19/20 and exact already-applied has no effects", async () => {
  const { target, prepared, f } = await setup();
  expect(await lstat(target).catch(() => null)).toBeNull();
  const plan = JSON.parse(await readFile(join(prepared.stagingRoot, "delivery-plan.json"), "utf8"));
  expect(plan.files).toHaveLength(14);
  expect(plan.targetRoot).toBe(target);
  const registration = JSON.parse(
    await readFile(join(prepared.stagingRoot, "architecture/hermes-context-provider.json"), "utf8"),
  );
  expect(registration.consumerRoot).toBe(target);
  expect(registration.providerRoot).toBe(f.providerRoot);
  expect(await readFile(join(prepared.stagingRoot, "package.json"), "utf8")).toContain(
    '"@palantirkc/hermes-new-source-facade"',
  );
  const receipt = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(receipt.status).toBe("applied");
  expect(receipt.completed.filter((p) => p.kind === "source")).toHaveLength(14);
  expect(receipt.uncertain).toEqual([]);
  const child = Bun.spawn(
    [
      process.execPath,
      "-e",
      `const root=${JSON.stringify(target)};const api=await import(root+'/src/ontology-driven/provider-source.ts');const proof=await api.observeHermesContextProvider(root);await proof.verify();console.log(JSON.stringify({observed:proof.pins.length,consumer:proof.pins.length+1}));`,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [exit, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect({ exit, stderr }).toEqual({ exit: 0, stderr: "" });
  expect(JSON.parse(stdout)).toEqual({ observed: 19, consumer: 20 });
  const already = await applyHermesSourceDelivery(prepared, prepared.planSha256, "already-applied");
  expect(already.status).toBe("already-applied");
  expect(already.completed).toEqual([]);
  expect((await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent")).status).toBe(
    "refused",
  );
  expect(await lstat(join(target, "project.json")).catch(() => null)).toBeNull();
  expect(await lstat(join(target, "state")).catch(() => null)).toBeNull();
}, 60000);
test("staged source and review digest drift refuse before target creation", async () => {
  const { target, prepared } = await setup();
  expect((await applyHermesSourceDelivery(prepared, "0".repeat(64), "absent")).completed).toEqual(
    [],
  );
  await writeFile(join(prepared.stagingRoot, "src/adoption/semantic-context.ts"), "changed");
  const r = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(r.status).toBe("refused");
  expect(r.reason).toBe("DELIVERY_STAGED_SOURCE_CHANGED");
  expect(await lstat(target).catch(() => null)).toBeNull();
}, 60000);
test("canonical source and installed dependency changes refuse before any target writes", async () => {
  const { target, prepared, f } = await setup();
  const source = join(
    f.providerRoot,
    "packages/native-hermes-adapter/src/ontology-driven/contracts.ts",
  );
  const bytes = await readFile(source);
  await writeFile(source, Buffer.concat([bytes, Buffer.from("\n// changed")]));
  const sourceResult = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(sourceResult.reason).toBe("DELIVERY_CANONICAL_SOURCE_DRIFT");
  expect(sourceResult.completed).toEqual([]);
  await writeFile(source, bytes);
  const dependency = join(f.providerRoot, "node_modules/@palantirkc/ontology/dist/index.js");
  await writeFile(dependency, "throw Error('MUST_NOT_IMPORT');\n");
  const depResult = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(depResult.status).toBe("refused");
  expect(depResult.completed).toEqual([]);
  expect(await lstat(target).catch(() => null)).toBeNull();
}, 60000);
test("third-state source/package and unknown target files are preserved and refuse before writes", async () => {
  const { target, prepared } = await setup();
  await mkdir(target);
  await writeFile(join(target, "package.json"), "existing independent dependencies");
  const before = await readFile(join(target, "package.json"));
  const r = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(r.status).toBe("refused");
  expect(r.completed).toEqual([]);
  expect(await readFile(join(target, "package.json"))).toEqual(before);
  await rm(join(target, "package.json"));
  await writeFile(join(target, "unknown.ts"), "throw Error('MUST_NOT_IMPORT');");
  expect((await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent")).reason).toBe(
    "DELIVERY_TARGET_THIRD_STATE",
  );
}, 60000);
test("symlink target and conflicting fixed link refuse without following foreign roots", async () => {
  const { f, target, prepared } = await setup();
  const foreign = join(f.fixtureRoot, "foreign");
  await mkdir(foreign);
  await symlink(foreign, target);
  expect((await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent")).status).toBe(
    "refused",
  );
  expect(await readdir(foreign)).toEqual([]);
  await rm(target);
  await mkdir(join(target, "node_modules/@palantirkc"), { recursive: true });
  await symlink(foreign, join(target, "node_modules/@palantirkc/native-hermes-adapter"));
  const r = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(r.reason).toBe("DELIVERY_LINK_CONFLICT");
  expect(r.completed).toEqual([]);
}, 60000);
test("preexisting owner data stays byte-exact and is never source attestation", async () => {
  const { target, prepared } = await setup();
  await mkdir(join(target, "state"), { recursive: true });
  const data = Buffer.from("opaque existing data, not interpreted");
  await writeFile(join(target, "project.json"), data);
  await writeFile(join(target, "state/next-session.json"), data);
  const r = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(r.status).toBe("applied");
  for (const p of ["project.json", "state/next-session.json"])
    expect(await readFile(join(target, p))).toEqual(data);
  expect(r.originalSourceRestored).toBe(false);
  expect(r.nativePermissionGranted).toBe(false);
}, 60000);
test("bounded partial delivery reports completed bytes; no rollback, replacement or automatic retry", async () => {
  const { target, prepared } = await setup();
  const partial = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent", 2);
  expect(partial.status).toBe("partial");
  expect(partial.reason).toBe("DELIVERY_WRITE_CEILING_REACHED");
  expect(partial.completed.filter((p) => p.kind === "source")).toHaveLength(2);
  const first = await readFile(join(target, deliveryPaths[0]!));
  const retry = await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent");
  expect(retry.status).toBe("refused");
  expect(retry.completed).toEqual([]);
  expect(await readFile(join(target, deliveryPaths[0]!))).toEqual(first);
  expect(
    (await applyHermesSourceDelivery(prepared, prepared.planSha256, "already-applied")).status,
  ).toBe("refused");
}, 60000);
test("modified already-applied bytes cannot qualify even with unchanged plan digest", async () => {
  const { target, prepared } = await setup();
  expect((await applyHermesSourceDelivery(prepared, prepared.planSha256, "absent")).status).toBe(
    "applied",
  );
  await writeFile(join(target, "bun.lock"), "third state");
  const r = await applyHermesSourceDelivery(prepared, prepared.planSha256, "already-applied");
  expect(r.status).toBe("refused");
  expect(r.completed).toEqual([]);
}, 60000);

test("public finite source and target denominators cannot be mutated in-process", () => {
  for (const paths of [modules, sourcePaths, generationPaths, deliveryPaths]) {
    expect(Object.isFrozen(paths)).toBe(true);
    expect(Reflect.set(paths, 0, "../MUST_NOT_ADMIT")).toBe(false);
  }
});
