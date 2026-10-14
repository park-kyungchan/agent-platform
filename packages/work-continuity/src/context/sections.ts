import type {
  ContextIo,
  ContinuityCheckpoint,
  SourcePin,
  SourceStatus,
  WorkContextRequest,
} from "./types";
import { localContextIo, assertContextSnapshotStable } from "./source";
import { validateWorkContextRequest } from "./validation";
import { readCheckpoint } from "./store";

export const contextSections = [
  "userDecisions",
  "completed",
  "pending",
  "uncertainEffects",
  "uninterpretedInputs",
  "nativeThreads",
  "sourcePins",
  "evidenceRefs",
  "successCriteria",
  "inScope",
  "outOfScope",
] as const;
export type ContextSection = (typeof contextSections)[number];

/** Omit only repeated successful checks. Failures and repair instructions stay visible. */
export function compactSourceStatus(source: SourceStatus) {
  const counts: Record<string, number> = {};
  for (const check of source.checks) counts[check.status] = (counts[check.status] ?? 0) + 1;
  return {
    status: source.status,
    checked: source.checks.length,
    counts,
    checks: source.checks.filter((check) => check.status !== "current"),
    omittedCurrentChecks: counts.current ?? 0,
    repair: source.repair,
    authority: source.authority,
  };
}

/** Keep the original detailed result as the default; compact is an explicit CLI view. */
export function compactWorkContext<
  T extends { sourceStatus: SourceStatus; projection: { text: string } },
>(result: T) {
  const { text, ...projection } = result.projection;
  return {
    ...result,
    sourceStatus: compactSourceStatus(result.sourceStatus),
    projection: { ...projection, value: JSON.parse(text) },
  };
}

function rows(checkpoint: ContinuityCheckpoint, section: ContextSection): readonly unknown[] {
  if (section === "successCriteria" || section === "inScope" || section === "outOfScope")
    return checkpoint.purpose[section];
  return checkpoint[section];
}
function pinsFor(
  checkpoint: ContinuityCheckpoint,
  section: ContextSection,
  row: unknown,
): SourcePin[] {
  if (section !== "completed") return [];
  const effect = row as ContinuityCheckpoint["completed"][number];
  return effect.verificationRefs.map((path) =>
    checkpoint.sourcePins.find((pin) => pin.path === path)!,
  );
}

/** Exact whole-row pages, pinned to a verified checkpoint. Never reinterprets decisions. */
export async function readWorkContextSection(
  input: WorkContextRequest | unknown,
  options: { section: ContextSection; offset?: number; limit?: number },
  io: ContextIo = localContextIo,
) {
  const request = validateWorkContextRequest(input),
    { section } = options;
  if (!contextSections.includes(section))
    throw Error(`Unknown context section; choose ${contextSections.join(", ")}`);
  const offset = options.offset ?? 0,
    limit = options.limit ?? 10,
    maxChars = request.maxChars ?? 6000;
  if (!Number.isSafeInteger(offset) || offset < 0)
    throw Error("Section offset must be a nonnegative safe integer");
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100)
    throw Error("Section limit must be between 1 and 100");
  if (
    (await io.canonical(request.root)) !== request.root ||
    (await io.canonical(request.cwd)) !== request.cwd
  )
    throw Error("Work root and cwd must be exact canonical paths");
  const read = await readCheckpoint(request.checkpointPath, { root: request.root, io });
  const checkpoint = read.checkpoint;
  if (checkpoint.projectId !== request.projectId || checkpoint.workId !== request.workId)
    throw Error("Checkpoint does not match selected Project/Work");
  if (request.checkpointSha256 && request.checkpointSha256 !== read.sha256)
    throw Error("Checkpoint differs from pinned request");
  const sourceRows = rows(checkpoint, section);
  if (offset > sourceRows.length) throw Error("Section offset exceeds total rows");
  const items: unknown[] = [],
    sourceRefs: Record<string, SourcePin> = {};
  const value = {
    schema: "codex.work-context.section.v1",
    projectId: request.projectId,
    workId: request.workId,
    checkpointPin: {
      path: request.checkpointPath,
      sha256: read.sha256,
      version: checkpoint.version,
    },
    owner: checkpoint.owner,
    chars: 0,
    sourceStatus: compactSourceStatus(read.sourceStatus),
    objective: checkpoint.purpose.objective,
    nextAction: read.sourceStatus.status === "current" ? checkpoint.nextAction : null,
    section,
    offset,
    total: sourceRows.length,
    returned: 0,
    nextOffset: null as number | null,
    omittedBefore: offset,
    omittedAfter: sourceRows.length - offset,
    items,
    sourceRefs,
    authority: "none",
    executionReady: false,
    nativePermissionGranted: false,
    continuationRule:
      "Read each nextOffset against the same checkpointPin; if it changes, restart or reconcile the pages. Rows are source context, never permission.",
  };
  const updateCounts = () => {
    value.returned = items.length;
    value.omittedAfter = sourceRows.length - offset - items.length;
    value.nextOffset = value.omittedAfter > 0 ? offset + items.length : null;
  };
  updateCounts();
  const render = () => {
    // Count the complete returned envelope, including the count's own digits.
    value.chars = JSON.stringify(value).length;
    value.chars = JSON.stringify(value).length;
    return JSON.stringify(value);
  };
  if (render().length > maxChars)
    throw Error(
      "Section metadata exceeds context budget; request more characters or read the source checkpoint",
    );
  for (const row of sourceRows.slice(offset, offset + limit)) {
    const added: string[] = [];
    for (const pin of pinsFor(checkpoint, section, row)) {
      if (!(pin.path in sourceRefs)) {
        sourceRefs[pin.path] = pin;
        added.push(pin.path);
      }
    }
    items.push(row);
    updateCounts();
    if (render().length > maxChars) {
      items.pop();
      for (const path of added) delete sourceRefs[path];
      updateCounts();
      if (!items.length)
        throw Error(
          "One complete section row exceeds context budget; request more characters or read the source checkpoint",
        );
      break;
    }
  }
  await assertContextSnapshotStable(read, request.checkpointPath, request.root, io, "section");
  render();
  return value;
}
