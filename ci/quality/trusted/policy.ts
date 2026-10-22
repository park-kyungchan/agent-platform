import { createHash } from "node:crypto";
import { requireObservedSourceWorkload, type SourceAssertionEvidence } from "./source-completion";

import catalogue from "./semantic-catalogue.json";
import { assertedSemanticCase, semanticRequestDigest, type SemanticCase } from "./semantic";

export const eligibilityClaim = "bounded-p-semantic-v1";
export const requiredSemanticCases = catalogue.cases as unknown as SemanticCase[];
export const repository = "park-kyungchan/agent-platform";
export const repositoryId = 1413326539;
export const appId = 5417476;
export const installationId = 173913750;
export const sourceBunSha256 = "7ce7d6b654eddeec20afddf2396317fba6dc19ab52788a27e894e1806bf8f24b";
export const baseImage =
  "node:22.23.3-bookworm@sha256:d05fa967fa1bbe6ba4fb48f9d4c55b5e6a8c8300aedb6b99a11c248a877d7bee";
export const checkName = "Trusted source policy";
export const jobName = "Trusted source controller";
export const requiredSteps = [
  "Load immutable policy",
  "Observe isolated source checks",
  "Upload controller evidence",
];
export const requiredChecks = [
  "static-quality",
  "source",
  "runtime-layout",
  "candidate-quality",
] as const;
export const retainedContexts = [
  "Canonical source checks",
  "Quality and architecture checks",
  "Exact head native independent review",
  "Source and native review gate",
  "native-independent-review",
];
export function insist(value: unknown, code: string): asserts value {
  if (!value) throw Error("TRUSTED_CI_" + code);
}
export const sha256 = (bytes: Uint8Array | string): string =>
  createHash("sha256").update(bytes).digest("hex");
export const oid = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{40}$/.test(v);
export const digest = (v: unknown): v is string =>
  typeof v === "string" && /^[a-f0-9]{64}$/.test(v);
export const positive = (v: unknown): v is number => Number.isSafeInteger(v) && Number(v) > 0;
export function object(v: unknown): Record<string, unknown> {
  insist(v && typeof v === "object" && !Array.isArray(v), "OBJECT_REQUIRED");
  return v as Record<string, unknown>;
}
export function safePath(v: string): string {
  insist(
    v.length > 0 &&
      v.length < 512 &&
      /^[A-Za-z0-9_.@/%+ -]+$/.test(v) &&
      !v.startsWith("/") &&
      !v.split("/").some((s) => ["", ".", "..", ".git"].includes(s)),
    "SAFE_PATH_REQUIRED",
  );
  return v;
}
export function canonical(v: unknown): string {
  if (v === null || typeof v === "string" || typeof v === "boolean") return JSON.stringify(v);
  if (typeof v === "number" && Number.isSafeInteger(v)) return String(v);
  if (Array.isArray(v)) return "[" + v.map(canonical).join(",") + "]";
  insist(
    v && typeof v === "object" && Object.getPrototypeOf(v) === Object.prototype,
    "PLAIN_JSON_REQUIRED",
  );
  return (
    "{" +
    Object.keys(v)
      .sort()
      .map((k) => JSON.stringify(k) + ":" + canonical((v as Record<string, unknown>)[k]))
      .join(",") +
    "}"
  );
}
export const trustedDigest = (v: unknown): string => sha256(canonical(v));
export const same = (a: unknown, b: unknown): boolean => canonical(a) === canonical(b);
export function parseReport(bytes: Uint8Array): ExecutionReport {
  insist(bytes.length > 0 && bytes.length <= 4_000_000, "REPORT_SIZE");
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const value: unknown = JSON.parse(text);
  insist(JSON.stringify(value, null, 2) + "\n" === text, "UNAMBIGUOUS_REPORT_REQUIRED");
  return value as ExecutionReport;
}
/** Loaded from the native owner's immutable profile, never from a candidate request. */
export interface TrustedPolicy {
  schema: "agent-platform.trusted-ci-policy.v2";
  release: string;
  revision: number;
  protectedTag: string;
  workflowSha256: string;
  controllerClosureSha256: string;
  workerRecipeSha256: string;
  toolPinsSha256: string;
  dependencyInputs: Record<string, string>;
  dispatcher: { id: 241037670 };
  reviewPublisher: { id: 241037670; login: string };
  maxAuditAgeSeconds: number;
  /** The host compares these assertions outside every candidate process. */
  probes: SemanticCase[];
  catalogueSha256: string;
  coverageSha256: string;
  legacyMinimum: { packageTests: string[]; qualityTests: string[]; supportFiles: string[] };
}
export interface Subject {
  commit: string;
  tree: string;
  parents: string[];
  sourceManifestSha256: string;
  dependencyInputsSha256: string;
}
export interface Target {
  kind: "pull-request" | "protected-main";
  pr: number;
  base: string;
  authored: Subject;
  sourceBase: string;
  tested: Subject;
}
export interface WorkerObservation {
  occurrence: string;
  containerId: string;
  imageId: string;
  baseImage: typeof baseImage;
  recipeSha256: string;
  role: "prepare" | "audit" | "checker" | "candidate" | "probe";
  network: "bridge" | "none";
  inspectSha256: string;
  inputSha256: string;
  dependencyTreeSha256: string | null;
  toolPinsSha256: string;
  startedAt: string;
  finishedAt: string;
  exitCode: number;
  oomKilled: boolean;
  timedOut: boolean;
  stdoutSha256: string;
  stderrSha256: string;
  boundaryVerified: true;
}
export interface ExecutionReport {
  schema: "agent-platform.trusted-ci-execution.v2";
  policyRelease: string;
  policyRevision: number;
  controllerClosureSha256: string;
  workflowSha256: string;
  runId: number;
  attempt: number;
  requestId: string;
  target: Target;
  preparation: {
    subject: string;
    dependencyInputsSha256: string;
    dependencyTreeSha256: string;
    toolPinsSha256: string;
    observation: WorkerObservation;
  }[];
  audits: {
    subject: string;
    dependencyInputsSha256: string;
    observedAt: string;
    feed: "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk";
    observation: WorkerObservation;
  }[];
  checks: { id: string; subject: Subject; argv: string[]; observation: WorkerObservation }[];
  probes: {
    id: string;
    subject: Subject;
    inputSha256: string;
    actual: unknown;
    observation: WorkerObservation;
  }[];
  sourceAssertions: { subject: string; occurrence: string; evidence: SourceAssertionEvidence }[];
  eligibilityClaim: typeof eligibilityClaim;
  fullLegacyAssertionsAuthenticated: false;
  status: "pass" | "fail" | "held";
  nativePermissionGranted: false;
  acceptanceDecision: null;
}
export interface ProviderRun {
  id: number;
  run_attempt: number;
  head_sha: string;
  path: string;
  event: string;
  status: string;
  conclusion: string;
  head_branch: string;
  actor: { id: number };
  triggering_actor: { id: number };
  repository: { id: number };
  display_title: string;
}
export interface ProviderJob {
  id: number;
  run_id: number;
  run_attempt: number;
  name: string;
  head_sha: string;
  status: string;
  conclusion: string;
  steps: { name: string; number: number; status: string; conclusion: string }[];
}
export const workflowPath = ".github/workflows/trusted-source.yml";
export const runTitle = (requestId: string): string => "Trusted source " + requestId;
export const subjects = (t: Target): Subject[] =>
  t.kind === "pull-request" ? [t.authored, t.tested] : [t.tested];
export function command(id: (typeof requiredChecks)[number]): string[] {
  return [
    "/usr/local/bin/bun",
    "--no-env-file",
    "--no-install",
    "/policy/ci/quality/trusted/driver.ts",
    id,
  ];
}
export function validateTrustedPolicy(p: TrustedPolicy): void {
  insist(
    p.schema === "agent-platform.trusted-ci-policy.v2" && oid(p.release) && positive(p.revision),
    "IMMUTABLE_POLICY_REQUIRED",
  );
  insist(/^trusted-ci-v[1-9][0-9]*$/.test(p.protectedTag), "IMMUTABLE_PROTECTED_TAG_REQUIRED");
  insist(
    [p.workflowSha256, p.controllerClosureSha256, p.workerRecipeSha256, p.toolPinsSha256].every(
      digest,
    ),
    "POLICY_CLOSURE_REQUIRED",
  );
  insist(
    p.dispatcher.id === 241037670 &&
      p.reviewPublisher.id === 241037670 &&
      /^[A-Za-z0-9-]+$/.test(p.reviewPublisher.login),
    "DISTINCT_ROLE_BINDINGS_REQUIRED",
  );
  insist(
    positive(p.maxAuditAgeSeconds) && p.maxAuditAgeSeconds <= 86400,
    "AUDIT_FRESHNESS_REQUIRED",
  );
  insist(
    Object.keys(p.dependencyInputs).length >= 2 && Object.keys(p.dependencyInputs).length <= 256,
    "FROZEN_DEPENDENCY_INPUTS_REQUIRED",
  );
  for (const [path, value] of Object.entries(p.dependencyInputs))
    insist(safePath(path) && digest(value), "DEPENDENCY_PIN_REQUIRED");
  insist(
    Object.hasOwn(p.dependencyInputs, "package.json") &&
      Object.hasOwn(p.dependencyInputs, "bun.lock"),
    "FROZEN_LOCK_REQUIRED",
  );
  insist(
    p.probes.length > 0 &&
      p.probes.length <= 256 &&
      new Set(p.probes.map((x) => x.id)).size === p.probes.length,
    "INVARIANT_DENOMINATOR_REQUIRED",
  );
  insist(
    same(p.probes, requiredSemanticCases) &&
      p.catalogueSha256 === trustedDigest(p.probes) &&
      digest(p.coverageSha256),
    "EXACT_P_SEMANTIC_CATALOGUE_REQUIRED",
  );
  insist(
    p.legacyMinimum &&
      p.legacyMinimum.packageTests.length > 0 &&
      p.legacyMinimum.qualityTests.length > 0 &&
      p.legacyMinimum.supportFiles.length > 0,
    "LEGACY_P_MINIMUM_REQUIRED",
  );
  for (const paths of Object.values(p.legacyMinimum)) {
    insist(new Set(paths).size === paths.length, "DUPLICATE_LEGACY_P_MINIMUM");
    for (const path of paths) safePath(path);
  }
  for (const probe of p.probes) {
    insist(
      /^[a-z][a-z0-9-]{0,79}$/.test(probe.id) &&
        safePath(probe.module) &&
        /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(probe.exportName),
      "FIXED_INVARIANT_REQUIRED",
    );
    insist(
      canonical(probe.args).length < 65536 &&
        canonical(probe.expected).length < 65536 &&
        probe.callers.length > 0 &&
        probe.family.length > 0,
      "BOUNDED_INVARIANT_REQUIRED",
    );
  }
}
export function validateTarget(t: Target): void {
  insist(
    [t.base, t.sourceBase].every(oid) && t.authored.commit !== t.tested.commit,
    "AUTHORED_AND_MERGED_SUBJECTS_REQUIRED",
  );
  for (const s of [t.authored, t.tested])
    insist(
      oid(s.commit) &&
        oid(s.tree) &&
        s.parents.every(oid) &&
        digest(s.sourceManifestSha256) &&
        digest(s.dependencyInputsSha256),
      "EXACT_SUBJECT_REQUIRED",
    );
  insist(
    t.authored.parents.length === 1 && t.authored.parents[0] === t.sourceBase,
    "ACTUAL_AUTHORED_PARENT_REQUIRED",
  );
  insist(same(t.tested.parents, [t.base, t.authored.commit]), "ORDERED_MECHANICAL_MERGE_REQUIRED");
  insist(
    t.kind === "pull-request" ? positive(t.pr) : t.kind === "protected-main" && t.pr === 0,
    "SUPPORTED_TARGET_REQUIRED",
  );
}
/** Pure validation only. It creates no native/App capability. */
export function qualifyTrustedExecution(p: TrustedPolicy, e: ExecutionReport, now: number): void {
  validateTrustedPolicy(p);
  validateTarget(e.target);
  insist(
    e.schema === "agent-platform.trusted-ci-execution.v2" &&
      e.policyRelease === p.release &&
      e.policyRevision === p.revision &&
      e.controllerClosureSha256 === p.controllerClosureSha256 &&
      e.workflowSha256 === p.workflowSha256,
    "EXECUTION_POLICY_MISMATCH",
  );
  insist(
    positive(e.runId) &&
      positive(e.attempt) &&
      /^[a-f0-9-]{36}$/.test(e.requestId) &&
      e.status === "pass" &&
      e.nativePermissionGranted === false &&
      e.acceptanceDecision === null &&
      e.eligibilityClaim === eligibilityClaim &&
      e.fullLegacyAssertionsAuthenticated === false,
    "SUCCESSFUL_SOURCE_REPORT_REQUIRED",
  );
  const wanted = subjects(e.target),
    occurrences = new Set<string>();
  function observed(
    o: WorkerObservation,
    s: Subject,
    role: WorkerObservation["role"],
    network: WorkerObservation["network"],
  ) {
    insist(
      /^[a-f0-9-]{36}$/.test(o.occurrence) && !occurrences.has(o.occurrence),
      "UNIQUE_WORKER_OCCURRENCE_REQUIRED",
    );
    occurrences.add(o.occurrence);
    insist(
      /^[a-f0-9]{64}$/.test(o.containerId) &&
        /^sha256:[a-f0-9]{64}$/.test(o.imageId) &&
        o.baseImage === baseImage &&
        o.recipeSha256 === p.workerRecipeSha256 &&
        o.boundaryVerified === true &&
        o.role === role &&
        o.network === network,
      "WORKER_BOUNDARY_REQUIRED",
    );
    insist(
      [o.inspectSha256, o.stdoutSha256, o.stderrSha256].every(digest) &&
        o.inputSha256 === trustedDigest(s) &&
        o.toolPinsSha256 === p.toolPinsSha256,
      "OBSERVED_SUBJECT_MISMATCH",
    );
    insist(
      o.exitCode === 0 &&
        !o.oomKilled &&
        !o.timedOut &&
        Number.isFinite(Date.parse(o.startedAt)) &&
        Date.parse(o.finishedAt) >= Date.parse(o.startedAt) &&
        Date.parse(o.finishedAt) <= now + 60000,
      "OBSERVED_COMMAND_FAILED",
    );
  }
  insist(
    e.preparation.length === wanted.length &&
      e.audits.length === wanted.length &&
      e.checks.length === wanted.length * requiredChecks.length &&
      e.probes.length === wanted.length * p.probes.length,
    "REQUIRED_DENOMINATOR_CHANGED",
  );
  for (const s of wanted) {
    const prep = e.preparation.filter((x) => x.subject === s.commit);
    insist(
      prep.length === 1 &&
        prep[0]!.dependencyInputsSha256 === s.dependencyInputsSha256 &&
        digest(prep[0]!.dependencyTreeSha256) &&
        prep[0]!.toolPinsSha256 === p.toolPinsSha256,
      "PREPARATION_REQUIRED",
    );
    insist(
      prep[0]!.observation.dependencyTreeSha256 === null,
      "PREPARATION_IS_NOT_PREEXISTING_DEPENDENCY_PROOF",
    );
    observed(prep[0]!.observation, s, "prepare", "bridge");
    const audits = e.audits.filter((a) => a.subject === s.commit),
      audit = audits[0];
    insist(
      audits.length === 1 &&
        audit &&
        audit.dependencyInputsSha256 === s.dependencyInputsSha256 &&
        audit.feed === "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk" &&
        Date.parse(audit.observedAt) === Date.parse(audit.observation.finishedAt) &&
        now - Date.parse(audit.observedAt) <= p.maxAuditAgeSeconds * 1000,
      "FULL_QUALITY_CURRENT_AUDIT_REQUIRED",
    );
    insist(
      audit.observation.dependencyTreeSha256 === prep[0]!.dependencyTreeSha256,
      "AUDIT_DEPENDENCY_TREE_MISMATCH",
    );
    observed(audit.observation, s, "audit", "bridge");
    for (const id of requiredChecks) {
      const xs = e.checks.filter((x) => x.id === id && same(x.subject, s));
      insist(xs.length === 1 && same(xs[0]!.argv, command(id)), "FIXED_CHECK_SUBJECT_REQUIRED");
      insist(
        xs[0]!.observation.dependencyTreeSha256 === prep[0]!.dependencyTreeSha256,
        "COMMAND_DEPENDENCY_TREE_MISMATCH",
      );
      observed(xs[0]!.observation, s, id === "static-quality" ? "checker" : "candidate", "none");
    }
    for (const probe of p.probes) {
      const xs = e.probes.filter((x) => x.id === probe.id && same(x.subject, s));
      insist(
        xs.length === 1 &&
          xs[0]!.inputSha256 === semanticRequestDigest(probe) &&
          same(xs[0]!.actual, assertedSemanticCase(probe)),
        "INVARIANT_ASSERTION_FAILED",
      );
      insist(
        xs[0]!.observation.dependencyTreeSha256 === prep[0]!.dependencyTreeSha256,
        "PROBE_DEPENDENCY_TREE_MISMATCH",
      );
      observed(xs[0]!.observation, s, "probe", "none");
    }
  }
  // Only the fixed external semantic catalogue is eligible; legacy completion stays unknown.
  requireObservedSourceWorkload(e.sourceAssertions, e.checks, wanted);
}
