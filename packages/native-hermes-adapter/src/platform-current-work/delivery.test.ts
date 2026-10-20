import { afterEach, beforeAll, expect, test } from "bun:test";
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createHash } from "node:crypto";
import {
  deliverHermesNamespace,
  namespacePayloadSha256,
  type NamespaceDeliveryContract,
  type NamespaceDeliveryFile,
  type NamespaceDeliveryPayload,
} from "./delivery.ts";
import { platformEntryPoints } from "./provider.ts";

const roots: string[] = [];
const hash = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
let compiler: NamespaceDeliveryContract["compiler"];
beforeAll(async () => {
  const path = await realpath(process.execPath),
    bytes = await readFile(path);
  compiler = { path, bytes: bytes.length, sha256: hash(bytes) };
});
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});
function file(path: string, text: string): NamespaceDeliveryFile {
  const bytes = Buffer.from(text);
  return {
    path,
    bytes: bytes.length,
    sha256: hash(bytes),
    base64: bytes.toString("base64"),
    mode: 0o600,
  };
}
async function fixture() {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "hermes-namespace-fictional-")));
  roots.push(parent);
  const legacy = join(parent, "fictional-legacy.json"),
    namespace = join(parent, "new-source");
  await writeFile(legacy, "fictional legacy bytes\n", { mode: 0o600 });
  const source = file("agent-platform/src/fictional.ts", "export const fictional = true;\n");
  const depRoot = "node_modules/.bun/fictional@1/node_modules/fictional";
  const dep = file(
    "agent-platform/" + depRoot + "/package.json",
    '{"name":"fictional","version":"1"}\n',
  );
  const pin = (file: NamespaceDeliveryFile) => ({
    path: file.path.slice("agent-platform/".length),
    bytes: file.bytes,
    sha256: file.sha256,
  });
  const registration = file(
    "platform-source-registration.json",
    JSON.stringify({
      schema: "hermes.platform-source-provider.v2",
      providerRoot: join(namespace, "agent-platform"),
      packageName: "@palantirkc/native-hermes-adapter",
      entrypoints: platformEntryPoints,
      sourcePins: [pin(source)],
      dependencyPins: [pin(dep)],
      dependencies: [{ name: "fictional", version: "1", root: depRoot }],
      authority: "none",
      nativePermissionGranted: false,
    }) + "\n",
  );
  const bundles = Object.keys(platformEntryPoints).map((name) =>
    file("bin/" + name, "// fictional bundle " + name + "\n"),
  );
  const payload: NamespaceDeliveryPayload = {
    schema: "hermes.namespace-delivery-payload.v1",
    provenance: {
      sourceCommit: "a".repeat(40),
      sourceRegistrationSha256: hash("fictional host registration"),
      runtimeRegistrationSha256: registration.sha256,
      compilerSha256: compiler.sha256,
      bundles: bundles.map((b) => ({ name: b.path.slice(4), bytes: b.bytes, sha256: b.sha256 })),
    },
    files: [
      source,
      dep,
      registration,
      ...bundles,
      file("data/selected-original.json", '{"fictional":true}\n'),
    ],
    aliases: [
      { path: "agent-platform/node_modules/fictional", target: "agent-platform/" + depRoot },
    ],
  };
  const stat = await lstat(parent),
    legacyBytes = await readFile(legacy);
  const contract: NamespaceDeliveryContract = {
    schema: "hermes.namespace-delivery-contract.v1",
    operationId: "fictional-source-operation",
    taskId: "FICTIONAL-SOURCE-INSTALL",
    namespace,
    uid: process.getuid!(),
    gid: process.getgid!(),
    parent: {
      dev: stat.dev,
      ino: stat.ino,
      uid: stat.uid,
      gid: stat.gid,
      mode: stat.mode & 0o7777,
    },
    compiler,
    legacyManifest: { path: legacy, bytes: legacyBytes.length, sha256: hash(legacyBytes) },
    layout: {
      providerDirectory: "agent-platform",
      bundleDirectory: "bin",
      dataDirectory: "data",
      registrationFile: "platform-source-registration.json",
    },
    payloadSha256: namespacePayloadSha256(payload),
    maxArtifactWrites: payload.files.length + payload.aliases.length,
  };
  return { parent, namespace, legacy, contract, payload, source };
}
const exists = async (path: string) => {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
};

test("finite delivery writes exact source/build/DATA/aliases and durable receipt without native use claims", async () => {
  const f = await fixture(),
    before = await readFile(f.legacy),
    receipt = await deliverHermesNamespace(f.contract, f.payload);
  expect(receipt.status).toBe("delivered-awaiting-owner-verification");
  expect(receipt.completed).toBe(f.payload.files.length + f.payload.aliases.length);
  expect(receipt.claimDurable).toBe(true);
  expect(receipt.persistenceFailure).toBe(false);
  expect(receipt.terminalState).toBe("confirmed");
  expect(receipt.retryAllowed).toBe(false);
  expect(receipt.nativeTaskId).toBeNull();
  expect(receipt.nativeAttemptId).toBeNull();
  expect(receipt.nativeUseEstablished).toBe(false);
  expect(receipt.runtimeClosureVerification).toBe("required-separate-owner-step");
  for (const file of f.payload.files) {
    expect(hash(await readFile(join(f.namespace, file.path)))).toBe(file.sha256);
    expect((await lstat(join(f.namespace, file.path))).mode & 0o777).toBe(file.mode);
  }
  expect(await realpath(join(f.namespace, f.payload.aliases[0]!.path))).toBe(
    join(f.namespace, f.payload.aliases[0]!.target),
  );
  expect(await readFile(f.legacy)).toEqual(before);
  const durable = await readFile(receipt.receipt!.path);
  expect(hash(durable)).toBe(receipt.receipt!.sha256);
  expect(durable.length).toBe(receipt.receipt!.bytes);
  const snapshot = JSON.parse(durable.toString());
  expect(snapshot.completed).toBe(receipt.completed);
  expect(snapshot.terminalState).toBe("pending");
  expect(snapshot.journal.scope).toBe("verified-prefix");
  expect(snapshot.receipt).toBeNull();
  const journal = await readFile(receipt.journal!.path);
  expect(hash(journal)).toBe(receipt.journal!.sha256);
  expect(receipt.journal!.scope).toBe("terminal");
  expect(hash(journal.subarray(0, snapshot.journal.bytes))).toBe(snapshot.journal.sha256);
  const terminal = JSON.parse(journal.toString().trim().split("\n").at(-1)!);
  expect(terminal.receipt).toEqual(receipt.receipt);
  expect(terminal.receiptOutcome).toBe("persisted-and-verified");
  expect(journal.toString()).not.toContain("fictional bundle");
});

test("an existing namespace/claim refuses a second apply even for identical bytes and operation", async () => {
  const f = await fixture();
  const first = await deliverHermesNamespace(f.contract, f.payload);
  const claim = await readFile(join(f.namespace, ".namespace-delivery-claim.json")),
    receipt = await readFile(first.receipt!.path);
  const second = await deliverHermesNamespace(f.contract, f.payload);
  expect(second.status).toBe("refused");
  expect(second.reason).toBe("EXISTING_NAMESPACE_REQUIRES_RECONCILIATION");
  expect(second.namespaceCreated).toBe(false);
  expect(second.completed).toBe(0);
  expect(await readFile(join(f.namespace, ".namespace-delivery-claim.json"))).toEqual(claim);
  expect(await readFile(first.receipt!.path)).toEqual(receipt);
});

for (const invalid of [
  "../outside",
  "/outside",
  "data/../outside",
  "data//name",
  "data/\\outside",
  ".namespace-delivery-claim.json",
]) {
  test("invalid transport path refuses before namespace mutation: " + invalid, async () => {
    const f = await fixture();
    f.payload.files.at(-1)!.path = invalid;
    f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
    const receipt = await deliverHermesNamespace(f.contract, f.payload);
    expect(receipt.status).toBe("refused");
    expect(receipt.namespaceCreated).toBe(false);
    expect(await exists(f.namespace)).toBe(false);
  });
}
test("alias escape, alias parent and unregistered target are rejected before any mutation", async () => {
  for (const target of ["../foreign", "data", "agent-platform/src"]) {
    const f = await fixture();
    f.payload.aliases[0]!.target = target;
    f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
    expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
    expect(await exists(f.namespace)).toBe(false);
  }
  const f = await fixture();
  f.payload.aliases[0]!.path = "agent-platform/node_modules";
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(await exists(f.namespace)).toBe(false);
});
test("reviewed payload digest and byte/hash mismatch refuse before namespace mutation", async () => {
  const f = await fixture();
  f.payload.files[0]!.base64 = Buffer.from("changed").toString("base64");
  expect((await deliverHermesNamespace(f.contract, f.payload)).reason).toBe(
    "DELIVERY_PAYLOAD_BYTES_MISMATCH",
  );
  expect(await exists(f.namespace)).toBe(false);
  const fresh = await fixture();
  fresh.contract.payloadSha256 = "b".repeat(64);
  expect((await deliverHermesNamespace(fresh.contract, fresh.payload)).reason).toBe(
    "REVIEWED_DELIVERY_PAYLOAD_CHANGED",
  );
  expect(await exists(fresh.namespace)).toBe(false);
});
test("validated file metadata cannot be changed by a caller while filesystem work awaits", async () => {
  const f = await fixture(),
    pending = deliverHermesNamespace(f.contract, f.payload);
  (f.payload.files[0] as unknown as { mode: number }).mode = 0o777;
  f.payload.aliases[0]!.target = "../outside";
  const receipt = await pending;
  expect(receipt.status).toBe("delivered-awaiting-owner-verification");
  expect((await lstat(join(f.namespace, f.source.path))).mode & 0o777).toBe(0o600);
  expect(await realpath(join(f.namespace, "agent-platform/node_modules/fictional"))).toBe(
    join(f.namespace, "agent-platform/node_modules/.bun/fictional@1/node_modules/fictional"),
  );
});
test("target and ancestor symlinks are never followed into a foreign fixture", async () => {
  const f = await fixture(),
    foreign = join(f.parent, "foreign");
  await mkdir(foreign);
  await symlink(foreign, f.namespace);
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(await readdir(foreign)).toEqual([]);
  const g = await fixture(),
    alias = join(g.parent, "alias");
  await symlink(g.parent, alias);
  g.contract.namespace = join(alias, "new-source");
  // Update the declared registration root, so this refusal exercises the ancestor guard.
  const reg = g.payload.files.find((p) => p.path === "platform-source-registration.json")!;
  const parsed = JSON.parse(Buffer.from(reg.base64, "base64").toString());
  parsed.providerRoot = join(g.contract.namespace, "agent-platform");
  Object.assign(reg, file(reg.path, JSON.stringify(parsed) + "\n"));
  g.payload.provenance.runtimeRegistrationSha256 = reg.sha256;
  g.contract.payloadSha256 = namespacePayloadSha256(g.payload);
  expect((await deliverHermesNamespace(g.contract, g.payload)).status).toBe("refused");
  expect(await exists(g.namespace)).toBe(false);
});
test("parent inode/mode drift refuses before mutation and leaves legacy bytes unchanged", async () => {
  const f = await fixture();
  const moved = f.parent + ".moved",
    legacy = await readFile(f.legacy);
  await rename(f.parent, moved);
  roots.push(moved);
  await mkdir(f.parent, { mode: 0o700 });
  await writeFile(f.legacy, legacy);
  expect((await deliverHermesNamespace(f.contract, f.payload)).reason).toBe(
    "DELIVERY_PARENT_FINGERPRINT_CHANGED",
  );
  expect(await exists(f.namespace)).toBe(false);
  const g = await fixture();
  await chmod(g.parent, 0o755);
  expect((await deliverHermesNamespace(g.contract, g.payload)).reason).toBe(
    "DELIVERY_PARENT_FINGERPRINT_CHANGED",
  );
  expect(await exists(g.namespace)).toBe(false);
  expect(hash(await readFile(g.legacy))).toBe(g.contract.legacyManifest.sha256);
});
test("inherited object names cannot substitute for one of the six actual build bundles", async () => {
  const f = await fixture(),
    bundle = f.payload.provenance.bundles[0]!;
  f.payload.files.find((p) => p.path === "bin/" + bundle.name)!.path = "bin/toString";
  bundle.name = "toString";
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  expect((await deliverHermesNamespace(f.contract, f.payload)).reason).toBe(
    "EXACT_SIX_BUILD_BUNDLES_REQUIRED",
  );
  expect(await exists(f.namespace)).toBe(false);
});

/** Isolated fictional fault injection tests persistence without adding runtime callback hooks. */
async function journalFault(
  f: Awaited<ReturnType<typeof fixture>>,
  state: "complete" | "terminal" | "corrupt",
) {
  const code = `import {mock} from 'bun:test';
import * as fs from 'node:fs/promises';
const original=fs.open;
mock.module('node:fs/promises',()=>({...fs,open:async(...args)=>{const h=await original(...args);
 if(String(args[0]).endsWith('/.namespace-delivery-journal.jsonl')){const write=h.writeFile.bind(h);h.writeFile=async(data,...rest)=>{
  if(${state === "corrupt"}){const corrupted=Buffer.from(data);corrupted[0]^=1;return write(corrupted,...rest);}
  const event=JSON.parse(String(data));if(event.state===${JSON.stringify(state)}${state === "complete" ? "&&event.kind==='file'" : ""})throw Error('FICTIONAL_JOURNAL_FAILURE');
  return write(data,...rest);
 };}return h;}}));
const {deliverHermesNamespace}=await import(${JSON.stringify(join(import.meta.dir, "delivery.ts"))});
const receipt=await deliverHermesNamespace(${JSON.stringify(f.contract)},${JSON.stringify(f.payload)});
console.log(JSON.stringify(receipt));`;
  const child = Bun.spawn([process.execPath, "--no-env-file", "-e", code], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [status, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect({ status, stderr }).toEqual({ status: 0, stderr: "" });
  return JSON.parse(stdout);
}
test("failed durable completion retains the exact possibly written artifact as uncertain", async () => {
  const f = await fixture(),
    receipt = await journalFault(f, "complete");
  expect(receipt.status).toBe("partial");
  expect(receipt.reason).toBe("FICTIONAL_JOURNAL_FAILURE");
  expect(receipt.uncertain).toBe(f.source.path);
  expect(receipt.completed).toBe(0);
  expect(hash(await readFile(join(f.namespace, f.source.path)))).toBe(f.source.sha256);
  expect(JSON.parse(await readFile(receipt.receipt.path, "utf8")).uncertain).toBe(f.source.path);
});
test("terminal journal failure leaves the immutable receipt explicitly awaiting reconciliation", async () => {
  const f = await fixture(),
    receipt = await journalFault(f, "terminal");
  expect(receipt.status).toBe("partial");
  expect(receipt.persistenceFailure).toBe(true);
  expect(receipt.receipt).not.toBeNull();
  const durable = JSON.parse(await readFile(receipt.receipt.path, "utf8"));
  expect(receipt.terminalState).toBe("unknown");
  expect(durable.terminalState).toBe("pending");
  expect(durable.journal.scope).toBe("verified-prefix");
  expect((await readFile(receipt.journal.path, "utf8")).includes('"state":"terminal"')).toBe(false);
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(hash(await readFile(f.legacy))).toBe(f.contract.legacyManifest.sha256);
});
test("same-size journal corruption is never reported with a verified journal hash", async () => {
  const f = await fixture(),
    receipt = await journalFault(f, "corrupt");
  expect(receipt.status).toBe("partial");
  expect(receipt.reason).toBe("DELIVERY_JOURNAL_READBACK_FAILED");
  expect(receipt.persistenceFailure).toBe(true);
  expect(receipt.journal).toBeNull();
  expect(receipt.completed).toBe(0);
  expect(receipt.receipt).not.toBeNull();
  expect(JSON.parse(await readFile(receipt.receipt.path, "utf8")).journal).toBeNull();
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(hash(await readFile(f.legacy))).toBe(f.contract.legacyManifest.sha256);
});
test("same-size claim corruption fails held-descriptor hash readback and cannot retry", async () => {
  const f = await fixture();
  const code = `import {mock} from 'bun:test';
import * as fs from 'node:fs/promises';
const original=fs.open;
mock.module('node:fs/promises',()=>({...fs,open:async(...args)=>{const h=await original(...args);
 if(String(args[0]).endsWith('/.namespace-delivery-claim.json')){const write=h.writeFile.bind(h);h.writeFile=async(data,...rest)=>{
  const corrupted=Buffer.from(data);corrupted[0]^=1;return write(corrupted,...rest);
 };}return h;}}));
const {deliverHermesNamespace}=await import(${JSON.stringify(join(import.meta.dir, "delivery.ts"))});
console.log(JSON.stringify(await deliverHermesNamespace(${JSON.stringify(f.contract)},${JSON.stringify(f.payload)})));`;
  const child = Bun.spawn([process.execPath, "--no-env-file", "-e", code], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [status, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect({ status, stderr }).toEqual({ status: 0, stderr: "" });
  const receipt = JSON.parse(stdout);
  expect(receipt.status).toBe("partial");
  expect(receipt.reason).toBe("DELIVERY_WRITE_READBACK_FAILED");
  expect(receipt.claimDurable).toBe(false);
  expect(receipt.completed).toBe(0);
  expect(receipt.receipt).not.toBeNull();
  expect(JSON.parse(await readFile(receipt.receipt.path, "utf8")).status).toBe("partial");
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(hash(await readFile(f.legacy))).toBe(f.contract.legacyManifest.sha256);
});
test("legacy drift refuses without rewriting legacy or starting a namespace", async () => {
  const f = await fixture();
  await writeFile(f.legacy, "third state remains untouched");
  const receipt = await deliverHermesNamespace(f.contract, f.payload);
  expect(receipt.status).toBe("refused");
  expect(receipt.legacyManifestUnchanged).toBe(false);
  expect(await exists(f.namespace)).toBe(false);
  expect(await readFile(f.legacy, "utf8")).toBe("third state remains untouched");
});
test("bounded partial write has durable progress/receipt and cannot resume or roll back", async () => {
  const f = await fixture();
  f.contract.maxArtifactWrites = 2;
  const receipt = await deliverHermesNamespace(f.contract, f.payload);
  expect(receipt.status).toBe("partial");
  expect(receipt.reason).toBe("DELIVERY_ARTIFACT_WRITE_CEILING");
  expect(receipt.claimDurable).toBe(true);
  expect(receipt.completed).toBe(2);
  expect(receipt.persistenceFailure).toBe(false);
  expect(JSON.parse(await readFile(receipt.receipt!.path, "utf8")).status).toBe("partial");
  expect(
    (await readFile(receipt.journal!.path, "utf8"))
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .at(-1).status,
  ).toBe("partial");
  const before = await readFile(receipt.receipt!.path);
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
  expect(await readFile(receipt.receipt!.path)).toEqual(before);
  expect(hash(await readFile(f.legacy))).toBe(f.contract.legacyManifest.sha256);
});

for (const stage of ["allocation", "write", "readback"] as const) {
  test(
    "receipt " + stage + " failure is durably terminal partial even when stdout is lost",
    async () => {
      const f = await fixture();
      const code = `import {mock} from 'bun:test';
import * as fs from 'node:fs/promises';
const original=fs.open;
mock.module('node:fs/promises',()=>({...fs,open:async(...args)=>{
 const receipt=String(args[0]).endsWith('/.namespace-delivery-receipt.json');
 if(receipt&&${stage === "allocation"})throw Error('FICTIONAL_RECEIPT_ALLOCATION_FAILED');
 const h=await original(...args);
 if(receipt&&${stage === "write"})h.writeFile=async()=>{throw Error('FICTIONAL_RECEIPT_WRITE_FAILED');};
 if(receipt&&${stage === "readback"}){const read=h.read.bind(h);h.read=async(...args)=>{const r=await read(...args);if(r.bytesRead)args[0][0]^=1;return r;};}
 return h;}}));
const {deliverHermesNamespace}=await import(${JSON.stringify(join(import.meta.dir, "delivery.ts"))});
await deliverHermesNamespace(${JSON.stringify(f.contract)},${JSON.stringify(f.payload)});
// Deliberately retain no stdout receipt: recovery must use only the durable namespace.
`;
      const child = Bun.spawn([process.execPath, "--no-env-file", "-e", code], {
        stdout: "pipe",
        stderr: "pipe",
      });
      const [status, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect({ status, stdout, stderr }).toEqual({ status: 0, stdout: "", stderr: "" });
      const records = (
        await readFile(join(f.namespace, ".namespace-delivery-journal.jsonl"), "utf8")
      )
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line));
      const terminal = records.at(-1);
      expect(records.filter((row) => row.state === "terminal")).toHaveLength(1);
      expect(terminal.state).toBe("terminal");
      expect(terminal.status).toBe("partial");
      expect(terminal.persistenceFailure).toBe(true);
      expect(terminal.receiptOutcome).toBe("write-or-verification-failed");
      expect(terminal.reason).toBe("DELIVERY_RECEIPT_PERSISTENCE_FAILED");
      expect(terminal.receipt).toBeNull();
      expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe("refused");
      expect(hash(await readFile(f.legacy))).toBe(f.contract.legacyManifest.sha256);
    },
  );
}

test("a same-size atomically replaced executable pathname cannot stand in for the actual running image hash", async () => {
  const f = await fixture(),
    executable = join(f.parent, "fictional-bun"),
    replacement = join(f.parent, "fictional-bun-replacement");
  await copyFile(compiler.path, executable);
  await chmod(executable, 0o700);
  const replacementBytes = await readFile(compiler.path);
  replacementBytes[replacementBytes.length - 1] ^= 1;
  await writeFile(replacement, replacementBytes, { mode: 0o700 });
  const code = `import {rename,realpath} from 'node:fs/promises';
const {deliverHermesNamespace,namespacePayloadSha256}=await import(${JSON.stringify(join(import.meta.dir, "delivery.ts"))});
const c=${JSON.stringify(f.contract)},p=${JSON.stringify(f.payload)};
await rename(${JSON.stringify(replacement)},${JSON.stringify(executable)});
c.compiler={path:${JSON.stringify(executable)},bytes:${replacementBytes.length},sha256:${JSON.stringify(hash(replacementBytes))}};
p.provenance.compilerSha256=c.compiler.sha256;c.payloadSha256=namespacePayloadSha256(p);
console.log(JSON.stringify({namedPath:await realpath(c.compiler.path),receipt:await deliverHermesNamespace(c,p)}));`;
  const child = Bun.spawn([executable, "--no-env-file", "-e", code], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [status, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect({ status, stderr }).toEqual({ status: 0, stderr: "" });
  const observed = JSON.parse(stdout);
  expect(observed.namedPath).toBe(executable);
  expect(observed.receipt.reason).toBe("EXACT_RUNNING_DELIVERY_COMPILER_REQUIRED");
  expect(observed.receipt.status).toBe("refused");
  expect(await exists(f.namespace)).toBe(false);
  expect(hash(await readFile(compiler.path))).toBe(compiler.sha256);
});

for (const mask of [0o777, 0o400, 0o077, 0o022]) {
  test(
    "isolated owner umask " + mask.toString(8) + " is preserved and cannot create mode000",
    async () => {
      const f = await fixture(),
        before = process.umask();
      const code = `const {deliverHermesNamespace}=await import(${JSON.stringify(join(import.meta.dir, "delivery.ts"))});
process.umask(${mask});const before=process.umask();
const receipt=await deliverHermesNamespace(${JSON.stringify(f.contract)},${JSON.stringify(f.payload)});
console.log(JSON.stringify({before,after:process.umask(),receipt}));`;
      const child = Bun.spawn([process.execPath, "--no-env-file", "-e", code], {
        stdout: "pipe",
        stderr: "pipe",
      });
      const [status, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect({ status, stderr }).toEqual({ status: 0, stderr: "" });
      const result = JSON.parse(stdout);
      expect(result.before).toBe(mask);
      expect(result.after).toBe(mask);
      expect(process.umask()).toBe(before);
      if (mask & 0o700) {
        expect(result.receipt.reason).toBe("DELIVERY_OWNER_PERMISSIONS_MASKED");
        expect(await exists(f.namespace)).toBe(false);
      } else {
        expect(result.receipt.status).toBe("delivered-awaiting-owner-verification");
        const directories = new Set([f.namespace]);
        for (const entry of f.payload.files) {
          let parent = dirname(join(f.namespace, entry.path));
          while (parent !== f.parent) {
            directories.add(parent);
            parent = dirname(parent);
          }
        }
        for (const path of directories) expect((await lstat(path)).mode & 0o7777).toBe(0o700);
      }
    },
  );
}

for (const component of ["x".repeat(256), "€".repeat(86), "\ud800", "\udfff"]) {
  test("unrepresentable or overlong encoded component refuses before the once fence", async () => {
    const f = await fixture();
    f.payload.files.at(-1)!.path = "data/" + component;
    f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
    const receipt = await deliverHermesNamespace(f.contract, f.payload);
    expect(receipt.status).toBe("refused");
    expect(["DELIVERY_PATH_COMPONENT_LIMIT", "DELIVERY_ENCODED_PATH_REQUIRED"]).toContain(
      receipt.reason ?? "",
    );
    expect(await exists(f.namespace)).toBe(false);
  });
}
test("255-byte ASCII and UTF8 components remain valid with exact file names", async () => {
  for (const component of ["x".repeat(255), "€".repeat(85)]) {
    const f = await fixture();
    f.payload.files.at(-1)!.path = "data/" + component;
    f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
    expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe(
      "delivered-awaiting-owner-verification",
    );
    expect(await exists(join(f.namespace, "data", component))).toBe(true);
  }
});
test("encoded whole-path and alias component bounds are checked before namespace creation", async () => {
  const f = await fixture();
  f.payload.files.at(-1)!.path = "data/" + Array(5).fill("€".repeat(80)).join("/");
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  expect((await deliverHermesNamespace(f.contract, f.payload)).reason).toBe(
    "DELIVERY_ENCODED_PATH_REQUIRED",
  );
  expect(await exists(f.namespace)).toBe(false);
  const g = await fixture();
  g.payload.aliases[0]!.path = "agent-platform/node_modules/" + "x".repeat(256);
  g.contract.payloadSha256 = namespacePayloadSha256(g.payload);
  expect((await deliverHermesNamespace(g.contract, g.payload)).reason).toBe(
    "DELIVERY_PATH_COMPONENT_LIMIT",
  );
  expect(await exists(g.namespace)).toBe(false);
});
test("valid namespace plus artifact parents beyond2048 bytes use the consistent4095-byte directory bound", async () => {
  const f = await fixture(),
    parent = join(f.parent, ...Array(7).fill("p".repeat(240)));
  await mkdir(parent, { recursive: true, mode: 0o700 });
  f.namespace = join(parent, "new-source");
  f.contract.namespace = f.namespace;
  const s = await lstat(parent);
  f.contract.parent = { dev: s.dev, ino: s.ino, uid: s.uid, gid: s.gid, mode: s.mode & 0o7777 };
  const registration = f.payload.files.find((p) => p.path === "platform-source-registration.json")!;
  const parsed = JSON.parse(Buffer.from(registration.base64, "base64").toString());
  parsed.providerRoot = join(f.namespace, "agent-platform");
  Object.assign(registration, file(registration.path, JSON.stringify(parsed) + "\n"));
  f.payload.provenance.runtimeRegistrationSha256 = registration.sha256;
  const data = f.payload.files.at(-1)!;
  data.path = "data/" + Array(3).fill("d".repeat(150)).join("/") + "/original";
  expect(Buffer.byteLength(dirname(join(f.namespace, data.path)))).toBeGreaterThan(2048);
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  expect((await deliverHermesNamespace(f.contract, f.payload)).status).toBe(
    "delivered-awaiting-owner-verification",
  );
  expect(hash(await readFile(join(f.namespace, data.path)))).toBe(data.sha256);
});
test("otherwise valid bounded artifacts that cannot fit the complete journal refuse before claiming", async () => {
  const f = await fixture(),
    prefix = "data/" + Array(3).fill("x".repeat(255)).join("/");
  for (let n = 0; n < 4080; n++)
    f.payload.files.push(
      file(prefix + "/" + String(n).padStart(6, "0") + "y".repeat(234) + "/f", ""),
    );
  f.contract.maxArtifactWrites = f.payload.files.length + f.payload.aliases.length;
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  const receipt = await deliverHermesNamespace(f.contract, f.payload);
  expect(receipt.reason).toBe("DELIVERY_JOURNAL_PREFLIGHT_LIMIT");
  expect(receipt.status).toBe("refused");
  expect(receipt.namespaceCreated).toBe(false);
  expect(receipt.claimDurable).toBe(false);
  expect(await exists(f.namespace)).toBe(false);
});
test("journal preflight counts JSON escaping rather than unescaped path length", async () => {
  const f = await fixture(),
    prefix = "data/" + Array(3).fill('"'.repeat(255)).join("/");
  for (let n = 0; n < 2100; n++)
    f.payload.files.push(
      file(prefix + "/" + String(n).padStart(6, "0") + '"'.repeat(234) + "/f", ""),
    );
  const directories = new Set<string>();
  for (const entry of f.payload.files) {
    let path = dirname(entry.path);
    while (path !== ".") {
      directories.add(path);
      path = dirname(path);
    }
  }
  const events = [
    ...[...directories].map((path) => ({ kind: "directory", path })),
    ...f.payload.files.map(({ path }) => ({ kind: "file", path })),
  ];
  let encoded = 0,
    plain = 0;
  for (const event of events)
    for (const state of ["intent", "complete"]) {
      const json = JSON.stringify({ state, ...event }) + "\n";
      encoded += Buffer.byteLength(json);
      plain += Buffer.byteLength(json.replaceAll('\\"', '"'));
    }
  expect(plain).toBeLessThan(16 * 1024 * 1024);
  expect(encoded).toBeGreaterThan(16 * 1024 * 1024);
  f.contract.maxArtifactWrites = f.payload.files.length + f.payload.aliases.length;
  f.contract.payloadSha256 = namespacePayloadSha256(f.payload);
  expect((await deliverHermesNamespace(f.contract, f.payload)).reason).toBe(
    "DELIVERY_JOURNAL_PREFLIGHT_LIMIT",
  );
  expect(await exists(f.namespace)).toBe(false);
});

test("one-shot entry holds without a compiled contract and accepts no request-selected target", async () => {
  const entry = join(import.meta.dir, "native-delivery.ts");
  const unbound = Bun.spawn([process.execPath, "--no-env-file", entry], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [unboundStatus, unboundOut, unboundErr] = await Promise.all([
    unbound.exited,
    new Response(unbound.stdout).text(),
    new Response(unbound.stderr).text(),
  ]);
  expect({ status: unboundStatus, stderr: unboundErr }).toEqual({ status: 1, stderr: "" });
  expect(JSON.parse(unboundOut).status).toBe("held");
  const f = await fixture(),
    compiled = join(f.parent, "fictional-compiled-delivery.js");
  const build = Bun.spawn(
    [
      process.execPath,
      "--no-env-file",
      "build",
      entry,
      "--target=bun",
      "--outfile",
      compiled,
      "--define",
      "HERMES_NAMESPACE_DELIVERY_CONTRACT=" + JSON.stringify(f.contract),
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [buildStatus] = await Promise.all([
    build.exited,
    new Response(build.stdout).text(),
    new Response(build.stderr).text(),
  ]);
  expect(buildStatus).toBe(0);
  const invoke = async (payload: unknown, extra: string[] = []) => {
    const child = Bun.spawn([process.execPath, "--no-env-file", compiled, ...extra], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    await child.stdin.write(JSON.stringify(payload));
    await child.stdin.end();
    const [status, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(stderr).toBe("");
    return { status, receipt: JSON.parse(stdout) };
  };
  const extra = await invoke(f.payload, ["arbitrary-target"]);
  expect(extra.status).toBe(1);
  expect(await exists(f.namespace)).toBe(false);
  const selected = await invoke({ ...f.payload, namespace: "/arbitrary-target" });
  expect(selected.status).toBe(1);
  expect(await exists(f.namespace)).toBe(false);
  const delivered = await invoke(f.payload);
  expect(delivered.status).toBe(0);
  expect(delivered.receipt.status).toBe("delivered-awaiting-owner-verification");
  expect(delivered.receipt.nativePermissionGranted).toBe(false);
  expect(delivered.receipt.nativeEventId).toBeNull();
});
