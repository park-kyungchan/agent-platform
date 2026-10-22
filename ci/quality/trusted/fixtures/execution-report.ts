import { randomUUID } from "node:crypto";
import {
  baseImage,
  command,
  requiredChecks,
  trustedDigest,
  requiredSemanticCases,
  type TrustedPolicy,
  type Subject,
  type ExecutionReport,
  type WorkerObservation,
} from "../policy";
import { semanticRequestDigest, assertedSemanticCase } from "../semantic";
const h = (s: string) => s.repeat(64),
  oid = (s: string) => s.repeat(40);
export function fixture() {
  const p: TrustedPolicy = {
    schema: "agent-platform.trusted-ci-policy.v2",
    release: oid("a"),
    revision: 1,
    protectedTag: "trusted-ci-v1",
    workflowSha256: h("a"),
    controllerClosureSha256: h("b"),
    workerRecipeSha256: h("c"),
    toolPinsSha256: h("d"),
    dependencyInputs: { "package.json": h("e"), "bun.lock": h("f") },
    dispatcher: { id: 241037670 },
    reviewPublisher: { id: 241037670, login: "park-kyungchan" },
    maxAuditAgeSeconds: 3600,
    probes: structuredClone(requiredSemanticCases),
    catalogueSha256: trustedDigest(requiredSemanticCases),
    coverageSha256: h("8"),
    legacyMinimum: {
      packageTests: ["packages/work-continuity/src/git-work/v3.test.ts"],
      qualityTests: ["ci/quality/trusted/policy.test.ts"],
      supportFiles: ["ci/quality/trusted/fixtures/source-positive.ts"],
    },
  };
  const a: Subject = {
    commit: oid("b"),
    tree: oid("c"),
    parents: [oid("d")],
    sourceManifestSha256: h("1"),
    dependencyInputsSha256: h("2"),
  };
  const m: Subject = {
    commit: oid("e"),
    tree: oid("f"),
    parents: [oid("1"), a.commit],
    sourceManifestSha256: h("3"),
    dependencyInputsSha256: h("4"),
  };
  const e: ExecutionReport = {
    schema: "agent-platform.trusted-ci-execution.v2",
    policyRelease: p.release,
    policyRevision: 1,
    controllerClosureSha256: p.controllerClosureSha256,
    workflowSha256: p.workflowSha256,
    runId: 71,
    attempt: 2,
    requestId: randomUUID(),
    target: {
      kind: "pull-request",
      pr: 73,
      base: oid("1"),
      authored: a,
      sourceBase: oid("d"),
      tested: m,
    },
    preparation: [],
    audits: [],
    checks: [],
    probes: [],
    sourceAssertions: [],
    eligibilityClaim: "bounded-p-semantic-v1",
    fullLegacyAssertionsAuthenticated: false,
    status: "pass",
    nativePermissionGranted: false,
    acceptanceDecision: null,
  };
  const now = Date.now();
  const observation = (
    s: Subject,
    role: WorkerObservation["role"],
    network: WorkerObservation["network"],
  ): WorkerObservation => ({
    occurrence: randomUUID(),
    containerId: h("a"),
    imageId: "sha256:" + h("b"),
    baseImage,
    recipeSha256: p.workerRecipeSha256,
    role,
    network,
    inspectSha256: h("c"),
    inputSha256: trustedDigest(s),
    dependencyTreeSha256: role === "prepare" ? null : h("9"),
    toolPinsSha256: p.toolPinsSha256,
    startedAt: new Date(now - 2000).toISOString(),
    finishedAt: new Date(now - 1000).toISOString(),
    exitCode: 0,
    oomKilled: false,
    timedOut: false,
    stdoutSha256: h("d"),
    stderrSha256: h("e"),
    boundaryVerified: true,
  });
  for (const s of [a, m]) {
    e.preparation.push({
      subject: s.commit,
      dependencyInputsSha256: s.dependencyInputsSha256,
      dependencyTreeSha256: h("9"),
      toolPinsSha256: p.toolPinsSha256,
      observation: observation(s, "prepare", "bridge"),
    });
    const audit = observation(s, "audit", "bridge");
    e.audits.push({
      subject: s.commit,
      dependencyInputsSha256: s.dependencyInputsSha256,
      observedAt: audit.finishedAt,
      feed: "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk",
      observation: audit,
    });
    for (const id of requiredChecks)
      e.checks.push({
        id,
        subject: s,
        argv: command(id),
        observation: observation(s, id === "static-quality" ? "checker" : "candidate", "none"),
      });
    for (const c of p.probes)
      e.probes.push({
        id: c.id,
        subject: s,
        inputSha256: semanticRequestDigest(c),
        actual: assertedSemanticCase(c),
        observation: observation(s, "probe", "none"),
      });
  }
  for (const x of e.checks.filter((row) => row.id === "source"))
    e.sourceAssertions.push({
      subject: x.subject.commit,
      occurrence: x.observation.occurrence,
      evidence: {
        schema: "agent-platform.shared-process-source-observation.v1",
        state: "held",
        reason: "TRUSTED_CI_SOURCE_ASSERTIONS_NOT_PROCESS_ISOLATED",
        assertionBoundary: "candidate-and-P-assertions-share-process",
        assertionsAuthenticated: false,
        workload: {
          state: "exit-zero-only",
          exitCode: 0,
          timedOut: false,
          oomKilled: false,
          stdoutSha256: x.observation.stdoutSha256,
        },
      },
    });
  return { p, e, now };
}
