/** Exclusive synthetic source owner; never install as native authority. */
import { mkdtemp, mkdir, writeFile, readdir, chmod, rm } from "node:fs/promises";
import { join, dirname } from "node:path";
import { tmpdir } from "node:os";
import {
  normalizeContent,
  contentDigest,
  locationDigest,
  providerDigest,
  providerBytes,
  recoveryDigest,
  sourceHash,
  type SourceContentBinding,
  type SourceBindingInput,
  type OriginalSourceRecovery,
  type SourceProviderId,
} from "../source-contract.ts";
import {
  materializeSourceDistribution,
  type NativeSourceDistributionOwner,
} from "../source-distribution.ts";
import {
  observeSourceProcess,
  type NativeSourceReadOwner,
  type NativeSourceSelection,
} from "../source-runtime.ts";
export const fixtureRoots: string[] = [];
export async function cleanupSourceFixtures() {
  for (const root of fixtureRoots.splice(0)) {
    async function writable(path: string) {
      await chmod(path, 0o700);
      for (const entry of await readdir(path, { withFileTypes: true })) {
        if (entry.isSymbolicLink()) continue;
        const child = join(path, entry.name);
        if (entry.isDirectory()) await writable(child);
        else await chmod(child, 0o600);
      }
    }
    await writable(root);
    await rm(root, { recursive: true });
  }
}
export async function sourceFixture(options: { provider?: SourceProviderId; text?: string } = {}) {
  const root = await mkdtemp(join(tmpdir(), "s03-source-fixture-"));
  fixtureRoots.push(root);
  const sourceRoot = join(root, "source"),
    targetParent = join(root, "releases");
  await mkdir(sourceRoot);
  await mkdir(targetParent);
  const values = new Map([
    ["src/api.ts", options.text ?? 'export const value="source-v1";\n'],
    ["tools/loader.bin", "synthetic-observed-loader-image"],
  ]);
  for (const [path, bytes] of values) {
    await mkdir(dirname(join(sourceRoot, path)), { recursive: true });
    await writeFile(join(sourceRoot, path), bytes);
  }
  const content: SourceContentBinding = normalizeContent({
    schema: "common.source-content.v1",
    repositoryId: "fixture:source-owner",
    commit: "a".repeat(40),
    tree: "b".repeat(40),
    files: [...values].map(([path, text]) => ({
      path,
      sha256: sourceHash(text),
      bytes: Buffer.byteLength(text),
      mode: 0o444,
      class: path.startsWith("tools/") ? "tool" : "source",
    })),
    entries: [{ id: "api", path: "src/api.ts" }],
    dependencies: [],
    tools: [{ id: "synthetic-loader", version: "1", path: "tools/loader.bin" }],
  });
  const state = {
    active: true,
    main: "a".repeat(40),
    currentGeneration: "generation-1",
    snapshotChecks: 0,
    beforeSnapshotCheck: undefined as undefined | (() => Promise<void>),
    currentSelection: undefined as NativeSourceSelection | undefined,
    recoverySelection: undefined as NativeSourceSelection | undefined,
    toolHash: sourceHash(values.get("tools/loader.bin")!),
  };
  const distributionOwner: NativeSourceDistributionOwner = {
    async withSnapshot(expected, consume) {
      return consume({
        sourceRoot,
        targetParent,
        repositoryId: content.repositoryId,
        commit: content.commit,
        tree: content.tree,
        async readCommitted(path) {
          const bytes = values.get(path);
          if (bytes === undefined) throw Error("SYNTHETIC_COMMITTED_BLOB_ABSENT");
          return {
            commit: content.commit,
            tree: content.tree,
            path,
            bytes: new TextEncoder().encode(bytes),
          };
        },
        async assertCurrent() {
          state.snapshotChecks++;
          await state.beforeSnapshotCheck?.();
          if (!state.active || state.main !== content.commit)
            throw Error("SYNTHETIC_SNAPSHOT_CHANGED");
        },
      });
    },
  };
  const delivery = await materializeSourceDistribution(content, distributionOwner);
  const identity = await observeSourceProcess("synthetic-profile", "synthetic-source-owner");
  const location = {
    schema: "common.source-location.v1" as const,
    contentDigest: contentDigest(content),
    root: delivery.root,
    kind: "immutable-release" as const,
    identity,
    readOnly: true as const,
  };
  const registration = {
    schema: "common.source-provider.v1" as const,
    id: options.provider ?? "codex-context",
    contentDigest: location.contentDigest,
    locationDigest: locationDigest(location),
    generation: "generation-1",
    entries: [{ role: "api", entry: "api" }],
    authority: "none" as const,
    nativePermissionGranted: false as const,
  };
  const registrationPath = join(root, "bindings", "provider.json");
  await mkdir(dirname(registrationPath));
  await writeFile(registrationPath, providerBytes(registration));
  const binding: SourceBindingInput = {
    content,
    location,
    registration,
    registrationPath,
    mode: "current",
  };
  const selection = (b: SourceBindingInput): NativeSourceSelection => ({
    providerId: b.registration.id,
    mode: b.mode,
    contentDigest: contentDigest(b.content),
    locationDigest: locationDigest(b.location),
    registrationDigest: providerDigest(b.registration),
    generation: b.registration.generation,
    recoveryDigest: b.recovery ? recoveryDigest(b.recovery) : null,
  });
  state.currentSelection = selection(binding);
  const owner: NativeSourceReadOwner = {
    async withSourceRead(request, consume) {
      const observed =
        request.mode === "current" ? state.currentSelection : state.recoverySelection;
      if (!observed || request.providerId !== observed.providerId)
        throw Error("SYNTHETIC_NATIVE_SELECTION_ABSENT");
      const selected = structuredClone(observed),
        generation = state.currentGeneration;
      const lease = {
        selection: selected,
        authenticatedProfile: "synthetic-profile",
        authenticatedRole: "synthetic-source-owner",
        loadedTools: [{ id: "synthetic-loader", version: "1", sha256: state.toolHash }],
        assertCurrent() {
          if (
            !state.active ||
            (request.mode === "current" && generation !== state.currentGeneration)
          )
            throw Error("SYNTHETIC_NATIVE_SOURCE_REVOKED");
        },
      };
      const out = await consume(lease);
      lease.assertCurrent();
      return out;
    },
  };
  async function admitRecovery() {
    const recovery: OriginalSourceRecovery = {
      schema: "common.original-source-recovery.v1",
      providerId: registration.id,
      contentDigest: location.contentDigest,
      locationDigest: locationDigest(location),
      registrationDigest: providerDigest(registration),
      originalReceipt: {
        owner: "synthetic-original-owner",
        uri: "fixture://original-receipt/one",
        revision: "1",
        sha256: sourceHash("synthetic-original-receipt"),
      },
    };
    const old: SourceBindingInput = { ...structuredClone(binding), mode: "recovery", recovery };
    state.recoverySelection = selection(old);
    return old;
  }
  return {
    root,
    sourceRoot,
    targetParent,
    content,
    binding,
    owner,
    state,
    distributionOwner,
    selection,
    delivery,
    admitRecovery,
  };
}
