import { createHash } from "node:crypto";
import { lstat, open, realpath, mkdir } from "node:fs/promises";
import { constants } from "node:fs";
import { isAbsolute, resolve, relative } from "node:path";
import {
  readWorkContinuity,
  projectWorkContinuityData,
  validateEndpointRegistration,
  type ContinuityEndpointRegistration,
} from "./work-continuity";

export interface PinnedOwnerSource {
  path: string;
  sha256: string;
}
export interface WorkSelection {
  profile: string;
  sessionId: string;
  projectId: string;
  workId: string;
  issueIds: string[];
  source: PinnedOwnerSource;
  authorityRef: string;
}
export interface CurrentWorkManifest {
  schema: "hermes.platform-current-work.v1";
  workId: "hermes-platform-rearchitecture";
  version: number;
  ownerRoot: string;
  sourcePins: PinnedOwnerSource[];
  engineering: {
    plan: PinnedOwnerSource;
    interview: PinnedOwnerSource;
    checkpoint: PinnedOwnerSource;
    supplementalDecisions?: PinnedOwnerSource[];
    continuityRegistration?: ContinuityEndpointRegistration;
  };
  profiles: Array<{
    handle: string;
    role: string;
    nativeHome: string;
    nativeProjectIds: string[];
    workspace: string;
    semanticStatus: "active-owner" | "historical-optional-specialist";
  }>;
  selections: WorkSelection[];
  nativeSourceRevision: string;
}
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
export const currentWorkContextMaxChars = 14000;
const digest = (x: unknown) => typeof x === "string" && /^[a-f0-9]{64}$/.test(x);
declare const PLATFORM_NATIVE_MANIFEST_SHA256: string;
export function configuredNativeManifestPin() {
  if (
    typeof PLATFORM_NATIVE_MANIFEST_SHA256 === "string" &&
    digest(PLATFORM_NATIVE_MANIFEST_SHA256)
  )
    return PLATFORM_NATIVE_MANIFEST_SHA256;
  throw Error("Native bundle requires an independently promoted manifest SHA256");
}
/** Linux/WSL native seam: open each component relative to a held no-follow directory FD. */
export async function readExactFile(path: string, maxBytes: number, expectedSha256?: string) {
  if (
    !isAbsolute(path) ||
    resolve(path) !== path ||
    !Number.isSafeInteger(maxBytes) ||
    maxBytes < 1
  )
    throw Error("Invalid exact file path/bound");
  const parts = path.split("/").slice(1);
  let dir = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    for (const part of parts.slice(0, -1)) {
      const next = await open(
        `/proc/self/fd/${dir.fd}/${part}`,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await dir.close();
      dir = next;
    }
    const file = await open(
      `/proc/self/fd/${dir.fd}/${parts.at(-1)}`,
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    );
    try {
      const before = await file.stat();
      if (!before.isFile() || before.nlink !== 1 || before.size > maxBytes)
        throw Error("Bounded regular single-link source required");
      const buffer = Buffer.alloc(before.size + 1);
      let count = 0;
      while (count < buffer.length) {
        const next = await file.read(buffer, count, buffer.length - count, null);
        if (!next.bytesRead) break;
        count += next.bytesRead;
      }
      const bytes = buffer.subarray(0, count),
        after = await file.stat();
      if (
        bytes.length !== before.size ||
        after.dev !== before.dev ||
        after.ino !== before.ino ||
        after.size !== before.size ||
        after.mtimeMs !== before.mtimeMs ||
        after.ctimeMs !== before.ctimeMs ||
        (expectedSha256 && hash(bytes) !== expectedSha256)
      )
        throw Error("Owner source changed");
      return bytes;
    } finally {
      await file.close();
    }
  } finally {
    await dir.close();
  }
}
export async function openExactDirectory(path: string, create = false) {
  if (!isAbsolute(path) || resolve(path) !== path) throw Error("Exact directory required");
  let dir = await open("/", constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    for (const part of path.split("/").slice(1)) {
      const child = `/proc/self/fd/${dir.fd}/${part}`;
      if (create)
        try {
          await mkdir(child, { mode: 0o700 });
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
        }
      const next = await open(
        child,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
      );
      await dir.close();
      dir = next;
    }
    return dir;
  } catch (error) {
    await dir.close();
    throw error;
  }
}
function within(root: string, path: string) {
  const rel = relative(root, path);
  return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
}
export async function readOwnerSource(root: string, pin: PinnedOwnerSource, maxBytes = 1048576) {
  if (
    !isAbsolute(root) ||
    resolve(root) !== root ||
    !within(root, pin.path) ||
    !digest(pin.sha256) ||
    (await realpath(root)) !== root ||
    (await realpath(pin.path)) !== pin.path
  )
    throw Error("Exact admitted owner source required");
  return readExactFile(pin.path, maxBytes, pin.sha256);
}
async function readProfileManifest(
  manifestPath: string,
  profile: string,
  expectedManifestSha256?: string,
) {
  if ((await realpath(manifestPath)) !== manifestPath || (await lstat(manifestPath)).size > 1048576)
    throw Error("Unsafe manifest");
  const manifestBytes = await readExactFile(manifestPath, 1048576, expectedManifestSha256),
    manifest = JSON.parse(manifestBytes.toString()) as CurrentWorkManifest;
  if (
    manifest.schema !== "hermes.platform-current-work.v1" ||
    manifest.workId !== "hermes-platform-rearchitecture" ||
    !Number.isSafeInteger(manifest.version) ||
    manifest.version < 1 ||
    !Array.isArray(manifest.sourcePins) ||
    !Array.isArray(manifest.profiles) ||
    !Array.isArray(manifest.selections) ||
    manifest.sourcePins.length > 200 ||
    manifest.selections.length > 1000
  )
    throw Error("Invalid current Work manifest");
  const actor = manifest.profiles.filter((x) => x.handle === profile);
  if (actor.length !== 1) throw Error("Unknown/ambiguous native profile");
  return { manifestBytes, manifest, actor: actor[0]! };
}
/** Fixed manifest admission selects one domain data owner; it never authenticates a native actor. */
export async function readRegisteredWorkContinuity(
  manifestPath: string,
  profile: string,
  expectedManifestSha256?: string,
) {
  const { manifestBytes, manifest } = await readProfileManifest(
    manifestPath,
    profile,
    expectedManifestSha256,
  );
  const registration = manifest.engineering.continuityRegistration;
  if (!registration) throw Error("Current Work has no admitted continuity owner");
  validateEndpointRegistration(registration);
  if (
    registration.workId !== manifest.workId ||
    registration.ownerProjectId !== "hermes-harness-engineering" ||
    !registration.records
  )
    throw Error("Continuity owner must bind this engineering Work and current v2 record admission");
  const view = await readWorkContinuity(registration);
  await readExactFile(manifestPath, 1048576, hash(manifestBytes));
  return { manifestPin: { path: manifestPath, sha256: hash(manifestBytes) }, registration, view };
}
export async function readCurrentWork(
  manifestPath: string,
  profile: string,
  sessionId?: string,
  maxChars = currentWorkContextMaxChars,
  expectedManifestSha256?: string,
) {
  if (!Number.isSafeInteger(maxChars) || maxChars < 1024 || maxChars > 24000)
    throw Error("Invalid context bound");
  const { manifestBytes, manifest, actor } = await readProfileManifest(
    manifestPath,
    profile,
    expectedManifestSha256,
  );
  const sources = new Map<string, Uint8Array>();
  for (const pin of manifest.sourcePins) {
    if (sources.has(pin.path)) throw Error("Duplicate source identity");
    sources.set(pin.path, await readOwnerSource(manifest.ownerRoot, pin));
  }
  const value = (pin: PinnedOwnerSource) => {
    const bytes = sources.get(pin.path);
    if (!bytes || hash(bytes) !== pin.sha256) throw Error("Engineering pointer not admitted");
    return JSON.parse(bytes.toString());
  };
  let plan = value(manifest.engineering.plan),
    interview = value(manifest.engineering.interview),
    checkpoint = value(manifest.engineering.checkpoint);
  let planRef: object = manifest.engineering.plan,
    decisionRef: object = manifest.engineering.interview,
    checkpointRef: object = manifest.engineering.checkpoint;
  let continuity: ReturnType<typeof projectWorkContinuityData>["canonical"] | undefined;
  let checkpointVersion: number = checkpoint.version;
  const owner = manifest.engineering.continuityRegistration
    ? await readRegisteredWorkContinuity(manifestPath, profile, hash(manifestBytes))
    : undefined;
  if (owner) {
    const data = projectWorkContinuityData(owner.view);
    checkpoint = data.checkpoint.document;
    checkpointVersion = checkpoint.version;
    continuity = data.canonical;
    const original = (role: "plan" | "decisions") => {
      const pin = owner.registration.immutableSources.find((s) => s.role === role)!;
      const file = owner.view.canonical.bundle.files.find((f) => f.path === pin.path)!;
      return {
        document: JSON.parse(Buffer.from(file.base64, "base64").toString()),
        reference: {
          path: data.canonical.path,
          sha256: data.canonical.sha256,
          recordPath: pin.path,
          recordSha256: pin.sha256,
          originPath: pin.originPath,
        },
      };
    };
    const admittedPlan = original("plan"),
      admittedDecisions = original("decisions");
    plan = admittedPlan.document;
    interview = admittedDecisions.document;
    planRef = admittedPlan.reference;
    decisionRef = admittedDecisions.reference;
    checkpointRef = {
      path: data.canonical.path,
      sha256: data.canonical.sha256,
      recordPath: data.checkpoint.path,
      recordSha256: data.checkpoint.sha256,
      readMode: "continuity-read",
    };
  }
  if (
    plan.workId !== manifest.workId ||
    checkpoint.workId !== manifest.workId ||
    !Array.isArray(interview.decisions)
  )
    throw Error("Work/source identity mismatch");
  let supplementalDecisions: Array<{
    source: object;
    decisionIds: string[];
    choice: string;
    authorityScopePointer: string;
    evidenceRefsPointer: string;
  }> = (manifest.engineering.supplementalDecisions ?? []).map((pin) => {
    const decision = value(pin);
    if (
      decision.kind !== "decision" ||
      decision.payload?.status !== "accepted" ||
      decision.projectId !== "hermes-harness-engineering" ||
      decision.payload?.details?.workId !== manifest.workId
    )
      throw Error("Supplemental decision/source identity mismatch");
    return {
      source: pin,
      decisionIds: decision.payload.details.decisionIds ?? [decision.payload.details.decisionId],
      choice: decision.payload.choice,
      authorityScopePointer: "/payload/authority/scope",
      evidenceRefsPointer: "/payload/evidenceRefs",
    };
  });
  if (owner) {
    const data = projectWorkContinuityData(owner.view);
    // These record references share engineeringWork.continuity's one canonical
    // snapshot. Keep its digest once so current decisions fit the native budget.
    supplementalDecisions = data.decisionGroups.map((group) => {
      const file = owner.view.canonical.bundle.files.find((f) => f.path === group.decisionPath)!;
      const decision = JSON.parse(Buffer.from(file.base64, "base64").toString());
      return {
        source: { recordPath: file.path, recordSha256: file.sha256 },
        decisionIds: group.decisionIds,
        choice: decision.payload.choice,
        authorityScopePointer: "/payload/authority/scope",
        evidenceRefsPointer: "/payload/evidenceRefs",
      };
    });
  }
  const selections = manifest.selections.filter(
    (x) => x.profile === profile && x.sessionId === sessionId,
  );
  if (selections.length > 1) throw Error("Ambiguous session Work selection");
  const selected = selections[0];
  let selectedWork: unknown = {
    status: "unselected",
    reason: "A profile or chat alone does not select a product Work.",
  };
  if (selected) {
    if (
      !sessionId ||
      !selected.projectId ||
      !selected.workId ||
      !selected.authorityRef ||
      !Array.isArray(selected.issueIds)
    )
      throw Error("Incomplete explicit selection");
    const source = sources.get(selected.source.path);
    if (!source || hash(source) !== selected.source.sha256)
      throw Error("Selected Work source not admitted");
    selectedWork = {
      status: "explicitly-selected",
      binding: selected,
      value: JSON.parse(source.toString()),
    };
  }
  const architectureKeys = ["codeRepositories", "layers", "leads", "nativeExecution", "models"];
  const architecture = owner
    ? Object.fromEntries(architectureKeys.map((key) => [key, plan.architecture[key]]))
    : plan.architecture;
  const architectureProjection = owner
    ? {
        omitted: Object.keys(plan.architecture).filter((key) => !architectureKeys.includes(key)),
        fullRecord: planRef,
        jsonPointer: "/architecture",
      }
    : undefined;
  const result = {
    schema: "hermes.platform-current-work.projection.v1",
    manifestPin: { path: manifestPath, sha256: hash(manifestBytes) },
    version: manifest.version,
    actor,
    engineeringWork: {
      workId: manifest.workId,
      status: checkpoint.status,
      checkpointVersion,
      planRef,
      decisionRef,
      checkpointRef,
      ...(continuity ? { continuity } : {}),
      supplementalDecisions,
      architecture,
      ...(architectureProjection ? { architectureProjection } : {}),
      scope: plan.scope,
      budget: plan.budget,
      completed: checkpoint.completed,
      completedProjection: {
        total: Array.isArray(checkpoint.completed) ? checkpoint.completed.length : 0,
        omitted: 0,
        fullRecord: checkpointRef,
      },
      pending: checkpoint.pending,
      uncertain: checkpoint.uncertain,
      nextAction: checkpoint.nextAction,
    },
    selectedProductWork: selectedWork,
    nativeSourceRevision: manifest.nativeSourceRevision,
    authority: "none",
    nativePermissionGranted: false,
    executionReady: false,
  };
  // Long-lived Work keeps every completed effect in its pinned checkpoint. A bounded
  // startup projection can select recent entries without truncating current decisions.
  if (
    JSON.stringify(result).length > maxChars &&
    Array.isArray(checkpoint.completed) &&
    checkpoint.completed.length > 4
  ) {
    result.engineeringWork.completed = checkpoint.completed.slice(-4);
    result.engineeringWork.completedProjection.omitted = checkpoint.completed.length - 4;
  }
  const text = JSON.stringify(result);
  if (text.length > maxChars)
    throw Error(
      `Required current Work projection exceeds explicit context budget; ${owner ? "use continuity-read for the complete admitted checkpoint at " + continuity!.path : "read the full pinned checkpoint at " + manifest.engineering.checkpoint.path}. Current choices, pending work and uncertainty were not truncated.`,
    );
  await readExactFile(manifestPath, 1048576, hash(manifestBytes));
  for (const pin of manifest.sourcePins) await readOwnerSource(manifest.ownerRoot, pin);
  if (owner) await readExactFile(continuity!.path, 8 * 1024 * 1024, continuity!.sha256);
  return result;
}
