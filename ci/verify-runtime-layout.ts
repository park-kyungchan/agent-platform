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
      contract.canonicalRepository === "park-kyungchan/agent-platform",
    "Invalid canonical layout contract or native authority claim.",
  );
  const rootPackage = object(await jsonFile(root, "package.json"));
  requireValue(
    rootPackage.name === contract.commonPackage,
    "Common source package identity differs.",
  );
  const registry = object(await jsonFile(root, contract.sourceRegistry));
  requireValue(Array.isArray(registry.capabilities), "Missing canonical capability registry.");
  requireValue(
    JSON.stringify(contract.modules.map((module) => module.capabilityId).sort()) ===
      JSON.stringify(["infrastructure-domain", "native-codex-source", "native-hermes-context"]),
    "The canonical Codex, Hermes and infrastructure module boundaries are required.",
  );
  for (const module of contract.modules) {
    ownedName(module.packagePath, contract);
    const capability = registry.capabilities
      .map(object)
      .find((item) => item.id === module.capabilityId);
    requireValue(
      capability?.packagePath === module.packagePath &&
        capability.packageName === module.packageName &&
        capability.semanticOwner === module.semanticOwner &&
        contract.nativeSourceProjects.some(
          (project) =>
            module.semanticOwner.includes(project.id) &&
            typeof capability.nativeAdapterOwnerRoot === "string" &&
            capability.nativeAdapterOwnerRoot.endsWith("/" + project.homeRelativeRoot),
        ),
      "Canonical module ownership differs from the admitted capability registry.",
    );
    const manifest = object(await jsonFile(root, module.packagePath + "/package.json"));
    requireValue(
      manifest.name === module.packageName,
      "Canonical module package identity differs.",
    );
  }
  const ids = contract.nativeSourceProjects.map((item) => item.id);
  requireValue(new Set(ids).size === ids.length, "Duplicate native source owner.");
  for (const project of contract.nativeSourceProjects)
    requireValue(
      /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(project.id) &&
        safePath(project.homeRelativeRoot) &&
        project.homeRelativeRoot === "code/shared/" + project.id,
      "Native source project is outside its existing owner root.",
    );
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
