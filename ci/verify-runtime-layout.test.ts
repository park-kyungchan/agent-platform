import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import contractShape from "../architecture/runtime-filesystem-contract.json";
import {
  validateSourcePaths,
  verifyRuntimeLayout,
  type RuntimeFilesystemContract,
} from "./verify-runtime-layout";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function put(root: string, path: string, content: string): Promise<void> {
  await mkdir(dirname(join(root, path)), { recursive: true });
  await writeFile(join(root, path), content);
}

async function fixture(): Promise<{
  root: string;
  contract: RuntimeFilesystemContract;
  save: () => Promise<void>;
}> {
  const root = await mkdtemp(join(tmpdir(), "runtime-layout-"));
  roots.push(root);
  const contract = structuredClone(contractShape);
  const save = () =>
    put(root, "architecture/runtime-filesystem-contract.json", JSON.stringify(contract));
  for (const path of contract.sourcePolicy.ownedPaths)
    await put(root, path, "Source-only fixture.\n");
  await put(root, "package.json", JSON.stringify({ name: contract.commonPackage }));
  await put(
    root,
    contract.sourceRegistry,
    JSON.stringify({
      capabilities: contract.modules.map((module) => ({
        id: module.capabilityId,
        packagePath: module.packagePath,
        packageName: module.packageName,
        semanticOwner: module.semanticOwner,
        nativeAdapterOwnerRoot:
          "/fixture/" +
          contract.nativeSourceProjects.find((project) =>
            module.semanticOwner.includes(project.id),
          )!.homeRelativeRoot,
      })),
    }),
  );
  for (const module of contract.modules)
    await put(
      root,
      module.packagePath + "/package.json",
      JSON.stringify({ name: module.packageName }),
    );
  const skill = contract.skills[0];
  for (const member of skill.files)
    await put(root, skill.path + "/" + member, member.endsWith(".json") ? "{}" : "Reference.\n");
  await put(
    root,
    skill.path + "/SKILL.md",
    [
      "---",
      "name: " + skill.id,
      "description: Inspect the selected source without runtime dispatch.",
      "---",
      "[Checkpoint](references/checkpoint-contract.json)",
      "[Decision](references/standing-user-decision.json)",
      "",
    ].join("\n"),
  );
  await save();
  return { root, contract, save };
}

test("a standalone canonical source fixture reports explicit unverified native boundaries", async () => {
  const f = await fixture();
  const result = await verifyRuntimeLayout(f.root, {
    trackedPaths: [
      "packages/native-codex-adapter/package.json",
      "history/source537/original_name.json",
    ],
  });
  expect(result).toMatchObject({
    status: "pass",
    verificationClass: "source-layout-only",
    nativePermissionGranted: false,
    modules: 3,
    skillFiles: 4,
    nativeDeploymentSource: "not-inspected",
  });
  expect(result.residuals.length).toBeGreaterThan(0);
});

test("a package moved to another source owner cannot pass an unchanged capability declaration", async () => {
  const f = await fixture();
  await put(
    f.root,
    f.contract.modules[0].packagePath + "/package.json",
    JSON.stringify({
      name: "@another-owner/native-codex-adapter",
    }),
  );
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow(
    "package identity",
  );
});

test("relative skill references cannot escape the finite source bundle", async () => {
  const f = await fixture();
  const skill = f.contract.skills[0];
  await put(
    f.root,
    skill.path + "/references/home-routing.candidate.md",
    "[Outside](../../../../package.json)",
  );
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow(
    "source allowlist",
  );
});

test("skill references through a symlink are rejected even when the target is readable", async () => {
  const f = await fixture();
  const path = f.contract.skills[0].path + "/references/checkpoint-contract.json";
  await rm(join(f.root, path));
  await symlink(join(f.root, "package.json"), join(f.root, path));
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow("symlink");
});

test("unlisted skill members and unreadable JSON cannot silently enter the source projection", async () => {
  const f = await fixture();
  const skill = f.contract.skills[0];
  await put(f.root, skill.path + "/references/checkpoint-contract.json", "{broken");
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow();
  await put(f.root, skill.path + "/references/checkpoint-contract.json", "{}");
  await put(f.root, skill.path + "/references/extra.json", "{}");
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow("member set");
});

test("empty native metadata cannot pass a syntactically readable SKILL manifest", async () => {
  const f = await fixture();
  await put(
    f.root,
    f.contract.skills[0].path + "/SKILL.md",
    "---\nname: herdr-codex-long-work\ndescription: ''\n---\n",
  );
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow("metadata");
});

test("private state and credentials are rejected without renaming immutable custody records", () => {
  for (const path of [
    ".codex/auth.json",
    "packages/native-codex-adapter/state/session.json",
    "work/past/original/id_ed25519",
    "releases/state_5.sqlite",
    "ci/.env.local",
  ])
    expect(() => validateSourcePaths([path], contractShape.sourcePolicy)).toThrow();
  expect(() =>
    validateSourcePaths(
      [
        "history/source537/old_native_name.json",
        "work/project/work/records/native%3Aencoded-id.json",
      ],
      contractShape.sourcePolicy,
    ),
  ).not.toThrow();
});

test("pinned native Herdr state and configuration retain separate roles outside published source", () => {
  const runtime = contractShape.classes.find((item) => item.id === "runtime-state")!;
  const configuration = contractShape.classes.find((item) => item.id === "native-configuration")!;
  const nativeState = [
    "herdr.sock",
    "herdr-client.sock",
    "session.json",
    "session-snapshots/",
    "herdr-server.log",
  ];
  for (const name of nativeState) {
    expect(runtime.locations).toContain("$HOME/.config/herdr/" + name);
    expect(configuration.locations).not.toContain("$HOME/.config/herdr/" + name);
    expect(() =>
      validateSourcePaths(
        ["releases/.config/herdr/" + name + (name.endsWith("/") ? "snapshot.json" : "")],
        contractShape.sourcePolicy,
      ),
    ).toThrow();
  }
  expect(configuration.locations).toContain("$HOME/.config/herdr/config.toml");
  expect(runtime.locations).not.toContain("$HOME/.config/herdr/config.toml");
  expect(runtime.locations).not.toContain("$HOME/.local/share/herdr/");
});

test("named SSH private keys stay excluded even inside immutable-release or custody paths", () => {
  for (const path of [
    "releases/id_ed25519_vps_operations",
    "releases/id_ed25519-herdr",
    "history/source537/id_rsa_old",
    "releases/id_ecdsa_sk_vps",
    "releases/id_dsa_legacy",
  ])
    expect(() => validateSourcePaths([path], contractShape.sourcePolicy)).toThrow("Credential");
});

test("new owned naming and actual credential payloads fail before projection", async () => {
  const f = await fixture();
  const skill = f.contract.skills[0];
  const old = skill.path + "/references/home-routing.candidate.md";
  const next = "references/Bad_Name.json";
  await rm(join(f.root, old));
  skill.files[skill.files.indexOf("references/home-routing.candidate.md")] = next;
  await put(f.root, skill.path + "/" + next, "{}");
  await f.save();
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow("lower-kebab");
  skill.files[skill.files.indexOf(next)] = "references/home-routing.candidate.md";
  await rm(join(f.root, skill.path + "/" + next));
  await put(f.root, old, "-----BEGIN PRIVATE KEY-----\nfixture-only\n");
  await f.save();
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow(
    "Credential content",
  );
  await put(f.root, old, "Reference.\n");
  await put(
    f.root,
    skill.path + "/references/checkpoint-contract.json",
    JSON.stringify({
      token: "ghp_" + "a".repeat(36),
    }),
  );
  await expect(verifyRuntimeLayout(f.root, { trackedPaths: [] })).rejects.toThrow(
    "Credential content",
  );
});

async function nativeFixture(f: Awaited<ReturnType<typeof fixture>>): Promise<string> {
  const native = join(f.root, "native-owner");
  const projection = f.contract.deploymentProjection;
  for (const path of projection.requiredFiles)
    await put(native, projection.sourceRelativeRoot + "/" + path, "Source-only fixture.\n");
  await put(
    native,
    projection.sourceRelativeRoot + "/compose.yaml",
    [
      "services:",
      "  herdr:",
      "    volumes:",
      ...projection.persistentMounts.map(
        (mount) => "      - " + mount.source + ":" + mount.target + (mount.readOnly ? ":ro" : ""),
      ),
      "",
    ].join("\n"),
  );
  await put(
    native,
    projection.sourceRelativeRoot + "/herdr-runtime.service",
    [
      "ExecStart=/usr/bin/docker compose -f " +
        projection.compatibilityAliases[0].path +
        "/compose.yaml up",
      "ExecStop=/usr/bin/docker compose -f " + projection.deploymentRoot + "/compose.yaml down",
      "",
    ].join("\n"),
  );
  return native;
}

test("optional native source accepts declared compatibility routes without claiming a live alias", async () => {
  const f = await fixture();
  const native = await nativeFixture(f);
  const result = await verifyRuntimeLayout(f.root, { trackedPaths: [], nativeSourceRoot: native });
  expect(result.nativeDeploymentSource).toBe("verified-source-only");
  expect(result.nativePermissionGranted).toBe(false);
  expect(result.residuals.join(" ")).toContain("not inspected");
});

test("native source cannot replace the persistent Home with deployment files or an undeclared unit path", async () => {
  const f = await fixture();
  const native = await nativeFixture(f);
  const projection = f.contract.deploymentProjection;
  const composePath = projection.sourceRelativeRoot + "/compose.yaml";
  await put(
    native,
    composePath,
    [
      "services:",
      "  herdr:",
      "    volumes:",
      ...projection.persistentMounts.map(
        (mount, index) =>
          "      - " +
          (index === 0 ? projection.deploymentRoot : mount.source) +
          ":" +
          mount.target +
          (mount.readOnly ? ":ro" : ""),
      ),
      "",
    ].join("\n"),
  );
  await expect(
    verifyRuntimeLayout(f.root, { trackedPaths: [], nativeSourceRoot: native }),
  ).rejects.toThrow("mount ownership");
  await nativeFixture(f);
  await put(
    native,
    projection.sourceRelativeRoot + "/herdr-runtime.service",
    "ExecStart=/usr/bin/docker compose -f /tmp/unowned/compose.yaml up\nExecStop=/usr/bin/true\n",
  );
  await expect(
    verifyRuntimeLayout(f.root, { trackedPaths: [], nativeSourceRoot: native }),
  ).rejects.toThrow("undeclared compose");
});

test("an undeclared Docker socket bind cannot join the accepted persistent source mounts", async () => {
  const f = await fixture();
  const native = await nativeFixture(f);
  const projection = f.contract.deploymentProjection;
  await put(
    native,
    projection.sourceRelativeRoot + "/compose.yaml",
    [
      "services:",
      "  herdr:",
      "    volumes:",
      ...projection.persistentMounts.map(
        (mount) => "      - " + mount.source + ":" + mount.target + (mount.readOnly ? ":ro" : ""),
      ),
      "      - /var/run/docker.sock:/var/run/docker.sock",
      "",
    ].join("\n"),
  );
  await expect(
    verifyRuntimeLayout(f.root, { trackedPaths: [], nativeSourceRoot: native }),
  ).rejects.toThrow("undeclared persistent/private mount");
});
