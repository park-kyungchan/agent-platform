import { operationalTimeouts } from "../operational-timeouts";
import { prepareProbeEnvironment } from "./probe-environment";
import { loadBehaviorModule } from "./scenario-adapters";
import { callBehavior } from "./semantic";
import { cp, mkdir, readFile, writeFile, readdir, lstat } from "node:fs/promises";
import { join } from "node:path";
import { installPinnedTool, readToolPins } from "../native-tools";
import { runQuality, runTrustedStaticQuality } from "../run";
import { runTrustedSourceChecks } from "../../source-check";
import { verifyRuntimeLayout } from "../../verify-runtime-layout";
import { fixedProcess } from "./process";
import { insist, object, safePath, sha256, trustedDigest } from "./policy";
import { encodeDriverFailure, type DriverFailurePhase } from "./driver-failure";

const source = "/prepared/source",
  policy = "/policy";
let phase: DriverFailurePhase = "startup";
async function input(): Promise<Record<string, unknown>> {
  const bytes = await Bun.stdin.bytes();
  insist(bytes.length <= 128000, "WORKER_INPUT_LIMIT");
  return object(JSON.parse(new TextDecoder().decode(bytes)));
}
async function copyData(from: string, to: string, installInputsOnly: boolean): Promise<void> {
  for (const e of await readdir(from, { withFileTypes: true })) {
    if ([".git", "node_modules"].includes(e.name)) continue;
    const a = join(from, e.name),
      b = join(to, e.name);
    insist(!e.isSymbolicLink(), "SOURCE_SYMLINK_REFUSED");
    if (e.isDirectory()) {
      await mkdir(b, { recursive: true });
      await copyData(a, b, installInputsOnly);
    } else if (
      !installInputsOnly ||
      e.name === "package.json" ||
      e.name === "bun.lock" ||
      e.name.endsWith(".tgz")
    ) {
      insist((await lstat(a)).isFile(), "REGULAR_SOURCE_REQUIRED");
      await cp(a, b);
    }
  }
}
async function install(): Promise<void> {
  phase = "copy-install-inputs";
  await mkdir(source, { recursive: true });
  await copyData("/input", source, true);
  // The network phase has manifests/locks/local archives only: no bunfig, .npmrc, preload or source.
  phase = "frozen-install";
  await writeFile(source + "/.npmrc", "registry=https://registry.npmjs.org/\n");
  const r = await fixedProcess(
    [process.execPath, "--no-env-file", "install", "--frozen-lockfile", "--ignore-scripts"],
    {
      cwd: source,
      env: { HOME: "/job/home", BUN_INSTALL_CACHE_DIR: "/prepared/bun-cache" },
      timeoutMs: operationalTimeouts.frozenInstallMs,
    },
  );
  insist(r.code === 0 && !r.timedOut, "FROZEN_INSTALL_FAILED");
  phase = "install-tools";
  for (const pin of await readToolPins(policy)) await installPinnedTool(pin, "/prepared/tools");
  phase = "copy-full-source";
  await copyData("/input", source, false);
  // Git metadata is a fresh token-free P-created full-history checkout, never user native state.
  phase = "copy-git";
  await cp("/input/.git", source + "/.git", { recursive: true });
  phase = "prepare-result";
  console.log(
    JSON.stringify({
      status: "pass",
      preparation: "credential-free-frozen-inputs",
      dependencyInputsSha256: object(await input()).dependencyInputsSha256,
    }),
  );
}
async function materialize(): Promise<string> {
  const root = "/job/source";
  await cp(source, root, { recursive: true, dereference: false });
  await cp("/prepared/bun-cache", "/job/bun-cache", { recursive: true, dereference: false });
  await cp("/prepared/tools", "/job/home/.cache/agent-platform/ci-tools", {
    recursive: true,
    dereference: false,
  });
  process.env.BUN_INSTALL_CACHE_DIR = "/job/bun-cache";
  return root;
}
async function main(): Promise<void> {
  // The only foreign-owned Git checkout admitted here is the read-only immutable P mount.
  process.env.GIT_CONFIG_NOSYSTEM = "1";
  process.env.GIT_CONFIG_GLOBAL = "/dev/null";
  process.env.GIT_CONFIG_COUNT = "1";
  process.env.GIT_CONFIG_KEY_0 = "safe.directory";
  process.env.GIT_CONFIG_VALUE_0 = "/policy";
  await mkdir("/job/home", { recursive: true });
  await mkdir("/job/tmp", { recursive: true });
  const id = process.argv[2];
  if (id === "prepare") return install();
  if (id === "audit") {
    phase = "audit-inputs";
    const root = "/job/audit";
    await mkdir(root);
    await copyData(source, root, true);
    await writeFile(root + "/.npmrc", "registry=https://registry.npmjs.org/\n");
    // Required real network-dependent Bun audit, with P-selected flags and credential-free config.
    phase = "audit";
    const r = await fixedProcess(
      [process.execPath, "--no-env-file", "audit", "--json", "--audit-level=high"],
      {
        cwd: root,
        env: { HOME: "/job/home", BUN_INSTALL_CACHE_DIR: "/job/cache" },
        timeoutMs: operationalTimeouts.processMs,
      },
    );
    process.stdout.write(r.stdout);
    process.stderr.write(r.stderr);
    insist(r.code === 0 && !r.timedOut, "REQUIRED_NETWORK_AUDIT_FAILED");
    return;
  }
  if (id === "static-quality") {
    phase = "static-quality";
    // P parser/config/checker/tool code reads candidate bytes. It never imports candidate code/config.
    const result = await runTrustedStaticQuality(source, policy, "/prepared/tools");
    insist(result.status === "pass", "TRUSTED_STATIC_CHECK_FAILED");
    console.log(JSON.stringify(result));
    return;
  }
  if (id === "probe") {
    phase = "probe";
    Object.assign(process.env, await prepareProbeEnvironment());
    const probe = await input();
    insist(
      typeof probe.module === "string" && typeof probe.exportName === "string",
      "FIXED_PROBE_REQUIRED",
    );
    const frame = await callBehavior(probe, () =>
      loadBehaviorModule(source, safePath(String(probe.module))),
    );
    // Candidate return/throw data only. Host P owns comparisons and the report.
    process.stdout.write(JSON.stringify(frame) + "\n");
    return;
  }
  phase = "materialize";
  const root = await materialize();
  process.chdir(root);
  // This executes useful legacy workload, not an authenticated P assertion adapter.
  // Its assertions remain unauthenticated; the host certifies only its finite semantic catalogue.
  if (id === "source") {
    phase = "source";
    return runTrustedSourceChecks(root, policy);
  }
  if (id === "runtime-layout") {
    phase = "runtime-layout";
    const result = await verifyRuntimeLayout(root);
    console.log(JSON.stringify(result));
    return;
  }
  if (id === "candidate-quality") {
    phase = "candidate-quality";
    const audit = object((await input()).audit);
    insist(
      audit.dependencyInputsSha256 &&
        audit.exitCode === 0 &&
        audit.timedOut === false &&
        typeof audit.stdoutSha256 === "string",
      "EXPLICIT_EXTERNAL_AUDIT_REQUIRED",
    );
    const result = await runQuality(root, "all", undefined, {
      policyRoot: policy,
      dependencyAudit: async () => ({
        name: "separate-credential-free-bun-audit",
        exitCode: 0,
        timedOut: false,
        stdout: "",
        stderr: "",
        durationMs: 0,
        externalAudit: audit,
      }),
    });
    insist(result.status === "pass", "CANDIDATE_QUALITY_FAILED");
    console.log(
      JSON.stringify({
        status: result.status,
        reportSha256: sha256(JSON.stringify(result)),
        checks: result.checks.map(({ id, status }) => ({ id, status })),
        externalAuditSha256: trustedDigest(audit),
      }),
    );
    return;
  }
  throw Error("TRUSTED_CI_UNKNOWN_DRIVER");
}
if (import.meta.main)
  main().catch((error: unknown) => {
    process.stderr.write(encodeDriverFailure(error, process.argv[2], phase));
    process.exitCode = 1;
  });
