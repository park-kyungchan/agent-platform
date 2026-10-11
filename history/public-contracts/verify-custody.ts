import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve, relative, isAbsolute } from "node:path";
import { spawnSync } from "node:child_process";

type FilePin = { path: string; bytes: number; sha256: string };
type Repository = {
  repository: string;
  head: string;
  advertisedRefs: string[];
  bundle: { canonicalPath: string; bytes: number; sha256: string };
  headArchive: FilePin;
  headFiles: FilePin[];
};
const root = resolve(import.meta.dir, "../..");
const custody = JSON.parse(readFileSync(resolve(import.meta.dir, "custody.json"), "utf8")) as {
  schema: string; repositories: Repository[];
};
if (custody.schema !== "agent-platform.public-contract-history-custody.v1" || custody.repositories.length !== 4)
  throw Error("Unexpected custody scope");
function pinned(path: string, bytes: number, digest: string) {
  const full = resolve(root, path), rel = relative(root, full);
  if (rel.startsWith("..") || isAbsolute(rel)) throw Error("Custody path escapes canonical owner");
  const data = readFileSync(full);
  if (data.length !== bytes || createHash("sha256").update(data).digest("hex") !== digest)
    throw Error(`Custody bytes changed: ${path}`);
  return full;
}
function run(command: string, args: string[]) {
  const result = spawnSync(command, args, { cwd: root, maxBuffer: 8 * 1024 * 1024 });
  if (result.status !== 0) throw Error(`${command} failed: ${result.stderr.toString()}`);
  return result.stdout;
}
const receipts = custody.repositories.map(repository => {
  const bundle = pinned(repository.bundle.canonicalPath, repository.bundle.bytes, repository.bundle.sha256);
  const archive = pinned(repository.headArchive.path, repository.headArchive.bytes, repository.headArchive.sha256);
  const refs = run("git", ["bundle", "list-heads", bundle]).toString("utf8").trim().split("\n");
  for (const ref of repository.advertisedRefs) if (!refs.includes(ref)) throw Error(`Missing preserved ref: ${ref}`);
  if (!refs.includes(`${repository.head} HEAD`)) throw Error("Missing preserved HEAD");
  run("git", ["bundle", "verify", bundle]);
  const names = run("tar", ["-tzf", archive]).toString("utf8").trim().split("\n").filter(name => !name.endsWith("/"));
  if (names.length !== repository.headFiles.length || names.some(name => !repository.headFiles.some(file => file.path === name)))
    throw Error("HEAD archive inventory changed");
  for (const file of repository.headFiles) {
    const data = run("tar", ["-xOzf", archive, "--", file.path]);
    if (data.length !== file.bytes || createHash("sha256").update(data).digest("hex") !== file.sha256)
      throw Error(`Archived file changed: ${repository.repository}/${file.path}`);
  }
  return { repository: repository.repository, head: repository.head, filesVerified: names.length, status: "PRESERVED" };
});
console.log(JSON.stringify({ schema: "agent-platform.public-contract-custody-readback.v1", status: "VERIFIED", receipts,
  limits: ["Local immutable source custody only; remote deletion/publication and native runtime application are separate."] }));
