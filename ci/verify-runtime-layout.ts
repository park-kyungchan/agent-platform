import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, posix, relative, resolve, sep } from "node:path";
import contractShape from "../architecture/runtime-filesystem-contract.json";

export type RuntimeFilesystemContract = typeof contractShape;
export interface LayoutOptions {
  trackedPaths?: string[];
  nativeSourceRoot?: string;
}
export interface LayoutReport {
  schema: "agent-platform.runtime-layout-verification.v1";
  status: "pass";
  verificationClass: "source-layout-only";
  nativePermissionGranted: false;
  modules: number;
  skills: number;
  skillFiles: number;
  trackedPaths: number;
  nativeDeploymentSource: "not-inspected" | "verified-source-only";
  residuals: string[];
}

const contractPath = "architecture/runtime-filesystem-contract.json";
// Reviewed source identities, independent of the CI checkout and native HOME.
const admittedModules = [
  {
    capabilityId: "native-codex-source",
    packagePath: "packages/native-codex-adapter",
    packageName: "@palantirkc/native-codex-adapter",
    semanticOwner: "codex-harness-engineering",
    nativeSourceProjectId: "codex-harness-engineering",
    sourceRoot: "/home/palantirkc/projects/codex-harness-engineering",
  },
  {
    capabilityId: "native-hermes-context",
    packagePath: "packages/native-hermes-adapter",
    packageName: "@palantirkc/native-hermes-adapter",
    semanticOwner: "ontology-platform and hermes-harness-engineering",
    nativeSourceProjectId: "hermes-harness-engineering",
    sourceRoot: "/home/palantirkc/projects/hermes-harness-engineering",
  },
  {
    capabilityId: "infrastructure-domain",
    packagePath: "packages/infrastructure-operations",
    packageName: "@palantirkc/infrastructure-operations",
    semanticOwner: "vps-operations",
    nativeSourceProjectId: "vps-operations",
    sourceRoot: "/home/palantirkc/projects/vps-operations",
  },
] as const;
const unix = (path: string): string => path.split(sep).join("/");
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw Error(message);
}
const object = (value: unknown): Record<string, unknown> => {
  requireValue(value && typeof value === "object" && !Array.isArray(value), "Expected an object.");
  return value as Record<string, unknown>;
};
const safePath = (path: string): boolean =>
  typeof path === "string" &&
  path.length > 0 &&
  !isAbsolute(path) &&
  !path.includes("\\") &&
  !path.split("/").some((part) => ["", ".", ".."].includes(part));

async function exactFile(root: string, path: string, maxBytes = 256_000): Promise<string> {
  requireValue(safePath(path), "Unsafe source reference: " + path);
  const absolute = resolve(root, path);
  for (let directory = dirname(absolute); directory !== root; directory = dirname(directory)) {
    requireValue(directory.startsWith(root + sep), "Source reference escaped its owner.");
    const info = await lstat(directory);
    requireValue(
      info.isDirectory() && !info.isSymbolicLink(),
      "Source parent is not a real directory.",
    );
  }
  const info = await lstat(absolute);
  requireValue(
    info.isFile() && !info.isSymbolicLink() && (await realpath(absolute)) === absolute,
    "Source reference is not an exact regular file: " + path,
  );
  requireValue(info.size <= maxBytes, "Source file exceeds the bounded budget: " + path);
  return readFile(absolute, "utf8");
}

async function jsonFile(root: string, path: string): Promise<unknown> {
  return JSON.parse(await exactFile(root, path)) as unknown;
}

function credentialContent(text: string): void {
  requireValue(
    !/-----BEGIN (?:[A-Z]+ )*PRIVATE KEY-----|"(?:access_token|refresh_token|client_secret)"\s*:\s*"[^"]{8,}"|\b(?:gh[pousr]_[A-Za-z0-9]{24,}|github_pat_[A-Za-z0-9_]{24,}|sk-(?:proj-)?[A-Za-z0-9_-]{32,}|tskey-auth-[A-Za-z0-9_-]{20,})\b/.test(
      text,
    ),
    "Credential content is prohibited in maintained layout/skill source.",
  );
}

export function validateSourcePaths(
  paths: string[],
  policy: RuntimeFilesystemContract["sourcePolicy"],
): void {
  requireValue(paths.length <= 15_000, "Tracked source denominator exceeds the bounded budget.");
  for (const path of paths) {
    requireValue(safePath(path), "Unsafe tracked source path.");
    const parts = path.split("/");
    const basename = parts.at(-1)!;
    requireValue(
      !policy.forbiddenBasenames.includes(basename) &&
        !/^\.env(?:\.|$)|^id_(?:rsa|dsa|ecdsa|ed25519)(?:[._-]|$)|^ssh_host_|\.(?:pem|key|p12|pfx|sqlite|sqlite3|db)(?:-wal|-shm)?$/.test(
          basename,
        ),
      "Credential or native database path entered source: " + path,
    );
    const custody = policy.custodyPrefixes.some((prefix) => path.startsWith(prefix));
    requireValue(
      custody || !parts.some((part) => policy.excludedSegments.includes(part)),
      "Runtime/private/cache state entered maintained source: " + path,
    );
  }
}

function ownedName(path: string, contract: RuntimeFilesystemContract): void {
  requireValue(safePath(path), "Unsafe new owned path.");
  for (const part of path.split("/")) {
    requireValue(
      [".github", ...contract.naming.nativeRequiredNames].includes(part) ||
        /^[a-z0-9]+(?:-[a-z0-9]+)*(?:\.[a-z0-9]+(?:-[a-z0-9]+)*)*$/.test(part),
      "New owned source name must use lower-kebab-case: " + path,
    );
  }
}

async function skillMembers(root: string, directory: string): Promise<string[]> {
  const output: string[] = [];
  let directories = 0;
  async function visit(path: string): Promise<void> {
    const info = await lstat(join(root, path));
    requireValue(
      info.isDirectory() && !info.isSymbolicLink(),
      "Skill source root is not a real directory.",
    );
    requireValue(++directories <= 64 && path.length <= 1024, "Skill directory budget exceeded.");
    for (const item of await readdir(join(root, path), { withFileTypes: true })) {
      const name = path + "/" + item.name;
      requireValue(!item.isSymbolicLink(), "Skill source contains a symlink.");
      if (item.isDirectory()) await visit(name);
      else {
        requireValue(item.isFile(), "Skill source contains a non-file entry.");
        output.push(unix(relative(join(root, directory), join(root, name))));
      }
      requireValue(output.length <= 32, "Skill source exceeds the finite member budget.");
    }
  }
  await visit(directory);
  return output.sort();
}

async function verifySkill(
  root: string,
  skill: RuntimeFilesystemContract["skills"][number],
  contract: RuntimeFilesystemContract,
): Promise<number> {
  const module = contract.modules.find((item) => item.packagePath === skill.packagePath);
  requireValue(
    module?.semanticOwner === skill.semanticOwner &&
      skill.path === skill.packagePath + "/skills/" + skill.id &&
      skill.nativeProjection === "$HOME/.agents/skills/" + skill.id,
    "Skill ownership or native projection does not match its canonical package.",
  );
  const members = await skillMembers(root, skill.path);
  requireValue(
    JSON.stringify(members) === JSON.stringify([...new Set(skill.files)].sort()) &&
      skill.files.includes("SKILL.md"),
    "Skill member set differs from its explicit source allowlist.",
  );
  const texts = new Map<string, string>();
  for (const member of members) {
    const path = skill.path + "/" + member;
    ownedName(path, contract);
    validateSourcePaths([path], contract.sourcePolicy);
    const text = await exactFile(root, path, 128_000);
    credentialContent(text);
    texts.set(member, text);
    if (member.endsWith(".json")) JSON.parse(text);
  }
  const manifest = texts.get("SKILL.md")!;
  const frontmatter = manifest.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  requireValue(frontmatter, "SKILL.md requires YAML frontmatter.");
  const metadata = object(Bun.YAML.parse(frontmatter![1]) as unknown);
  requireValue(
    metadata.name === skill.id &&
      typeof metadata.description === "string" &&
      metadata.description.trim().length > 0 &&
      metadata.description.length <= 4096,
    "Skill metadata requires its declared name and a bounded nonempty description.",
  );
  for (const [member, text] of texts) {
    if (!member.endsWith(".md")) continue;
    for (const match of text.matchAll(/\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
      const link = match[1];
      if (/^(?:https?:|mailto:|#)/.test(link)) continue;
      const decoded = decodeURIComponent(link.split("#")[0]);
      requireValue(!isAbsolute(decoded) && !decoded.includes("\\"), "Unsafe skill reference.");
      const target = posix.normalize(posix.join(posix.dirname(member), decoded));
      requireValue(
        safePath(target) && members.includes(target),
        "Skill relative reference escapes its source allowlist.",
      );
      await exactFile(root, skill.path + "/" + target, 128_000);
    }
  }
  return members.length;
}

function absolutePath(path: string): boolean {
  return (
    typeof path === "string" &&
    path.startsWith("/") &&
    posix.normalize(path) === path &&
    !path.includes("\\")
  );
}

async function verifyDeploymentSource(
  nativeRoot: string,
  projection: RuntimeFilesystemContract["deploymentProjection"],
): Promise<void> {
  const compose = object(
    Bun.YAML.parse(
      await exactFile(nativeRoot, projection.sourceRelativeRoot + "/compose.yaml"),
    ) as unknown,
  );
  for (const path of projection.requiredFiles)
    await exactFile(nativeRoot, projection.sourceRelativeRoot + "/" + path);
  const services = object(compose.services);
  const mounts: { source: string; target: string; readOnly: boolean }[] = [];
  for (const service of Object.values(services)) {
    const definition = object(service);
    requireValue(Array.isArray(definition.volumes), "Deployment source requires explicit mounts.");
    for (const entry of definition.volumes) {
      if (typeof entry === "string") {
        const [source, target, mode] = entry.split(":");
        mounts.push({ source, target, readOnly: mode === "ro" });
      } else {
        const mount = object(entry);
        requireValue(
          mount.type === "bind" &&
            typeof mount.source === "string" &&
            typeof mount.target === "string",
          "Deployment source requires an explicit bind source and target.",
        );
        mounts.push({
          source: mount.source,
          target: mount.target,
          readOnly: mount.read_only === true,
        });
      }
    }
  }
  requireValue(
    mounts.length === projection.persistentMounts.length,
    "Deployment source contains an undeclared persistent/private mount.",
  );
  for (const expected of projection.persistentMounts) {
    requireValue(
      mounts.some(
        (mount) =>
          mount.source === expected.source &&
          mount.target === expected.target &&
          mount.readOnly === Boolean(expected.readOnly),
      ),
      "Persistent/private mount ownership changed in deployment source.",
    );
  }
  const unit = await exactFile(
    nativeRoot,
    projection.sourceRelativeRoot + "/herdr-runtime.service",
  );
  const allowed = [
    projection.deploymentRoot,
    ...projection.compatibilityAliases.map((alias) => alias.path),
  ];
  for (const operation of ["ExecStart", "ExecStop"]) {
    const line = unit.split("\n").find((value) => value.startsWith(operation + "="));
    requireValue(
      line && allowed.some((directory) => line.includes(directory + "/compose.yaml")),
      "Deployment unit references an undeclared compose projection.",
    );
  }
}

/** One reviewed runtime source checkout pair; it does not change canonical ownership. */
export function validateSourceProviderProjections(value: unknown): void {
  const expected = {
    id: "herdr-runtime-codex-source",
    classification: "explicit-runtime-source-checkout-only",
    canonicalConsumerRoot: "/home/palantirkc/projects/codex-harness-engineering",
    canonicalProviderRoot: "/home/palantirkc/projects/agent-platform",
    consumerRoot: "/home/palantirkc/code/shared/codex-harness-engineering",
    providerRoot: "/home/palantirkc/code/shared/agent-platform",
    providerProjectId: "agent-platform",
    packages: ["@palantirkc/work-continuity", "@palantirkc/native-codex-adapter"],
    authority: "none",
    nativePermissionGranted: false,
  };
  requireValue(
    Array.isArray(value) && value.length === 1,
    "Exactly one explicit source provider projection required.",
  );
  const actual = object(value[0]);
  requireValue(
    Object.keys(actual).sort().join("|") === Object.keys(expected).sort().join("|") &&
      Object.entries(expected).every(
        ([key, field]) => JSON.stringify(actual[key]) === JSON.stringify(field),
      ),
    "Explicit source provider projection identity or authority differs.",
  );
}

export async function verifyRuntimeLayout(
  directory: string,
  options: LayoutOptions = {},
): Promise<LayoutReport> {
  const root = resolve(directory);
  requireValue((await realpath(root)) === root, "Source owner root must be canonical.");
  const contract = (await jsonFile(root, contractPath)) as RuntimeFilesystemContract;
  credentialContent(await exactFile(root, contractPath));
  requireValue(
    contract.schema === "agent-platform.runtime-filesystem-contract.v1" &&
      contract.verificationClass === "source-layout-only" &&
      contract.nativePermissionGranted === false &&
      contract.canonicalRepository === "park-kyungchan/agent-platform" &&
      contract.commonPackage === "@palantirkc/agent-platform" &&
      contract.commonSourceRoot === "/home/palantirkc/projects/agent-platform" &&
      contract.sourceRegistry === "architecture/capability-source-owners.json",
    "Invalid canonical layout contract or native authority claim.",
  );
  validateSourceProviderProjections(contract.sourceProviderProjections);
  const rootPackage = object(await jsonFile(root, "package.json"));
  requireValue(
    rootPackage.name === contract.commonPackage,
    "Common source package identity differs.",
  );
  const project = object(await jsonFile(root, "project.json"));
  const details = object(object(project.payload).details);
  requireValue(
    project.kind === "project" &&
      project.id === "agent-platform:project:manifest" &&
      project.projectId === "agent-platform" &&
      project.domainId === "native-harness" &&
      details.sourceOwnerRoot === contract.commonSourceRoot &&
      details.intendedCanonicalRoot === contract.commonSourceRoot &&
      details.capabilitySourceRegistry === contract.sourceRegistry,
    "Common project identity or declared source owner differs.",
  );
  const registry = object(await jsonFile(root, contract.sourceRegistry));
  requireValue(
    registry.schema === "agent-platform.capability-source-owners.v1" &&
      registry.id === "agent-platform:architecture:capability-source-owners" &&
      registry.status === "current-source-owner-registry" &&
      registry.codeOwner === "agent-platform" &&
      Array.isArray(registry.capabilities),
    "Invalid canonical capability registry identity.",
  );
  requireValue(
    JSON.stringify(contract.modules.map((module) => module.capabilityId).sort()) ===
      JSON.stringify(["infrastructure-domain", "native-codex-source", "native-hermes-context"]),
    "The canonical Codex, Hermes and infrastructure module boundaries are required.",
  );
  const ids = contract.nativeSourceProjects.map((item) => item.id);
  requireValue(
    JSON.stringify([...ids].sort()) ===
      JSON.stringify(admittedModules.map((item) => item.nativeSourceProjectId).sort()),
    "The exact native source owners are required without duplicates.",
  );
  for (const project of contract.nativeSourceProjects) {
    const admitted = admittedModules.find((item) => item.nativeSourceProjectId === project.id);
    requireValue(
      admitted && absolutePath(project.sourceRoot) && project.sourceRoot === admitted.sourceRoot,
      "Native source project differs from its reviewed explicit owner root.",
    );
    requireValue(
      project.historicalSource.homeRelativeRoot === "code/shared/" + project.id &&
        project.historicalSource.namespace === "previous-source-owner-home" &&
        project.historicalSource.classification === "history-only-no-provider-fallback",
      "Historical source paths must retain their attributed non-provider classification.",
    );
  }
  for (const module of contract.modules) {
    ownedName(module.packagePath, contract);
    const admitted = admittedModules.find((item) => item.capabilityId === module.capabilityId);
    requireValue(
      admitted &&
        module.packagePath === admitted.packagePath &&
        module.packageName === admitted.packageName &&
        module.semanticOwner === admitted.semanticOwner &&
        module.nativeSourceProjectId === admitted.nativeSourceProjectId,
      "Canonical module differs from its fixed native/domain owner identity.",
    );
    const matches = registry.capabilities
      .map(object)
      .filter((item) => item.id === module.capabilityId);
    requireValue(matches.length === 1, "Canonical module registry identity must be unique.");
    const capability = matches[0];
    const nativeProject = contract.nativeSourceProjects.find(
      (project) => project.id === module.nativeSourceProjectId,
    );
    requireValue(
      capability.packagePath === module.packagePath &&
        capability.packageName === module.packageName &&
        capability.semanticOwner === module.semanticOwner &&
        nativeProject &&
        capability.nativeAdapterOwnerRoot === nativeProject.sourceRoot,
      "Canonical module ownership differs from the admitted capability registry.",
    );
    requireValue(
      object(capability.packageManifest).path ===
        contract.commonSourceRoot + "/" + module.packagePath + "/package.json",
      "Canonical module manifest root differs from its declared common source owner.",
    );
    const manifest = object(await jsonFile(root, module.packagePath + "/package.json"));
    requireValue(
      manifest.name === module.packageName,
      "Canonical module package identity differs.",
    );
  }
  for (const path of contract.sourcePolicy.ownedPaths) {
    ownedName(path, contract);
    await exactFile(root, path);
  }
  const paths =
    options.trackedPaths ??
    (() => {
      const result = Bun.spawnSync(["git", "-C", root, "ls-files", "-z"], {
        stdout: "pipe",
        stderr: "pipe",
      });
      requireValue(
        result.exitCode === 0 && result.stdout.length <= 2_000_000,
        "Cannot read bounded native Git source paths.",
      );
      return result.stdout.toString().split("\0").filter(Boolean);
    })();
  validateSourcePaths(paths, contract.sourcePolicy);
  let skillFiles = 0;
  requireValue(
    new Set(contract.skills.map((skill) => skill.id)).size === contract.skills.length,
    "Duplicate skill identity.",
  );
  for (const skill of contract.skills) skillFiles += await verifySkill(root, skill, contract);
  const projection = contract.deploymentProjection;
  requireValue(
    projection.serviceRoot === "/docker/herdr-runtime" &&
      projection.sourceRelativeRoot === "deployments/herdr-runtime" &&
      projection.deploymentRoot === projection.serviceRoot + "/deployment" &&
      projection.owner === "vps-operations",
    "Invalid native deployment source owner or root.",
  );
  for (const alias of projection.compatibilityAliases)
    requireValue(
      absolutePath(alias.path) &&
        alias.path === projection.serviceRoot + "/build" &&
        alias.target === projection.deploymentRoot &&
        alias.status === "proposed-requires-owner-review-and-live-readback",
      "Invalid compatibility alias or unverified host migration claim.",
    );
  for (const mount of projection.persistentMounts)
    requireValue(
      absolutePath(mount.source) &&
        absolutePath(mount.target) &&
        mount.source.startsWith(projection.serviceRoot + "/") &&
        !mount.source.startsWith(projection.deploymentRoot + "/"),
      "Persistent/private state overlaps deployment source.",
    );
  if (options.nativeSourceRoot)
    await verifyDeploymentSource(resolve(options.nativeSourceRoot), projection);
  return {
    schema: "agent-platform.runtime-layout-verification.v1",
    status: "pass",
    verificationClass: "source-layout-only",
    nativePermissionGranted: false,
    modules: contract.modules.length,
    skills: contract.skills.length,
    skillFiles,
    trackedPaths: paths.length,
    nativeDeploymentSource: options.nativeSourceRoot ? "verified-source-only" : "not-inspected",
    residuals: [
      "Native skill discovery, authentication, behavior and AcceptanceDecision are not verified.",
      "Host deployment migration/compatibility alias, process ownership and private runtime state are not inspected.",
      ...(options.nativeSourceRoot
        ? []
        : [
            "Optional native infrastructure source was not supplied; no private sibling checkout is assumed.",
          ]),
    ],
  };
}

if (import.meta.main) {
  try {
    const args = process.argv.slice(2);
    requireValue(
      args.length === 0 || (args.length === 2 && args[0] === "--native-source-root"),
      "Usage: bun ci/verify-runtime-layout.ts [--native-source-root PATH]",
    );
    console.log(
      JSON.stringify(
        await verifyRuntimeLayout(resolve(import.meta.dir, ".."), {
          nativeSourceRoot: args[1],
        }),
        null,
        2,
      ),
    );
  } catch (error) {
    console.error(
      error instanceof Error ? error.message : "Runtime layout source verification failed.",
    );
    process.exitCode = 1;
  }
}
