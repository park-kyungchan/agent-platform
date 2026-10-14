import { constants } from "node:fs";
import { open, realpath } from "node:fs/promises";
import { resolve } from "node:path";
import { hash, pin, requireValue, type Pin, type ReadPinned } from "./contracts";

/** Explicit input allowlist, bounded regular-file reads, no recursive resolution
 * of artifact references and no writes. Use the native caller's existing access. */
export function createPinnedReader(
  allowed: readonly Pin[],
  maxBytes = 2 * 1024 * 1024,
  baseDirectory = process.cwd(),
): ReadPinned {
  requireValue(
    Number.isSafeInteger(maxBytes) && maxBytes > 0 && maxBytes <= 8 * 1024 * 1024,
    "INVALID_READ_BUDGET",
    "maxBytes",
  );
  const pins = new Map<string, string>();
  const resolvePath = (path: string) => resolve(baseDirectory, path);
  for (const p of allowed) {
    pin(p);
    const path = resolvePath(p.path);
    requireValue(!pins.has(path) || pins.get(path) === p.sha256, "CONFLICTING_SOURCE_PINS", p.path);
    pins.set(path, p.sha256);
  }
  const read: ReadPinned = async (expected) => {
    pin(expected);
    const path = resolvePath(expected.path);
    requireValue(pins.get(path) === expected.sha256, "SOURCE_NOT_ADMITTED", expected.path);
    requireValue((await realpath(path)) === path, "SYMLINK_SOURCE", expected.path);
    const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const stat = await file.stat();
      requireValue(stat.isFile() && stat.size <= maxBytes, "INVALID_SOURCE_FILE", expected.path);
      const bytes = await file.readFile();
      const after = await file.stat();
      requireValue(
        bytes.length <= maxBytes &&
          after.size === stat.size &&
          after.mtimeMs === stat.mtimeMs &&
          hash(bytes) === expected.sha256,
        "SOURCE_PIN_CHANGED",
        expected.path,
      );
      return bytes;
    } finally {
      await file.close();
    }
  };
  read.resolvePath = resolvePath;
  return read;
}
