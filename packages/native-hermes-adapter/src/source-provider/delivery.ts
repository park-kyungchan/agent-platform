/** NEWsource initial delivery only. A plan digest is a review binding, never native admission. */
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import {
  lstat,
  mkdir,
  mkdtemp,
  open,
  readdir,
  readlink,
  realpath,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  composeHermesSourceProjection,
  generationPaths,
  generateHermesSourceFixture,
  modules,
  sourcePaths,
  verifySourceFixture,
} from "./generate.ts";
import { captureInstalledRuntimeClosure } from "./dependency-closure.ts";

const codeRoot = resolve(import.meta.dir, "../../../..");
const sha = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const json = (value: unknown) => JSON.stringify(value, null, 2) + "\n";
export const deliveryPaths = Object.freeze(
  [
    "package.json",
    "bun.lock",
    "architecture/hermes-context-provider.json",
    "source-successor.json",
    "src/ontology-driven/provider-source.ts",
    ...modules.map((name) => "src/" + name + ".ts"),
  ].sort(),
);
const directories = [
  "architecture",
  "src",
  "src/adoption",
  "src/ontology-driven",
  "node_modules",
  "node_modules/@palantirkc",
];
const linkPath = "node_modules/@palantirkc/native-hermes-adapter";
const packageName = "@palantirkc/hermes-new-source-facade";
const dependency = "file:../agent-platform/packages/native-hermes-adapter";

async function directory(path: string) {
  if (resolve(path) !== path) throw Error("EXACT_DELIVERY_DIRECTORY_REQUIRED");
  let handle = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    for (const part of path.split("/").slice(1)) {
      const next = await open(
        "/proc/self/fd/" + handle.fd + "/" + part,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await handle.close();
      handle = next;
    }
    const physical = await lstat(path),
      held = await handle.stat();
    if (
      (await realpath(path)) !== path ||
      !physical.isDirectory() ||
      physical.dev !== held.dev ||
      physical.ino !== held.ino
    )
      throw Error("DELIVERY_DIRECTORY_ALIAS");
    return handle;
  } catch (error) {
    await handle.close();
    throw error;
  }
}
async function sameDirectory(path: string, held: Awaited<ReturnType<typeof directory>>) {
  const named = await directory(path);
  try {
    const [actual, expected] = await Promise.all([named.stat(), held.stat()]);
    if (actual.dev !== expected.dev || actual.ino !== expected.ino)
      throw Error("DELIVERY_PARENT_CHANGED");
  } finally {
    await named.close();
  }
}
async function exact(path: string, max = 2 * 1024 * 1024) {
  const parent = await directory(dirname(path));
  try {
    const file = await open(
      "/proc/self/fd/" + parent.fd + "/" + basename(path),
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    try {
      const before = await file.stat();
      if (!before.isFile() || before.size > max || before.nlink < 1 || before.nlink > 64)
        throw Error("BOUNDED_DELIVERY_FILE_REQUIRED");
      const bytes = Buffer.alloc(before.size + 1);
      let count = 0;
      while (count < bytes.length) {
        const r = await file.read(bytes, count, bytes.length - count, null);
        if (!r.bytesRead) break;
        count += r.bytesRead;
      }
      const after = await file.stat(),
        physical = await lstat(path),
        parentPhysical = await lstat(dirname(path)),
        parentHeld = await parent.stat();
      if (
        !physical.isFile() ||
        physical.dev !== before.dev ||
        physical.ino !== before.ino ||
        parentPhysical.dev !== parentHeld.dev ||
        parentPhysical.ino !== parentHeld.ino ||
        count !== before.size ||
        before.dev !== after.dev ||
        before.ino !== after.ino ||
        before.size !== after.size ||
        before.mtimeMs !== after.mtimeMs ||
        before.ctimeMs !== after.ctimeMs ||
        (await realpath(path)) !== path
      )
        throw Error("DELIVERY_FILE_CHANGED");
      return bytes.subarray(0, count);
    } finally {
      await file.close();
    }
  } finally {
    await parent.close();
  }
}
const initialTool = sha(await exact(import.meta.path));
type Pin = { path: string; sha256: string };
export interface DeliveryPlan {
  schema: "hermes.new-source-initial-delivery.v1";
  targetRoot: string;
  providerRoot: string;
  parentIdentity: { dev: number; ino: number };
  originalSourceRestored: false;
  nativePermissionGranted: false;
  authority: "none";
  toolSha256: string;
  inputs: Pin[];
  runtimeDependencies: Awaited<ReturnType<typeof captureInstalledRuntimeClosure>>;
  files: { path: string; sha256: string; bytes: number; expected: "absent" }[];
  link: { path: string; target: string; expected: "absent" };
}
export interface PreparedDelivery {
  stagingRoot: string;
  planSha256: string;
}
function validateTarget(targetRoot: string, providerRoot: string) {
  if (
    resolve(targetRoot) !== targetRoot ||
    !/^[a-z][a-z0-9-]{0,79}$/u.test(basename(targetRoot)) ||
    basename(targetRoot) === "agent-platform" ||
    providerRoot !== resolve(targetRoot, "../agent-platform")
  )
    throw Error("EXPLICIT_FIXED_NEW_SOURCE_TARGET_REQUIRED");
}
async function current(plan: DeliveryPlan) {
  validateTarget(plan.targetRoot, plan.providerRoot);
  if (
    Bun.version !== "1.4.3" ||
    sha(await exact(import.meta.path)) !== initialTool ||
    plan.toolSha256 !== initialTool
  )
    throw Error("DELIVERY_TOOL_CHANGED_USE_FRESH_PROCESS");
  const expected = [...sourcePaths, ...generationPaths];
  if (
    plan.inputs.length !== expected.length ||
    new Set(plan.inputs.map((pin) => pin.path)).size !== expected.length ||
    expected.some((path) => !plan.inputs.some((pin) => pin.path === path))
  )
    throw Error("EXACT_DELIVERY_INPUTS_REQUIRED");
  const inputs = new Map<string, Buffer>();
  for (const pin of plan.inputs) {
    if (!/^[a-f0-9]{64}$/u.test(pin.sha256)) throw Error("INVALID_DELIVERY_DIGEST");
    const bytes = await exact(join(codeRoot, pin.path));
    if (
      sha(bytes) !== pin.sha256 ||
      sha(await exact(join(plan.providerRoot, pin.path))) !== pin.sha256
    )
      throw Error("DELIVERY_CANONICAL_SOURCE_DRIFT");
    inputs.set(pin.path, bytes);
  }
  const runtime = await captureInstalledRuntimeClosure(
    plan.providerRoot,
    exact,
    sha,
    false,
    modules.map((name) =>
      join(plan.providerRoot, "packages/native-hermes-adapter/src/" + name + ".ts"),
    ),
  );
  if (JSON.stringify(runtime) !== JSON.stringify(plan.runtimeDependencies))
    throw Error("DELIVERY_RUNTIME_DEPENDENCY_DRIFT");
  const parent = await directory(dirname(plan.targetRoot));
  try {
    const stat = await parent.stat();
    if (stat.dev !== plan.parentIdentity.dev || stat.ino !== plan.parentIdentity.ino)
      throw Error("DELIVERY_PARENT_CHANGED");
  } finally {
    await parent.close();
  }
  return inputs;
}
async function inventory(targetRoot: string) {
  const names: string[] = [];
  try {
    const h = await directory(targetRoot);
    await h.close();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return names;
    throw error;
  }
  const admitted = new Set([
    ...directories,
    ...deliveryPaths,
    linkPath,
    "project.json",
    "state",
    "state/next-session.json",
  ]);
  async function visit(path: string, prefix: string) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const relative = prefix + entry.name,
        absolute = join(path, entry.name);
      if (!admitted.has(relative) || names.length > 40) throw Error("DELIVERY_TARGET_THIRD_STATE");
      names.push(relative);
      if (relative === linkPath) {
        if (
          !entry.isSymbolicLink() ||
          (await readlink(absolute)) !==
            join(dirname(targetRoot), "agent-platform/packages/native-hermes-adapter") ||
          (await realpath(absolute)) !==
            join(dirname(targetRoot), "agent-platform/packages/native-hermes-adapter")
        )
          throw Error("DELIVERY_LINK_CONFLICT");
      } else if (directories.includes(relative) || relative === "state") {
        if (!entry.isDirectory() || entry.isSymbolicLink())
          throw Error("DELIVERY_TARGET_TYPE_CONFLICT");
        const h = await directory(absolute);
        await h.close();
        await visit(absolute, relative + "/");
      } else if (
        !entry.isFile() ||
        (await lstat(absolute)).nlink !== 1 ||
        (await realpath(absolute)) !== absolute
      )
        throw Error("DELIVERY_TARGET_TYPE_CONFLICT");
    }
  }
  await visit(targetRoot, "");
  return names;
}
async function absent(targetRoot: string) {
  const names = await inventory(targetRoot);
  if (deliveryPaths.some((path) => names.includes(path)) || names.includes(linkPath))
    throw Error("DELIVERY_REQUIRES_ALL14_AND_LINK_ABSENT");
}
/** Stages exclusively; never creates the designated target or owner data. */
export async function prepareHermesSourceDelivery(targetRoot: string): Promise<PreparedDelivery> {
  const providerRoot = resolve(targetRoot, "../agent-platform");
  validateTarget(targetRoot, providerRoot);
  await absent(targetRoot);
  const parent = await directory(dirname(targetRoot));
  let parentIdentity: { dev: number; ino: number };
  try {
    const stat = await parent.stat();
    parentIdentity = { dev: stat.dev, ino: stat.ino };
  } finally {
    await parent.close();
  }
  const fixture = await generateHermesSourceFixture();
  try {
    const manifest = await verifySourceFixture(fixture);
    const inputs = new Map<string, Buffer>();
    for (const pin of manifest.inputs as Pin[])
      inputs.set(pin.path, await exact(join(codeRoot, pin.path)));
    const ownerPackage = Buffer.from(
      json({
        name: packageName,
        private: true,
        type: "module",
        sourceOnly: true,
        originalSourceRestored: false,
        dependencies: { "@palantirkc/native-hermes-adapter": dependency },
      }),
    );
    // Reuse the canonical generator's offline lock; only its explicit NEW package identity changes.
    const fictionalLock = (await exact(join(fixture.codeOwnerRoot, "bun.lock"))).toString();
    const oldName = '"@palantirkc/hermes-new-source-fictional-facade"';
    if (fictionalLock.split(oldName).length !== 2)
      throw Error("EXACT_GENERATED_LOCK_IDENTITY_REQUIRED");
    const lock = Buffer.from(fictionalLock.replace(oldName, JSON.stringify(packageName)));
    const runtimeDependencies = await captureInstalledRuntimeClosure(
      providerRoot,
      exact,
      sha,
      false,
      modules.map((name) =>
        join(providerRoot, "packages/native-hermes-adapter/src/" + name + ".ts"),
      ),
    );
    const plan: DeliveryPlan = {
      schema: "hermes.new-source-initial-delivery.v1",
      targetRoot,
      providerRoot,
      parentIdentity,
      originalSourceRestored: false,
      nativePermissionGranted: false,
      authority: "none",
      toolSha256: initialTool,
      inputs: [...inputs].map(([path, bytes]) => ({ path, sha256: sha(bytes) })),
      runtimeDependencies,
      files: [],
      link: {
        path: linkPath,
        target: join(providerRoot, "packages/native-hermes-adapter"),
        expected: "absent",
      },
    };
    await current(plan);
    const files = await composeHermesSourceProjection(
      inputs,
      targetRoot,
      providerRoot,
      ownerPackage,
      lock,
    );
    const stagingRoot = await mkdtemp(
      join(await realpath(tmpdir()), "hermes-new-source-delivery-"),
    );
    for (const path of deliveryPaths) {
      const bytes = files.get(path)!;
      await mkdir(dirname(join(stagingRoot, path)), { recursive: true });
      await writeFile(join(stagingRoot, path), bytes, { flag: "wx" });
      plan.files.push({ path, sha256: sha(bytes), bytes: bytes.length, expected: "absent" });
    }
    await current(plan);
    await absent(targetRoot);
    const planBytes = json(plan);
    await writeFile(join(stagingRoot, "delivery-plan.json"), planBytes, { flag: "wx" });
    return { stagingRoot, planSha256: sha(planBytes) };
  } finally {
    await rm(fixture.fixtureRoot, { recursive: true });
  }
}
export interface DeliveryReceipt {
  status: "applied" | "already-applied" | "refused" | "partial";
  planSha256: string;
  completed: { path: string; kind: "directory" | "source" | "link" }[];
  uncertain: string[];
  reason: string | null;
  originalSourceRestored: false;
  nativePermissionGranted: false;
}
/** Explicit caller-owned review digest and expected state; no replace, rollback or automatic retry. */
export async function applyHermesSourceDelivery(
  prepared: PreparedDelivery,
  reviewedPlanSha256: string,
  expectedState: "absent" | "already-applied",
  writeCeiling = 15,
): Promise<DeliveryReceipt> {
  const receipt: DeliveryReceipt = {
    status: "refused",
    planSha256: reviewedPlanSha256,
    completed: [],
    uncertain: [],
    reason: null,
    originalSourceRestored: false,
    nativePermissionGranted: false,
  };
  let root: Awaited<ReturnType<typeof directory>> | undefined;
  let parent: Awaited<ReturnType<typeof directory>> | undefined;
  try {
    if (
      !/^[a-f0-9]{64}$/u.test(reviewedPlanSha256) ||
      prepared.planSha256 !== reviewedPlanSha256 ||
      !["absent", "already-applied"].includes(expectedState) ||
      !Number.isSafeInteger(writeCeiling) ||
      writeCeiling < 0 ||
      writeCeiling > 15
    )
      throw Error("EXPLICIT_REVIEWED_DELIVERY_REQUIRED");
    const planBytes = await exact(join(prepared.stagingRoot, "delivery-plan.json"));
    if (sha(planBytes) !== reviewedPlanSha256) throw Error("REVIEWED_DELIVERY_PLAN_CHANGED");
    const plan = JSON.parse(planBytes.toString()) as DeliveryPlan;
    if (
      Object.keys(plan).sort().join("|") !==
        "authority|files|inputs|link|nativePermissionGranted|originalSourceRestored|parentIdentity|providerRoot|runtimeDependencies|schema|targetRoot|toolSha256" ||
      plan.schema !== "hermes.new-source-initial-delivery.v1" ||
      plan.originalSourceRestored !== false ||
      plan.nativePermissionGranted !== false ||
      plan.authority !== "none" ||
      JSON.stringify(plan.link) !==
        JSON.stringify({
          path: linkPath,
          target: join(plan.providerRoot, "packages/native-hermes-adapter"),
          expected: "absent",
        }) ||
      !Array.isArray(plan.files) ||
      plan.files.length !== 14 ||
      plan.files.map((p) => p.path).join("|") !== deliveryPaths.join("|")
    )
      throw Error("EXACT_INITIAL_DELIVERY_PLAN_REQUIRED");
    const inputs = await current(plan),
      staged = new Map<string, Buffer>();
    for (const pin of plan.files) {
      const bytes = await exact(join(prepared.stagingRoot, pin.path));
      if (
        Object.keys(pin).sort().join("|") !== "bytes|expected|path|sha256" ||
        pin.expected !== "absent" ||
        bytes.length !== pin.bytes ||
        sha(bytes) !== pin.sha256
      )
        throw Error("DELIVERY_STAGED_SOURCE_CHANGED");
      staged.set(pin.path, bytes);
    }
    const pkg = json({
      name: packageName,
      private: true,
      type: "module",
      sourceOnly: true,
      originalSourceRestored: false,
      dependencies: { "@palantirkc/native-hermes-adapter": dependency },
    });
    if (staged.get("package.json")!.toString() !== pkg)
      throw Error("EXPLICIT_NEW_SOURCE_PACKAGE_REQUIRED");
    const expected = await composeHermesSourceProjection(
      inputs,
      plan.targetRoot,
      plan.providerRoot,
      staged.get("package.json")!,
      staged.get("bun.lock")!,
    );
    for (const [path, bytes] of expected)
      if (sha(bytes) !== sha(staged.get(path)!)) throw Error("DELIVERY_PROJECTION_CHANGED");
    const names = await inventory(plan.targetRoot);
    if (expectedState === "already-applied") {
      if (!names.includes(linkPath)) throw Error("DELIVERY_NOT_ALREADY_APPLIED");
      for (const pin of plan.files)
        if (sha(await exact(join(plan.targetRoot, pin.path))) !== pin.sha256)
          throw Error("DELIVERY_NOT_ALREADY_APPLIED");
      await current(plan);
      receipt.status = "already-applied";
      return receipt;
    }
    await absent(plan.targetRoot);
    await current(plan);
    parent = await directory(dirname(plan.targetRoot));
    const parentStat = await parent.stat();
    if (parentStat.dev !== plan.parentIdentity.dev || parentStat.ino !== plan.parentIdentity.ino)
      throw Error("DELIVERY_PARENT_CHANGED");
    const createDirectory = async (
      path: string,
      heldParent: Awaited<ReturnType<typeof directory>>,
    ) => {
      await sameDirectory(dirname(path), heldParent);
      receipt.uncertain.push(path);
      let created = false;
      try {
        // Only a basename may follow a descriptor: nested suffixes can follow substituted symlinks.
        await mkdir("/proc/self/fd/" + heldParent.fd + "/" + basename(path));
        created = true;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
        // An exclusive EEXIST has no effect; an existing directory still requires no-follow readback.
        receipt.uncertain.pop();
      }
      await sameDirectory(dirname(path), heldParent);
      const child = await directory(path);
      try {
        if (created) {
          await child.sync();
          await heldParent.sync();
          await sameDirectory(dirname(path), heldParent);
          await sameDirectory(path, child);
        }
      } finally {
        await child.close();
      }
      if (created) {
        receipt.completed.push({ path, kind: "directory" });
        receipt.uncertain.pop();
      }
    };
    await createDirectory(plan.targetRoot, parent);
    root = await directory(plan.targetRoot);
    const identity = await root.stat();
    const recheck = async () => {
      const h = await directory(plan.targetRoot);
      try {
        const s = await h.stat();
        if (s.dev !== identity.dev || s.ino !== identity.ino) throw Error("DELIVERY_ROOT_CHANGED");
      } finally {
        await h.close();
      }
      for (const pin of plan.inputs)
        if (
          sha(await exact(join(codeRoot, pin.path))) !== pin.sha256 ||
          sha(await exact(join(plan.providerRoot, pin.path))) !== pin.sha256
        )
          throw Error("DELIVERY_CANONICAL_SOURCE_DRIFT");
      if (sha(await exact(import.meta.path)) !== initialTool)
        throw Error("DELIVERY_TOOL_CHANGED_USE_FRESH_PROCESS");
    };
    for (const path of directories) {
      await recheck();
      const absolute = join(plan.targetRoot, path);
      const heldParent = await directory(dirname(absolute));
      try {
        await recheck();
        await createDirectory(absolute, heldParent);
      } finally {
        await heldParent.close();
      }
    }
    let writes = 0;
    for (const pin of plan.files) {
      if (writes >= writeCeiling) throw Error("DELIVERY_WRITE_CEILING_REACHED");
      await recheck();
      const heldParent = await directory(dirname(join(plan.targetRoot, pin.path)));
      try {
        await sameDirectory(dirname(join(plan.targetRoot, pin.path)), heldParent);
        receipt.uncertain.push(join(plan.targetRoot, pin.path));
        let file: Awaited<ReturnType<typeof open>>;
        try {
          file = await open(
            "/proc/self/fd/" + heldParent.fd + "/" + basename(pin.path),
            constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
            0o600,
          );
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code === "EEXIST") receipt.uncertain.pop();
          throw error;
        }
        try {
          await file.writeFile(staged.get(pin.path)!);
          await file.sync();
          await heldParent.sync();
          await recheck();
          await sameDirectory(dirname(join(plan.targetRoot, pin.path)), heldParent);
          const [held, named] = await Promise.all([
            file.stat(),
            lstat(join(plan.targetRoot, pin.path)),
          ]);
          if (
            !named.isFile() ||
            held.dev !== named.dev ||
            held.ino !== named.ino ||
            sha(await exact(join(plan.targetRoot, pin.path))) !== pin.sha256
          )
            throw Error("DELIVERY_SOURCE_READBACK_FAILED");
          await sameDirectory(dirname(join(plan.targetRoot, pin.path)), heldParent);
        } finally {
          await file.close();
        }
        receipt.completed.push({ path: join(plan.targetRoot, pin.path), kind: "source" });
        receipt.uncertain.pop();
        writes++;
      } finally {
        await heldParent.close();
      }
    }
    if (writes >= writeCeiling) throw Error("DELIVERY_WRITE_CEILING_REACHED");
    await recheck();
    const linkParent = await directory(join(plan.targetRoot, "node_modules/@palantirkc"));
    try {
      await sameDirectory(join(plan.targetRoot, "node_modules/@palantirkc"), linkParent);
      receipt.uncertain.push(join(plan.targetRoot, linkPath));
      try {
        await symlink(
          plan.link.target,
          "/proc/self/fd/" + linkParent.fd + "/native-hermes-adapter",
        );
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "EEXIST") receipt.uncertain.pop();
        throw error;
      }
      await linkParent.sync();
      await sameDirectory(join(plan.targetRoot, "node_modules/@palantirkc"), linkParent);
    } finally {
      await linkParent.close();
    }
    await recheck();
    await inventory(plan.targetRoot);
    await root.sync();
    await parent.sync();
    for (const pin of plan.files)
      if (sha(await exact(join(plan.targetRoot, pin.path))) !== pin.sha256)
        throw Error("DELIVERY_FINAL_READBACK_FAILED");
    await current(plan);
    receipt.completed.push({ path: join(plan.targetRoot, linkPath), kind: "link" });
    receipt.uncertain.pop();
    receipt.status = "applied";
    return receipt;
  } catch (error) {
    receipt.status = receipt.completed.length || receipt.uncertain.length ? "partial" : "refused";
    receipt.reason =
      error instanceof Error && /^[A-Z0-9_]+$/u.test(error.message)
        ? error.message
        : "DELIVERY_IO_OR_INPUT_REFUSAL";
    return receipt;
  } finally {
    for (const handle of [root, parent]) {
      try {
        await handle?.close();
      } catch {
        receipt.status =
          receipt.completed.length || receipt.uncertain.length ? "partial" : "refused";
        receipt.reason ??= "DELIVERY_DESCRIPTOR_CLOSE_FAILED";
      }
    }
  }
}
