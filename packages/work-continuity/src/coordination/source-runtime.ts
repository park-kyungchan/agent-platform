/** Native source-read boundary. No runtime loop, store, activation or authority
 * registry. Native owners authenticate role/profile, current selection, recovery
 * receipts and actual loaded tool image; JSON only supplies records to join. */
import { hostname } from "node:os";
import { readlink } from "node:fs/promises";
import { join, dirname, relative, isAbsolute, resolve } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import {
  normalizeBinding,
  contentDigest,
  contentBytes,
  locationDigest,
  providerDigest,
  providerBytes,
  recoveryDigest,
  sourceHash,
  verifySourceDependencyManifest,
  type SourceIdentity,
  type SourceBindingInput,
  type SourceProviderId,
} from "./source-contract.ts";
import { openSourceDirectory, readExactSource, verifySourceTree } from "./source-files.ts";
import { restrictedSourceImports, restrictedSourceBuiltins } from "./source-syntax.ts";
export interface NativeSourceSelection {
  providerId: SourceProviderId;
  mode: "current" | "recovery";
  contentDigest: string;
  locationDigest: string;
  registrationDigest: string;
  generation: string;
  recoveryDigest: string | null;
}
export interface NativeSourceReadLease {
  selection: Readonly<NativeSourceSelection>;
  authenticatedProfile: string;
  authenticatedRole: string;
  /** Actual owner-observed loader image identities, not request/tool JSON. */
  loadedTools: readonly { id: string; version: string; sha256: string }[];
  /** Native owner holds the source/policy/mount selection until consumption ends. */
  assertCurrent(): void | Promise<void>;
}
export interface NativeSourceReadOwner {
  withSourceRead<T>(
    request: Readonly<{ providerId: SourceProviderId; mode: "current" | "recovery" }>,
    consume: (lease: NativeSourceReadLease) => Promise<T>,
  ): Promise<T>;
}
export async function observeSourceProcess(profile: string, role: string): Promise<SourceIdentity> {
  if (typeof process.getuid !== "function" || typeof process.getgid !== "function")
    throw Error("SOURCE_PROCESS_IDENTITY_UNSUPPORTED");
  return {
    host: hostname(),
    mountNamespace: await readlink("/proc/self/ns/mnt"),
    uid: process.getuid(),
    gid: process.getgid(),
    profile,
    role,
  };
}
const loadedImages = new Map<string, string>();
const rejectedImages = new Set<string>();
function filePath(root: string, path: string) {
  const value = resolve(root, path);
  if (relative(root, value).startsWith("..") || isAbsolute(relative(root, value)))
    throw Error("SOURCE_PATH_ESCAPE");
  return value;
}
function cachePaths(): Set<string> {
  if (!require.cache || typeof require.cache !== "object")
    throw Error("SOURCE_MODULE_CACHE_OBSERVATION_UNSUPPORTED");
  return new Set(
    Object.keys(require.cache).map((path) =>
      path.startsWith("file:") ? fileURLToPath(path) : path,
    ),
  );
}
async function verifyExecutableGraph(binding: SourceBindingInput) {
  const { content, location } = binding,
    declared = new Map(content.files.map((pin) => [filePath(location.root, pin.path), pin]));
  for (const dep of content.dependencies) {
    verifySourceDependencyManifest(
      await readExactSource(filePath(location.root, dep.manifest)),
      dep,
    );
  }
  const seen = new Set<string>();
  async function visit(path: string) {
    if (seen.has(path)) return;
    seen.add(path);
    const pin = declared.get(path);
    if (!pin || !["source", "dependency"].includes(pin.class))
      throw Error("SOURCE_UNDECLARED_EXECUTABLE_EDGE");
    if (path.endsWith(".json")) return;
    if (!/\.[cm]?[jt]s$/u.test(path)) throw Error("SOURCE_UNSUPPORTED_EXECUTABLE_FORMAT");
    const text = new TextDecoder("utf-8", { fatal: true }).decode(await readExactSource(path));
    // Complete restricted grammar admission, not a claim that scanImports plus
    // loader-name regexes describe arbitrary JavaScript execution. Parse original
    // bytes before any transpiler/macro/config/compiler operation can observe it.
    for (const specifier of restrictedSourceImports(text)) {
      if (restrictedSourceBuiltins.includes(specifier)) continue;
      if (specifier.startsWith("node:") || specifier === "bun" || specifier.startsWith("bun:"))
        throw Error("SOURCE_UNSUPPORTED_LOADER_CAPABILITY");
      if (!specifier.startsWith(".")) {
        const dep = content.dependencies.find(
          (item) => specifier === item.name || specifier.startsWith(item.name + "/"),
        );
        if (!dep) throw Error("SOURCE_UNDECLARED_PACKAGE_EDGE");
        const selected = Bun.resolveSync(specifier, dirname(path));
        if (!selected.startsWith(filePath(location.root, dep.root) + "/"))
          throw Error("SOURCE_PACKAGE_RESOLUTION_ESCAPE");
        await visit(selected);
      } else await visit(Bun.resolveSync(specifier, dirname(path)));
    }
  }
  for (const entry of content.entries) await visit(filePath(location.root, entry.path));
  return seen;
}
export interface QualifiedSourceView {
  readonly proof: {
    schema: "common.qualified-source.v1";
    contentDigest: string;
    locationDigest: string;
    registrationDigest: string;
    generation: string;
    mode: "current" | "recovery";
    loaderContract: "restricted-esm-expression.v1";
    nativePermissionGranted: false;
  };
  read(path: string): Promise<Uint8Array>;
  load(entry: string): Promise<Readonly<Record<string, unknown>>>;
  verify(): Promise<void>;
}
export async function withQualifiedSource<T>(
  input: SourceBindingInput,
  owner: NativeSourceReadOwner,
  consume: (source: QualifiedSourceView) => T | Promise<T>,
): Promise<T> {
  if (!owner || typeof owner.withSourceRead !== "function")
    throw Error("SOURCE_AUTHENTICATED_NATIVE_OWNER_REQUIRED");
  const binding = normalizeBinding(input),
    request = { providerId: binding.registration.id, mode: binding.mode };
  return owner.withSourceRead(request, async (lease) => {
    const expected = {
      providerId: binding.registration.id,
      mode: binding.mode,
      contentDigest: contentDigest(binding.content),
      locationDigest: locationDigest(binding.location),
      registrationDigest: providerDigest(binding.registration),
      generation: binding.registration.generation,
      recoveryDigest: binding.recovery ? recoveryDigest(binding.recovery) : null,
    };
    const root = binding.location.root,
      image = sourceHash(JSON.stringify([root, binding.location.identity, expected.contentDigest]));
    let closed = false;
    const held = await openSourceDirectory(root);
    async function current() {
      if (closed) throw Error("SOURCE_VIEW_CLOSED");
      await lease.assertCurrent();
      if (
        Object.keys(expected).some(
          (key) =>
            lease.selection[key as keyof NativeSourceSelection] !==
            expected[key as keyof NativeSourceSelection],
        ) ||
        Object.keys(lease.selection).length !== Object.keys(expected).length
      )
        throw Error("SOURCE_CURRENT_OWNER_SELECTION_MISMATCH");
      if (
        JSON.stringify(
          await observeSourceProcess(lease.authenticatedProfile, lease.authenticatedRole),
        ) !== JSON.stringify(binding.location.identity)
      )
        throw Error("SOURCE_NAMESPACE_PRINCIPAL_PROFILE_ROLE_MISMATCH");
      for (const tool of binding.content.tools) {
        const pin = binding.content.files.find((row) => row.path === tool.path)!;
        if (
          lease.loadedTools.filter(
            (observed) =>
              observed.id === tool.id &&
              observed.version === tool.version &&
              observed.sha256 === pin.sha256,
          ).length !== 1
        )
          throw Error("SOURCE_ACTUAL_LOADED_TOOL_MISMATCH");
      }
      const named = await openSourceDirectory(root);
      try {
        const a = await held.stat(),
          b = await named.stat();
        if (a.dev !== b.dev || a.ino !== b.ino) throw Error("SOURCE_LOCATION_INODE_CHANGED");
      } finally {
        await named.close();
      }
    }
    async function verify() {
      await current();
      if (
        sourceHash(await readExactSource(binding.registrationPath, 2097152)) !==
          expected.registrationDigest ||
        Buffer.from(await readExactSource(binding.registrationPath, 2097152)).toString("utf8") !==
          providerBytes(binding.registration)
      )
        throw Error("SOURCE_PROVIDER_REGISTRATION_CHANGED");
      await verifySourceTree(root, binding.content, contentBytes(binding.content));
      await current();
    }
    const view: QualifiedSourceView = Object.freeze({
      proof: Object.freeze({
        schema: "common.qualified-source.v1",
        contentDigest: expected.contentDigest,
        locationDigest: expected.locationDigest,
        registrationDigest: expected.registrationDigest,
        generation: expected.generation,
        mode: binding.mode,
        loaderContract: "restricted-esm-expression.v1",
        nativePermissionGranted: false,
      }),
      verify,
      async read(path: string) {
        await verify();
        const pin = binding.content.files.find((row) => row.path === path);
        if (!pin) throw Error("SOURCE_UNDECLARED_READ");
        const bytes = await readExactSource(filePath(root, pin.path));
        if (sourceHash(bytes) !== pin.sha256) throw Error("SOURCE_READ_DRIFT");
        await current();
        return bytes;
      },
      async load(entryId: string) {
        const entry = binding.content.entries.find((row) => row.id === entryId);
        if (!entry || !binding.registration.entries.some((row) => row.entry === entryId))
          throw Error("SOURCE_UNDECLARED_ENTRY");
        const path = filePath(root, entry.path);
        if (rejectedImages.has(path)) throw Error("SOURCE_CHANGED_USE_FRESH_PROCESS");
        try {
          await verify();
          const graph = await verifyExecutableGraph(binding);
          await current();
          const cached = cachePaths();
          for (const module of graph) {
            const previous = loadedImages.get(module);
            if ((previous && previous !== image) || (cached.has(module) && previous !== image))
              throw Error("SOURCE_FIRST_IMPORT_OR_LOADED_IMAGE_MISMATCH");
          }
          const module: unknown = await import(pathToFileURL(path).href);
          // Defense in depth only; grammar/closure refusal above precedes any
          // payload evaluation, including loads of already cached outside code.
          for (const cachedPath of cachePaths())
            if (!cached.has(cachedPath) && !graph.has(cachedPath))
              throw Error("SOURCE_UNDECLARED_EXECUTED_MODULE");
          await verify();
          await verifyExecutableGraph(binding);
          if (!module || typeof module !== "object") throw Error("SOURCE_MODULE_EXPORTS_REQUIRED");
          for (const modulePath of graph) loadedImages.set(modulePath, image);
          return module as Readonly<Record<string, unknown>>;
        } catch (error) {
          rejectedImages.add(path);
          throw error;
        }
      },
    });
    try {
      await verify();
      const out = await consume(view);
      await verify();
      return out;
    } finally {
      closed = true;
      await held.close();
    }
  });
}
