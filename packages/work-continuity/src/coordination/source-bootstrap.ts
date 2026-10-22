/** Fixed compiler path for a standalone first-import guard. Execute only in the
 * separately qualified source runner/native build owner, never on author Host. */
import { mkdir, lstat, open } from "node:fs/promises";
import { constants } from "node:fs";
import type { FileHandle } from "node:fs/promises";
import { resolve, join, dirname, basename } from "node:path";
import { sourceHash, sourceAbsolutePath } from "./source-contract.ts";
import { readExactSource, openSourceDirectory } from "./source-files.ts";
export const bootstrapInputs = Object.freeze(
  [
    "source-contract.ts",
    "source-files.ts",
    "source-runtime.ts",
    "source-syntax.ts",
    "source-bootstrap-entry.ts",
    "source-bootstrap.ts",
  ].map((name) => "packages/work-continuity/src/coordination/" + name),
);
export interface BootstrapBuildRequest {
  sourceRoot: string;
  outputRoot: string;
  compiler: { path: string; sha256: string; version: string };
  inputs: readonly { path: string; sha256: string }[];
}
export interface NativeBootstrapBuildOwner {
  withBuild<T>(
    request: Readonly<BootstrapBuildRequest>,
    consume: (lease: {
      /** Actual native exclusive output writer/namespace lease, retained until
       * publication/readback ends. A request JSON path is not that lease. */
      output: Readonly<{ root: string; parent: string; exclusiveWriter: true }>;
      assertCurrent(): void | Promise<void>;
    }) => Promise<T>,
  ): Promise<T>;
}
const canonicalSourceRoot = resolve(import.meta.dir, "../../../..");
const initialSelf = sourceHash(await readExactSource(import.meta.path));
export async function compileSourceBootstrap(
  request: BootstrapBuildRequest,
  owner: NativeBootstrapBuildOwner,
) {
  request = structuredClone(request);
  if (!owner || typeof owner.withBuild !== "function")
    throw Error("SOURCE_NATIVE_BUILD_OWNER_REQUIRED");
  if (
    sourceAbsolutePath(request.sourceRoot) !== canonicalSourceRoot ||
    request.inputs.length !== bootstrapInputs.length ||
    new Set(request.inputs.map((row) => row.path)).size !== bootstrapInputs.length ||
    bootstrapInputs.some((path) => !request.inputs.some((row) => row.path === path))
  )
    throw Error("SOURCE_EXACT_BOOTSTRAP_INPUTS_REQUIRED");
  sourceAbsolutePath(request.outputRoot);
  sourceAbsolutePath(request.compiler.path);
  return owner.withBuild(request, async (lease) => {
    const parentPath = dirname(request.outputRoot);
    if (
      lease.output?.root !== request.outputRoot ||
      lease.output.parent !== parentPath ||
      lease.output.exclusiveWriter !== true
    )
      throw Error("SOURCE_NATIVE_EXCLUSIVE_BUILD_OUTPUT_REQUIRED");
    // Resolve every ancestor without following aliases before the first mkdir.
    const parent = await openSourceDirectory(parentPath);
    let directory: FileHandle | undefined,
      created = false;
    const directoryAnchor = () => {
      if (!directory) throw Error("SOURCE_BOOTSTRAP_OUTPUT_NOT_CREATED");
      // Child compiler opens the parent's held directory, not its mutable name.
      // Native owner must preserve procfs access for this separately qualified
      // same-principal build. Unsupported platforms refuse, never fall back.
      return "/proc/" + process.pid + "/fd/" + directory.fd;
    };
    async function current() {
      await lease.assertCurrent();
      if (
        lease.output.root !== request.outputRoot ||
        lease.output.parent !== parentPath ||
        lease.output.exclusiveWriter !== true
      )
        throw Error("SOURCE_NATIVE_BUILD_OUTPUT_LEASE_CHANGED");
      if (sourceHash(await readExactSource(import.meta.path)) !== initialSelf)
        throw Error("SOURCE_BUILD_TOOL_CHANGED_USE_FRESH_PROCESS");
      if (
        sourceHash(await readExactSource(request.compiler.path)) !== request.compiler.sha256 ||
        request.compiler.version !== Bun.version
      )
        throw Error("SOURCE_COMPILER_IMAGE_MISMATCH");
      for (const pin of request.inputs)
        if (sourceHash(await readExactSource(join(request.sourceRoot, pin.path))) !== pin.sha256)
          throw Error("SOURCE_BOOTSTRAP_INPUT_CHANGED");
      for (const [path, held] of [
        [parentPath, parent],
        ...(directory ? [[request.outputRoot, directory] as const] : []),
      ] as const) {
        const named = await openSourceDirectory(path);
        try {
          const before = await held.stat(),
            after = await named.stat();
          if (before.dev !== after.dev || before.ino !== after.ino)
            throw Error("SOURCE_BOOTSTRAP_OUTPUT_PARENT_OR_DIRECTORY_CHANGED");
        } finally {
          await named.close();
        }
      }
    }
    try {
      await current();
      const anchoredPath = "/proc/self/fd/" + parent.fd + "/" + basename(request.outputRoot);
      try {
        await mkdir(anchoredPath, { mode: 0o700 });
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "EEXIST")
          throw Error("SOURCE_BOOTSTRAP_OUTPUT_EXISTS_RECONCILE_NO_RETRY");
        throw error;
      }
      created = true;
      directory = await open(
        anchoredPath,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await current();
      const output = join(request.outputRoot, "source-bootstrap.mjs"),
        metadata = join(request.outputRoot, "source-bootstrap.meta.json"),
        anchoredOutput = join(directoryAnchor(), "source-bootstrap.mjs"),
        anchoredMetadata = join(directoryAnchor(), "source-bootstrap.meta.json");
      const child = Bun.spawn(
        [
          request.compiler.path,
          "--no-env-file",
          "build",
          join(
            request.sourceRoot,
            "packages/work-continuity/src/coordination/source-bootstrap-entry.ts",
          ),
          "--target=bun",
          "--format=esm",
          "--outfile",
          anchoredOutput,
          "--metafile=" + anchoredMetadata,
        ],
        {
          cwd: request.sourceRoot,
          stdin: "ignore",
          stdout: "pipe",
          stderr: "pipe",
          env: {
            PATH: "/usr/local/bin:/usr/bin:/bin",
            HOME: directoryAnchor(),
            BUN_RUNTIME_TRANSPILER_CACHE_PATH: join(directoryAnchor(), ".compiler-cache"),
          },
        },
      );
      const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
      const outputHash = async (stream: ReadableStream<Uint8Array>) => {
        const reader = stream.getReader();
        let length = 0;
        const chunks: Uint8Array[] = [];
        try {
          while (true) {
            const part = await reader.read();
            if (part.done) break;
            length += part.value.byteLength;
            if (length <= 1048576) chunks.push(part.value);
          }
          return {
            bytes: length,
            sha256: length <= 1048576 ? sourceHash(Buffer.concat(chunks)) : null,
          };
        } finally {
          reader.releaseLock();
        }
      };
      let completed;
      try {
        const [exitCode, stdout, stderr] = await Promise.all([
          child.exited,
          outputHash(child.stdout),
          outputHash(child.stderr),
        ]);
        completed = { exitCode, stdout, stderr };
      } finally {
        clearTimeout(timer);
      }
      if (completed.exitCode !== 0) throw Error("SOURCE_BOOTSTRAP_BUILD_FAILED_RECONCILE");
      await current();
      if ((await lstat(output)).size > 1048576) throw Error("SOURCE_BOOTSTRAP_OUTPUT_BOUND");
      const meta = JSON.parse(
        new TextDecoder("utf-8", { fatal: true }).decode(await readExactSource(metadata, 1048576)),
      ) as {
        inputs?: Record<string, { imports?: { path: string; external?: boolean }[] }>;
        outputs?: Record<string, { imports?: { path: string; external?: boolean }[] }>;
      };
      const expected = bootstrapInputs
        .filter((path) => !path.endsWith("/source-bootstrap.ts"))
        .map((path) => resolve(request.sourceRoot, path))
        .sort();
      if (
        !meta.inputs ||
        JSON.stringify(
          Object.keys(meta.inputs)
            .map((path) => resolve(request.sourceRoot, path))
            .sort(),
        ) !== JSON.stringify(expected) ||
        Object.values(meta.outputs ?? {}).some((row) =>
          (row.imports ?? []).some(
            (edge) =>
              edge.external &&
              !edge.path.startsWith("node:") &&
              !["bun", "bun:sqlite"].includes(edge.path),
          ),
        )
      )
        throw Error("SOURCE_BOOTSTRAP_COMPILER_CLOSURE_MISMATCH");
      const bytes = await readExactSource(output, 1048576),
        proof = {
          schema: "common.source-bootstrap-build.v1",
          sourceRoot: request.sourceRoot,
          inputs: request.inputs,
          compiler: request.compiler,
          output: {
            path: "source-bootstrap.mjs",
            bytes: bytes.byteLength,
            sha256: sourceHash(bytes),
          },
          metafileSha256: sourceHash(await readExactSource(metadata, 1048576)),
          completed,
          nativePermissionGranted: false,
        };
      await current();
      const fd = await open(
        join(directoryAnchor(), "source-bootstrap.proof.json"),
        constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
        0o444,
      );
      try {
        await fd.writeFile(JSON.stringify(proof, null, 2) + "\n");
        await fd.sync();
      } finally {
        await fd.close();
      }
      await directory.sync();
      await parent.sync();
      await current();
      return proof;
    } catch (error) {
      if (created)
        throw new Error("SOURCE_BOOTSTRAP_INCOMPLETE_RECONCILE_NO_RETRY", { cause: error });
      throw error;
    } finally {
      await directory?.close();
      await parent.close();
    }
  });
}
