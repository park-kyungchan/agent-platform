import { lstat, mkdir, readFile, readdir, realpath, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { hash } from "./support";

/** Copy the actual frozen runtime dependency graph, resolved from each real importer.
 * A dereferenced top-level link loses Bun's sibling/nested resolution topology. Instead,
 * materialize one exact byte-verified registry package per admitted name and join every
 * declared runtime dependency/peer to that same private node_modules. Conflicts fail closed.
 * This fixture helper installs nothing and never reads code by importing it. */
export async function stageFrozenFixtureDependencies(source: string, target: string) {
  const seen = new Map<string, { origin: string; version: string; tree: string }>();
  const pins: { path: string; sha256: string; bytes: number }[] = [];
  let total = 0;
  const inside = (root: string, path: string) => {
    const p = relative(root, path);
    return p !== "" && !p.startsWith("..") && !p.startsWith("/");
  };
  async function stage(name: string, importer: string) {
    if (!/^(@[a-z0-9-]+\/)?[a-z0-9-]+$/.test(name)) throw Error("FIXTURE_RUNTIME_DEPENDENCY_NAME_REQUIRED");
    const manifestPath = await realpath(Bun.resolveSync(name + "/package.json", importer));
    const origin = dirname(manifestPath);
    if (!inside(join(source, "node_modules"), origin)) throw Error("FIXTURE_RUNTIME_DEPENDENCY_ESCAPED_SOURCE");
    const manifestBytes = await readFile(manifestPath);
    const pkg = JSON.parse(manifestBytes.toString()) as {
      name: string; version: string; dependencies?: Record<string, string>;
      optionalDependencies?: Record<string, string>; peerDependencies?: Record<string, string>;
      peerDependenciesMeta?: Record<string, { optional?: boolean }>;
    };
    if (pkg.name !== name || !pkg.version) throw Error("FIXTURE_RUNTIME_DEPENDENCY_IDENTITY_REQUIRED");
    const prior = seen.get(name);
    if (prior) {
      if (prior.origin !== origin || prior.version !== pkg.version) throw Error("FIXTURE_RUNTIME_DEPENDENCY_CONFLICT");
      return;
    }
    if (seen.size >= 32) throw Error("FIXTURE_RUNTIME_DEPENDENCY_COUNT_LIMIT");
    const entry = { origin, version: pkg.version, tree: "" };
    seen.set(name, entry);
    const copied: { path: string; sha256: string; bytes: number }[] = [];
    const destination = join(target, "node_modules", name);
    async function copy(directory: string) {
      for (const item of await readdir(directory, { withFileTypes: true })) {
        // Runtime edges are resolved independently below; never dereference/copy an unqualified alias.
        if (item.name === "node_modules") continue;
        const path = join(directory, item.name), suffix = relative(origin, path);
        if (item.isSymbolicLink()) throw Error("FIXTURE_RUNTIME_PACKAGE_SYMLINK_REFUSED");
        if (item.isDirectory()) { await copy(path); continue; }
        const before = await lstat(path);
        if (!before.isFile() || before.size > 1_048_576 || copied.length + pins.length >= 4096) throw Error("FIXTURE_RUNTIME_FILE_LIMIT");
        const bytes = await readFile(path), after = await lstat(path);
        if (before.ino !== after.ino || before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs || bytes.length !== before.size) throw Error("FIXTURE_RUNTIME_SOURCE_CHANGED");
        total += bytes.length;
        if (total > 32 * 1024 * 1024) throw Error("FIXTURE_RUNTIME_BYTE_LIMIT");
        await mkdir(dirname(join(destination, suffix)), { recursive: true });
        await writeFile(join(destination, suffix), bytes, { flag: "wx", mode: before.mode & 0o777 });
        if (hash(await readFile(path)) !== hash(bytes) || hash(await readFile(join(destination, suffix))) !== hash(bytes)) throw Error("FIXTURE_RUNTIME_COPY_MISMATCH");
        copied.push({ path: suffix, sha256: hash(bytes), bytes: bytes.length });
      }
    }
    await copy(origin);
    if (hash(await readFile(manifestPath)) !== hash(manifestBytes)) throw Error("FIXTURE_RUNTIME_MANIFEST_CHANGED");
    entry.tree = hash(JSON.stringify(copied.sort((a, b) => a.path.localeCompare(b.path))));
    pins.push(...copied.map((pin) => ({ ...pin, path: relative(target, join(destination, pin.path)) })));
    const required = { ...pkg.dependencies, ...pkg.peerDependencies };
    for (const child of Object.keys(required).sort()) {
      // Optional absent peers are not fabricated as installed dependencies. An installed peer is copied.
      if (pkg.peerDependenciesMeta?.[child]?.optional) {
        try { Bun.resolveSync(child + "/package.json", origin); } catch { continue; }
      }
      await stage(child, origin);
    }
    for (const child of Object.keys(pkg.optionalDependencies ?? {}).sort()) {
      try { Bun.resolveSync(child + "/package.json", origin); } catch { continue; }
      await stage(child, origin);
    }
  }
  await mkdir(join(target, "node_modules/@palantirkc"), { recursive: true });
  // All three admitted source adapters depend on the immutable Ontology runtime closure.
  await stage("@palantirkc/ontology", resolve(source, "packages/work-continuity"));
  return {
    packages: [...seen].map(([name, value]) => ({ name, version: value.version, treeSha256: value.tree })),
    pins,
  };
}
