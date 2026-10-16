/** Single canonical finite dependency observer shared by v2 and the generated v1 guard.
 * The generator embeds this byte-exact builtin-only module as a projection, never a second implementation. */
import { readdir as dependencyReaddir, realpath as dependencyRealpath } from "node:fs/promises";
import {
  dirname as dependencyDirname,
  isAbsolute as dependencyIsAbsolute,
  join as dependencyJoin,
  relative as dependencyRelative,
} from "node:path";

export interface RuntimeDependencyPin {
  path: string;
  sha256: string;
  bytes: number;
}
export interface RuntimeDependency {
  name: string;
  version: string;
  root: string;
}
export async function captureInstalledRuntimeClosure(
  root: string,
  read: (path: string, maxBytes: number) => Promise<Uint8Array>,
  digest: (bytes: Uint8Array | string) => string,
  includeWorkContinuity = true,
): Promise<{ dependencies: RuntimeDependency[]; dependencyPins: RuntimeDependencyPin[] }> {
  const dependencies: RuntimeDependency[] = [],
    dependencyPins: RuntimeDependencyPin[] = [];
  const seen = new Set<string>();
  let totalBytes = 0;
  function within(base: string, path: string) {
    const name = dependencyRelative(base, path);
    return name !== "" && !name.startsWith("..") && !dependencyIsAbsolute(name);
  }
  async function correspondence(packageRoot: string, installed: RuntimeDependencyPin[]) {
    const releasePath = dependencyJoin(root, "releases/palantirkc-ontology-0.2.0.tgz");
    const release = await read(releasePath, 1048576);
    // This immutable release pin is existing Upper Ontology0.2.0, never a newly authored release.
    if (digest(release) !== "3f9995d0b91d2ad39dfc7cedbb6e35b1cb9d29c2e57223ca0c076d2030319406")
      throw Error("IMMUTABLE_ONTOLOGY_RELEASE_CLOSURE_DRIFT");
    const archive = await new Bun.Archive(release).files();
    if (!archive.size || archive.size > 2048) throw Error("IMMUTABLE_RELEASE_FILE_LIMIT");
    const expected = new Map<string, string>();
    let bytes = 0;
    for (const [path, file] of archive) {
      if (
        !path.startsWith("package/") ||
        path.includes("\\") ||
        path.split("/").some((part) => !part || part === "." || part === "..") ||
        file.size > 1048576
      )
        throw Error("INVALID_IMMUTABLE_RELEASE_MEMBER");
      bytes += file.size;
      if (bytes > 32 * 1024 * 1024) throw Error("IMMUTABLE_RELEASE_BYTE_LIMIT");
      const name = path.slice("package/".length);
      if (expected.has(name)) throw Error("DUPLICATE_IMMUTABLE_RELEASE_MEMBER");
      expected.set(name, digest(new Uint8Array(await file.arrayBuffer())));
    }
    const observed = new Map(
      installed.map((pin) => [
        dependencyRelative(packageRoot, dependencyJoin(root, pin.path)),
        pin.sha256,
      ]),
    );
    // The fixture generator never creates an owner project record. An immutable package's non-executable
    // project metadata may consequently be absent, but if installed it must match the same release.
    for (const [path, hash] of expected)
      if (!(path === "project.json" && !observed.has(path)) && observed.get(path) !== hash)
        throw Error("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
    for (const [path, hash] of observed)
      if (expected.get(path) !== hash) throw Error("IMMUTABLE_ONTOLOGY_INSTALL_CLOSURE_DRIFT");
  }
  async function visit(name: string, from: string) {
    if (!/^(@[a-z0-9-]+\/)?[a-z0-9-]+$/u.test(name))
      throw Error("INVALID_PLATFORM_DEPENDENCY_NAME");
    const packagePath = await dependencyRealpath(Bun.resolveSync(name + "/package.json", from));
    const packageRoot = dependencyDirname(packagePath);
    if (
      name === "@palantirkc/work-continuity" &&
      packageRoot === dependencyJoin(root, "packages/work-continuity")
    )
      return;
    if (!within(dependencyJoin(root, "node_modules"), packagePath))
      throw Error("PLATFORM_DEPENDENCY_ESCAPED_OWNER");
    if (seen.has(packageRoot)) return;
    seen.add(packageRoot);
    if (seen.size > 32) throw Error("PLATFORM_DEPENDENCY_COUNT_LIMIT");
    const manifest: unknown = JSON.parse(Buffer.from(await read(packagePath, 65536)).toString());
    if (!manifest || typeof manifest !== "object" || Array.isArray(manifest))
      throw Error("INVALID_PLATFORM_DEPENDENCY");
    const data = manifest as Record<string, unknown>;
    if (
      data.name !== name ||
      typeof data.version !== "string" ||
      !data.version ||
      (name === "@palantirkc/ontology" && data.version !== "0.2.0")
    )
      throw Error("PLATFORM_DEPENDENCY_IDENTITY_DRIFT");
    dependencies.push({ name, version: data.version, root: dependencyRelative(root, packageRoot) });
    const installed: RuntimeDependencyPin[] = [];
    async function walk(directory: string) {
      for (const entry of await dependencyReaddir(directory, { withFileTypes: true })) {
        if (entry.name === "node_modules") continue;
        const path = dependencyJoin(directory, entry.name);
        if (entry.isSymbolicLink()) throw Error("UNADMITTED_PLATFORM_DEPENDENCY_SYMLINK");
        if (entry.isDirectory()) await walk(path);
        else if (entry.isFile()) {
          if (dependencyPins.length >= 2048) throw Error("PLATFORM_DEPENDENCY_FILE_LIMIT");
          const bytes = await read(path, 1048576);
          totalBytes += bytes.length;
          if (totalBytes > 32 * 1024 * 1024) throw Error("PLATFORM_DEPENDENCY_BYTE_LIMIT");
          const pin = {
            path: dependencyRelative(root, path),
            sha256: digest(bytes),
            bytes: bytes.length,
          };
          dependencyPins.push(pin);
          installed.push(pin);
        } else throw Error("UNADMITTED_PLATFORM_DEPENDENCY_FILE");
      }
    }
    await walk(packageRoot);
    if (name === "@palantirkc/ontology") await correspondence(packageRoot, installed);
    if (data.dependencies !== undefined) {
      if (
        !data.dependencies ||
        typeof data.dependencies !== "object" ||
        Array.isArray(data.dependencies)
      )
        throw Error("INVALID_PLATFORM_DEPENDENCIES");
      const children = Object.keys(data.dependencies);
      if (children.length > 32) throw Error("PLATFORM_DEPENDENCY_COUNT_LIMIT");
      for (const child of children.sort()) await visit(child, packageRoot);
    }
  }
  await visit("@palantirkc/ontology", root);
  if (includeWorkContinuity)
    await visit(
      "@palantirkc/work-continuity",
      dependencyJoin(root, "packages/native-hermes-adapter"),
    );
  return {
    dependencies: dependencies.sort((a, b) => a.name.localeCompare(b.name)),
    dependencyPins: dependencyPins.sort((a, b) => a.path.localeCompare(b.path)),
  };
}
