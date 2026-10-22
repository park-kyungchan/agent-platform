import { mkdir } from "node:fs/promises";

/** Literal credential-free private worker locations. Never inherit a controller/native HOME,
 * registry credential, proxy, NODE_OPTIONS or Bun configuration into a behavior child. */
export async function prepareProbeEnvironment(): Promise<Record<string, string>> {
  const env = {
    HOME: "/job/home",
    TMPDIR: "/job/tmp",
    BUN_INSTALL_CACHE_DIR: "/job/bun-cache",
    BUN_RUNTIME_TRANSPILER_CACHE_PATH: "/job/transpiler-cache",
  };
  // Reuse the qualified job cache when prepared; never clear it or disable provenance/cache checks.
  // These paths are outside every generated source fixture, so Bun cannot put its cache links there.
  for (const path of Object.values(env)) await mkdir(path, { recursive: true });
  return env;
}
