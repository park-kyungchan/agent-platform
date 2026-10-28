import { resolve } from "node:path";

const root = resolve(import.meta.dir, "../..");
const sourceFiles = new Set([
  "run.ts",
  "source-author.ts",
  "source-author-cli.ts",
  "publish-review-artifact.ts",
  "located-error.ts",
]);

function sourceLocation(stack: string): string | undefined {
  for (const line of stack.split("\n")) {
    if (!line.trimStart().startsWith("at ")) continue;
    const frame = line.match(/(?:\(|\s)((?:file:\/\/)?\/[^\n()]+\.ts):(\d{1,7}):\d+\)?$/);
    if (!frame || Number(frame[2]) < 1) continue;
    const path = frame[1].replace(/^file:\/\//, "");
    const prefix = `${root}/ci/reuse/`;
    if (path.startsWith(prefix) && sourceFiles.has(path.slice(prefix.length)))
      return `ci/reuse/${path.slice(prefix.length)}:${frame[2]}`;
  }
}

/** Report only a fixed boundary code and an admitted throw site, never error payloads. */
export function locatedError(
  error: unknown,
  code: "caller_source_refused" | "source_author_refused" | "review_artifact_refused",
): string {
  const fallback = new Error();
  const location =
    sourceLocation(error instanceof Error ? (error.stack ?? "") : "") ??
    sourceLocation(fallback.stack ?? "");
  return `${code}@${location}`;
}
