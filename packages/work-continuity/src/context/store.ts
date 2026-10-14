import { mkdir, readFile, rename, rm, open, realpath, lstat } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { randomUUID } from "node:crypto";
import type { CheckpointRead, ContextIo, ContinuityCheckpoint } from "./types";
import {
  assertAbsolutePath,
  checkSources,
  decode,
  insideRoot,
  localContextIo,
  readExact,
  sha256,
} from "./source";
import { validateCheckpoint } from "./validation";

export async function readCheckpoint(
  path: string,
  options: { root?: string; verifySources?: boolean; io?: ContextIo } = {},
): Promise<CheckpointRead> {
  const io = options.io ?? localContextIo;
  assertAbsolutePath(path);
  // When no root is supplied the checkpoint declares ownership, then canonical containment is verified before use.
  const raw = options.root ? await readExact(path, options.root, io) : await io.read(path);
  if (raw.byteLength > 2_097_152) throw Error("Checkpoint exceeds 2 MiB");
  const checkpoint = validateCheckpoint(JSON.parse(decode(raw)));
  const root = options.root ?? checkpoint.root;
  if (checkpoint.root !== root) throw Error("Checkpoint owner root differs from selected root");
  const bytes = options.root ? raw : await readExact(path, root, io);
  if (sha256(bytes) !== sha256(raw)) throw Error("Checkpoint changed during ownership read");
  const sourceStatus =
    options.verifySources === false
      ? {
          status: "repair-required" as const,
          checks: [],
          repair: [
            {
              path,
              action: "reobserve-source" as const,
              reason: "Source verification was explicitly skipped",
            },
          ],
          authority: "none" as const,
        }
      : await checkSources(checkpoint.sourcePins, root, io);
  if (sha256(await readExact(path, root, io)) !== sha256(bytes))
    throw Error("Checkpoint changed during source verification");
  return { checkpoint, sha256: sha256(bytes), sourceStatus };
}

function assertTransition(
  before: ContinuityCheckpoint | undefined,
  after: ContinuityCheckpoint,
  expectedVersion: number,
) {
  if (after.version !== expectedVersion + 1)
    throw Error("Checkpoint version must increment expectedVersion exactly once");
  if (!before) {
    if (expectedVersion !== 0) throw Error("Missing predecessor checkpoint");
    return;
  }
  if (before.version !== expectedVersion) throw Error("Checkpoint version conflict");
  for (const key of ["projectId", "workId", "root"] as const)
    if (before[key] !== after[key]) throw Error("Checkpoint identity cannot change");
  if (Date.parse(after.updatedAt) < Date.parse(before.updatedAt))
    throw Error("Checkpoint time cannot move backwards");
  for (const prior of before.completed) {
    const retained = after.completed.find((row) => row.id === prior.id);
    if (!retained || JSON.stringify(prior) !== JSON.stringify(retained))
      throw Error("Completed effects are immutable and must be retained");
  }
  for (const prior of before.userDecisions) {
    const retained = after.userDecisions.find((row) => row.id === prior.id);
    if (
      !retained ||
      retained.answer !== prior.answer ||
      JSON.stringify(retained.source) !== JSON.stringify(prior.source) ||
      (prior.status === "superseded" && retained.status !== "superseded")
    )
      throw Error("Actual user answer provenance must be retained");
    if (
      prior.status === "accepted" &&
      retained.status === "superseded" &&
      !after.userDecisions.some((row) => row.supersedes === prior.id)
    )
      throw Error("Superseded decision requires a retained replacement");
  }
}
async function durableNewFile(path: string, bytes: Uint8Array, mode: number) {
  const handle = await open(path, "wx", mode);
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
}

/** Compare-and-swap under an exclusive lock; every successor is an immutable work-owned snapshot. */
export async function writeCheckpoint(
  path: string,
  input: unknown,
  options: { root: string; expectedVersion: number; expectedHash?: string },
): Promise<{ path: string; sha256: string; version: number; snapshotPath: string }> {
  const checkpoint = validateCheckpoint(input);
  assertAbsolutePath(path);
  assertAbsolutePath(options.root);
  if (
    checkpoint.root !== options.root ||
    !insideRoot(options.root, path) ||
    (await realpath(options.root)) !== options.root
  )
    throw Error("Checkpoint write requires exact owning root");
  if (!Number.isSafeInteger(options.expectedVersion) || options.expectedVersion < 0)
    throw Error("Expected version required");
  await mkdir(dirname(path), { recursive: true });
  if ((await realpath(dirname(path))) !== dirname(path))
    throw Error("Checkpoint parent must be canonical");
  const lock = `${path}.lock`;
  try {
    await mkdir(lock, { mode: 0o700 });
  } catch (error) {
    if (typeof error === "object" && error !== null && "code" in error && error.code === "EEXIST")
      throw Error("Checkpoint writer conflict; inspect existing lock rather than removing it");
    throw error;
  }
  let temporary: string | undefined;
  try {
    let previous: ContinuityCheckpoint | undefined;
    let previousHash: string | undefined;
    let mode = 0o600;
    try {
      const metadata = await lstat(path);
      if (!metadata.isFile() || metadata.isSymbolicLink())
        throw Error("Checkpoint must be a regular canonical file");
      const bytes = await readFile(path);
      previousHash = sha256(bytes);
      previous = validateCheckpoint(JSON.parse(decode(bytes)));
      mode = metadata.mode & 0o777;
    } catch (error) {
      if (!(
        typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "ENOENT"
      ))
        throw error;
    }
    if (options.expectedHash !== undefined && options.expectedHash !== previousHash)
      throw Error("Checkpoint predecessor hash conflict");
    assertTransition(previous, checkpoint, options.expectedVersion);
    const sourceStatus = await checkSources(checkpoint.sourcePins, options.root, localContextIo);
    if (sourceStatus.status !== "current")
      throw Error(
        `Checkpoint sources require repair: ${sourceStatus.repair.map((row) => `${row.path}:${row.reason}`).join(", ")}`,
      );
    const bytes = new TextEncoder().encode(`${JSON.stringify(checkpoint, null, 2)}\n`);
    if (bytes.length > 2_097_152) throw Error("Checkpoint exceeds 2 MiB");
    const digest = sha256(bytes);
    const historyRoot = join(dirname(path), "history", basename(path, ".json"));
    await mkdir(historyRoot, { recursive: true, mode: 0o700 });
    if ((await realpath(historyRoot)) !== historyRoot)
      throw Error("Checkpoint history must be canonical");
    const snapshotPath = join(
      historyRoot,
      `v${String(checkpoint.version).padStart(6, "0")}-${digest}.json`,
    );
    try {
      await durableNewFile(snapshotPath, bytes, mode);
    } catch (error) {
      // A failed pre-publication attempt may already have left this exact immutable successor.
      if (!(
        typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "EEXIST"
      ))
        throw error;
      if (
        (await realpath(snapshotPath)) !== snapshotPath ||
        sha256(await readFile(snapshotPath)) !== digest
      )
        throw Error("Existing snapshot conflicts with immutable successor");
    }
    temporary = join(dirname(path), `.${basename(path)}.${randomUUID()}.tmp`);
    await durableNewFile(temporary, bytes, mode);
    // Verify pins again after candidate persistence and before publishing the successor.
    if (
      (await checkSources(checkpoint.sourcePins, options.root, localContextIo)).status !== "current"
    )
      throw Error("Checkpoint source changed before publication");
    await rename(temporary, path);
    temporary = undefined;
    const directory = await open(dirname(path), "r");
    try {
      await directory.sync();
    } finally {
      await directory.close();
    }
    return { path, sha256: digest, version: checkpoint.version, snapshotPath };
  } finally {
    if (temporary) await rm(temporary, { force: true });
    await rm(lock, { recursive: true });
  }
}
