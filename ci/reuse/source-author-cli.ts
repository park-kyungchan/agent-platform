import { readFile } from "node:fs/promises";
import {
  captureCodexSourceAuthor,
  parseSourceAuthorJson,
  sourceAuthorGit,
  verifyCommittedSourceAuthor,
} from "./source-author";

if (import.meta.main) {
  try {
    const args = process.argv.slice(2);
    if (args[0] === "capture" && args.length === 2) {
      const selection = parseSourceAuthorJson(await readFile(args[1]!));
      const base = await sourceAuthorGit(process.cwd(), ["rev-parse", "HEAD"]);
      const metadata = captureCodexSourceAuthor(selection, base, process.env);
      console.log(JSON.stringify(metadata, null, 2));
    } else if (args[0] === "verify" && args.length === 1) {
      const repository = process.env.COMMON_CI_REPOSITORY ?? "";
      if (process.env.GITHUB_ACTIONS === "true" && repository !== process.env.GITHUB_REPOSITORY)
        throw Error("Authoritative GitHub repository differs");
      const readback = await verifyCommittedSourceAuthor({
        workspace: process.env.GITHUB_WORKSPACE ?? process.cwd(),
        repository,
        projectId: process.env.COMMON_CI_PROJECT_ID ?? "",
        sourceHead: process.env.COMMON_CI_SOURCE_SHA ?? "",
        metadataPath: process.env.COMMON_CI_SOURCE_AUTHOR_METADATA,
      });
      console.log(JSON.stringify(readback));
    } else throw Error("Use capture <exact-scope.json> or verify with explicit source inputs");
  } catch {
    console.error(
      "Source-author metadata refused: require actual native capture and exact committed readback",
    );
    process.exitCode = 1;
  }
}
