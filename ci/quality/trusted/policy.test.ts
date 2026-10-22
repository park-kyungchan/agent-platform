import { describe, expect, test } from "bun:test";
import { fixture } from "./fixtures/execution-report";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  baseImage,
  command,
  qualifyTrustedExecution,
  sha256,
  trustedDigest,
  validateTarget,
  type ExecutionReport,
} from "./policy";
import { readReportArchive } from "./artifact";
import { verifyRun, verifyPolicyRef, type ProviderReader } from "./provider";
import { WorkerController, validateWorkerInspect, type WorkerSpec } from "./worker";
import { fixedProcess, ProcessOutputCapture, ProcessOutputLimitError, type ProcessResult, outputMetadata } from "./process";
import { qualityDriverDiagnostic, parseDriverDiagnostic, driverDiagnosticLimit, driverFailureLimit, encodeDriverFailure, parseDriverFailure } from "./driver-failure";

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
    "TMPDIR=/job/tmp",
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
        "/job": "rw,nosuid,nodev,exec,size=2147483648,uid=1000,gid=1000,mode=0700",
        "/tmp": "rw,nosuid,nodev,noexec,size=268435456,uid=1000,gid=1000,mode=0700",
      },
    },
    Mounts: [{ Type: "bind", Source: "/approved-policy", Destination: "/policy", RW: false }],
    State: { Running: false, Status: "created" },
  };
  expect(() => validateWorkerInspect(raw, spec, id)).not.toThrow();
  for (const mutate of [
    (r: typeof raw) => {
      r.Config.Env = env.filter((value) => value !== "TMPDIR=/job/tmp");
    },
    (r: typeof raw) => {
      r.Config.Env = env.map((value) => (value === "TMPDIR=/job/tmp" ? "TMPDIR=/tmp" : value));
    },
    (r: typeof raw) => {
      r.HostConfig.Tmpfs["/job"] = r.HostConfig.Tmpfs["/job"].replace(",exec,", ",noexec,");
    },
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

// Exercise the real build state machine with inert Docker/journal boundaries only.
class RegistrationWorker extends WorkerController {
  calls: { args: string[]; maxBytes: number }[] = [];
  records: Record<string, unknown>[] = [];
  order: string[] = [];
  registration: ProcessResult | Error = {
    code: 0,
    timedOut: false,
    stdout: Buffer.from("registered\n"),
    stderr: Buffer.alloc(0),
  };
  inspectCode = 0;
  buildFailure: Error | null = null;
  baseMetadata = { Os: "linux", Architecture: "amd64", RepoDigests: [baseImage] };
  builtMetadata = { Id: "sha256:" + h("f"), Os: "linux", Architecture: "amd64" };
  override async record(value: unknown): Promise<void> {
    const row = value as Record<string, unknown>;
    this.records.push(row);
    this.order.push(String(row.phase));
  }
  override async docker(args: string[], maxBytes = 4_000_000): Promise<ProcessResult> {
    this.calls.push({ args, maxBytes });
    this.order.push("docker:" + args[0]);
    if (args[0] === "pull") {
      if (this.registration instanceof Error) throw this.registration;
      return this.registration;
    }
    if (args[0] === "build" && this.buildFailure) throw this.buildFailure;
    if (args[0] === "build")
      return {
        code: 0,
        timedOut: false,
        stdout: Buffer.from(this.builtMetadata.Id + "\n"),
        stderr: Buffer.alloc(0),
      };
    if (args[0] === "image" && args[1] === "inspect")
      return {
        code: this.inspectCode,
        timedOut: false,
        stdout: Buffer.from(
          JSON.stringify([args[2] === baseImage ? this.baseMetadata : this.builtMetadata]),
        ),
        stderr: Buffer.alloc(0),
      };
    throw Error("Unexpected fixture Docker request");
  }
}
async function registrationWorker(): Promise<RegistrationWorker> {
  const { p } = fixture(),
    root = resolve(import.meta.dir, "../../..");
  p.workerRecipeSha256 = sha256(await readFile(root + "/ci/quality/trusted/worker.Dockerfile"));
  return new RegistrationWorker(p, root, "unused-inert-fixture-journal");
}

test("exact official base registration settles before build or Engine inspection", async () => {
  const w = await registrationWorker();
  expect(await w.build()).toBe(w.builtMetadata.Id);
  expect(w.calls.map(({ args }) => args[0])).toEqual(["pull", "build", "image", "image"]);
  expect(w.calls[0]).toEqual({
    args: ["pull", "--platform=linux/amd64", baseImage],
    maxBytes: 128000,
  });
  expect(w.order.slice(0, 5)).toEqual([
    "base-registration-intent",
    "docker:pull",
    "base-registration-result",
    "image-build-intent",
    "docker:build",
  ]);
  expect(w.calls[2]!.args).toEqual(["image", "inspect", baseImage]);
  expect(w.records[1]!.transport).toEqual(outputMetadata(w.registration as ProcessResult));
  expect(w.records[1]!.retryAllowed).toBe(false);
});

test("failed, timed-out or unknown base registration journals once and never builds or retries", async () => {
  for (const outcome of [
    {
      code: 17,
      timedOut: false,
      stdout: Buffer.alloc(0),
      stderr: Buffer.from("fixture-private-output"),
    },
    { code: 0, timedOut: true, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) },
    new Error("fixture-private-transport-error"),
  ]) {
    const w = await registrationWorker();
    w.registration = outcome;
    await expect(w.build()).rejects.toThrow(
      outcome instanceof Error ? outcome.message : "BASE_REGISTRATION_UNKNOWN_RECONCILE_REQUIRED",
    );
    expect(w.calls).toHaveLength(1);
    expect(w.order).toEqual([
      "base-registration-intent",
      "docker:pull",
      "base-registration-result",
    ]);
    expect(w.records[1]!.retryAllowed).toBe(false);
    expect(JSON.stringify(w.records)).not.toContain("fixture-private");
    if (outcome instanceof Error) {
      expect(w.records[1]!.transport).toBeNull();
      expect(w.records[1]!.errorSha256).toBe(sha256(outcome.message));
    } else expect(w.records[1]!.transport).toEqual(outputMetadata(outcome));
  }
});

test("registration does not replace official base metadata or swallow inspect failure", async () => {
  for (const invalid of ["inspect", "digest", "os", "arch"] as const) {
    const w = await registrationWorker();
    if (invalid === "inspect") w.inspectCode = 1;
    if (invalid === "digest") w.baseMetadata.RepoDigests = ["node@sha256:" + h("a")];
    if (invalid === "os") w.baseMetadata.Os = "other";
    if (invalid === "arch") w.builtMetadata.Architecture = "arm64";
    await expect(w.build()).rejects.toThrow(
      invalid === "inspect"
        ? "IMAGE_METADATA_REQUIRED"
        : "ACTUAL_OFFICIAL_BASE_AND_LOCAL_IMAGE_REQUIRED",
    );
    expect(w.calls.filter(({ args }) => args[0] === "pull")).toHaveLength(1);
    expect(w.records.some((row) => row.phase === "image-observed")).toBe(false);
  }
});

test("driver failure preserves finite OS/guard/phase data without raw failure contents", () => {
  const error = Object.assign(new TypeError("fixture-private-path-and-content"), {
    code: "EACCES",
    errno: -13,
  });
  let reads = 0;
  Object.defineProperty(error, "stack", {
    get: () => {
      reads++;
      return "fixture-private-stack";
    },
  });
  Object.defineProperty(error, "path", {
    get: () => {
      reads++;
      return "fixture-private-path";
    },
  });
  const encoded = encodeDriverFailure(error, "prepare", "copy-git");
  expect(Buffer.byteLength(encoded)).toBeLessThanOrEqual(driverFailureLimit);
  expect(encoded).not.toContain("fixture-private");
  expect(reads).toBe(0);
  const parsed = parseDriverFailure(Buffer.from(encoded), "prepare", [
    "/usr/local/bin/bun",
    "--no-env-file",
    "--no-install",
    "/policy/ci/quality/trusted/driver.ts",
    "prepare",
  ]);
  expect(parsed).toEqual({
    schema: "agent-platform.trusted-driver-failure.v1",
    driver: "prepare",
    phase: "copy-git",
    code: "TRUSTED_CI_DRIVER_FAILED",
    errorType: "TypeError",
    osCode: "EACCES",
    errno: -13,
  });
  expect(
    JSON.parse(
      encodeDriverFailure(
        new Error("TRUSTED_CI_FROZEN_INSTALL_FAILED"),
        "prepare",
        "frozen-install",
      ),
    ).code,
  ).toBe("TRUSTED_CI_FROZEN_INSTALL_FAILED");
  expect(
    JSON.parse(
      encodeDriverFailure(new Error("TRUSTED_CI_NOT_DECLARED"), "prepare", "frozen-install"),
    ).code,
  ).toBe("TRUSTED_CI_DRIVER_FAILED");
  const large = encodeDriverFailure(
    new Error("fixture-private".repeat(10000)),
    "prepare",
    "copy-git",
  );
  expect(Buffer.byteLength(large)).toBeLessThanOrEqual(driverFailureLimit);
  expect(large).not.toContain("fixture-private");
  const accessorError = {};
  for (const field of ["message", "code", "errno"])
    Object.defineProperty(accessorError, field, {
      get: () => {
        reads++;
        return "fixture-private-getter";
      },
    });
  expect(encodeDriverFailure(accessorError, "prepare", "copy-git")).not.toContain(
    "fixture-private",
  );
  expect(reads).toBe(0);
});

test("driver diagnostic parser rejects mixed, truncated, forged and wrong-boundary data", () => {
  const argv = [
    "/usr/local/bin/bun",
    "--no-env-file",
    "--no-install",
    "/policy/ci/quality/trusted/driver.ts",
    "prepare",
  ];
  const encoded = encodeDriverFailure(
    new Error("TRUSTED_CI_FROZEN_INSTALL_FAILED"),
    "prepare",
    "frozen-install",
  );
  const valid = JSON.parse(encoded) as Record<string, unknown>;
  for (const value of [
    { ...valid, schema: "other" },
    { ...valid, phase: "audit" },
    { ...valid, driver: "audit" },
    { ...valid, code: "TRUSTED_CI_NOT_DECLARED" },
    { ...valid, osCode: "fixture-private-os-data" },
    { ...valid, errno: 4096 },
    { ...valid, errorType: "fixture-private-type" },
    { ...valid, success: true },
  ])
    expect(
      parseDriverFailure(Buffer.from(JSON.stringify(value) + "\n"), "prepare", argv),
    ).toBeNull();
  for (const bytes of [
    Buffer.from("unrelated-stderr\n" + encoded),
    Buffer.from(encoded + encoded),
    Buffer.from(encoded.slice(0, -3)),
    Buffer.from(" ".repeat(driverFailureLimit + 1)),
    Buffer.from(encoded.replace('{"schema":', '{"schema":"duplicate","schema":')),
    Buffer.concat([Buffer.from(encoded), Buffer.from([255])]),
    Buffer.concat([Buffer.from([239, 187, 191]), Buffer.from(encoded)]),
  ])
    expect(parseDriverFailure(bytes, "prepare", argv)).toBeNull();
  expect(parseDriverFailure(Buffer.from(encoded), "audit", argv)).toBeNull();
  expect(parseDriverFailure(Buffer.from(encoded), "prepare", ["untrusted-command"])).toBeNull();
});


test("fixedProcess counts raw combined bytes at zero/below/equal/above its explicit cap", async () => {
  for (const size of [0, 63, 64, 65]) {
    for (const stream of ["stdout", "stderr", "both"] as const) {
      const first = stream === "stderr" ? 0 : stream === "both" ? Math.floor(size / 2) : size;
      const argv = [process.execPath, "-e", `import {writeSync} from 'node:fs';writeSync(1,Buffer.alloc(${first},97));writeSync(2,Buffer.alloc(${size - first},98));`];
      if (size <= 64) {
        const result = await fixedProcess(argv, { cwd: "/tmp", maxBytes: 64, timeoutMs: 5000 });
        expect(result.code).toBe(0);
        expect(result.timedOut).toBe(false);
        expect(result.stdout.length + result.stderr.length).toBe(size);
      } else {
        let failure: unknown;
        try { await fixedProcess(argv, { cwd: "/tmp", maxBytes: 64, timeoutMs: 5000 }); }
        catch (error) { failure = error; }
        expect(failure).toBeInstanceOf(ProcessOutputLimitError);
        const metadata = (failure as ProcessOutputLimitError).metadata;
        expect(metadata).toMatchObject({ maxBytes: 64, retainedBytes: 64, limitExceeded: true, captureComplete: false, hashScope: "observed-raw-bytes", retryAllowed: false });
        expect(metadata.observedCombinedBytes).toBeGreaterThan(64);
        expect(metadata.observedCombinedBytes).toBe(metadata.observedStdoutBytes + metadata.observedStderrBytes);
        expect(JSON.stringify(metadata)).not.toContain('"stdout":');
      }
    }
  }
}, 30_000);

test("fixedProcess preserves raw split UTF8, invalid budgets, nonzero, spawn and timeout boundaries", async () => {
  const argv = [process.execPath, "-e", "import {writeSync} from 'node:fs';writeSync(1,Buffer.from([240]));writeSync(1,Buffer.from([159,152,128]));writeSync(2,Buffer.from([255]));"];
  const result = await fixedProcess(argv, { cwd: "/tmp", maxBytes: 5 });
  expect(result.stdout).toEqual(Buffer.from([240,159,152,128]));
  expect(result.stderr).toEqual(Buffer.from([255]));
  expect(outputMetadata(result).stdoutSha256).toBe(sha256(Buffer.from([240,159,152,128])));
  for (const maxBytes of [0, -1, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1])
    await expect(fixedProcess(["/missing-fixture"], { cwd: "/tmp", maxBytes })).rejects.toThrow("OUTPUT_BUDGET_INVALID");
  expect((await fixedProcess([process.execPath, "-e", "process.exit(17)"], { cwd: "/tmp" })).code).toBe(17);
  await expect(fixedProcess(["/missing-fixture-executable"], { cwd: "/tmp" })).rejects.toThrow();
  const timed = await fixedProcess([process.execPath, "-e", "setTimeout(()=>{},5000)"], { cwd: "/tmp", timeoutMs: 50 });
  expect(timed.timedOut).toBe(true);
  const capture = new ProcessOutputCapture(1);
  capture.observe("stdout", Buffer.from([255,254]));
  expect(capture.failure(true, null, "SIGKILL").metadata).toMatchObject({ timedOut: true, captureComplete: false, observedStdoutSha256: sha256(Buffer.from([255,254])) });
});

function diagnosticStatic(status: "pass" | "fail") {
  return { schema: "agent-platform.trusted-static-quality.v1", status,
    checks: ["format", "typed-lint", "architecture", "workflow-security", "maintained-secrets"].map((id, i) => ({id, status: i === 0 ? status : "pass"})),
    tests: 94, files: 200, nativePermissionGranted: false };
}
test("driver diagnostics preserve the static projection and reject noncanonical/private/forged envelopes", () => {
  const value = diagnosticStatic("fail"), argv = command("static-quality");
  const encoded = JSON.stringify(value) + "\n";
  expect(parseDriverDiagnostic(Buffer.from(encoded), "checker", argv, [])).toEqual(value);
  expect(parseDriverDiagnostic(Buffer.from(JSON.stringify(diagnosticStatic("pass")) + "\n"), "checker", argv, [])).toMatchObject({ status: "pass" });
  for (const text of [
    encoded.slice(0,-1), "\ufeff"+encoded, encoded+"private\n", encoded.replace('"status":"fail"','"status":"fail","status":"fail"'),
    JSON.stringify({...value, secret:"PRIVATE-DIAGNOSTIC"})+"\n", JSON.stringify({...value, checks:[null]})+"\n",
    JSON.stringify({...value, status:"pass"})+"\n", "x".repeat(driverDiagnosticLimit+1),
  ]) expect(parseDriverDiagnostic(Buffer.from(text), "checker", argv, [])).toBeNull();
  expect(parseDriverDiagnostic(Buffer.from([255]), "checker", argv, [])).toBeNull();
  expect(parseDriverDiagnostic(Buffer.from(encoded), "candidate", argv, [])).toBeNull();
  expect(parseDriverDiagnostic(Buffer.from(encoded), "checker", ["untrusted"], [])).toBeNull();
});

test("candidate failure summaries bind counts, hashes and admitted locations without retaining raw details", () => {
  const path = "ci/quality/failure-locations.test.ts";
  const report = { checks: [{id:"quality-negative-fixtures",status:"fail" as const,details:{ error:"PRIVATE-DIAGNOSTIC", failureDiagnostics:{locations:[{path,line:42,column:7}],total:1,omitted:0,diagnosticComplete:true}}}] };
  const diagnostic = qualityDriverDiagnostic(report, 500, h("a"));
  const summary = {status:"fail",reportSha256:h("a"),checks:diagnostic.checks,externalAuditSha256:h("b")};
  const encode = (d: unknown) => Buffer.from(JSON.stringify(summary)+"\n"+JSON.stringify(d)+"\n");
  expect(parseDriverDiagnostic(encode(diagnostic), "candidate", command("candidate-quality"), [path])).toEqual(diagnostic);
  expect(JSON.stringify(diagnostic)).not.toContain("PRIVATE-DIAGNOSTIC");
  for (const mutation of [
    {...diagnostic,status:"pass"}, {...diagnostic,failed:0}, {...diagnostic,reference:"arbitrary-store"},
    {...diagnostic,failureLocations:[{path:"../private.test.ts",line:1,column:1}]},
    {...diagnostic,checks:[null]}, {...diagnostic,checks:[{id:"arbitrary",status:"fail"}]}, {...diagnostic,extra:true},
  ]) expect(parseDriverDiagnostic(encode(mutation), "candidate", command("candidate-quality"), [path])).toBeNull();
  expect(parseDriverDiagnostic(encode(diagnostic), "candidate", command("candidate-quality"), [])).toBeNull();
});

class FailureJournalWorker extends WorkerController {
  records: Record<string, unknown>[] = [];
  calls: string[][] = [];
  failAt = "";
  cleanupFails = false;
  failure = new ProcessOutputCapture(1);
  diagnostic = diagnosticStatic("fail");
  constructor() {
    super(fixture().p, resolve(import.meta.dir,"../../.."), "unused-inert-journal");
    this.failure.observe("stdout", Buffer.from("PRIVATE-DIAGNOSTIC"));
  }
  override async record(value: unknown) { this.records.push(value as Record<string, unknown>); }
  override async attach(): Promise<ProcessResult> {
    if (this.failAt === "attach") throw this.failure.failure(false, null, "SIGKILL");
    return { code:1, timedOut:false, stdout:Buffer.from(JSON.stringify(this.diagnostic)+"\n"), stderr:Buffer.from(encodeDriverFailure(new Error("TRUSTED_CI_TRUSTED_STATIC_CHECK_FAILED"),"static-quality","static-quality")) };
  }
  override async docker(args: string[]): Promise<ProcessResult> {
    this.calls.push(args);
    if (args[0] === this.failAt || (args[0] === "kill" && this.cleanupFails))
      throw this.failure.failure(false, null, "SIGKILL");
    if (args[0] === "create") return {code:0,timedOut:false,stdout:Buffer.from(h("a")+"\n"),stderr:Buffer.alloc(0)};
    if (args[0] === "kill") return {code:0,timedOut:false,stdout:Buffer.alloc(0),stderr:Buffer.alloc(0)};
    if (args[0] === "inspect") {
      const started = this.records.some((row) => row.phase === "start-intent");
      const environment = ["HOME=/job/home","PATH=/usr/local/bin:/usr/bin:/bin","LANG=C.UTF-8","CI=true","RUNNER_TEMP=/job/tmp","TMPDIR=/job/tmp"];
      const config = {Id:h("a"),Image:"sha256:"+h("b"),Config:{User:"1000:1000",WorkingDir:"/job",Env:environment,Entrypoint:["/usr/bin/env","-i",...environment],Cmd:command("static-quality")},
        HostConfig:{NetworkMode:"none",Privileged:false,ReadonlyRootfs:true,CapDrop:["ALL"],SecurityOpt:["no-new-privileges:true"],PidMode:"",IpcMode:"private",Memory:2147483648,NanoCpus:2000000000,PidsLimit:256,Tmpfs:{"/job":"rw,nosuid,nodev,exec,size=2147483648,uid=1000,gid=1000,mode=0700","/tmp":"rw,nosuid,nodev,noexec,size=268435456,uid=1000,gid=1000,mode=0700"}},
        Mounts:[],State:started?{Running:false,Status:"exited",OOMKilled:false,ExitCode:1}:{Running:false,Status:"created"}};
      return {code:0,timedOut:false,stdout:Buffer.from(JSON.stringify([config])),stderr:Buffer.alloc(0)};
    }
    throw Error("Unexpected inert command");
  }
}
const failureSpec: WorkerSpec = {role:"checker",imageId:"sha256:"+h("b"),network:"none",mounts:[],argv:command("static-quality")};
test("same-occurrence create/inspect/attach cap failures journal before cleanup and never retry", async () => {
  for (const phase of ["create","inspect","attach"]) {
    const worker = new FailureJournalWorker(); worker.failAt = phase;
    await expect(worker.run(fixture().e.target.authored, failureSpec)).rejects.toThrow("PROCESS_OUTPUT_LIMIT");
    const failure = worker.records.find((r) => r.failureKind === "output-limit")!;
    expect(failure).toMatchObject({captureComplete:false,retryAllowed:false,observedCombinedBytes:18});
    expect(failure.occurrence).toBe(worker.records[0]!.occurrence);
    expect(worker.calls.filter((args) => args[0] === "create")).toHaveLength(1);
    expect(worker.calls.filter((args) => args[0] === "kill")).toHaveLength(phase === "attach" ? 1 : 0);
    expect(JSON.stringify(worker.records)).not.toContain("PRIVATE-DIAGNOSTIC");
  }
  const worker = new FailureJournalWorker(); worker.failAt = "attach"; worker.cleanupFails = true;
  await expect(worker.run(fixture().e.target.authored, failureSpec)).rejects.toThrow("PROCESS_OUTPUT_LIMIT");
  expect(worker.records.at(-1)).toMatchObject({phase:"worker-cleanup-result",cleanupUnknown:true});
});
test("actual worker result journals static failure diagnostics and preserves its failed process", async () => {
  const worker = new FailureJournalWorker();
  const result = await worker.run(fixture().e.target.authored, failureSpec);
  expect(result.observation.exitCode).toBe(1);
  expect(worker.records.at(-1)).toMatchObject({phase:"result",diagnosticOnly:true,driverDiagnostic:diagnosticStatic("fail"),driverFailure:{code:"TRUSTED_CI_TRUSTED_STATIC_CHECK_FAILED"}});
});

test("output-capped registration/build preserve their correlated intent and no-build/no-retry fences", async () => {
  const capture = new ProcessOutputCapture(1);
  capture.observe("stderr",Buffer.from("PRIVATE-BUILD-OUTPUT"));
  for (const stage of ["registration","build"]) {
    const worker = await registrationWorker();
    const error = capture.failure(false,null,"SIGKILL");
    if(stage === "registration") worker.registration=error;
    else worker.buildFailure=error;
    await expect(worker.build()).rejects.toBe(error);
    const diagnostic=worker.records.find((r)=>r.failureKind==="output-limit")!;
    expect(diagnostic).toMatchObject({captureComplete:false,retryAllowed:false,observedStderrBytes:Buffer.byteLength("PRIVATE-BUILD-OUTPUT")});
    expect(diagnostic.occurrence).toBe(worker.records[0]!.occurrence);
    expect(worker.calls.filter(({args})=>args[0]==="pull")).toHaveLength(1);
    expect(worker.calls.filter(({args})=>args[0]==="build")).toHaveLength(stage === "registration" ? 0 : 1);
    expect(worker.calls.some(({args})=>args[0]==="image")).toBe(false);
    expect(JSON.stringify(worker.records)).not.toContain("PRIVATE-BUILD-OUTPUT");
  }
});


test("candidate diagnostic plus its existing summary stays within the combined display budget", () => {
  const locations=Array.from({length:16},(_,i)=>({path:`ci/${"x".repeat(470)}${i}.test.ts`,line:i+1,column:1}));
  const report={checks:[{id:"quality-negative-fixtures",status:"fail" as const,details:{failureDiagnostics:{locations,total:16,omitted:0,diagnosticComplete:true}}}]};
  const summary={status:"fail",reportSha256:h("a"),checks:report.checks.map(({id,status})=>({id,status})),externalAuditSha256:h("b")};
  const prefix=JSON.stringify(summary)+"\n";
  const diagnostic=qualityDriverDiagnostic(report,1000,h("a"),driverDiagnosticLimit-Buffer.byteLength(prefix));
  const bytes=Buffer.from(prefix+JSON.stringify(diagnostic)+"\n");
  expect(bytes.length).toBeLessThanOrEqual(driverDiagnosticLimit);
  expect(diagnostic.diagnosticComplete).toBe(false);
  expect(diagnostic.locationOmitted).toBeGreaterThan(0);
  expect(parseDriverDiagnostic(bytes,"candidate",command("candidate-quality"),locations.map((l)=>l.path))).toEqual(diagnostic);
});
