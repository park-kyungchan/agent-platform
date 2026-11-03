// Finite deliberate-retirement contract frozen in S03/native-context-02 before this candidate.
// Every executable copy, canary, input, HOME and effect target is fictional and disposable.
import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

const owner = "Hermes";
const sourcePath = join(import.meta.dir, "native-hook.ts");
const cases = [
  { id: "startup-selected", input: { hook_event_name: "pre_llm_call", session_id: "synthetic-public" } },
  { id: "foreign-event", input: { hook_event_name: "unrelated" } },
  { id: "malformed", input: "not-json" },
  { id: "missing-bindings", input: {} },
  { id: "private-sentinel", input: { prompt: "SYNTHETIC_PRIVATE_SENTINEL", extra: { run_id: "must-not-create-or-adopt" } } },
] as const;
const sha = (text: string) => createHash("sha256").update(text).digest("hex");
const legacy = ["provider.ts", "hook-context.ts", "work-method-source.ts", "diagnostics.ts", "../platform-rearchitecture/input.ts"];

// The trap records even swallowed exceptions. The independent tree comparison
// also detects writes when instrumentation is disabled in a negative control.
const preload = `
import fs from "node:fs";
import { mock } from "bun:test";
import child from "node:child_process";
const append = fs.appendFileSync;
const denied = name => (...args) => {
  append(process.env.RETIRED_EFFECT_LOG, name + "\\n");
  throw Error("FICTIONAL_RETIRED_EFFECT:" + name);
};
for (const name of ["openSync", "readFileSync", "readSync", "statSync", "lstatSync", "realpathSync", "readdirSync", "accessSync", "existsSync", "writeFileSync", "appendFileSync", "writeSync", "mkdirSync", "renameSync", "unlinkSync", "createReadStream", "createWriteStream", "open", "readFile", "writeFile", "appendFile", "stat", "lstat", "realpath", "readdir", "access", "mkdir", "rename", "unlink"])
  fs[name] = denied("fs." + name);
for (const name of ["open", "readFile", "writeFile", "appendFile", "stat", "lstat", "realpath", "readdir", "access", "mkdir", "rename", "unlink"])
  fs.promises[name] = denied("fs.promises." + name);
for (const name of ["spawn", "spawnSync", "exec", "execSync", "execFile", "execFileSync", "fork"])
  child[name] = denied("child_process." + name);
// Bun named builtin imports do not follow syncBuiltinESMExports. Override the
// child-only module exports as well; the named-read negative control proves it.
for (const name of ["node:fs", "fs"]) mock.module(name, () => ({ ...fs, default: fs }));
for (const name of ["node:fs/promises", "fs/promises"]) mock.module(name, () => ({ ...fs.promises, default: fs.promises }));
for (const name of ["node:child_process", "child_process"]) mock.module(name, () => ({ ...child, default: child }));
// Node-compatible process.stdout/stderr use Bun.file(1/2) internally.
// Those are judged as exact channels; every data path and stdin fd still refuses.
const nativeFile = Bun.file;
Bun.file = (...args) => args[0] === 1 || args[0] === 2 ? nativeFile(...args) : denied("Bun.file")(...args);
Bun.write = denied("Bun.write");
Bun.spawn = denied("Bun.spawn");
Bun.spawnSync = denied("Bun.spawnSync");
Bun.stdin.stream = denied("Bun.stdin.stream");
globalThis.fetch = denied("fetch");
`;

async function tree(root: string, relative = ""): Promise<Record<string, string>> {
  const result: Record<string, string> = {};
  for (const entry of await readdir(join(root, relative), { withFileTypes: true })) {
    const path = join(relative, entry.name);
    if (entry.isDirectory()) Object.assign(result, await tree(root, path));
    else result[path] = sha(await readFile(join(root, path), "utf8"));
  }
  return result;
}

async function fixture(source: string, canaries: boolean) {
  const root = await mkdtemp(join(tmpdir(), "fictional-retired-" + owner + "-"));
  const entry = join(root, "entry", "native-hook.ts");
  await mkdir(join(root, "entry"));
  await mkdir(join(root, "home"));
  await writeFile(entry, source);
  await writeFile(join(root, "preload.ts"), preload);
  await writeFile(join(root, "private-sentinel"), "SYNTHETIC_PRIVATE_SENTINEL");
  await writeFile(join(root, "bindings.json"), "malformed-fictional-bindings");
  if (canaries) {
    for (const path of legacy) {
      const target = join(root, "entry", path);
      await mkdir(join(target, ".."), { recursive: true });
      await writeFile(target, `await Bun.write(${JSON.stringify(join(root, "legacy-imported"))}, "legacy import"); throw Error("LEGACY_IMPORT_CANARY");`);
    }
  }
  return { root, entry, cleanup: () => rm(root, { recursive: true, force: true }) };
}

type Observation = { code: number; stdout: string; stderr: string; effects: boolean };
const conforms = (r: Observation) => r.code === 0 && r.stdout === "{}\n" && r.stderr === "" && !r.effects;

async function invoke(f: Awaited<ReturnType<typeof fixture>>, input: unknown, options: { trap?: boolean; callable?: boolean } = {}): Promise<Observation> {
  const stdin = typeof input === "string" ? input : JSON.stringify(input);
  const trap = options.trap !== false;
  let executable = f.entry;
  if (options.callable) {
    executable = join(f.root, "callable.ts");
    await writeFile(executable, `
      import { runNativeHook, admitsCurrentHomeContext } from ${JSON.stringify(f.entry)};
      if (typeof admitsCurrentHomeContext !== "function") throw Error("PROMOTION_API_MISSING");
      const options = new Proxy({}, { get() { throw Error("OPTIONS_MUST_NOT_BE_READ"); } });
      process.stdout.write((await runNativeHook(${JSON.stringify(stdin)}, ${JSON.stringify(join(f.root, "bindings.json"))}, options)) + "\\n");
    `);
  }
  const before = await tree(f.root);
  const argv = [process.execPath, "--no-env-file", ...(trap ? ["--preload", join(f.root, "preload.ts")] : []), executable,
    "--bindings", join(f.root, "bindings.json"), "--context-only", "--selected-session-only", "--bindings-sha256", "invalid"];
  const started = performance.now();
  const child = Bun.spawn(argv, {
    cwd: f.root,
    env: { PATH: process.env.PATH ?? "", HOME: join(f.root, "home"), TMPDIR: f.root, BUN_INSTALL: join(f.root, "home"), RETIRED_EFFECT_LOG: join(f.root, "effect-log") },
    stdin: new Blob([stdin]), stdout: "pipe", stderr: "pipe",
  });
  const childStat = await readFile("/proc/" + child.pid + "/stat", "utf8").catch(() => "");
  const pgid = childStat ? Number(childStat.slice(childStat.lastIndexOf(")") + 2).split(" ")[2]) : null;
  const timer = setTimeout(() => child.kill("SIGKILL"), 10_000);
  let result: Observation;
  try {
    const [code, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
    result = { code, stdout, stderr, effects: JSON.stringify(await tree(f.root)) !== JSON.stringify(before) };
  } finally { clearTimeout(timer); }
  // Optional raw command evidence stays in the caller's explicit disposable root.
  if (process.env.RETIRED_HOOK_EVIDENCE_DIR) {
    const directory = await mkdtemp(join(process.env.RETIRED_HOOK_EVIDENCE_DIR, owner + "-"));
    for (const [name, value] of Object.entries({ stdin, stdout: result.stdout, stderr: result.stderr }))
      await writeFile(join(directory, name), value, { flag: "wx" });
    await writeFile(join(directory, "result.json"), JSON.stringify({ argv, cwd: f.root, pid: child.pid, pgid, durationMs: performance.now() - started, ...result, trap, callable: !!options.callable, channelSha256: { stdin: sha(stdin), stdout: sha(result.stdout), stderr: sha(result.stderr) } }, null, 2), { flag: "wx" });
  }
  return result;
}

test(owner + " has no legacy value import at the source entry boundary", async () => {
  const source = await readFile(sourcePath, "utf8");
  const executable = source.replace(/^import type .*;\n/gm, "").replace(/^\/\/.*$/gm, "");
  expect(executable).not.toMatch(/\b(?:import|require)\s*(?:\(|["\'{*])/);
});

for (const c of cases) {
  test(owner + " " + c.id + " ignores input, bindings and missing/poisoned legacy modules", async () => {
    for (const canaries of [false, true]) {
      const f = await fixture(await readFile(sourcePath, "utf8"), canaries);
      try {
        expect(await invoke(f, c.input)).toEqual({ code: 0, stdout: "{}\n", stderr: "", effects: false });
        
      } finally { await f.cleanup(); }
    }
  });
}

test(owner + " effect/import/channel judge rejects deliberate negative controls", async () => {
  const source = await readFile(sourcePath, "utf8");
  const controls = [
    { source: 'await Bun.file(process.cwd() + "/private-sentinel").text();\n' + source },
    { source: source.replace('process.stdout.write("{}\\n")', 'process.stdout.write("{\\"context\\":true}\\n")') },
    { source: 'import "./hook-context.ts";\n' + source },
    { source: 'import { readFileSync as forbiddenRead } from "node:fs"; forbiddenRead(process.cwd() + "/private-sentinel");\n' + source },
    { source: 'await Bun.write(process.cwd() + "/captured-prompt", "SYNTHETIC_PRIVATE_SENTINEL");\n' + source },
    { source: 'await Bun.write(process.cwd() + "/boundary", "SYNTHETIC_PRIVATE_SENTINEL");\n' + source, trap: false },
    { source: source + '\nprocess.exitCode = 7;\n' },
    { source: source + '\nprocess.stderr.write("unexpected");\n' },
    { source: source.replace('process.stdout.write("{}\\n")', 'process.stdout.write("{}")') },
  ];
  for (const control of controls) {
    expect(control.source).not.toBe(source);
    const f = await fixture(control.source, true);
    try { expect(conforms(await invoke(f, cases[4].input, control))).toBe(false); }
    finally { await f.cleanup(); }
  }
  // Qualify the canary itself, independently of whether the entry imports it.
  const f = await fixture('import "./hook-context.ts";\n', true);
  try {
    const r = await invoke(f, {}, { trap: false });
    expect(r.effects).toBe(true);
    expect(r.stderr).toContain("LEGACY_IMPORT_CANARY");
    expect(conforms(r)).toBe(false);
  } finally { await f.cleanup(); }
});


