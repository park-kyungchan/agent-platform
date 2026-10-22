import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { readQualityScope, exactSourceFile } from "../scope";
import { insist } from "./policy";

/** P selects QA test/controller bytes; imported candidate code still shares their process.
 * This projection preserves the required workload but cannot authenticate completed assertions.
 */
export async function pinQaController(target: string, policy: string): Promise<void> {
  insist(target !== policy, "DISTINCT_QA_SUBJECT_REQUIRED");
  const scope = await readQualityScope(policy);
  for (const directory of ["ci", "scripts", "verification", "owner-fixture"]) {
    await cp(join(policy, directory), join(target, directory), {
      recursive: true,
      force: true,
      dereference: false,
    });
  }
  for (const path of [...scope.testFiles, ...scope.testSupportFiles]) {
    const destination = join(target, path);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, await exactSourceFile(policy, path));
  }
  // package scripts select workload; P retains the admitted script meaning during this projection.
  for (const path of [
    "package.json",
    ...scope.formatFiles.filter((p) => /^packages\/[^/]+\/package.json$/.test(p)),
  ]) {
    const original = JSON.parse(await exactSourceFile(target, path)) as Record<string, unknown>;
    const fixed = JSON.parse(await exactSourceFile(policy, path)) as Record<string, unknown>;
    original.scripts = fixed.scripts ?? {};
    await writeFile(join(target, path), JSON.stringify(original, null, 2) + "\n");
  }
  for (const path of [
    ".prettierrc.json",
    "ci/quality/tsconfig.json",
    "ci/quality/tool-pins.json",
    "ci/quality/any-baseline.json",
  ]) {
    await writeFile(join(target, path), await readFile(join(policy, path)));
  }
}
