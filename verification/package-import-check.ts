import * as legacy from "../compatibility/codex-harness-engineering/src/workspace/handoff";
import * as canonical from "@palantirkc/work-continuity/handoff";
import { realpath, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
const resolved = await realpath(
  fileURLToPath(import.meta.resolve("@palantirkc/work-continuity/handoff")),
);
const expected = fileURLToPath(
  new URL("../packages/work-continuity/src/handoff.ts", import.meta.url),
);
if (resolved !== expected)
  throw Error("Package resolved outside transported canonical implementation");
if (
  JSON.stringify(Object.keys(legacy).sort()) !== JSON.stringify(Object.keys(canonical).sort()) ||
  Object.keys(canonical).some((key) => (canonical as any)[key] !== (legacy as any)[key])
)
  throw Error("Legacy exports do not forward the single canonical module");
if (
  createHash("sha256")
    .update(await readFile(resolved))
    .digest("hex") !== "8052ee2f5072e6589da711c838feb3dfa095e8d75e1ac17ed027e0494e36bb3a"
)
  throw Error("Canonical implementation bytes drift");
console.log(
  JSON.stringify({
    schema: "workspace.full-source-package-import-readback.v1",
    resolvedCanonicalPath: resolved,
    legacyAndCanonicalSameModule: true,
    exports: Object.keys(canonical).sort(),
    sha256: "8052ee2f5072e6589da711c838feb3dfa095e8d75e1ac17ed027e0494e36bb3a",
    modelCalls: 0,
    canonicalIssueStateMutation: false,
  }),
);
