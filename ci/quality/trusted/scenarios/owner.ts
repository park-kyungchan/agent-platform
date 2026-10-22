import { cp, mkdir, readFile, writeFile, symlink, readdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { contextProjectionPaths } from "../../../source-projection";
import { scratch, hash, write, capture } from "./support";
const namespaceKinds = (api: Record<string, unknown>) =>
  Object.fromEntries(Object.keys(api).sort().map((name) => [name, typeof api[name]]));
const nativePaths = [
  "package.json",
  "bun.lock",
  "packages/native-codex-adapter/package.json",
  ...["hooks", "home-context", "evaluation-ledger"].map(
    (n) => `packages/native-codex-adapter/src/native/${n}.ts`,
  ),
];
export async function ownerFixture(source: string, root: string) {
  const common = join(root, "projects/agent-platform"),
    owner = join(root, "projects/codex-harness-engineering");
  await mkdir(common, { recursive: true });
  await mkdir(owner, { recursive: true });
  for (const path of [
    "packages",
    "releases",
    "project.json",
    "ontology-pin.json",
    "package.json",
    "bun.lock",
  ])
    await cp(join(source, path), join(common, path), {
      recursive: true,
      dereference: false,
      filter: (p) =>
        !p.split("/").some((x) => ["node_modules", ".git", "tests", "fixtures"].includes(x)) &&
        !/\.(test|spec)\.ts$/.test(p),
    });
  // Fictional aliases only; no installation/network/configuration of an actual native owner.
  await mkdir(join(common, "node_modules"), { recursive: true });
  for (const entry of await readdir(join(source, "node_modules"), { withFileTypes: true })) {
    if (entry.name === "@palantirkc") continue;
    await symlink(
      join(source, "node_modules", entry.name),
      join(common, "node_modules", entry.name),
    );
  }
  await mkdir(join(common, "node_modules/@palantirkc"), { recursive: true });
  for (const name of ["work-continuity", "native-codex-adapter", "native-hermes-adapter"])
    await symlink(join(common, "packages", name), join(common, "node_modules/@palantirkc", name));
  await symlink(
    join(source, "node_modules/@palantirkc/ontology"),
    join(common, "node_modules/@palantirkc/ontology"),
  );
  // P selects the admitted owner guard bytes; H/M supplies only the observed common source.
  await cp(
    join(resolve(import.meta.dir, "../../../.."), "owner-fixture/codex-harness-engineering/src"),
    join(owner, "src"),
    { recursive: true },
  );
  await mkdir(join(owner, "node_modules/@palantirkc"), { recursive: true });
  for (const name of ["work-continuity", "native-codex-adapter"])
    await symlink(join(common, "packages", name), join(owner, "node_modules/@palantirkc", name));
  const project = JSON.parse(await readFile(join(common, "project.json"), "utf8"));
  project.payload.details.sourceOwnerRoot = common;
  await write(common, "project.json", JSON.stringify(project) + "\n");
  await write(
    owner,
    "package.json",
    JSON.stringify({
      name: "fictional-owner",
      private: true,
      type: "module",
      dependencies: {
        "@palantirkc/work-continuity": "file:../agent-platform/packages/work-continuity",
        "@palantirkc/native-codex-adapter": "file:../agent-platform/packages/native-codex-adapter",
      },
    }) + "\n",
  );
  const policy = JSON.parse(
    await readFile(join(source, "architecture/runtime-filesystem-contract.json"), "utf8"),
  );
  const projections = [
    {
      ...policy.sourceProviderProjections[0],
      canonicalConsumerRoot: owner,
      canonicalProviderRoot: common,
      consumerRoot: join(root, "code/shared/codex-harness-engineering"),
      providerRoot: join(root, "code/shared/agent-platform"),
    },
  ];
  const pins = async (paths: readonly string[]) =>
    Promise.all(
      paths.map(async (path) => ({ path, sha256: hash(await readFile(join(common, path))) })),
    );
  const registration = {
    schema: "codex.context-provider-source-closure.v6",
    consumerRoot: owner,
    providerRoot: common,
    providerProjectId: "agent-platform",
    packageName: "@palantirkc/work-continuity",
    modulePath: "packages/work-continuity/src/context/index.ts",
    gitWorkModulePath: "packages/work-continuity/src/git-work/index.ts",
    sourcePins: await pins(contextProjectionPaths("v6")),
    sourceProjections: projections,
    authority: "none",
    nativePermissionGranted: false,
  };
  await write(owner, "architecture/context-provider.json", JSON.stringify(registration) + "\n");
  for (const n of [
    "index",
    "types",
    "validation",
    "source",
    "store",
    "context",
    "boundary",
    "sections",
  ])
    await write(
      owner,
      `src/continuity/${n}.ts`,
      n === "index"
        ? `// codex-context-provider-registration-sha256: ${hash(await readFile(join(owner, "architecture/context-provider.json")))}\nexport * from '@palantirkc/work-continuity/context';\n`
        : `export * from '@palantirkc/work-continuity/context/${n}';\n`,
    );
  const native = {
    schema: "codex.native-adapter-source-closure.v2",
    consumerRoot: owner,
    providerRoot: common,
    packageName: "@palantirkc/native-codex-adapter",
    sourcePins: await pins(nativePaths),
    sourceProjections: projections,
    authority: "none",
    nativePermissionGranted: false,
  };
  await write(owner, "architecture/native-codex-provider.json", JSON.stringify(native) + "\n");
  const hooks = join(owner, "src/native/hooks.ts");
  await writeFile(
    hooks,
    (await readFile(hooks, "utf8")).replace(
      /codex-native-provider-registration-sha256: [a-f0-9]{64}/,
      "codex-native-provider-registration-sha256: " +
        hash(await readFile(join(owner, "architecture/native-codex-provider.json"))),
    ),
  );
  return { common, owner, registration };
}
export async function ownerScenario(source: string, id: string): Promise<unknown> {
  if (
    ["owner-hermes-positive", "owner-hermes-first-import", "owner-hermes-after-drift", "owner-fixed-hermes"].includes(id)
  ) {
    const generator = (await import(
      join(source, "packages/native-hermes-adapter/src/source-provider/generate.ts")
    )) as typeof import("../../../../packages/native-hermes-adapter/src/source-provider/generate");
    const f = await generator.generateHermesSourceFixture();
    try {
      if (id === "owner-hermes-first-import")
        await import(
          join(f.providerRoot, "packages/native-hermes-adapter/src/ontology-driven/work-context.ts")
        );
      const adapter = (await import(
        join(f.codeOwnerRoot, "src/ontology-driven/provider-source.ts")
      )) as {
        observeHermesContextProvider: (
          root: string,
        ) => Promise<{
          schema: string;
          packageName: string;
          pins: { path: string; sha256: string }[];
          verify: () => Promise<void>;
        }>;
      };
      const proof = await adapter.observeHermesContextProvider(f.codeOwnerRoot);
      if (id === "owner-hermes-after-drift") {
        const path = join(
          f.providerRoot,
          "packages/native-hermes-adapter/src/ontology-driven/work-context.ts",
        );
        await writeFile(path, (await readFile(path, "utf8")) + "\n// observed drift\n");
        const observed = await capture(() => proof.verify());
        if (observed.error)
          observed.error.message = observed.error.message.replaceAll(f.fixtureRoot, "<fixture>");
        return { verification: observed };
      }
      await proof.verify();
      const publicExports = [];
      for (const name of generator.modules) {
        const facade = await import(join(f.codeOwnerRoot, "src", name + ".ts"));
        const canonical = await import(
          join(f.providerRoot, "packages/native-hermes-adapter/src", name + ".ts")
        );
        publicExports.push({
          module: name,
          facade: Object.keys(facade).sort(),
          canonical: Object.keys(canonical).sort(),
          ...(id === "owner-fixed-hermes" ? {
            facadeKinds: namespaceKinds(facade), canonicalKinds: namespaceKinds(canonical),
          } : {}),
        });
      }
      return {
        publicExports,
        ...(id === "owner-fixed-hermes" ? { moduleNames: publicExports.map((m) => m.module) } : {}),
        schema: proof.schema,
        packageName: proof.packageName,
        pinPaths: proof.pins.map((p) => p.path.replace(f.fixtureRoot, "<fixture>")),
        sourceCount: proof.pins.length,
      };
    } finally {
      await rm(f.fixtureRoot, { recursive: true, force: true });
    }
  }
  return scratch("owner", async (root) => {
    const f = await ownerFixture(source, root);
    const guard = (await import(join(f.owner, "src/native/context-provider.ts"))) as {
      readRegisteredContextProvider: (
        root: string,
        module: string,
        index: Uint8Array,
        read: (
          path: string,
        ) => Promise<{ bytes: Uint8Array; pin: { path: string; sha256: string } }>,
      ) => Promise<{
        pins: { path: string; sha256: string }[];
        workMethodAvailable?: true;
        gitWorkModulePath?: string;
        sourceLocationId?: string;
      }>;
    };
    if (
      /^owner-v[1-6]$/.test(id) ||
      [
        "owner-before-drift",
        "owner-after-read-drift",
        "owner-tool-drift",
        "owner-dependency-drift",
        "owner-package-export-negative",
      ].includes(id)
    ) {
      const version = /^owner-v([1-6])$/.exec(id)?.[1] ?? "6";
      const value = structuredClone(f.registration);
      if (version !== "6") {
        value.schema = "codex.context-provider-source-closure.v" + version;
        value.sourcePins = value.sourcePins.filter(
          (p) => p.path !== "packages/work-continuity/src/work-method.ts",
        );
        const path = join(f.common, "packages/work-continuity/src/context/index.ts");
        await writeFile(
          path,
          (await readFile(path, "utf8")).replace(
            /^export \* from ["']\.\.\/work-method(?:\.ts)?["'];\r?\n/gm,
            "",
          ),
        );
      }
      if (Number(version) < 5) delete (value as Partial<typeof value>).sourceProjections;
      if (Number(version) < 4)
        value.sourcePins = value.sourcePins.filter(
          (p) => !p.path.startsWith("packages/work-continuity/src/workspace/"),
        );
      if (Number(version) < 3) {
        delete (value as Partial<typeof value>).gitWorkModulePath;
        value.sourcePins = value.sourcePins.filter(
          (p) =>
            !p.path.startsWith("packages/work-continuity/src/git-work/") &&
            !p.path.endsWith("logical-identity.ts") &&
            !p.path.endsWith("operational-state.ts"),
        );
      }
      if (version === "1") {
        value.sourcePins = value.sourcePins.filter(
          (p) => p.path !== "packages/work-continuity/releases/palantirkc-ontology-0.2.0.tgz",
        );
        const path = join(f.common, "packages/work-continuity/package.json"),
          pkg = JSON.parse(await readFile(path, "utf8"));
        delete pkg.dependencies["@palantirkc/ontology"];
        pkg.peerDependencies = { "@palantirkc/ontology": "0.2.0" };
        await writeFile(path, JSON.stringify(pkg) + "\n");
        const rootPkg = JSON.parse(await readFile(join(f.common, "package.json"), "utf8"));
        rootPkg.overrides = {
          "@palantirkc/ontology": "file:releases/palantirkc-ontology-0.2.0.tgz",
        };
        await writeFile(join(f.common, "package.json"), JSON.stringify(rootPkg) + "\n");
      }
      if (id === "owner-package-export-negative") {
        const p = join(f.common, "packages/work-continuity/package.json"),
          pkg = JSON.parse(await readFile(p, "utf8"));
        pkg.exports["./git-work"] = "./src/context/index.ts";
        await writeFile(p, JSON.stringify(pkg) + "\n");
      }
      for (const p of value.sourcePins) p.sha256 = hash(await readFile(join(f.common, p.path)));
      await writeFile(
        join(f.owner, "architecture/context-provider.json"),
        JSON.stringify(value) + "\n",
      );
      const index = Buffer.from(
        `// codex-context-provider-registration-sha256: ${hash(await readFile(join(f.owner, "architecture/context-provider.json")))}\nexport * from '@palantirkc/work-continuity/context';\n`,
      );
      const drift =
        id === "owner-tool-drift"
          ? "bun.lock"
          : id === "owner-dependency-drift"
            ? "package.json"
            : "packages/work-continuity/src/git-work/source.ts";
      if (["owner-before-drift", "owner-tool-drift", "owner-dependency-drift"].includes(id))
        await writeFile(
          join(f.common, drift),
          Buffer.concat([await readFile(join(f.common, drift)), Buffer.from("drift")]),
        );
      const reads = new Map<string, number>();
      const proof = await guard.readRegisteredContextProvider(
        f.owner,
        join(f.owner, "src/continuity/index.ts"),
        index,
        async (path) => {
          let data = await readFile(path);
          const n = (reads.get(path) ?? 0) + 1;
          reads.set(path, n);
          if (id === "owner-after-read-drift" && path === join(f.common, drift) && n > 1)
            data = Buffer.concat([data, Buffer.from("drift")]);
          return { bytes: data, pin: { path, sha256: hash(data) } };
        },
      );
      return {
        version,
        pinCount: proof.pins.length,
        workMethodAvailable: proof.workMethodAvailable ?? false,
        gitWorkModule: proof.gitWorkModulePath?.replace(f.common, "<provider>") ?? null,
        sourceLocation: proof.sourceLocationId ?? null,
      };
    }
    const imports: Record<string, string> = {
      "owner-first-git": "git-work/index.ts",
      "owner-first-memory": "workspace/recall-request.ts",
      "owner-first-work-method": "work-method.ts",
    };
    if (imports[id]) await import(join(f.common, "packages/work-continuity/src", imports[id]!));
    if (["owner-public-exports", "owner-fixed-context", ...Object.keys(imports)].includes(id)) {
      const loader = (await import(join(f.owner, "src/native/context-owner.ts"))) as {
        loadCodexOwnerContext: () => Promise<{
          index: Record<string, unknown>;
          gitWork?: Record<string, unknown>;
          workMethod?: Record<string, unknown>;
          verifyCurrentSourceClosure: () => Promise<void>;
        }>;
      };
      const context = await loader.loadCodexOwnerContext();
      await context.verifyCurrentSourceClosure();
      if (id === "owner-fixed-context") return {
        facade: namespaceKinds(context.index),
        canonical: namespaceKinds(await import(join(f.common, "packages/work-continuity/src/context/index.ts"))),
      };
      const git = context.gitWork as unknown as Record<string, unknown>;
      const method = context.workMethod as unknown as Record<string, unknown>;
      return {
        contextExports: Object.keys(context.index).sort(),
        canonicalContextExports: Object.keys(
          await import(join(f.common, "packages/work-continuity/src/context/index.ts")),
        ).sort(),
        gitExports: Object.fromEntries(
          [
            "resolveGitWorkSelection",
            "openGitWork",
            "readGitWorkContext",
            "compactGitWorkContext",
            "decodeCompactGitWorkContext",
          ].map((n) => [n, typeof git[n]]),
        ),
        workMethodExports: Object.fromEntries(
          [
            "validateWorkMethodStanding",
            "validateWorkMethodPacket",
            "validateWorkMethodReceipt",
            "workMethodPacketSha256",
            "workMethodReceiptSha256",
            "workMethodRequiredSources",
            "decideWorkMethodAdmission",
            "evaluateWorkMethodCoverage",
          ].map((n) => [n, typeof method[n]]),
        ),
      };
    }
    if (["owner-native-positive", "owner-native-after-drift"].includes(id)) {
      const native = (await import(join(f.owner, "src/native/provider-closure.ts"))) as {
        readNativeAdapterSourceClosure: () => Promise<{
          pins: { path: string; sha256: string }[];
          sourceLocationId?: string;
          providerRoot: string | null;
          verify: () => Promise<void>;
        }>;
      };
      const proof = await native.readNativeAdapterSourceClosure();
      if (id.endsWith("after-drift"))
        await writeFile(
          join(f.common, nativePaths[3]!),
          Buffer.concat([
            await readFile(join(f.common, nativePaths[3]!)),
            Buffer.from("\n//drift"),
          ]),
        );
      await proof.verify();
      return {
        pinCount: proof.pins.length,
        sourceLocation: proof.sourceLocationId,
        provider: proof.providerRoot?.replace(root, "<fixture>"),
      };
    }
    throw Error("TRUSTED_CI_UNKNOWN_OWNER_SCENARIO");
  });
}
