import { mkdir, open, chmod, lstat, rename } from "node:fs/promises";
import { constants } from "node:fs";
import { join, dirname } from "node:path";
import {
  normalizeContent,
  contentBytes,
  contentDigest,
  sourceAbsolutePath,
  sourceHash,
  sourceInstalledSelection,
  verifySourceDependencyManifest,
  type SourceContentBinding,
  type SourceFilePin,
  type SourceInstalledSelection,
} from "./source-contract.ts";
import { openSourceDirectory, readExactSource, verifySourceTree } from "./source-files.ts";
export interface SourceSnapshotLease {
  sourceRoot: string;
  targetParent: string;
  repositoryId: string;
  commit: string;
  tree: string;
  /** Native source owner holds the selected Git/source snapshot and exclusive
   * target-parent writer through completion. No JSON field grants this lease. */
  assertCurrent(): void | Promise<void>;
  /** Existing native Git source owner reads the exact selected immutable blob;
   * dirty checkout bytes alone cannot establish commit/tree membership. */
  readCommitted(
    path: string,
  ): Promise<{ commit: string; tree: string; path: string; bytes: Uint8Array } | null>;
  /** Null from readCommitted means actual selected-tree absence, not a caller
   * classification. Native installed owner independently qualifies external
   * package/tool identity, complete inventory and immutable provenance. */
  readInstalled?(selection: Readonly<SourceInstalledSelection>): Promise<{
    selection: SourceInstalledSelection;
    provenance: { owner: string; uri: string; revision: string; sha256: string };
    bytes: Uint8Array;
  }>;
}
export interface NativeSourceDistributionOwner {
  withSnapshot<T>(
    expected: Readonly<SourceContentBinding>,
    consume: (lease: SourceSnapshotLease) => Promise<T>,
  ): Promise<T>;
}
/** Materializes only declared, exact committed inputs in a new immutable digest
 * directory. No install, code evaluation, Git mutation, pointer update, native
 * activation or publication. Unknown outcomes are reconciled by the owner. */
export async function materializeSourceDistribution(
  input: unknown,
  owner: NativeSourceDistributionOwner,
) {
  const content = normalizeContent(input),
    digest = contentDigest(content),
    manifest = contentBytes(content);
  if (Buffer.byteLength(manifest) > 2097152 || !owner || typeof owner.withSnapshot !== "function")
    throw Error("SOURCE_NATIVE_SNAPSHOT_OWNER_REQUIRED");
  return owner.withSnapshot(content, async (lease) => {
    if (
      lease.repositoryId !== content.repositoryId ||
      lease.commit !== content.commit ||
      lease.tree !== content.tree
    )
      throw Error("SOURCE_GIT_SNAPSHOT_MISMATCH");
    const sourceRoot = sourceAbsolutePath(lease.sourceRoot),
      parent = sourceAbsolutePath(lease.targetParent);
    await lease.assertCurrent();
    const source = await openSourceDirectory(sourceRoot),
      targetParent = await openSourceDirectory(parent);
    const target = join(parent, digest),
      stage = join(parent, ".staging-" + digest);
    let created = false;
    const installedSources = new Map<
      string,
      {
        selection: Omit<SourceInstalledSelection, "path">;
        provenance: { owner: string; uri: string; revision: string; sha256: string };
      }
    >();
    async function current() {
      await lease.assertCurrent();
      for (const [path, fd] of [
        [sourceRoot, source],
        [parent, targetParent],
      ] as const) {
        const held = await fd.stat(),
          named = await lstat(path);
        if (named.dev !== held.dev || named.ino !== held.ino || named.isSymbolicLink())
          throw Error("SOURCE_SNAPSHOT_PARENT_CHANGED");
      }
    }
    async function verifiedInput(pin: SourceFilePin) {
      await current();
      const original = await lease.readCommitted(pin.path);
      await current();
      let observed: Uint8Array;
      if (original) {
        if (
          original.commit !== content.commit ||
          original.tree !== content.tree ||
          original.path !== pin.path
        )
          throw Error("SOURCE_IMMUTABLE_GIT_BLOB_MISMATCH");
        observed = original.bytes;
      } else if (original === null) {
        if (!["dependency", "tool"].includes(pin.class) || !lease.readInstalled)
          throw Error("SOURCE_NATIVE_INSTALLED_PROVENANCE_REQUIRED");
        const selection = sourceInstalledSelection(content, pin);
        const installed = await lease.readInstalled(selection);
        await current();
        if (
          Object.keys(installed.selection).sort().join(",") !==
            Object.keys(selection).sort().join(",") ||
          Object.keys(selection).some(
            (key) =>
              installed.selection[key as keyof SourceInstalledSelection] !==
              selection[key as keyof SourceInstalledSelection],
          ) ||
          !installed.provenance.owner?.trim() ||
          !installed.provenance.uri?.trim() ||
          !installed.provenance.revision?.trim() ||
          installed.provenance.sha256 !== selection.inventoryDigest
        )
          throw Error("SOURCE_INSTALLED_PROVENANCE_MISMATCH");
        const { path: _, ...inventory } = selection;
        const receipt = {
          selection: inventory,
          provenance: {
            owner: installed.provenance.owner,
            uri: installed.provenance.uri,
            revision: installed.provenance.revision,
            sha256: installed.provenance.sha256,
          },
        };
        const key = JSON.stringify(inventory),
          previous = installedSources.get(key);
        if (previous && JSON.stringify(previous) !== JSON.stringify(receipt))
          throw Error("SOURCE_INSTALLED_PROVENANCE_CHANGED");
        installedSources.set(key, receipt);
        observed = installed.bytes;
      } else throw Error("SOURCE_EXACT_GIT_MEMBERSHIP_OBSERVATION_REQUIRED");
      if (observed.byteLength !== pin.bytes || sourceHash(observed) !== pin.sha256)
        throw Error("SOURCE_IMMUTABLE_GIT_BLOB_OR_INSTALLED_BYTES_MISMATCH");
      const bytes = await readExactSource(join(sourceRoot, pin.path));
      if (bytes.byteLength !== pin.bytes || sourceHash(bytes) !== pin.sha256)
        throw Error("SOURCE_SNAPSHOT_INPUT_MISMATCH");
      return bytes;
    }
    try {
      await current();
      try {
        await lstat(target);
        throw Error("SOURCE_TARGET_EXISTS_RECONCILE_NO_RETRY");
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
      // Validate every Git/external ownership join before the first output write.
      // Relabeling a tracked helper never exempts its committed bytes.
      for (const pin of content.files) await verifiedInput(pin);
      for (const dep of content.dependencies) {
        verifySourceDependencyManifest(await readExactSource(join(sourceRoot, dep.manifest)), dep);
      }
      // An existing stage means an original uncertain operation, never permission
      // to remove/restart it. The native owner reconciles its exact state.
      await mkdir(stage, { mode: 0o700 });
      created = true;
      for (const pin of content.files) {
        await current();
        const bytes = await verifiedInput(pin);
        const path = join(stage, pin.path);
        await mkdir(dirname(path), { recursive: true, mode: 0o700 });
        const held = await openSourceDirectory(dirname(path));
        try {
          const out = await open(
            "/proc/self/fd/" + held.fd + "/" + path.split("/").at(-1),
            constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
            0o600,
          );
          try {
            await out.writeFile(bytes);
            await out.chmod(pin.mode);
            await out.sync();
          } finally {
            await out.close();
          }
        } finally {
          await held.close();
        }
      }
      const mf = await open(
        join(stage, "source-content.json"),
        constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
        0o444,
      );
      try {
        await mf.writeFile(manifest);
        await mf.sync();
      } finally {
        await mf.close();
      }
      // Verify source snapshot again before finalizing: a moving checkout cannot
      // be laundered by copying each file from a different generation.
      for (const pin of content.files) {
        await current();
        await verifiedInput(pin);
      }
      const dirs = new Set<string>([stage]);
      for (const pin of content.files) {
        let p = dirname(join(stage, pin.path));
        while (p !== stage) {
          dirs.add(p);
          p = dirname(p);
        }
      }
      for (const path of [...dirs].sort((a, b) => b.length - a.length)) await chmod(path, 0o555);
      await verifySourceTree(stage, content, manifest);
      await current();
      // Exclusive writer lease is a native-owner obligation; a plain rename is
      // not advertised as a hostile-writer no-replace primitive.
      try {
        await lstat(target);
        throw Error("SOURCE_TARGET_CHANGED_RECONCILE");
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
      await rename(stage, target);
      await targetParent.sync();
      await current();
      await verifySourceTree(target, content, manifest);
      return {
        schema: "common.source-distribution-result.v1" as const,
        repositoryId: content.repositoryId,
        commit: content.commit,
        tree: content.tree,
        contentDigest: digest,
        root: target,
        fileCount: content.files.length,
        bytes: content.files.reduce((n, p) => n + p.bytes, 0),
        externalSources: [...installedSources.values()],
        nativePermissionGranted: false as const,
        published: false as const,
      };
    } catch (error) {
      if (created) throw new Error("SOURCE_DISTRIBUTION_INCOMPLETE_RECONCILE", { cause: error });
      throw error;
    } finally {
      await source.close();
      await targetParent.close();
    }
  });
}
