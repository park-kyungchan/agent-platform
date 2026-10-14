/** Source-domain Work data only. Native tools/lifecycle/authentication remain authoritative.
 * Common CAS and source-byte transfer never authenticate actors or gate native runtime. */
import { createHash } from "node:crypto";
import { lstat } from "node:fs/promises";
import { resolve, join } from "node:path";
import { readExactFile } from "./work-reader";
import { validateWorkIdentityDocuments } from "./work-identity";
import {
  validateWorkRecordAdmission,
  readAdmittedWorkRecords,
  type WorkRecordAdmission,
} from "./work-records";
import {
  packHandoff,
  receiveHandoff,
  readCanonicalHandoff,
  mutateCanonicalHandoff,
  validateHandoff,
  type HandoffBundle,
  type CanonicalHandoff,
  type WriterToken,
} from "@palantirkc/work-continuity/handoff";
import {
  planBoundaryCustody,
  verifyBoundaryCustodyReadback,
  exposeCustodyGaps,
  validateNativeCheckpoint,
  type CustodyBoundary,
  type NativeCheckpointSnapshot,
} from "../platform-governance/boundary-custody.ts";
const digest = (b: Uint8Array | string) => createHash("sha256").update(b).digest("hex");
const stable = (v: unknown): string =>
  JSON.stringify(v, (_key, x) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(
          Object.keys(x)
            .sort()
            .map((k) => [k, x[k]]),
        )
      : x,
  );
function check(ok: unknown, message: string): asserts ok {
  if (!ok) throw Error("Work continuity: " + message);
}
function object(v: unknown): asserts v is Record<string, any> {
  check(v && typeof v === "object" && !Array.isArray(v), "object required");
}
function keys(v: object, allowed: string[]) {
  check(
    Object.keys(v).every((k) => allowed.includes(k)),
    "unknown record field",
  );
}
function text(v: unknown): asserts v is string {
  check(
    typeof v === "string" &&
      v.trim() === v &&
      v.length > 0 &&
      v.length <= 2048 &&
      !/[\u0000-\u001f]/.test(v),
    "bounded literal required",
  );
}
function stringList(v: unknown): asserts v is string[] {
  check(
    Array.isArray(v) && v.length <= 256 && new Set(v).size === v.length,
    "bounded unique references required",
  );
  v.forEach(text);
}
export type WorkActorReference =
  | {
      runtime: "native-hermes";
      actorId: string;
      profileId: string;
      sessionId: string;
      nativeProofRef: string;
    }
  | {
      runtime: "native-codex";
      actorId: string;
      profileId: string | null;
      sessionId: string;
      nativeProofRef: string;
    };
export interface ImmutableWorkSource {
  role: "issue" | "spec" | "plan-binding" | "plan" | "decisions" | "authorization";
  path: string;
  sha256: string;
  originPath: string;
}
export interface ContinuityEndpointRegistration {
  schema: "hermes.work-continuity-endpoint.v1";
  endpointId: string;
  ownerProjectId: string;
  workId: string;
  checkpointId: string;
  nativeRoot: string;
  immutableSources: ImmutableWorkSource[];
  maxStateBytes: number;
  records?: WorkRecordAdmission;
}
export interface ExplicitWorkSelection {
  schema: "hermes.explicit-work-selection.v1";
  projectId: string;
  workId: string;
  issueId: string;
  specId: string;
  specRevision: number;
  planId: string;
  planRevision: number;
  planBindingPath: string;
  planPath: string;
  immutablePurposeDigest: string;
  actor: WorkActorReference | null;
}
export interface VerifiedShare {
  boundaryId: string;
  bundleSha256: string;
  receiverId: string;
  receiverProofRef: string;
}
export interface MutableWorkState {
  schema: "hermes.mutable-owner-work-state.v1";
  selection: ExplicitWorkSelection | null;
  boundaries: CustodyBoundary[];
  missedHookBoundaryIds: string[];
  verifiedShares: VerifiedShare[];
}
export interface WorkContinuityView {
  registration: ContinuityEndpointRegistration;
  canonical: CanonicalHandoff;
  state: MutableWorkState;
  snapshot: NativeCheckpointSnapshot;
  token: WriterToken;
  digest: string;
  writerLockPresent: boolean;
}
export interface BoundaryUpdate {
  expected: WriterToken;
  expectedDigest: string;
  boundary: CustodyBoundary;
}
export const mutableCheckpointFields = [
  "version",
  "status",
  "completed",
  "pending",
  "uncertain",
  "nextAction",
  "updatedAtUtc",
] as const;
export type WorkCheckpointCorePatch = {
  version: number;
  updatedAtUtc: string;
  status?: "in-progress" | "paused" | "complete";
  completed?: string[];
  pending?: string[];
  uncertain?: Array<{ effect: string; status: string; reason: string; sourceRefs?: string[] }>;
  nextAction?: string;
};
export type WorkBoundaryRequest = BoundaryUpdate & {
  document?: unknown;
  core?: WorkCheckpointCorePatch;
};
export type BoundaryUpdateResult =
  | { status: "recorded" | "already-recorded"; view: WorkContinuityView }
  | {
      status: "effect-unknown-reconcile-before-retry";
      writeId: string;
      expected: WriterToken;
      errorClass: string;
    };
function actorReference(v: WorkActorReference) {
  object(v);
  keys(v, ["runtime", "actorId", "profileId", "sessionId", "nativeProofRef"]);
  check(["native-hermes", "native-codex"].includes(v.runtime), "actual native runtime required");
  for (const s of [v.actorId, v.sessionId, v.nativeProofRef]) text(s);
  if (v.runtime === "native-hermes" || v.profileId !== null) text(v.profileId);
}
export function validateEndpointRegistration(r: ContinuityEndpointRegistration) {
  object(r);
  keys(r, [
    "schema",
    "endpointId",
    "ownerProjectId",
    "workId",
    "checkpointId",
    "nativeRoot",
    "immutableSources",
    "maxStateBytes",
    "records",
  ]);
  check(r.schema === "hermes.work-continuity-endpoint.v1", "registration schema");
  for (const s of [r.endpointId, r.ownerProjectId, r.workId, r.checkpointId]) text(s);
  check(resolve(r.nativeRoot) === r.nativeRoot, "exact native endpoint root required");
  check(
    Number.isSafeInteger(r.maxStateBytes) && r.maxStateBytes > 0 && r.maxStateBytes <= 1048576,
    "state bound",
  );
  check(
    Array.isArray(r.immutableSources) && r.immutableSources.length === 6,
    "six immutable purpose sources required",
  );
  const roles = new Set(),
    paths = new Set();
  for (const s of r.immutableSources) {
    object(s);
    keys(s, ["role", "path", "sha256", "originPath"]);
    check(
      ["issue", "spec", "plan-binding", "plan", "decisions", "authorization"].includes(s.role) &&
        !roles.has(s.role) &&
        !paths.has(s.path),
      "unique source roles/paths",
    );
    check(
      typeof s.path === "string" &&
        /^[A-Za-z0-9_.-]+\.json$/.test(s.path) &&
        s.path !== "work-state.json" &&
        /^[a-f0-9]{64}$/.test(s.sha256),
      "exact admitted immutable JSON pin required",
    );
    check(
      typeof s.originPath === "string" && resolve(s.originPath) === s.originPath,
      "exact original source locator required",
    );
    roles.add(s.role);
    paths.add(s.path);
  }
  if (r.records) {
    validateWorkRecordAdmission(r.records);
    check(
      r.records.immutablePurposeSources.every((p) =>
        r.immutableSources.some((s) => stable(s) === stable(p)),
      ),
      "manifest original purpose pins differ from selected Work",
    );
    check(
      r.records.ownerProjectId === r.ownerProjectId && r.records.workId === r.workId,
      "same Work record scope",
    );
    for (const slot of [...r.records.supplements, ...r.records.currentRecords]) {
      check(!paths.has(slot.path), "record path collides with purpose source");
      paths.add(slot.path);
    }
  }
}
function sourceProof(r: ContinuityEndpointRegistration, bundle: HandoffBundle) {
  validateEndpointRegistration(r);
  validateHandoff(bundle);
  if (r.records)
    readAdmittedWorkRecords(bundle, r.records, [
      ...r.immutableSources.map((s) => s.path),
      "work-state.json",
    ]);
  else
    check(bundle.files.length === 7, "only six purpose documents and one Work state are admitted");
  const docs: Record<string, any> = {};
  for (const s of r.immutableSources) {
    const found = bundle.files.filter((f) => f.path === s.path);
    check(found.length === 1 && found[0]!.sha256 === s.sha256, "immutable purpose source drift");
    docs[s.role] = JSON.parse(Buffer.from(found[0]!.base64, "base64").toString("utf8"));
    object(docs[s.role]);
  }
  const proof = validateWorkIdentityDocuments(
    {
      issue: docs.issue,
      spec: docs.spec,
      planBinding: docs["plan-binding"],
      plan: docs.plan,
      decisions: docs.decisions,
      authorization: docs.authorization,
    },
    {
      projectId: r.ownerProjectId,
      workId: r.workId,
      pins: r.immutableSources
        .filter((s) => ["plan", "decisions", "authorization"].includes(s.role))
        .map((s) => ({
          role: s.role as "plan" | "decisions" | "authorization",
          originPath: s.originPath,
          sha256: s.sha256,
        })),
    },
  );
  check(
    bundle.issueId === proof.issueId,
    "canonical Issue cannot be inferred from Work or native Task",
  );
  return {
    issueId: proof.issueId,
    specId: proof.specId,
    specRevision: proof.specRevision,
    planId: proof.planId,
    planRevision: proof.planRevision,
    planBindingPath: r.immutableSources.find((s) => s.role === "plan-binding")!.path,
    planPath: r.immutableSources.find((s) => s.role === "plan")!.path,
    immutablePurposeDigest: digest(
      stable(
        r.records
          ? { original: r.immutableSources, supplements: r.records.supplements }
          : r.immutableSources,
      ),
    ),
  };
}

function stateFrom(r: ContinuityEndpointRegistration, bundle: HandoffBundle): MutableWorkState {
  const proof = sourceProof(r, bundle),
    files = bundle.files.filter((f) => f.path === "work-state.json");
  check(
    files.length === 1 && files[0]!.byteLength <= r.maxStateBytes,
    "bounded Work state required",
  );
  const state = JSON.parse(Buffer.from(files[0]!.base64, "base64").toString("utf8"));
  object(state);
  keys(state, ["schema", "selection", "boundaries", "missedHookBoundaryIds", "verifiedShares"]);
  check(
    state.schema === "hermes.mutable-owner-work-state.v1" &&
      Array.isArray(state.boundaries) &&
      state.boundaries.length <= 256 &&
      Array.isArray(state.verifiedShares) &&
      state.verifiedShares.length <= 256,
    "bounded state metadata required",
  );
  stringList(state.missedHookBoundaryIds);
  validateNativeCheckpoint({
    ownerId: r.ownerProjectId,
    workId: r.workId,
    checkpointId: r.checkpointId,
    revision: 0,
    digest: "source-validation-only",
    boundaries: state.boundaries,
    missedHookBoundaryIds: state.missedHookBoundaryIds,
  });
  if (state.selection !== null) {
    const s = state.selection;
    object(s);
    keys(s, [
      "schema",
      "projectId",
      "workId",
      "issueId",
      "specId",
      "specRevision",
      "planId",
      "planRevision",
      "planBindingPath",
      "planPath",
      "immutablePurposeDigest",
      "actor",
    ]);
    check(
      s.schema === "hermes.explicit-work-selection.v1" &&
        s.projectId === r.ownerProjectId &&
        s.workId === r.workId &&
        Object.entries(proof).every(([key, value]) => s[key] === value),
      "selection must bind exact pinned Issue/Spec/Plan",
    );
    if (s.actor !== null) actorReference(s.actor);
  }
  for (const share of state.verifiedShares) {
    object(share);
    keys(share, ["boundaryId", "bundleSha256", "receiverId", "receiverProofRef"]);
    for (const s of [share.boundaryId, share.receiverId, share.receiverProofRef]) text(s);
    check(
      /^[a-f0-9]{64}$/.test(share.bundleSha256) &&
        state.boundaries.some((b: any) => b.boundaryId === share.boundaryId),
      "share must reference recorded boundary",
    );
  }
  return state as unknown as MutableWorkState;
}
export function validateContinuityCapsule(
  registration: ContinuityEndpointRegistration,
  bundle: HandoffBundle,
) {
  return stateFrom(registration, bundle);
}
function bundleWithState(view: WorkContinuityView, state: MutableWorkState): HandoffBundle {
  const bytes = Buffer.from(JSON.stringify(state, null, 2) + "\n");
  check(bytes.length <= view.registration.maxStateBytes, "state exceeds bound");
  const files = view.canonical.bundle.files.filter((f) => f.path !== "work-state.json");
  files.push({
    path: "work-state.json",
    sha256: digest(bytes),
    byteLength: bytes.length,
    base64: bytes.toString("base64"),
  });
  return {
    ...view.canonical.bundle,
    files,
    sourceRevision: digest(bytes),
    completedEffectIds: [
      ...new Set([
        ...view.canonical.bundle.completedEffectIds,
        ...state.boundaries.flatMap((b) => b.completedEffectRefs),
      ]),
    ],
    nextAction: {
      id: "owner-reconcile-work-continuity",
      action:
        "Read current Work and reconcile checkpoint/share debt; no native execution authority inferred.",
      path: "work-state.json",
    },
  };
}
function tokenSame(a: WriterToken, b: WriterToken) {
  return a.writerId === b.writerId && a.epoch === b.epoch && a.revision === b.revision;
}
async function inspect(r: ContinuityEndpointRegistration): Promise<WorkContinuityView> {
  validateEndpointRegistration(r);
  const path = join(r.nativeRoot, "current.json"),
    raw = await readExactFile(path, 8 * 1024 * 1024),
    canonical = await readCanonicalHandoff(r.nativeRoot);
  await readExactFile(path, 8 * 1024 * 1024, digest(raw));
  check(
    Buffer.from(raw).toString() === JSON.stringify(canonical, null, 2) + "\n",
    "canonical endpoint bytes differ from inspected owner representation",
  );
  const state = stateFrom(r, canonical.bundle),
    snapshot: NativeCheckpointSnapshot = {
      ownerId: r.ownerProjectId,
      workId: r.workId,
      checkpointId: r.checkpointId,
      revision: canonical.revision,
      digest: digest(raw),
      boundaries: state.boundaries,
      missedHookBoundaryIds: state.missedHookBoundaryIds,
    };
  exposeCustodyGaps(snapshot, [], []);
  let writerLockPresent = false;
  try {
    const lock = await lstat(join(r.nativeRoot, ".writer-lock"));
    check(lock.isDirectory() && !lock.isSymbolicLink(), "unsafe writer lock");
    writerLockPresent = true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  return {
    registration: structuredClone(r),
    canonical,
    state,
    snapshot,
    token: { writerId: canonical.writerId, epoch: canonical.epoch, revision: canonical.revision },
    digest: snapshot.digest,
    writerLockPresent,
  };
}
/** Initial bytes only. No initialization/self-election action is exposed by this seam. */
export async function prepareInitialContinuityBundle(
  sourceRoot: string,
  registration: ContinuityEndpointRegistration,
  expectedIssueId: string,
) {
  validateEndpointRegistration(registration);
  const paths = [
    ...registration.immutableSources.map((s) => s.path),
    "work-state.json",
    ...(registration.records
      ? [...registration.records.supplements, ...registration.records.currentRecords].map(
          (s) => s.path,
        )
      : []),
  ];
  for (const s of registration.immutableSources)
    await readExactFile(join(sourceRoot, s.path), 1048576, s.sha256);
  if (registration.records)
    for (const slot of registration.records.supplements)
      await readExactFile(join(sourceRoot, slot.path), 262144, slot.sha256);
  const stateBytes = await readExactFile(
    join(sourceRoot, "work-state.json"),
    registration.maxStateBytes,
  );
  const bundle = await packHandoff(sourceRoot, paths, {
    issueId: expectedIssueId,
    sourceRevision: digest(stateBytes),
    completedEffectIds: [],
    checks: [],
    nextAction: {
      id: "owner-reconcile-work-continuity",
      action:
        "Read selected Work source and preserve completed/pending records; native runtime supplies its own identities and permissions.",
      path: "work-state.json",
    },
  });
  stateFrom(registration, bundle);
  for (const s of registration.immutableSources)
    await readExactFile(join(sourceRoot, s.path), 1048576, s.sha256);
  return bundle;
}
/** Receiver-owned path selection is a separate existing owner responsibility. Copies are never writers. */
export async function receiveWorkContinuityCopy(
  parent: string,
  destination: string,
  bundle: HandoffBundle,
  registration: ContinuityEndpointRegistration,
) {
  stateFrom(registration, bundle);
  try {
    const receipt = await receiveHandoff(parent, destination, bundle);
    for (const f of bundle.files)
      await readExactFile(join(receipt.root, f.path), f.byteLength + 1, f.sha256);
    return {
      status: "received-bytes-verified" as const,
      receipt,
      writerAuthority: false,
      semanticAcceptance: false,
    };
  } catch (error) {
    return {
      status: "receive-effect-unknown-reconcile-before-retry" as const,
      destination: join(parent, destination),
      bundleSha256: validateHandoff(bundle).bundleSha256,
      errorClass: error instanceof Error ? error.name : "unknown",
      deleteOrRetryAllowed: false,
    };
  }
}
/** Observer integration candidate: mechanical debt only, never derives semantic boundaries from payload/history. */
/** Read selected source data; does not select a native session or authorize a tool. */
export const readWorkContinuity = inspect;
/** Complete admitted checkpoint data for an explicit owner read, without source bundle/body dumps. */
export function projectWorkContinuityData(view: WorkContinuityView) {
  check(view.registration.records, "current record admission not configured");
  const records = readAdmittedWorkRecords(view.canonical.bundle, view.registration.records, [
    ...view.registration.immutableSources.map((s) => s.path),
    "work-state.json",
  ]);
  return {
    schema: "hermes.work-continuity-data.projection.v1" as const,
    ownerProjectId: view.registration.ownerProjectId,
    workId: view.registration.workId,
    canonical: {
      path: join(view.registration.nativeRoot, "current.json"),
      sha256: view.digest,
      token: view.token,
    },
    checkpoint: records.checkpoint,
    decisionGroups: records.decisionGroups,
    authority: "none" as const,
    nativePermissionGranted: false as const,
    executionReady: false as const,
    semanticAcceptance: "not-inferred" as const,
  };
}
/** Provenance supplied by existing native tools is data, never permission. Unknown stays null. */
export function createWorkSelection(
  view: WorkContinuityView,
  reference: WorkActorReference | null = null,
): ExplicitWorkSelection {
  if (reference !== null) actorReference(reference);
  return {
    schema: "hermes.explicit-work-selection.v1",
    projectId: view.registration.ownerProjectId,
    workId: view.registration.workId,
    ...sourceProof(view.registration, view.canonical.bundle),
    actor: reference,
  };
}
/** Source checkpoint CAS only. No native RPC/session/model/HTTPX/tool/authentication interception. */
export async function recordWorkBoundary(
  registration: ContinuityEndpointRegistration,
  request: WorkBoundaryRequest,
): Promise<BoundaryUpdateResult> {
  object(request);
  keys(request, ["expected", "expectedDigest", "boundary", "document", "core"]);
  object(request.expected);
  keys(request.expected, ["writerId", "epoch", "revision"]);
  check(/^[a-f0-9]{64}$/.test(request.expectedDigest), "exact predecessor digest required");
  check(
    request.document === undefined || request.core === undefined,
    "supply core patch or complete document, not both",
  );
  const view = await inspect(registration);
  check(
    tokenSame(view.token, request.expected) && view.digest === request.expectedDigest,
    "stale writer token or endpoint digest",
  );
  const planned = planBoundaryCustody(view.snapshot, request.boundary),
    next = structuredClone(view.state);
  let bundle = view.canonical.bundle;
  if (request.document !== undefined || request.core !== undefined) {
    check(registration.records, "current record admission not configured");
    const records = registration.records,
      base = [...registration.immutableSources.map((s) => s.path), "work-state.json"];
    const before = readAdmittedWorkRecords(bundle, records, base);
    if (request.core !== undefined) {
      object(request.core);
      keys(request.core, [...mutableCheckpointFields]);
      check(
        Object.hasOwn(request.core, "version") && Object.hasOwn(request.core, "updatedAtUtc"),
        "explicit successor version and update time required",
      );
    }
    const document =
      request.core === undefined
        ? request.document
        : { ...before.checkpoint.document, ...request.core };
    object(document);
    const semanticFields = new Set<string>(mutableCheckpointFields);
    const bytes = Buffer.from(JSON.stringify(document, null, 2) + "\n"),
      slot = records.currentRecords.find((s) => s.role === "current-continuity")!;
    bundle = {
      ...bundle,
      files: bundle.files.map((f) =>
        f.path === slot.path
          ? {
              path: f.path,
              sha256: digest(bytes),
              byteLength: bytes.length,
              base64: bytes.toString("base64"),
            }
          : f,
      ),
    };
    const after = readAdmittedWorkRecords(bundle, records, base);
    for (const key of new Set([
      ...Object.keys(before.checkpoint.document),
      ...Object.keys(document),
    ]))
      if (!semanticFields.has(key))
        check(
          Object.hasOwn(before.checkpoint.document, key) &&
            Object.hasOwn(document, key) &&
            stable(document[key]) === stable(before.checkpoint.document[key]),
          "checkpoint cannot rewrite source-reference metadata",
        );
    if (planned.status === "already-recorded")
      check(
        after.checkpoint.sha256 === before.checkpoint.sha256,
        "recorded boundary cannot be reused for different checkpoint bytes",
      );
    else {
      check(
        after.checkpoint.document.version === before.checkpoint.document.version + 1,
        "checkpoint must increment current version once",
      );
      for (const effect of before.checkpoint.document.completed)
        check(
          after.checkpoint.document.completed.includes(effect),
          "completed checkpoint history lost",
        );
    }
  }
  if (planned.status === "already-recorded") return { status: "already-recorded", view };
  next.boundaries.push(structuredClone(request.boundary));
  bundle = bundleWithState({ ...view, canonical: { ...view.canonical, bundle } }, next);
  try {
    await mutateCanonicalHandoff(
      registration.nativeRoot,
      view.token,
      planned.request.idempotencyKey,
      { kind: "commit", bundle },
    );
    const after = await inspect(registration);
    verifyBoundaryCustodyReadback(view.snapshot, planned.request, after.snapshot);
    return { status: "recorded", view: after };
  } catch (error) {
    return {
      status: "effect-unknown-reconcile-before-retry",
      writeId: planned.request.idempotencyKey,
      expected: view.token,
      errorClass: error instanceof Error ? error.name : "unknown",
    };
  }
}
export function exposeWorkContinuityDebt(
  view: WorkContinuityView,
  observedBoundaryIds: string[] = [],
  failedOpenIds: string[] = [],
) {
  stringList(observedBoundaryIds);
  stringList(failedOpenIds);
  return {
    schema: "hermes.work-continuity-debt.v1",
    workId: view.registration.workId,
    custody: exposeCustodyGaps(view.snapshot, observedBoundaryIds, failedOpenIds),
    pendingShareBoundaryIds: view.state.boundaries
      .filter((b) => !view.state.verifiedShares.some((s) => s.boundaryId === b.boundaryId))
      .map((b) => b.boundaryId),
    writerLockPresent: view.writerLockPresent,
    authority: "none",
    semanticAcceptance: "not-inferred",
    meaning:
      "Source-data reconciliation only; native tools/lifecycle and user-selected profile stay authoritative.",
  };
}
