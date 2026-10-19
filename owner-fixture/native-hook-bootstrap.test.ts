import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const roots: string[] = [];
const digest = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
async function write(path: string, text: string) { await mkdir(dirname(path), { recursive: true }); await writeFile(path, text); }
afterEach(async () => { for (const path of roots.splice(0)) await rm(path, { recursive: true, force: true }); });
async function fixture(mode: "normal" | "late-drift" = "normal") {
  const root = await mkdtemp(join(tmpdir(), "codex-native-bootstrap-")); roots.push(root);
  const owner = join(root, "codex-harness-engineering"), common = join(root, "agent-platform"),
    pkgRoot = join(common, "packages/native-codex-adapter"), marker = join(root, "implementation-imported");
  const native = join(pkgRoot, "src/native"), names = ["hooks", "home-context", "evaluation-ledger"];
  const pkg = { name: "@palantirkc/native-codex-adapter", type: "module", exports: Object.fromEntries(names.map(name => ["./" + name, "./src/native/" + name + ".ts"])), dependencies: { "@palantirkc/work-continuity": "file:../work-continuity" } };
  await write(join(common, "package.json"), "{}"); await write(join(common, "bun.lock"), "synthetic-lock");
  await write(join(pkgRoot, "package.json"), JSON.stringify(pkg));
  await write(join(native, "home-context.ts"), "export const home = true;\n");
  await write(join(native, "evaluation-ledger.ts"), "export const ledger = true;\n");
  await write(join(native, "hooks.ts"), `import {writeFile} from 'node:fs/promises';
await writeFile(${JSON.stringify(marker)}, 'imported');
export const readHookBindings=()=>({});
export async function handleHook(){await Promise.resolve();
${mode === "late-drift" ? `await writeFile(${JSON.stringify(join(native, "home-context.ts"))}, 'changed after async event');` : ""}
return {stdout:JSON.stringify({hookSpecificOutput:{hookEventName:'SessionStart',additionalContext:'synthetic-purpose'}})};}
`);
  for (const name of ["provider-closure", "hook-cli", ...names])
    await write(join(owner, "src/native/" + name + ".ts"), await readFile(join(import.meta.dir, "codex-harness-engineering/src/native/" + name + ".ts"), "utf8"));
  await write(join(owner, "src/native/context-owner.ts"), "export const loadCodexOwnerContext=()=>({});\n");
  await write(join(owner, "src/native/context-provider.ts"), await readFile(join(import.meta.dir, "codex-harness-engineering/src/native/context-provider.ts"), "utf8"));
  await write(join(owner, "package.json"), JSON.stringify({ type: "module", dependencies: { "@palantirkc/native-codex-adapter": "file:../agent-platform/packages/native-codex-adapter" } }));
  await write(join(owner, "bun.lock"), "synthetic-owner-lock");
  await mkdir(join(owner, "node_modules/@palantirkc"), { recursive: true });
  await symlink(pkgRoot, join(owner, "node_modules/@palantirkc/native-codex-adapter"));
  const paths = ["package.json", "bun.lock", "packages/native-codex-adapter/package.json", ...names.map(name => "packages/native-codex-adapter/src/native/" + name + ".ts")];
  const registration = { schema: "codex.native-adapter-source-closure.v1", consumerRoot: owner, providerRoot: common, packageName: pkg.name, sourcePins: await Promise.all(paths.map(async path => ({ path, sha256: digest(await readFile(join(common, path))) }))), authority: "none", nativePermissionGranted: false };
  const registrationBytes = JSON.stringify(registration) + "\n", registrationPath = join(owner, "architecture/native-codex-provider.json");
  await write(registrationPath, registrationBytes);
  const wrapper = join(owner, "src/native/hooks.ts");
  await write(wrapper, (await readFile(wrapper, "utf8")).replace(/codex-native-provider-registration-sha256: [a-f0-9]{64}/, "codex-native-provider-registration-sha256: " + digest(Buffer.from(registrationBytes))));
  const bindingsPath = join(owner, "architecture/bindings.json"); await write(bindingsPath, "{}");
  return { root, owner, native, marker, registrationPath, bindingsPath, wrapper };
}
async function invoke(f: Awaited<ReturnType<typeof fixture>>, payload: unknown = { hook_event_name: "SessionStart", source: "fork" }) {
  const child = Bun.spawn([process.execPath, join(f.owner, "src/native/hook-cli.ts"), "--bindings", f.bindingsPath], {
    stdin: new Blob([JSON.stringify(payload)]), stdout: "pipe", stderr: "pipe", env: { ...process.env, BUN_OPTIONS: "" },
  });
  return { code: await child.exited, stdout: await new Response(child.stdout).text(), stderr: await new Response(child.stderr).text() };
}
test("native entrypoint verifies fixed source before the first implementation import", async () => {
  const f = await fixture(); await write(f.registrationPath, "unreviewed-registration");
  const result = await invoke(f);
  expect(result.code).toBe(0); expect(result.stdout).toContain("context held");
  expect(await Bun.file(f.marker).exists()).toBe(false); expect(result.stderr).toBe("");
  expect(result.stdout).not.toContain(f.root);
});
test("missing marker cannot downgrade registered native first-import protection", async () => {
  const f = await fixture(); await write(f.wrapper, (await readFile(f.wrapper, "utf8")).replace(/^.*\n/, ""));
  expect((await invoke(f)).stdout).toContain("context held"); expect(await Bun.file(f.marker).exists()).toBe(false);
});
test("valid unchanged native command imports implementation and returns protocol context", async () => {
  const f = await fixture(); const result = await invoke(f);
  expect(result.code).toBe(0); expect(await Bun.file(f.marker).exists()).toBe(true);
  expect(JSON.parse(result.stdout).hookSpecificOutput.additionalContext).toBe("synthetic-purpose");
});
test("late async source drift withholds completed context and exposes no private bytes", async () => {
  const f = await fixture("late-drift"), result = await invoke(f, { secret: "private-event-original" });
  expect(await Bun.file(f.marker).exists()).toBe(true); expect(result.code).toBe(0);
  expect(result.stdout).toContain("context held"); expect(result.stdout).not.toContain("synthetic-purpose");
  expect(result.stdout).not.toContain("private-event-original"); expect(result.stdout).not.toContain(f.root);
});
test("provider source symlink is refused before implementation import", async () => {
  const f = await fixture(), path = join(f.native, "home-context.ts"), target = join(f.root, "foreign.ts");
  await write(target, await readFile(path, "utf8")); await rm(path); await symlink(target, path);
  const result = await invoke(f); expect(result.stdout).toContain("context held");
  expect(await Bun.file(f.marker).exists()).toBe(false);
});
test("pre-registration owner-only route remains distinct from registered common provider", async () => {
  const f = await fixture(); await rm(f.registrationPath);
  await write(f.wrapper, "export const readHookBindings=()=>({}); export async function handleHook(){return {stdout:'legacy-owner-only'};}\n");
  for (const name of ["home-context", "evaluation-ledger"])
    await write(join(f.owner, "src/native/" + name + ".ts"), "export const legacyOwnerOnly=true;\n");
  const result = await invoke(f);
  expect(result.code).toBe(0); expect(result.stdout.trim()).toBe("legacy-owner-only");
  expect(await Bun.file(f.marker).exists()).toBe(false);
});
