import { readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";

const validationScript =
  "if (commit.sha !== head || !/^[a-f0-9]{40}$/.test(commit.tree?.sha ?? '')) {\n  throw new Error('GitHub API returned another head or invalid tree');\n}\nlet receiptVerification;\ntry {\n  receiptVerification = await validateReceiptWithEvidence(\n    receipt, head, commit.tree.sha, repo.owner + '/' + repo.repo,\n    {\n      readTree: async treeSha => api('GET /repos/{owner}/{repo}/git/trees/{tree_sha}', { tree_sha: treeSha, recursive: '1' }),\n      readFile: async (path, ref) => api('GET /repos/{owner}/{repo}/contents/{path}', { path, ref }),\n    },\n  );\n} catch (error) {\n  throw new Error('Receipt does not prove independent native review of this exact head and tree', { cause: error });\n}";

export async function observerValidationScript(root: string): Promise<string> {
  const transpiler = new Bun.Transpiler({ loader: "ts", target: "node" });
  const validator = transpiler
    .transformSync(
      (await readFile(join(root, "ci/quality/receipt-validator.ts"), "utf8")).replace(
        'import { createHash } from "node:crypto";',
        'const { createHash } = require("node:crypto");',
      ),
    )
    .replace(/^export /gm, "");
  const gate = transpiler
    .transformSync(
      (await readFile(join(root, "ci/quality/receipt-validation-gate.ts"), "utf8")).replace(
        /^import \{[\s\S]*?\} from "\.\/receipt-validator";\n/,
        "",
      ),
    )
    .replace(/^export /gm, "");
  return validator + "\n" + gate + "\n" + validationScript;
}

export async function synchronizeObserverProjection(root: string, write = false): Promise<void> {
  const path = join(root, ".github/workflows/native-review-arrival.yml"),
    original = await readFile(path, "utf8");
  const start = "            // native-review-validator:start\n",
    end = "            // native-review-validator:end";
  const from = original.indexOf(start),
    to = original.indexOf(end, from + start.length);
  if (
    from < 0 ||
    to < 0 ||
    original.indexOf(start, from + 1) !== -1 ||
    original.indexOf(end, to + 1) !== -1
  )
    throw Error("Unique native validator projection markers required");
  const expected =
    (await observerValidationScript(root))
      .split("\n")
      .map((line) => (line ? "            " + line : ""))
      .join("\n") + "\n";
  const actual = original.slice(from + start.length, to);
  if (actual === expected) return;
  if (!write)
    throw Error(
      "Native observer validator projection drift; regenerate reviewed source with --write",
    );
  if (process.env.GITHUB_ACTIONS === "true")
    throw Error("Hosted CI cannot regenerate its own review predicate");
  await writeFile(path, original.slice(0, from + start.length) + expected + original.slice(to));
}

if (import.meta.main) {
  if (process.argv.length !== 3 || !["--check", "--write"].includes(process.argv[2]!))
    throw Error("Usage: bun ci/quality/observer-projection.ts [--check|--write]");
  await synchronizeObserverProjection(
    resolve(import.meta.dir, "../.."),
    process.argv[2] === "--write",
  );
}
