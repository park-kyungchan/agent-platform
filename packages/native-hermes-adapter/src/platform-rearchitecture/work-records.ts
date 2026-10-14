/** Finite source-data selection and joins. No native execution, actor/authentication or runtime gate. */
import { createHash } from "node:crypto";
import { join, resolve, relative } from "node:path";
import { readExactFile } from "./work-reader";
import {
  packHandoff,
  validateHandoff,
  type HandoffBundle,
} from "@palantirkc/work-continuity/handoff";
export interface WorkSourceAssertion {
  pointer: string;
  equals: unknown;
}
interface SourceSlot {
  path: string;
  originPath: string;
  maxBytes: number;
  schemaPointer: string;
  schema: string;
  projectPointer?: string;
  workPointer?: string;
  assertions?: WorkSourceAssertion[];
}
export interface PinnedWorkSupplement extends SourceSlot {
  role: "decision" | "direction" | "evidence";
  sha256: string;
}
export interface CurrentWorkRecordSlot extends SourceSlot {
  role: "current-continuity" | "current-reference";
  sha256?: string;
}
export interface PurposeSource {
  role: "issue" | "spec" | "plan-binding" | "plan" | "decisions" | "authorization";
  path: string;
  originPath: string;
  sha256: string;
}
export interface WorkDecisionGroup {
  id: string;
  decisionIds: string[];
  decisionPath: string;
  directionPath: string;
  evidencePath: string;
  decisionIdPointers: string[];
  directionIdPointers: string[];
  directionDecisionRefPointer: string;
  directionEvidenceRefPointer?: string;
}
export interface WorkSourceJoin {
  kind: "equal" | "source-path" | "sha256";
  left: { path: string; pointer: string };
  right: { path: string; pointer?: string };
}
export interface WorkRecordAdmission {
  schema: "hermes.work-record-admission.v2";
  ownerProjectId: string;
  workId: string;
  ownerRoot: string;
  immutablePurposeSources: PurposeSource[];
  supplements: PinnedWorkSupplement[];
  currentRecords: CurrentWorkRecordSlot[];
  decisionGroups: WorkDecisionGroup[];
  joins: WorkSourceJoin[];
  manifestPin?: { path: string; sha256: string };
}
const sha = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const stable = (value: unknown): string =>
  JSON.stringify(value, (_k, v) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, v[k]]),
        )
      : v,
  );
function check(ok: unknown, message: string): asserts ok {
  if (!ok) throw Error("Work records: " + message);
}
function record(v: unknown): asserts v is Record<string, any> {
  check(v && typeof v === "object" && !Array.isArray(v), "object required");
}
function literal(v: unknown): asserts v is string {
  check(
    typeof v === "string" &&
      v.length > 0 &&
      v.length <= 8192 &&
      !/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(v),
    "bounded literal required",
  );
}
function exact(v: object, keys: string[]) {
  check(
    Object.keys(v).every((k) => keys.includes(k)),
    "unknown manifest field",
  );
}
function references(v: unknown): asserts v is string[] {
  check(
    Array.isArray(v) && v.length <= 256 && new Set(v).size === v.length,
    "bounded unique references required",
  );
  v.forEach(literal);
}
/** JSON pointers select existing data only; never paths to open or executable expressions. */
function pointer(doc: any, p: string): any {
  check(
    typeof p === "string" && p.length <= 512 && /^\/(?:[^~]|~[01])*$/.test(p),
    "literal JSON pointer required",
  );
  for (const raw of p.slice(1).split("/")) {
    const k = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    check(
      !["__proto__", "constructor", "prototype"].includes(k) &&
        doc !== null &&
        typeof doc === "object" &&
        Object.hasOwn(doc, k),
      "source join pointer missing",
    );
    doc = doc[k];
  }
  return doc;
}
function safeOrigin(root: string, path: string) {
  check(
    resolve(path) === path && path.startsWith(root + "/"),
    "selected source must stay in owner",
  );
  const r = relative(root, path);
  check(
    !r.includes("\\") &&
      r.endsWith(".json") &&
      !r.split("/").some((x) => !x || x === "." || x === "..") &&
      ["decisions", "evidence", "verification", "state", "work"].includes(r.split("/")[0]!),
    "explicit source-owned JSON locator required",
  );
}
export function validateWorkRecordAdmission(a: WorkRecordAdmission) {
  record(a);
  exact(a, [
    "schema",
    "ownerProjectId",
    "workId",
    "ownerRoot",
    "immutablePurposeSources",
    "supplements",
    "currentRecords",
    "decisionGroups",
    "joins",
    "manifestPin",
  ]);
  check(a.schema === "hermes.work-record-admission.v2", "admission schema");
  literal(a.ownerProjectId);
  literal(a.workId);
  check(resolve(a.ownerRoot) === a.ownerRoot, "absolute owner root");
  check(
    Array.isArray(a.immutablePurposeSources) && a.immutablePurposeSources.length === 6,
    "original six immutable purpose pins required",
  );
  const purposeRoles = new Set<string>(),
    paths = new Set<string>();
  for (const p of a.immutablePurposeSources) {
    record(p);
    exact(p, ["role", "path", "originPath", "sha256"]);
    check(
      ["issue", "spec", "plan-binding", "plan", "decisions", "authorization"].includes(p.role) &&
        !purposeRoles.has(p.role),
      "unique original purpose roles",
    );
    purposeRoles.add(p.role);
  }
  check(
    Array.isArray(a.supplements) &&
      a.supplements.length <= 48 &&
      Array.isArray(a.currentRecords) &&
      a.currentRecords.length <= 32 &&
      a.currentRecords.filter((p) => p.role === "current-continuity").length === 1,
    "finite selected source slots and one checkpoint required",
  );
  for (const s of [...a.immutablePurposeSources, ...a.supplements, ...a.currentRecords]) {
    check(
      /^[A-Za-z0-9_.-]+\.json$/.test(s.path) && s.path !== "work-state.json" && !paths.has(s.path),
      "unique flat source filename required",
    );
    paths.add(s.path);
    safeOrigin(a.ownerRoot, s.originPath);
    if ("sha256" in s && s.sha256 !== undefined)
      check(/^[a-f0-9]{64}$/.test(s.sha256), "exact source digest required");
  }
  for (const s of [...a.supplements, ...a.currentRecords]) {
    record(s);
    exact(s, [
      "role",
      "path",
      "originPath",
      "maxBytes",
      "schemaPointer",
      "schema",
      "projectPointer",
      "workPointer",
      "assertions",
      "sha256",
    ]);
    check(
      ["decision", "direction", "evidence", "current-continuity", "current-reference"].includes(
        s.role,
      ) &&
        Number.isSafeInteger(s.maxBytes) &&
        s.maxBytes > 0 &&
        s.maxBytes <= 262144,
      "bounded source category",
    );
    literal(s.schema);
    check(typeof s.schemaPointer === "string", "schema pointer required");
    check(
      s.role === "current-continuity" || /^[a-f0-9]{64}$/.test(s.sha256 ?? ""),
      "selected evidence/reference bytes must be pinned",
    );
    check(
      s.assertions === undefined || (Array.isArray(s.assertions) && s.assertions.length <= 32),
      "finite literal assertions",
    );
  }
  check(
    a.supplements.every((s) => ["decision", "direction", "evidence"].includes(s.role)) &&
      a.currentRecords.every((s) => ["current-continuity", "current-reference"].includes(s.role)),
    "source category/table mismatch",
  );
  check(
    Array.isArray(a.decisionGroups) &&
      a.decisionGroups.length <= 16 &&
      Array.isArray(a.joins) &&
      a.joins.length <= 128,
    "finite source groups/joins",
  );
  const used = new Set<string>(),
    ids = new Set<string>();
  for (const g of a.decisionGroups) {
    record(g);
    exact(g, [
      "id",
      "decisionIds",
      "decisionPath",
      "directionPath",
      "evidencePath",
      "decisionIdPointers",
      "directionIdPointers",
      "directionDecisionRefPointer",
      "directionEvidenceRefPointer",
    ]);
    literal(g.id);
    references(g.decisionIds);
    check(
      g.decisionIds.length > 0 && g.decisionIds.every((id) => /^D[0-9]+$/.test(id) && !ids.has(id)),
      "unique actual decision identifiers",
    );
    g.decisionIds.forEach((id) => ids.add(id));
    for (const [path, role] of [
      [g.decisionPath, "decision"],
      [g.directionPath, "direction"],
      [g.evidencePath, "evidence"],
    ]) {
      check(
        a.supplements.some((s) => s.path === path && s.role === role),
        "complete decision/direction/evidence source group",
      );
      used.add(path!);
    }
    check(
      g.decisionIdPointers.length === g.decisionIds.length &&
        g.directionIdPointers.length === g.decisionIds.length,
      "decision ID joins required",
    );
  }
  check(
    a.supplements.every((s) => used.has(s.path)),
    "no unclassified supplemental source",
  );
  for (const j of a.joins) {
    record(j);
    exact(j, ["kind", "left", "right"]);
    check(
      ["equal", "source-path", "sha256"].includes(j.kind) &&
        paths.has(j.left.path) &&
        paths.has(j.right.path),
      "join must use selected files",
    );
  }
}
/** Typed core stays stable while bounded source-owned metadata can evolve without a v8-key whitelist. */
export function validateMeaningfulCheckpoint(value: unknown, workId: string) {
  record(value);
  check(
    value.schema === "hermes.platform-rearchitecture.continuity.v1" && value.workId === workId,
    "same Work checkpoint",
  );
  check(
    Number.isSafeInteger(value.version) &&
      value.version > 0 &&
      ["in-progress", "paused", "complete"].includes(value.status),
    "checkpoint version/lifecycle",
  );
  references(value.completed);
  references(value.pending);
  references(value.activeWriters);
  literal(value.nextAction);
  literal(value.updatedAtUtc);
  check(
    Array.isArray(value.uncertain) && value.uncertain.length <= 64,
    "bounded uncertain effects",
  );
  for (const e of value.uncertain) {
    record(e);
    literal(e.effect);
    literal(e.status);
    literal(e.reason);
    if (e.sourceRefs !== undefined) references(e.sourceRefs);
  }
  const walk = (v: any, depth = 0) => {
    check(depth <= 12, "checkpoint metadata depth bound");
    if (v === null || typeof v === "boolean") return;
    if (typeof v === "number") {
      check(Number.isFinite(v), "finite metadata number");
      return;
    }
    if (typeof v === "string") {
      check(v.length <= 8192, "bounded metadata text");
      return;
    }
    if (Array.isArray(v)) {
      check(v.length <= 256, "bounded metadata array");
      v.forEach((x) => walk(x, depth + 1));
      return;
    }
    record(v);
    check(Object.keys(v).length <= 128, "bounded metadata keys");
    for (const [k, x] of Object.entries(v)) {
      check(
        ![
          "__proto__",
          "constructor",
          "prototype",
          "transcript",
          "rawtranscript",
          "conversationhistory",
          "conversation_history",
          "user_message",
          "messages",
          "accesstoken",
          "refreshtoken",
          "apikey",
          "password",
        ].includes(k.toLowerCase()),
        "raw transcript/credential payload is not checkpoint metadata",
      );
      walk(x, depth + 1);
    }
  };
  walk(value);
  return value;
}
export function readAdmittedWorkRecords(
  bundle: HandoffBundle,
  a: WorkRecordAdmission,
  basePaths: readonly string[] = [],
) {
  validateWorkRecordAdmission(a);
  validateHandoff(bundle);
  const slots = [...a.supplements, ...a.currentRecords],
    admitted = new Set([...basePaths, ...slots.map((s) => s.path)]);
  check(
    bundle.files.length === admitted.size && bundle.files.every((f) => admitted.has(f.path)),
    "only exact selected record bytes allowed",
  );
  const rows = new Map<string, { document: any; sha256: string; originPath: string }>();
  for (const p of a.immutablePurposeSources) {
    const f = bundle.files.find((f) => f.path === p.path);
    if (f) check(f.sha256 === p.sha256, "original purpose source drift");
    rows.set(p.path, {
      document: f ? JSON.parse(Buffer.from(f.base64, "base64").toString()) : undefined,
      sha256: p.sha256,
      originPath: p.originPath,
    });
  }
  const records = slots.map((slot) => {
    const file = bundle.files.find((f) => f.path === slot.path);
    check(
      file && file.byteLength <= slot.maxBytes && (!slot.sha256 || slot.sha256 === file.sha256),
      "selected source pin drift or bound",
    );
    const document = JSON.parse(Buffer.from(file.base64, "base64").toString());
    record(document);
    check(pointer(document, slot.schemaPointer) === slot.schema, "source schema drift");
    for (const [p, expected] of [
      [slot.projectPointer, a.ownerProjectId],
      [slot.workPointer, a.workId],
    ])
      if (p) check(pointer(document, p) === expected, "same Project/Work source join");
    if (document.projectId !== undefined)
      check(document.projectId === a.ownerProjectId, "foreign Project source");
    if (document.workId !== undefined) check(document.workId === a.workId, "foreign Work source");
    if (document.payload?.details?.workId !== undefined)
      check(document.payload.details.workId === a.workId, "foreign Work payload");
    for (const x of slot.assertions ?? [])
      check(
        stable(pointer(document, x.pointer)) === stable(x.equals),
        "source literal assertion drift",
      );
    if (slot.role === "current-continuity") validateMeaningfulCheckpoint(document, a.workId);
    rows.set(slot.path, { document, sha256: file.sha256, originPath: slot.originPath });
    return {
      role: slot.role,
      path: slot.path,
      originPath: slot.originPath,
      sha256: file.sha256,
      byteLength: file.byteLength,
      document,
    };
  });
  const decisionGroups = a.decisionGroups.map((g) => {
    const d = rows.get(g.decisionPath)!.document,
      x = rows.get(g.directionPath)!.document,
      e = rows.get(g.evidencePath)!.document;
    check(
      d.kind === "decision" &&
        d.payload?.status === "accepted" &&
        e.kind === "evidence" &&
        e.payload?.result?.outcome === "success" &&
        e.payload.result.contentMatched === true,
      "accepted decision/successful evidence source required",
    );
    check(
      stable(g.decisionIdPointers.map((p) => pointer(d, p))) === stable(g.decisionIds) &&
        stable(g.directionIdPointers.map((p) => pointer(x, p))) === stable(g.decisionIds),
      "actual decision ID/source joins",
    );
    check(
      resolve(a.ownerRoot, pointer(x, g.directionDecisionRefPointer)) ===
        rows.get(g.decisionPath)!.originPath,
      "direction/decision source locator join",
    );
    if (g.directionEvidenceRefPointer)
      check(
        resolve(a.ownerRoot, pointer(x, g.directionEvidenceRefPointer)) ===
          rows.get(g.evidencePath)!.originPath,
        "direction/evidence source locator join",
      );
    check(
      d.payload.authority?.sourceEvidenceRef === e.id &&
        d.payload.evidenceRefs?.includes(e.id) &&
        ["actorType", "actorId", "scope"].every(
          (k) => d.payload.authority[k] === e.payload.details?.authority?.[k],
        ),
      "actual decision/evidence authority source join",
    );
    return {
      id: g.id,
      decisionIds: g.decisionIds,
      decisionPath: g.decisionPath,
      directionPath: g.directionPath,
      evidencePath: g.evidencePath,
    };
  });
  for (const j of a.joins) {
    const left = rows.get(j.left.path),
      right = rows.get(j.right.path);
    check(left && right, "joined source missing");
    const v = pointer(left.document, j.left.pointer);
    if (j.kind === "source-path")
      check(
        typeof v === "string" && resolve(a.ownerRoot, v) === right.originPath,
        "source locator join drift",
      );
    else if (j.kind === "sha256") check(v === right.sha256, "source hash join drift");
    else {
      check(j.right.pointer, "right data pointer required");
      const r = pointer(right.document, j.right.pointer);
      check(stable(v) === stable(r), "source value join drift");
    }
  }
  return {
    checkpoint: records.find((r) => r.role === "current-continuity")!,
    records,
    decisionGroups,
    authority: "none",
    semanticAcceptance: "not-inferred",
  };
}
export async function packAdmittedWorkRecordBytes(
  sourceRoot: string,
  a: WorkRecordAdmission,
  issueId: string,
) {
  validateWorkRecordAdmission(a);
  const slots = [...a.supplements, ...a.currentRecords],
    pins = new Map<string, string>();
  for (const s of slots) {
    const raw = await readExactFile(join(sourceRoot, s.path), s.maxBytes, s.sha256);
    pins.set(s.path, sha(raw));
  }
  const bundle = await packHandoff(
    sourceRoot,
    slots.map((s) => s.path),
    {
      issueId,
      sourceRevision: sha(JSON.stringify([...pins])),
      completedEffectIds: [],
      checks: [],
      nextAction: {
        id: "read-current-work-records",
        action:
          "Read actual completed/pending/uncertain source bytes; no native authority inferred.",
        path: a.currentRecords.find((s) => s.role === "current-continuity")!.path,
      },
    },
  );
  readAdmittedWorkRecords(bundle, a);
  for (const s of slots)
    await readExactFile(join(sourceRoot, s.path), s.maxBytes, pins.get(s.path));
  if (a.manifestPin) await readExactFile(a.manifestPin.path, 65536, a.manifestPin.sha256);
  return bundle;
}
export async function loadWorkSourceManifest(path: string) {
  const raw = await readExactFile(path, 65536),
    m = JSON.parse(raw.toString());
  record(m);
  check(m.schema === "hermes.work-source-manifest.v1", "source manifest schema");
  const convert = (s: any) => ({ ...s, originPath: resolve(m.ownerRoot, s.originPath) });
  const admission: WorkRecordAdmission = {
    schema: "hermes.work-record-admission.v2",
    ownerRoot: m.ownerRoot,
    ownerProjectId: m.ownerProjectId,
    workId: m.workId,
    immutablePurposeSources: m.immutablePurposeSources.map(convert),
    supplements: m.supplements.map(convert),
    currentRecords: m.currentRecords.map(convert),
    decisionGroups: m.decisionGroups,
    joins: m.joins,
    manifestPin: { path, sha256: sha(raw) },
  };
  validateWorkRecordAdmission(admission);
  for (const s of admission.immutablePurposeSources)
    await readExactFile(s.originPath, 262144, s.sha256);
  await readExactFile(path, 65536, sha(raw));
  return { manifest: m, admission, manifestPin: admission.manifestPin! };
}
export async function currentEngineeringRecordAdmission(ownerRoot: string) {
  if (!ownerRoot || resolve(ownerRoot) !== ownerRoot)
    throw Error("EXPLICIT_LEGACY_SOURCE_OWNER_REQUIRED");
  return (
    await loadWorkSourceManifest(
      join(ownerRoot, "architecture/platform-rearchitecture/work-source-manifest.json"),
    )
  ).admission;
}
