import { operationalTimeouts } from "../operational-timeouts";
import { randomUUID } from "node:crypto";
import { appendFile, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  baseImage,
  insist,
  object,
  same,
  sha256,
  trustedDigest,
  type Subject,
  type TrustedPolicy,
  type WorkerObservation,
} from "./policy";
import { fixedProcess, outputMetadata, processFailureMetadata, type ProcessResult } from "./process";
import { parseDriverFailure, parseDriverDiagnostic } from "./driver-failure";

export interface WorkerMount {
  source: string;
  target: string;
  readOnly: boolean;
  type: "bind" | "volume";
}
export interface WorkerSpec {
  role: WorkerObservation["role"];
  imageId: string;
  network: "bridge" | "none";
  mounts: WorkerMount[];
  argv: string[];
  stdin?: Uint8Array;
  dependencyTreeSha256?: string;
}
const absent = (x: unknown): boolean =>
  x === null || x === undefined || (Array.isArray(x) && x.length === 0);
const env = [
  "HOME=/job/home",
  "PATH=/usr/local/bin:/usr/bin:/bin",
  "LANG=C.UTF-8",
  "CI=true",
  "RUNNER_TEMP=/job/tmp",
  "TMPDIR=/job/tmp",
];
/** Validate actual daemon state before any candidate code can start. No report flags are accepted. */
export function validateWorkerInspect(raw: unknown, s: WorkerSpec, id: string): void {
  const i = object(raw),
    host = object(i.HostConfig),
    config = object(i.Config),
    observedEnv = config.Env;
  insist(
    i.Id === id &&
      i.Image === s.imageId &&
      config.User === "1000:1000" &&
      config.WorkingDir === "/job" &&
      Array.isArray(observedEnv) &&
      env.every((v) => observedEnv.includes(v)) &&
      observedEnv.every(
        (v: string) =>
          env.includes(v) || /^NODE_VERSION=22\.23\.3$/.test(v) || /^YARN_VERSION=[0-9.]+$/.test(v),
      ),
    "ACTUAL_WORKER_IDENTITY_REQUIRED",
  );
  insist(
    same(config.Entrypoint, ["/usr/bin/env", "-i", ...env]) && same(config.Cmd, s.argv),
    "ACTUAL_WORKER_COMMAND_REQUIRED",
  );
  insist(
    host.NetworkMode === s.network &&
      host.Privileged === false &&
      host.ReadonlyRootfs === true &&
      same(host.CapDrop, ["ALL"]) &&
      same(host.SecurityOpt, ["no-new-privileges:true"]),
    "ACTUAL_WORKER_ISOLATION_REQUIRED",
  );
  insist(
    host.PidMode === "" &&
      host.IpcMode === "private" &&
      !host.UsernsMode &&
      absent(host.Devices) &&
      absent(host.DeviceRequests) &&
      absent(host.CapAdd) &&
      absent(host.VolumesFrom) &&
      absent(host.Links) &&
      absent(host.ExtraHosts) &&
      absent(host.GroupAdd) &&
      !host.PublishAllPorts,
    "ACTUAL_WORKER_HOST_ACCESS_REFUSED",
  );
  insist(
    host.Memory === 2147483648 && host.NanoCpus === 2000000000 && host.PidsLimit === 256,
    "WORKER_RESOURCE_LIMIT_REQUIRED",
  );
  insist(
    same(object(host.Tmpfs), {
      "/job": "rw,nosuid,nodev,exec,size=2147483648,uid=1000,gid=1000,mode=0700",
      "/tmp": "rw,nosuid,nodev,noexec,size=268435456,uid=1000,gid=1000,mode=0700",
    }),
    "PRIVATE_TMPFS_REQUIRED",
  );
  insist(
    Array.isArray(i.Mounts) && i.Mounts.length === s.mounts.length,
    "WORKER_MOUNT_DENOMINATOR_REQUIRED",
  );
  for (const m of s.mounts) {
    const hits = (i.Mounts as unknown[]).map(object).filter((x) => x.Destination === m.target);
    insist(
      hits.length === 1 &&
        hits[0]!.Type === m.type &&
        hits[0]!.RW === !m.readOnly &&
        (m.type === "volume" ? hits[0]!.Name === m.source : hits[0]!.Source === m.source),
      "ACTUAL_WORKER_MOUNT_REQUIRED",
    );
    insist(
      (["/policy", "/input", "/prepared"].includes(m.target) && m.readOnly) ||
        (m.target === "/prepared" && m.type === "volume" && s.role === "prepare"),
      "FORBIDDEN_WORKER_MOUNT",
    );
  }
  insist(
    object(i.State).Running === false && object(i.State).Status === "created",
    "UNSTARTED_WORKER_REQUIRED",
  );
}
export class WorkerController {
  constructor(
    readonly policy: TrustedPolicy,
    readonly policyRoot: string,
    readonly journal: string,
  ) {}
  async docker(args: string[], maxBytes = 4_000_000) {
    return fixedProcess(["/usr/bin/docker", ...args], {
      cwd: this.policyRoot,
      timeoutMs: operationalTimeouts.processMs,
      maxBytes,
    });
  }
  async record(value: unknown): Promise<void> {
    await appendFile(this.journal, JSON.stringify(value) + "\n", { mode: 0o600 });
  }
  private async failureRecord(value: Record<string, unknown>, error: unknown): Promise<void> {
    try {
      await this.record({ ...value, transport: null, ...processFailureMetadata(error) });
    } catch {
      // A failed journal cannot establish retention. Preserve the original transport error.
    }
  }
  private async observedDocker(
    phase: string,
    context: Record<string, unknown>,
    args: string[],
    maxBytes: number,
  ): Promise<ProcessResult> {
    try {
      return await this.docker(args, maxBytes);
    } catch (error) {
      await this.failureRecord({ phase, ...context }, error);
      throw error;
    }
  }
  async attach(containerId: string, stdin?: Uint8Array): Promise<ProcessResult> {
    return fixedProcess(["/usr/bin/docker", "start", "--attach", "--interactive", containerId], {
      cwd: this.policyRoot,
      stdin,
      timeoutMs: operationalTimeouts.workerMs,
      maxBytes: 8_000_000,
    });
  }
  private async stopKnownWorker(occurrence: string, containerId: string): Promise<void> {
    try {
      const result = await this.docker(["kill", "--signal", "KILL", containerId], 4096);
      await this.record({
        phase: "worker-cleanup-result",
        occurrence,
        containerId,
        transport: outputMetadata(result),
        cleanupUnknown: result.code !== 0 || result.timedOut,
        retryAllowed: false,
      });
    } catch (error) {
      await this.failureRecord({
        phase: "worker-cleanup-result", occurrence, containerId, cleanupUnknown: true,
      }, error);
    }
  }
  async run(
    subject: Subject,
    spec: WorkerSpec,
  ): Promise<{ observation: WorkerObservation; stdout: Buffer }> {
    insist(/^sha256:[a-f0-9]{64}$/.test(spec.imageId), "HOST_OBSERVED_IMAGE_ID_REQUIRED");
    const occurrence = randomUUID(),
      name = "trusted-source-" + occurrence;
    const args = [
      "create",
      "--name",
      name,
      "--user",
      "1000:1000",
      "--workdir",
      "/job",
      "--network",
      spec.network,
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges:true",
      "--read-only",
      "--ipc",
      "private",
      "--memory",
      "2147483648",
      "--cpus",
      "2",
      "--pids-limit",
      "256",
      "--tmpfs",
      "/job:rw,nosuid,nodev,exec,size=2147483648,uid=1000,gid=1000,mode=0700",
      "--tmpfs",
      "/tmp:rw,nosuid,nodev,noexec,size=268435456,uid=1000,gid=1000,mode=0700",
      "--interactive",
    ];
    for (const value of env) args.push("--env", value);
    for (const m of spec.mounts) {
      insist(
        m.type === "volume"
          ? /^trusted-source-[a-f0-9-]{36}$/.test(m.source)
          : resolve(m.source) === m.source && !m.source.includes(","),
        "EXACT_PRIVATE_MOUNT_REQUIRED",
      );
      args.push(
        "--mount",
        `type=${m.type},source=${m.source},target=${m.target}${m.readOnly ? ",readonly" : ""}`,
      );
    }
    args.push(spec.imageId, ...spec.argv);
    await this.record({
      phase: "create-intent",
      occurrence,
      name,
      subject,
      spec: { ...spec, stdin: undefined },
      stdinSha256: spec.stdin ? sha256(spec.stdin) : null,
    });
    const created = await this.observedDocker("create-result", { occurrence, name }, args, 4096);
    await this.record({
      phase: "create-result",
      occurrence,
      name,
      transport: outputMetadata(created),
    });
    // Any timeout/error leaves the named occurrence fenced. Never retry create/start.
    insist(
      created.code === 0 && !created.timedOut && /^[a-f0-9]{64}\n$/.test(created.stdout.toString()),
      "CREATE_UNKNOWN_RECONCILE_REQUIRED",
    );
    const containerId = created.stdout.toString().trim();
    await this.record({ phase: "created", occurrence, containerId });
    const inspected = await this.observedDocker("inspect-result", { occurrence, containerId }, ["inspect", containerId], 128000);
    await this.record({
      phase: "inspect-result",
      occurrence,
      containerId,
      transport: outputMetadata(inspected),
    });
    insist(inspected.code === 0 && !inspected.timedOut, "INSPECT_UNKNOWN_RECONCILE_REQUIRED");
    const records: unknown = JSON.parse(inspected.stdout.toString());
    insist(Array.isArray(records) && records.length === 1, "EXACT_CONTAINER_INSPECT_REQUIRED");
    validateWorkerInspect(records[0], spec, containerId);
    const startedAt = new Date().toISOString();
    await this.record({
      phase: "start-intent",
      occurrence,
      containerId,
      inspectSha256: sha256(inspected.stdout),
    });
    let result;
    try {
      result = await this.attach(containerId, spec.stdin);
    } catch (error) {
      await this.failureRecord({ phase: "worker-transport-failure", occurrence, containerId }, error);
      await this.stopKnownWorker(occurrence, containerId);
      throw error;
    }
    if (result.timedOut) {
      // Stop only this known occurrence; a failed cleanup is journaled and remains unknown.
      await this.stopKnownWorker(occurrence, containerId);
    }
    const final = await this.observedDocker("final-inspect-result", { occurrence, containerId }, ["inspect", containerId], 128000);
    insist(final.code === 0 && !final.timedOut, "FINAL_INSPECT_UNKNOWN_RECONCILE_REQUIRED");
    const state = object(
      (JSON.parse(final.stdout.toString()) as Record<string, unknown>[])[0]!.State,
    );
    insist(
      state.Running === false &&
        typeof state.OOMKilled === "boolean" &&
        Number.isInteger(state.ExitCode),
      "STOPPED_WORKER_REQUIRED",
    );
    const observation: WorkerObservation = {
      occurrence,
      containerId,
      imageId: spec.imageId,
      baseImage,
      recipeSha256: this.policy.workerRecipeSha256,
      role: spec.role,
      network: spec.network,
      inspectSha256: sha256(inspected.stdout),
      inputSha256: trustedDigest(subject),
      dependencyTreeSha256: spec.dependencyTreeSha256 ?? null,
      toolPinsSha256: this.policy.toolPinsSha256,
      startedAt,
      finishedAt: new Date().toISOString(),
      exitCode: Number(state.ExitCode),
      oomKilled: state.OOMKilled,
      timedOut: result.timedOut,
      stdoutSha256: sha256(result.stdout),
      stderrSha256: sha256(result.stderr),
      boundaryVerified: true,
    };
    let driverDiagnostic: Record<string, unknown> | null = null;
    let diagnosticReason = "missing-or-invalid";
    try {
      const coverage = object(JSON.parse(await readFile(this.policyRoot + "/ci/quality/trusted/coverage.json", "utf8")));
      const knownPaths = Array.isArray(coverage.legacyTests)
        ? coverage.legacyTests.map(object).map((row) => row.path).filter((path): path is string => typeof path === "string")
        : [];
      driverDiagnostic = parseDriverDiagnostic(result.stdout, spec.role, spec.argv, knownPaths);
      if (driverDiagnostic) diagnosticReason = "validated-diagnostic-only";
    } catch {
      diagnosticReason = "collection-failed";
    }
    await this.record({
      phase: "result",
      observation,
      transport: outputMetadata(result),
      finalInspectSha256: sha256(final.stdout),
      diagnosticOnly: true,
      driverDiagnostic,
      diagnosticReason,
      diagnosticStdoutSha256: sha256(result.stdout),
      driverFailure:
        observation.exitCode !== 0 || result.code !== 0 || result.timedOut
          ? parseDriverFailure(result.stderr, spec.role, spec.argv)
          : null,
    });
    insist(
      result.code === observation.exitCode || observation.exitCode !== 0,
      "TRANSPORT_EXIT_MISMATCH",
    );
    return { observation, stdout: result.stdout };
  }
  async build(): Promise<string> {
    const recipe = await readFile(this.policyRoot + "/ci/quality/trusted/worker.Dockerfile");
    insist(
      sha256(recipe) === this.policy.workerRecipeSha256 &&
        recipe.toString().startsWith("FROM " + baseImage + "\n"),
      "PINNED_WORKER_RECIPE_REQUIRED",
    );
    const occurrence = randomUUID();
    await this.record({
      phase: "base-registration-intent",
      occurrence,
      baseImage,
      platform: "linux/amd64",
      retryAllowed: false,
    });
    let registration: ProcessResult;
    try {
      registration = await this.docker(["pull", "--platform=linux/amd64", baseImage], 128000);
    } catch (error) {
      await this.failureRecord({ phase: "base-registration-result", occurrence, baseImage }, error);
      throw error;
    }
    await this.record({
      phase: "base-registration-result",
      occurrence,
      baseImage,
      transport: outputMetadata(registration),
      retryAllowed: false,
    });
    insist(
      registration.code === 0 && !registration.timedOut,
      "BASE_REGISTRATION_UNKNOWN_RECONCILE_REQUIRED",
    );
    await this.record({ phase: "image-build-intent", occurrence, recipeSha256: sha256(recipe), baseImage });
    const result = await this.observedDocker(
      "image-build-result",
      { occurrence, baseImage },
      [
        "build",
        "--pull=false",
        "--network=default",
        "--platform=linux/amd64",
        "--quiet",
        "--file",
        this.policyRoot + "/ci/quality/trusted/worker.Dockerfile",
        this.policyRoot,
      ],
      128000,
    );
    insist(
      sha256(await readFile(this.policyRoot + "/ci/quality/trusted/worker.Dockerfile")) ===
        sha256(recipe),
      "RECIPE_CHANGED_DURING_BUILD",
    );
    await this.record({
      phase: "image-build-result",
      occurrence,
      transport: outputMetadata(result),
      retryAllowed: false,
    });
    insist(result.code === 0 && !result.timedOut, "IMAGE_BUILD_UNKNOWN_RECONCILE_REQUIRED");
    const id = result.stdout.toString().trim();
    const base = await this.observedDocker("base-image-inspect-failure", { occurrence, baseImage }, ["image", "inspect", baseImage], 128000);
    const built = await this.observedDocker("built-image-inspect-failure", { occurrence, imageId: id }, ["image", "inspect", id], 128000);
    insist(
      base.code === 0 && built.code === 0 && !base.timedOut && !built.timedOut,
      "IMAGE_METADATA_REQUIRED",
    );
    const baseInfo = object((JSON.parse(base.stdout.toString()) as unknown[])[0]);
    const builtInfo = object((JSON.parse(built.stdout.toString()) as unknown[])[0]);
    insist(
      baseInfo.Os === "linux" &&
        baseInfo.Architecture === "amd64" &&
        Array.isArray(baseInfo.RepoDigests) &&
        baseInfo.RepoDigests.some((d: string) =>
          d.endsWith("@sha256:d05fa967fa1bbe6ba4fb48f9d4c55b5e6a8c8300aedb6b99a11c248a877d7bee"),
        ) &&
        builtInfo.Id === id &&
        builtInfo.Os === "linux" &&
        builtInfo.Architecture === "amd64",
      "ACTUAL_OFFICIAL_BASE_AND_LOCAL_IMAGE_REQUIRED",
    );
    await this.record({
      phase: "image-observed",
      imageId: id,
      baseMetadataSha256: sha256(base.stdout),
      builtMetadataSha256: sha256(built.stdout),
      registryPublicationRequired: false,
    });
    insist(/^sha256:[a-f0-9]{64}$/.test(id), "BUILT_IMAGE_ID_REQUIRED");
    return id;
  }
}
