/** Canonical exact filesystem observer for source v1. Never traverses a native
 * data root or executes a file. Directory/file descriptors defeat path aliases. */
import { constants } from "node:fs";
import { open, lstat, realpath, readdir } from "node:fs/promises";
import { dirname, basename, join } from "node:path";
import {
  sourceAbsolutePath,
  sourceLimits,
  sourceHash,
  type SourceContentBinding,
} from "./source-contract.ts";
export async function openSourceDirectory(path: string) {
  sourceAbsolutePath(path);
  let fd = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    for (const part of path.split("/").slice(1).filter(Boolean)) {
      const next = await open(
        "/proc/self/fd/" + fd.fd + "/" + part,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await fd.close();
      fd = next;
    }
    const held = await fd.stat(),
      named = await lstat(path);
    if (
      !named.isDirectory() ||
      held.dev !== named.dev ||
      held.ino !== named.ino ||
      (await realpath(path)) !== path
    )
      throw Error("SOURCE_DIRECTORY_ALIAS_OR_DRIFT");
    return fd;
  } catch (error) {
    await fd.close();
    throw error;
  }
}
export async function readExactSource(
  path: string,
  max: number = sourceLimits.fileBytes,
): Promise<Uint8Array> {
  const parent = await openSourceDirectory(dirname(sourceAbsolutePath(path)));
  try {
    const fd = await open(
      "/proc/self/fd/" + parent.fd + "/" + basename(path),
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    try {
      const before = await fd.stat();
      if (!before.isFile() || before.nlink < 1 || before.nlink > 64 || before.size > max)
        throw Error("SOURCE_BOUNDED_REGULAR_FILE_REQUIRED");
      const bytes = Buffer.alloc(before.size + 1);
      let count = 0;
      while (count < bytes.length) {
        const n = await fd.read(bytes, count, bytes.length - count, null);
        if (!n.bytesRead) break;
        count += n.bytesRead;
      }
      const after = await fd.stat(),
        named = await lstat(path),
        namedParent = await lstat(dirname(path)),
        heldParent = await parent.stat();
      if (
        count !== before.size ||
        !named.isFile() ||
        named.isSymbolicLink() ||
        named.dev !== before.dev ||
        named.ino !== before.ino ||
        namedParent.dev !== heldParent.dev ||
        namedParent.ino !== heldParent.ino ||
        before.dev !== after.dev ||
        before.ino !== after.ino ||
        before.size !== after.size ||
        before.mtimeMs !== after.mtimeMs ||
        before.ctimeMs !== after.ctimeMs ||
        (await realpath(path)) !== path
      )
        throw Error("SOURCE_CHANGED_DURING_EXACT_READ");
      return bytes.subarray(0, count);
    } finally {
      await fd.close();
    }
  } finally {
    await parent.close();
  }
}
export async function verifySourceTree(
  root: string,
  content: SourceContentBinding,
  manifestBytes: string,
  immutable = true,
) {
  const held = await openSourceDirectory(root);
  try {
    const actual: string[] = [];
    const expected = new Set([...content.files.map((pin) => pin.path), "source-content.json"]);
    const allowedDirectories = new Set<string>();
    for (const file of expected) {
      let p = dirname(file);
      while (p !== ".") {
        allowedDirectories.add(p);
        p = dirname(p);
      }
    }
    async function visit(path: string, prefix: string) {
      const dir = await openSourceDirectory(path);
      try {
        const stat = await dir.stat();
        if (immutable && (stat.mode & 0o222) !== 0)
          throw Error("SOURCE_RELEASE_DIRECTORY_WRITABLE");
        for (const entry of await readdir("/proc/self/fd/" + dir.fd, { withFileTypes: true })) {
          const name = prefix + entry.name;
          if (entry.isSymbolicLink()) throw Error("SOURCE_TREE_SYMLINK");
          if (entry.isDirectory()) {
            if (!allowedDirectories.has(name)) throw Error("SOURCE_UNDECLARED_DIRECTORY");
            await visit(join(path, entry.name), name + "/");
          } else if (entry.isFile()) {
            if (!expected.has(name)) throw Error("SOURCE_UNDECLARED_FILE");
            actual.push(name);
          } else throw Error("SOURCE_NON_REGULAR_MEMBER");
          if (actual.length > sourceLimits.files + 1) throw Error("SOURCE_TREE_BOUND");
        }
      } finally {
        await dir.close();
      }
    }
    await visit(root, "");
    if (actual.length !== expected.size) throw Error("SOURCE_INCOMPLETE_RELEASE");
    if (
      immutable &&
      (((await lstat(join(root, "source-content.json"))).mode & 0o777) !== 0o444 ||
        (await lstat(join(root, "source-content.json"))).nlink !== 1)
    )
      throw Error("SOURCE_MANIFEST_MODE_MISMATCH");
    if (
      Buffer.from(
        await readExactSource(join(root, "source-content.json"), sourceLimits.metadataBytes),
      ).toString("utf8") !== manifestBytes
    )
      throw Error("SOURCE_MANIFEST_BYTES_MISMATCH");
    for (const pin of content.files) {
      const path = join(root, pin.path),
        bytes = await readExactSource(path),
        stat = await lstat(path);
      if (
        bytes.byteLength !== pin.bytes ||
        sourceHash(bytes) !== pin.sha256 ||
        (immutable && ((stat.mode & 0o777) !== pin.mode || stat.nlink !== 1))
      )
        throw Error("SOURCE_MEMBER_BYTES_OR_MODE_MISMATCH");
    }
    const after = await openSourceDirectory(root);
    try {
      const a = await held.stat(),
        b = await after.stat();
      if (a.dev !== b.dev || a.ino !== b.ino) throw Error("SOURCE_ROOT_CHANGED");
    } finally {
      await after.close();
    }
  } finally {
    await held.close();
  }
}
