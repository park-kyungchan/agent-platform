import { createHash } from "node:crypto";
import { exactSourceFile, type QualityScope } from "./scope";

export interface QualitySourceSnapshot {
  schema: "agent-platform.quality-source-snapshot.v1";
  files: { path: string; sha256: string }[];
  contentSha256: string;
  scopeSha256: string;
}

const hash = (value: string): string => createHash("sha256").update(value).digest("hex");

/** Bind every maintained check input to its bytes, separately from a dirty checkout's HEAD. */
export async function captureQualitySources(scope: QualityScope): Promise<QualitySourceSnapshot> {
  const registry = JSON.parse(
    await exactSourceFile(scope.root, "architecture/capability-source-owners.json"),
  ) as { capabilities: { sourcePins: { relativePath: string }[] }[] };
  const policy = JSON.parse(
    await exactSourceFile(scope.root, "architecture/quality-policy.json"),
  ) as { ownedDefinitionInputs: { path: string }[] };
  const paths = [
    ...new Set([
      ...scope.formatFiles,
      ...registry.capabilities.flatMap((entry) => entry.sourcePins.map((pin) => pin.relativePath)),
      ...policy.ownedDefinitionInputs.map((pin) => pin.path),
      "architecture/capability-source-owners.json",
      "architecture/quality-policy.json",
      "ci/quality/any-baseline.json",
      "bun.lock",
      ".editorconfig",
      ".prettierignore",
    ]),
  ].sort();
  const files: QualitySourceSnapshot["files"] = [];
  for (const path of paths)
    files.push({ path, sha256: hash(await exactSourceFile(scope.root, path)) });
  const { root: _hostRoot, ...portableScope } = scope;
  return {
    schema: "agent-platform.quality-source-snapshot.v1",
    files,
    contentSha256: hash(JSON.stringify(files)),
    scopeSha256: hash(JSON.stringify(portableScope)),
  };
}

export function compareQualitySources(
  before: QualitySourceSnapshot,
  after: QualitySourceSnapshot,
): {
  status: "pass" | "fail";
  changedPaths: string[];
  scopeChanged: boolean;
} {
  const previous = new Map(before.files.map((file) => [file.path, file.sha256]));
  const next = new Map(after.files.map((file) => [file.path, file.sha256]));
  const changedPaths = [...new Set([...previous.keys(), ...next.keys()])]
    .filter((path) => previous.get(path) !== next.get(path))
    .sort();
  const scopeChanged = before.scopeSha256 !== after.scopeSha256;
  return {
    status: changedPaths.length || scopeChanged ? "fail" : "pass",
    changedPaths,
    scopeChanged,
  };
}
