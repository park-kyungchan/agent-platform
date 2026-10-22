import { stageFrozenFixtureDependencies } from "./scenarios/dependencies";
import { prepareProbeEnvironment } from "./probe-environment";
import { expect, test } from "bun:test";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { requiredSemanticCases, sha256 } from "./policy";
import fixedApi from "./predecessor-api.json";
import { semanticRequest, superviseSemanticCase, type SemanticCase } from "./semantic";
import { fixedProcess } from "./process";
const source = resolve(import.meta.dir, "../../..");
const selected = (id: string) => {
  const c = requiredSemanticCases.find((c) => c.id === "scenario-" + id);
  if (!c) throw Error("FIXED_PREFLIGHT_CASE_MISSING");
  return c;
};
async function run(c: SemanticCase, root: string) {
  const r = await fixedProcess(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      join(import.meta.dir, "fixtures/behavior-call-driver.ts"),
      join(root, c.module),
      root,
    ],
    {
      cwd: "/tmp",
      env: await prepareProbeEnvironment(),
      stdin: Buffer.from(JSON.stringify(semanticRequest(c))),
      timeoutMs: 120000,
      maxBytes: 128000,
    },
  );
  return {
    stdout: r.stdout,
    observation: {
      exitCode: r.code,
      timedOut: r.timedOut,
      oomKilled: false,
      stdoutSha256: sha256(r.stdout),
    },
  };
}
/** Existing isolated probe only. Every mutation below affects disposable production source bytes;
 * no candidate result JSON or host oracle is patched, and no live owner path is writable. */
async function disposable(use: (root: string) => Promise<void>) {
  const root = await mkdtemp(join(tmpdir(), "s04-pf-source-"));
  try {
    for (const path of [
      "packages",
      "releases",
      "architecture",
      "owner-fixture",
      "project.json",
      "ontology-pin.json",
      "package.json",
      "bun.lock",
    ])
      await cp(join(source, path), join(root, path), {
        recursive: true,
        dereference: false,
        filter: (path) =>
          !path
            .split("/")
            .some((part) => ["node_modules", ".git", "tests", "fixtures"].includes(part)) &&
          !/\.(test|spec)\.ts$/.test(path),
      });
    // Materialize every actual runtime dependency/peer from its immutable importer,
    // preserving its bytes without relying on the source installation's nested aliases.
    await stageFrozenFixtureDependencies(source, root);
    await mkdir(join(root, "node_modules/@palantirkc"), { recursive: true });
    for (const name of ["work-continuity", "native-codex-adapter", "native-hermes-adapter"])
      await symlink(join(root, "packages", name), join(root, "node_modules/@palantirkc", name));
    await use(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
async function mutate(root: string, path: string, before: string, after: string) {
  const file = join(root, path),
    text = await readFile(file, "utf8");
  if (text.split(before).length !== 2) throw Error("EXACT_REVIEWED_SOURCE_MUTATION_REQUIRED");
  await writeFile(file, text.replace(before, after));
}
async function positive(id: string, root = source) {
  const c = selected(id);
  await superviseSemanticCase(
    c,
    () => run(c, root),
    () => {},
  );
}
async function rejectedBehavior(id: string, root: string) {
  const c = selected(id),
    actual = await run(c, root);
  // A mutation setup/import error is not defect-detection evidence for this control.
  expect(actual.observation.exitCode).toBe(0);
  const frame = JSON.parse(actual.stdout.toString());
  expect(frame.schema).toBe("agent-platform.behavior-result.v1");
  expect(frame.outcome.kind).toBe("return");
  await expect(
    superviseSemanticCase(
      c,
      async () => actual,
      () => {},
    ),
  ).rejects.toThrow("REQUIRED_SEMANTIC_CASE_FAILED");
  return frame.outcome.value;
}
test("PF01 real public governing/history/composition branches are externally compared against genuine positives", async () => {
  for (const [id, before, after] of [
    ["memory-authority-governing-false", "return verdict === true;", "return true;"],
    [
      "memory-history-current",
      'header.status !== "active" || address.currentness !== "current"',
      "false",
    ],
    ["memory-transport-compact", 'mode: request.transport ?? "full",', 'mode: "full",'],
  ] as const) {
    await positive(id);
    await disposable(async (root) => {
      await positive(id, root);
      await mutate(root, "packages/work-continuity/src/memory/query.ts", before!, after!);
      await rejectedBehavior(id!, root);
    });
  }
}, 600000);
test("PF02 genuine fixed predecessor contracts reject real simultaneous canonical/facade export loss", async () => {
  for (const [id, path, before, after, lost] of [
    [
      "owner-fixed-context",
      "packages/work-continuity/src/context/index.ts",
      'export { recordBoundary } from "./boundary";',
      "// disposable removal of one maintained reexport",
      "recordBoundary",
    ],
    [
      "owner-fixed-hermes",
      "packages/native-hermes-adapter/src/ontology-driven/work-context.ts",
      "export const REVIEWED_PACK_MANIFEST =",
      "const REVIEWED_PACK_MANIFEST =",
      "REVIEWED_PACK_MANIFEST",
    ],
  ] as const) {
    await positive(id!);
    await disposable(async (root) => {
      await positive(id, root);
      await mutate(root, path!, before!, after!);
      const value = await rejectedBehavior(id!, root);
      const pair = id === "owner-fixed-context" ? value : value.publicExports[0];
      expect(Object.hasOwn(pair.facadeKinds ?? pair.facade, lost!)).toBe(false);
      expect(Object.hasOwn(pair.canonicalKinds ?? pair.canonical, lost!)).toBe(false);
    });
  }
}, 600000);

test("the P catalogue retains the entire explicit predecessor API denominator independently of H/M", () => {
  expect(fixedApi.predecessor.head).toBe("4cd6eea995700177159a3d3d42bf63024fb5fc13");
  const context = selected("owner-fixed-context"),
    hermes = selected("owner-fixed-hermes");
  if (context.expected.kind !== "predicates" || hermes.expected.kind !== "predicates")
    throw Error("FIXED_EXPORT_COMPARISONS_REQUIRED");
  for (const pointer of ["/facade", "/canonical"])
    expect(context.expected.values.find((row) => row.pointer === pointer)?.equals).toEqual(
      fixedApi.context,
    );
  for (const [i, [name, kinds]] of Object.entries(fixedApi.hermes).entries()) {
    expect(
      hermes.expected.values.find((row) => row.pointer === `/publicExports/${i}/module`)?.equals,
    ).toBe(name);
    for (const side of ["facadeKinds", "canonicalKinds"])
      expect(
        hermes.expected.values.find((row) => row.pointer === `/publicExports/${i}/${side}`)?.equals,
      ).toEqual(kinds);
  }
  expect(hermes.expected.values.find((row) => row.pointer === "/moduleNames")?.equals).toEqual(
    Object.keys(fixedApi.hermes),
  );
});
