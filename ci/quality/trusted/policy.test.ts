import { describe, expect, test } from "bun:test";
import { fixture } from "./fixtures/execution-report";
import {
  command,
  qualifyTrustedExecution,
  sha256,
  trustedDigest,
  validateTarget,
  type ExecutionReport,
} from "./policy";
import { readReportArchive } from "./artifact";
import { verifyRun, verifyPolicyRef, type ProviderReader } from "./provider";
import { validateWorkerInspect, type WorkerSpec } from "./worker";

const h = (s: string) => s.repeat(64),
  oid = (s: string) => s.repeat(40);
describe("immutable P policy and execution subjects", () => {
  test("complete exact H/M records qualify only the explicit external semantic catalogue", () => {
    const { p, e, now } = fixture();
    expect(() => qualifyTrustedExecution(p, e, now)).not.toThrow();
    expect(e.nativePermissionGranted).toBe(false);
  });
  test("caller full-completion claims and an owner digest cannot replace external assertions", () => {
    const { p, e, now } = fixture();
    Object.assign(e, {
      sourceAssertions: { state: "pass", assertionsAuthenticated: true },
      nativeOwnerApprovalSha256: h("8"),
      completionNonce: "candidate-can-observe-its-own-process",
    });
    expect(() => qualifyTrustedExecution(p, e, now)).toThrow("SOURCE_WORKLOAD_EVIDENCE_REQUIRED");
  });
  test("candidate case removal and claimed full legacy certification cannot pass", () => {
    const { p, e, now } = fixture();
    p.probes.pop();
    p.catalogueSha256 = trustedDigest(p.probes);
    expect(() => qualifyTrustedExecution(p, e, now)).toThrow("EXACT_P_SEMANTIC_CATALOGUE");
    const valid = fixture();
    Object.assign(valid.e, { fullLegacyAssertionsAuthenticated: true });
    expect(() => qualifyTrustedExecution(valid.p, valid.e, valid.now)).toThrow();
    valid.e.fullLegacyAssertionsAuthenticated = false;
    Object.assign(valid.e.sourceAssertions[0]!.evidence, { assertionsAuthenticated: true });
    expect(() => qualifyTrustedExecution(valid.p, valid.e, valid.now)).toThrow(
      "SOURCE_WORKLOAD_EVIDENCE_MISMATCH",
    );
  });
  test("exit zero and retained names do not replace external required invariants", () => {
    for (const change of [
      (e: ExecutionReport) => {
        e.probes = [];
      },
      (e: ExecutionReport) => {
        e.probes[0]!.actual = { kind: "return", value: true };
      },
      (e: ExecutionReport) => {
        e.checks[0]!.argv = ["sh", "-c", "exit 0"];
      },
      (e: ExecutionReport) => {
        e.checks[0]!.observation.exitCode = 1;
      },
    ]) {
      const { p, e, now } = fixture();
      change(e);
      expect(() => qualifyTrustedExecution(p, e, now)).toThrow();
    }
  });
  test("a policy-source edit cannot approve its own removal of old P checks", () => {
    const { p, e, now } = fixture();
    e.checks.pop();
    Object.assign(e, {
      policyChangeApproval: { ownerDecisionSha256: h("0"), removedTestPaths: [] },
    });
    expect(() => qualifyTrustedExecution(p, e, now)).toThrow("REQUIRED_DENOMINATOR");
  });
  test("H results cannot be relabeled as M, reversed/nested/foreign ancestry is refused", () => {
    for (const change of [
      (e: ExecutionReport) => {
        e.checks.at(-1)!.subject = e.target.authored;
      },
      (e: ExecutionReport) => {
        e.checks.at(-1)!.observation.inputSha256 = trustedDigest(e.target.authored);
      },
      (e: ExecutionReport) => {
        e.target.tested.parents.reverse();
      },
      (e: ExecutionReport) => {
        e.target.authored.parents.push(oid("9"));
      },
      (e: ExecutionReport) => {
        e.target.sourceBase = oid("9");
      },
    ]) {
      const { p, e, now } = fixture();
      change(e);
      expect(() => qualifyTrustedExecution(p, e, now)).toThrow();
    }
  });
  test("main uses actual postmerge subject rather than a fake skipped prospective success", () => {
    const { p, e, now } = fixture();
    e.target.kind = "protected-main";
    e.target.pr = 0;
    const commit = e.target.tested.commit;
    e.preparation = e.preparation.filter((x) => x.subject === commit);
    e.audits = e.audits.filter((x) => x.subject === commit);
    e.checks = e.checks.filter((x) => x.subject.commit === commit);
    e.probes = e.probes.filter((x) => x.subject.commit === commit);
    e.sourceAssertions = e.sourceAssertions.filter((x) => x.subject === commit);
    expect(() => qualifyTrustedExecution(p, e, now)).not.toThrow();
    e.target.pr = 1;
    expect(() => validateTarget(e.target)).toThrow();
  });
  test("required audit and tool/dependency preparation cannot be skipped, stale or offline", () => {
    for (const change of [
      (e: ExecutionReport) => {
        e.audits = [];
      },
      (e: ExecutionReport) => {
        e.audits[0]!.observation.network = "none";
      },
      (e: ExecutionReport) => {
        e.audits[0]!.observation.exitCode = 1;
      },
      (e: ExecutionReport) => {
        e.audits[0]!.observedAt = new Date(0).toISOString();
      },
      (e: ExecutionReport) => {
        e.preparation[0]!.dependencyInputsSha256 = h("8");
      },
      (e: ExecutionReport) => {
        e.preparation[0]!.toolPinsSha256 = h("8");
      },
    ]) {
      const { p, e, now } = fixture();
      change(e);
      expect(() => qualifyTrustedExecution(p, e, now)).toThrow();
    }
  });
  test("reused worker occurrence, OOM, timeout and boundary failure retain failure", () => {
    for (const change of [
      (e: ExecutionReport) => {
        e.checks[0]!.observation.occurrence = e.probes[0]!.observation.occurrence;
      },
      (e: ExecutionReport) => {
        e.checks[0]!.observation.oomKilled = true;
      },
      (e: ExecutionReport) => {
        e.checks[0]!.observation.timedOut = true;
      },
      (e: ExecutionReport) => {
        e.checks[0]!.observation.imageId = "latest";
      },
    ]) {
      const { p, e, now } = fixture();
      change(e);
      expect(() => qualifyTrustedExecution(p, e, now)).toThrow();
    }
  });
});
test("provider run provenance is raw path@ref, actual actor and triggering actor, exact attempt", () => {
  const { p, e } = fixture();
  const r = {
    id: e.runId,
    run_attempt: e.attempt,
    repository: { id: 1413326539 },
    event: "workflow_dispatch",
    head_sha: p.release,
    head_branch: p.protectedTag,
    path: `.github/workflows/trusted-source.yml@refs/tags/${p.protectedTag}`,
    actor: { id: p.dispatcher.id },
    triggering_actor: { id: p.dispatcher.id },
    display_title: "Trusted source " + e.requestId,
    status: "completed",
    conclusion: "success",
  };
  expect(() => verifyRun(p, r, e.runId, e.attempt, e.requestId, true)).not.toThrow();
  for (const change of [
    { path: ".github/workflows/trusted-source.yml" },
    { path: r.path.replace("refs/tags/", "refs/heads/") },
    { head_sha: e.target.authored.commit },
    { triggering_actor: { id: 1 } },
    { event: "workflow_call" },
    { run_attempt: 1 },
    { conclusion: "skipped" },
  ])
    expect(() =>
      verifyRun(p, { ...r, ...change }, e.runId, e.attempt, e.requestId, true),
    ).toThrow();
});
test("mutable policy tag or workflow bytes cannot authenticate P", async () => {
  const { p } = fixture();
  const reader: ProviderReader = {
    json: async () => ({
      ref: "refs/tags/" + p.protectedTag,
      object: { type: "commit", sha: oid("f") },
    }),
    archive: async () => new Uint8Array(),
  };
  await expect(verifyPolicyRef(reader, p)).rejects.toThrow("POLICY_TAG_MOVED");
});
test("actual worker config refuses auth/socket mounts, privileged env and host namespace", () => {
  const { p } = fixture(),
    id = h("a");
  const spec: WorkerSpec = {
    role: "checker",
    imageId: "sha256:" + h("b"),
    network: "none",
    mounts: [{ source: "/approved-policy", target: "/policy", readOnly: true, type: "bind" }],
    argv: command("static-quality"),
  };
  const env = [
    "HOME=/job/home",
    "PATH=/usr/local/bin:/usr/bin:/bin",
    "LANG=C.UTF-8",
    "CI=true",
    "RUNNER_TEMP=/job/tmp",
  ];
  const raw = {
    Id: id,
    Image: spec.imageId,
    Config: {
      User: "1000:1000",
      WorkingDir: "/job",
      Env: env,
      Entrypoint: ["/usr/bin/env", "-i", ...env],
      Cmd: spec.argv,
    },
    HostConfig: {
      NetworkMode: "none",
      Privileged: false,
      ReadonlyRootfs: true,
      CapDrop: ["ALL"],
      SecurityOpt: ["no-new-privileges:true"],
      PidMode: "",
      IpcMode: "private",
      Memory: 2147483648,
      NanoCpus: 2000000000,
      PidsLimit: 256,
      Tmpfs: {
        "/job": "rw,nosuid,nodev,size=2147483648,uid=1000,gid=1000,mode=0700",
        "/tmp": "rw,nosuid,nodev,noexec,size=268435456,uid=1000,gid=1000,mode=0700",
      },
    },
    Mounts: [{ Type: "bind", Source: "/approved-policy", Destination: "/policy", RW: false }],
    State: { Running: false, Status: "created" },
  };
  expect(() => validateWorkerInspect(raw, spec, id)).not.toThrow();
  for (const mutate of [
    (r: typeof raw) => {
      r.Config.Env = [...env, "GITHUB_TOKEN=fixture-not-a-credential"];
    },
    (r: typeof raw) => {
      r.HostConfig.Privileged = true;
    },
    (r: typeof raw) => {
      r.HostConfig.PidMode = "host";
    },
    (r: typeof raw) => {
      r.Mounts[0]!.RW = true;
    },
    (r: typeof raw) => {
      r.Mounts.push({
        Type: "bind",
        Source: "/var/run/docker.sock",
        Destination: "/socket",
        RW: false,
      });
    },
  ]) {
    const changed = structuredClone(raw);
    mutate(changed);
    expect(() => validateWorkerInspect(changed, spec, id)).toThrow();
  }
  expect(p.probes.length).toBeGreaterThan(0);
});

function crc32(bytes: Uint8Array): number {
  let n = 0xffffffff;
  for (const byte of bytes) {
    n ^= byte;
    for (let i = 0; i < 8; i++) n = (n >>> 1) ^ (n & 1 ? 0xedb88320 : 0);
  }
  return (n ^ 0xffffffff) >>> 0;
}
function zip(text: string, name = "trusted-execution.json"): Buffer {
  const data = Buffer.from(text),
    filename = Buffer.from(name),
    local = Buffer.alloc(30),
    central = Buffer.alloc(46),
    end = Buffer.alloc(22);
  local.writeUInt32LE(0x04034b50);
  local.writeUInt16LE(20, 4);
  local.writeUInt32LE(crc32(data), 14);
  local.writeUInt32LE(data.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(filename.length, 26);
  central.writeUInt32LE(0x02014b50);
  central.writeUInt16LE(20, 6);
  central.writeUInt32LE(crc32(data), 16);
  central.writeUInt32LE(data.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(filename.length, 28);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(central.length + filename.length, 12);
  end.writeUInt32LE(local.length + filename.length + data.length, 16);
  return Buffer.concat([local, filename, data, central, filename, end]);
}
test("artifact binding hashes archive bytes and report bytes separately; no archive extraction", () => {
  const bytes = zip("fixture report\n");
  expect(readReportArchive(bytes, sha256(bytes)).toString()).toBe("fixture report\n");
  expect(() => readReportArchive(bytes, h("1"))).toThrow("EXACT_ARTIFACT");
  const changed = Buffer.from(bytes);
  changed[30 + "trusted-execution.json".length] ^= 1;
  expect(() => readReportArchive(changed, sha256(changed))).toThrow("INTEGRITY");
  const escaped = zip("fixture report\n", "../trusted-execution.json");
  expect(() => readReportArchive(escaped, sha256(escaped))).toThrow("EXACT_REPORT_MEMBER");
  const multiple = Buffer.from(bytes);
  multiple.writeUInt16LE(2, multiple.length - 12);
  expect(() => readReportArchive(multiple, sha256(multiple))).toThrow("SINGLE_MEMBER");
});

test("issuer rejects a raw JSON context before any profile/key/provider access", async () => {
  const { issueNative } = await import("./github-app");
  await expect(
    issueNative({ profile: {}, intent: {}, assertCurrent() {}, record() {} } as never),
  ).rejects.toThrow("ACTUAL_NATIVE_CONTEXT_REQUIRED");
});
test("actual jobs/steps, exact artifact and attempt are mandatory before report bytes", async () => {
  const { fetchExecution } = await import("./provider");
  const { p, e } = fixture(),
    bytes = zip(JSON.stringify(e, null, 2) + "\n");
  const run = {
    id: e.runId,
    run_attempt: e.attempt,
    repository: { id: 1413326539 },
    event: "workflow_dispatch",
    head_sha: p.release,
    head_branch: p.protectedTag,
    path: `.github/workflows/trusted-source.yml@refs/tags/${p.protectedTag}`,
    actor: { id: p.dispatcher.id },
    triggering_actor: { id: p.dispatcher.id },
    display_title: "Trusted source " + e.requestId,
    status: "completed",
    conclusion: "success",
  };
  const job = {
    id: 81,
    run_id: e.runId,
    run_attempt: e.attempt,
    name: "Trusted source controller",
    head_sha: p.release,
    status: "completed",
    conclusion: "success",
    steps: [
      "Load immutable policy",
      "Observe isolated source checks",
      "Upload controller evidence",
    ].map((name, n) => ({ name, number: n + 1, status: "completed", conclusion: "success" })),
  };
  const artifact = {
    id: 91,
    name: `trusted-source-${e.runId}-${e.attempt}`,
    size_in_bytes: bytes.length,
    expired: false,
    digest: "sha256:" + sha256(bytes),
    workflow_run: { id: e.runId, head_sha: p.release, head_branch: p.protectedTag },
  };
  let jobs: unknown[] = [job],
    artifacts: unknown[] = [artifact],
    archiveBytes = bytes;
  const reader: ProviderReader = {
    async json(path) {
      if (path.endsWith(`/actions/runs/${e.runId}`)) return run;
      if (path.includes("/jobs?")) return { total_count: jobs.length, jobs };
      if (path.includes("/artifacts?")) return { total_count: artifacts.length, artifacts };
      throw Error("unexpected fixture path");
    },
    archive: async () => archiveBytes,
  };
  expect((await fetchExecution(reader, p, e.runId, e.attempt, e.requestId)).reportSha256).toBe(
    sha256(JSON.stringify(e, null, 2) + "\n"),
  );
  for (const conclusion of ["skipped", "cancelled", "neutral", "timed_out", "failure"]) {
    jobs = [{ ...job, steps: job.steps.map((s, n) => (n ? s : { ...s, conclusion })) }];
    await expect(fetchExecution(reader, p, e.runId, e.attempt, e.requestId)).rejects.toThrow(
      "REQUIRED_STEP",
    );
  }
  jobs = [job];
  artifacts = [artifact, { ...artifact, id: 92 }];
  await expect(fetchExecution(reader, p, e.runId, e.attempt, e.requestId)).rejects.toThrow(
    "UNSHADOWED",
  );
  artifacts = [{ ...artifact, name: `trusted-source-${e.runId}-1` }];
  await expect(fetchExecution(reader, p, e.runId, e.attempt, e.requestId)).rejects.toThrow();
  artifacts = [artifact];
  archiveBytes = zip("changed report");
  await expect(fetchExecution(reader, p, e.runId, e.attempt, e.requestId)).rejects.toThrow(
    "EXACT_ARTIFACT",
  );
});

test("live main/head/prospective merge drift invalidates the exact native target", async () => {
  const { verifyCurrentTarget } = await import("./source");
  const { e } = fixture();
  let main = e.target.base,
    head = e.target.authored.commit,
    merge = e.target.tested.commit;
  const reader: ProviderReader = {
    json: async (path) =>
      path.endsWith("git/ref/heads/main")
        ? { ref: "refs/heads/main", object: { sha: main } }
        : {
            number: e.target.pr,
            state: "open",
            merged: false,
            head: { sha: head, repo: { full_name: "park-kyungchan/agent-platform" } },
            base: {
              sha: e.target.base,
              ref: "main",
              repo: { full_name: "park-kyungchan/agent-platform" },
            },
            merge_commit_sha: merge,
          },
    archive: async () => new Uint8Array(),
  };
  await expect(verifyCurrentTarget(reader, e.target)).resolves.toBeUndefined();
  main = oid("9");
  await expect(verifyCurrentTarget(reader, e.target)).rejects.toThrow("MAIN_DRIFT");
  main = e.target.base;
  head = oid("9");
  await expect(verifyCurrentTarget(reader, e.target)).rejects.toThrow("HEAD_BASE_OR_MERGE_DRIFT");
  head = e.target.authored.commit;
  merge = oid("9");
  await expect(verifyCurrentTarget(reader, e.target)).rejects.toThrow("HEAD_BASE_OR_MERGE_DRIFT");
});
