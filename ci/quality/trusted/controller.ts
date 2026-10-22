import { superviseSemanticCase, semanticRequest, semanticRequestDigest } from "./semantic";
import { randomUUID } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  chmod,
  writeFile,
  readFile,
  lstat,
  readdir,
  readlink,
} from "node:fs/promises";
import { join, resolve, relative } from "node:path";
import { tmpdir } from "node:os";
import { readQualityScope } from "../scope";
import { buildPolicy } from "./release";
import { githubReader, verifyPolicyRef, verifyRun, repoPath } from "./provider";
import { checkout, readSubject, readAuthor, git, sourceFiles, verifyCurrentTarget } from "./source";
import { WorkerController } from "./worker";
import { superviseSourceAssertions, SourceAssertionBoundaryHold } from "./source-completion";
import {
  command,
  insist,
  object,
  positive,
  oid,
  same,
  sha256,
  trustedDigest,
  subjects,
  requiredChecks,
  validateTarget,
  qualifyTrustedExecution,
  type ExecutionReport,
  type Target,
} from "./policy";

async function treeDigest(root: string): Promise<string> {
  const entries: { path: string; sha256: string }[] = [];
  async function walk(dir: string): Promise<void> {
    for (const e of await readdir(dir, { withFileTypes: true })) {
      const f = join(dir, e.name),
        path = relative(root, f);
      insist(entries.length < 100000, "PREPARATION_TREE_LIMIT");
      if (e.isDirectory()) await walk(f);
      else if (e.isSymbolicLink())
        entries.push({ path, sha256: sha256("symlink\0" + (await readlink(f))) });
      else {
        const s = await lstat(f);
        insist(s.isFile() && s.size <= 100_000_000, "PREPARATION_FILE_LIMIT");
        entries.push({ path, sha256: sha256(await readFile(f)) });
      }
    }
  }
  await walk(root);
  return trustedDigest(entries.sort((a, b) => a.path.localeCompare(b.path)));
}
/** Only immutable-tag workflow_dispatch is supported. The candidate supplies identifiers, never P. */
export async function controller(): Promise<void> {
  const env = process.env,
    policyRoot = resolve(import.meta.dir, "../../..");
  insist(
    env.GITHUB_ACTIONS === "true" &&
      env.GITHUB_EVENT_NAME === "workflow_dispatch" &&
      oid(env.GITHUB_SHA) &&
      env.GITHUB_REF_TYPE === "tag" &&
      env.GITHUB_REF_PROTECTED === "true",
    "PROTECTED_IMMUTABLE_TAG_DISPATCH_REQUIRED",
  );
  const token = env.GITHUB_TOKEN;
  insist(
    token &&
      env.GITHUB_WORKSPACE === policyRoot &&
      positive(Number(env.GITHUB_RUN_ID)) &&
      positive(Number(env.GITHUB_RUN_ATTEMPT)),
    "TRUSTED_WORKFLOW_ENVIRONMENT_REQUIRED",
  );
  const requestId = env.TRUSTED_REQUEST_ID;
  insist(
    requestId &&
      /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(requestId),
    "EXACT_REQUEST_ID_REQUIRED",
  );
  const p = await buildPolicy(policyRoot, env.GITHUB_SHA),
    reader = githubReader(token);
  insist(env.GITHUB_REF === "refs/tags/" + p.protectedTag, "SELECTED_POLICY_REF_MISMATCH");
  await verifyPolicyRef(reader, p);
  verifyRun(
    p,
    await reader.json(repoPath(`actions/runs/${env.GITHUB_RUN_ID}`)),
    Number(env.GITHUB_RUN_ID),
    Number(env.GITHUB_RUN_ATTEMPT),
    requestId,
    false,
  );
  const head = env.TRUSTED_HEAD,
    base = env.TRUSTED_BASE,
    merged = env.TRUSTED_MERGE;
  insist(oid(head) && oid(base) && oid(merged) && head !== merged, "EXACT_H_B_M_REQUIRED");
  const scratch = await mkdtemp(join(tmpdir(), "trusted-source-controller-"));
  await chmod(scratch, 0o755);
  const reportPath = join(scratch, "trusted-execution.json"),
    journal = join(scratch, "controller.jsonl");
  // These paths are never mounted into any worker or named by any candidate-controlled output.
  await writeFile(journal, "", { mode: 0o644, flag: "wx" });
  if (env.GITHUB_OUTPUT)
    await writeFile(env.GITHUB_OUTPUT, `report-path=${reportPath}\njournal-path=${journal}\n`, {
      flag: "a",
    });
  const hRoot = join(scratch, "authored"),
    mRoot = join(scratch, "merged");
  await mkdir(hRoot);
  await mkdir(mRoot);
  await checkout(hRoot, head, token);
  await checkout(mRoot, merged, token);
  const authored = await readSubject(reader, hRoot, head),
    tested = await readSubject(reader, mRoot, merged);
  const author = await readAuthor(hRoot, authored);
  await readAuthor(mRoot, tested);
  const target: Target = {
    kind: env.TRUSTED_KIND === "protected-main" ? "protected-main" : "pull-request",
    pr: Number(env.TRUSTED_PR),
    base,
    authored,
    sourceBase: author.metadata.sourceBaseSha,
    tested,
  };
  validateTarget(target);
  insist(
    (await git(mRoot, ["merge-tree", "--write-tree", "--no-messages", base, head])) === tested.tree,
    "ACTUAL_MECHANICAL_MERGE_REQUIRED",
  );
  await verifyCurrentTarget(reader, target);
  await sourceFiles(hRoot);
  await sourceFiles(mRoot);
  const policyScope = await readQualityScope(policyRoot);
  const w = new WorkerController(p, policyRoot, journal),
    imageId = await w.build();
  const report: ExecutionReport = {
    schema: "agent-platform.trusted-ci-execution.v2",
    policyRelease: p.release,
    policyRevision: p.revision,
    controllerClosureSha256: p.controllerClosureSha256,
    workflowSha256: p.workflowSha256,
    runId: Number(env.GITHUB_RUN_ID),
    attempt: Number(env.GITHUB_RUN_ATTEMPT),
    requestId,
    target,
    preparation: [],
    audits: [],
    checks: [],
    probes: [],
    sourceAssertions: [],
    eligibilityClaim: "bounded-p-semantic-v1",
    fullLegacyAssertionsAuthenticated: false,
    status: "fail",
    nativePermissionGranted: false,
    acceptanceDecision: null,
  };
  try {
    for (const s of subjects(target)) {
      const root = s.commit === head ? hRoot : mRoot,
        volume = "trusted-source-" + randomUUID();
      await w.record({ phase: "volume-create-intent", volume, subject: s });
      const made = await w.docker(["volume", "create", "--driver", "local", volume], 4096);
      insist(
        made.code === 0 && !made.timedOut && made.stdout.toString().trim() === volume,
        "VOLUME_CREATE_UNKNOWN_RECONCILE_REQUIRED",
      );
      const mounts = [
        { source: policyRoot, target: "/policy", readOnly: true, type: "bind" as const },
      ];
      const prep = await w.run(s, {
        role: "prepare",
        imageId,
        network: "bridge",
        mounts: [
          ...mounts,
          { source: root, target: "/input", readOnly: true, type: "bind" },
          { source: volume, target: "/prepared", readOnly: false, type: "volume" },
        ],
        argv: [
          "/usr/local/bin/bun",
          "--no-env-file",
          "--no-install",
          "/policy/ci/quality/trusted/driver.ts",
          "prepare",
        ],
        stdin: Buffer.from(JSON.stringify({ dependencyInputsSha256: s.dependencyInputsSha256 })),
      });
      insist(prep.observation.exitCode === 0 && !prep.observation.timedOut, "PREPARATION_FAILED");
      const volumeInfo = await w.docker(["volume", "inspect", volume], 8192);
      insist(volumeInfo.code === 0 && !volumeInfo.timedOut, "EXACT_VOLUME_INSPECT_REQUIRED");
      const info = object((JSON.parse(volumeInfo.stdout.toString()) as unknown[])[0]);
      insist(
        info.Name === volume &&
          info.Driver === "local" &&
          !info.Options &&
          typeof info.Mountpoint === "string" &&
          info.Mountpoint === `/var/lib/docker/volumes/${volume}/_data`,
        "PRIVATE_LOCAL_VOLUME_REQUIRED",
      );
      report.preparation.push({
        subject: s.commit,
        dependencyInputsSha256: s.dependencyInputsSha256,
        dependencyTreeSha256: await treeDigest(info.Mountpoint),
        toolPinsSha256: p.toolPinsSha256,
        observation: prep.observation,
      });
      const dependencyTreeSha256 = report.preparation.at(-1)!.dependencyTreeSha256;
      const frozenMounts = [
        ...mounts,
        { source: volume, target: "/prepared", readOnly: true, type: "volume" as const },
      ];
      const audit = await w.run(s, {
        role: "audit",
        imageId,
        dependencyTreeSha256,
        network: "bridge",
        mounts: frozenMounts,
        argv: [
          "/usr/local/bin/bun",
          "--no-env-file",
          "--no-install",
          "/policy/ci/quality/trusted/driver.ts",
          "audit",
        ],
      });
      report.audits.push({
        subject: s.commit,
        dependencyInputsSha256: s.dependencyInputsSha256,
        observedAt: audit.observation.finishedAt,
        feed: "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk",
        observation: audit.observation,
      });
      for (const id of requiredChecks) {
        const run = () =>
          w.run(s, {
            role: id === "static-quality" ? "checker" : "candidate",
            imageId,
            dependencyTreeSha256,
            network: "none",
            mounts: frozenMounts,
            argv: command(id),
            ...(id === "candidate-quality"
              ? {
                  stdin: Buffer.from(
                    JSON.stringify({
                      audit: {
                        dependencyInputsSha256: s.dependencyInputsSha256,
                        exitCode: audit.observation.exitCode,
                        timedOut: audit.observation.timedOut,
                        stdoutSha256: audit.observation.stdoutSha256,
                      },
                    }),
                  ),
                }
              : {}),
          });
        if (id === "source") {
          // This host-owned gate never reads source stdout/test counts as assertion proof.
          await superviseSourceAssertions(run, (source) => {
            report.sourceAssertions.push({
              subject: s.commit,
              occurrence: source.result.observation.occurrence,
              evidence: source.evidence,
            });
            report.checks.push({
              id,
              subject: s,
              argv: command(id),
              observation: source.result.observation,
            });
          });
          continue;
        }
        const result = await run();
        report.checks.push({ id, subject: s, argv: command(id), observation: result.observation });
        if (id === "candidate-quality") {
          // Additional untrusted workload: its own JSON is not the full-quality verdict.
          insist(
            result.observation.exitCode === 0 &&
              !result.observation.timedOut &&
              !result.observation.oomKilled,
            "CANDIDATE_QUALITY_WORKLOAD_FAILED",
          );
          continue;
        }
        // Only P's data-only checker roles may describe their own completion here.
        // These roles do not import candidate implementation or execute candidate config.
        let completion: Record<string, unknown> | undefined;
        for (const line of result.stdout.toString().split("\n")) {
          try {
            const value = object(JSON.parse(line));
            if (value.status === "pass") completion = value;
          } catch {
            /* Bounded child output is data; never execute or log it. */
          }
        }
        insist(completion && completion.status === "pass", "REQUIRED_CHECK_COMPLETION_MISSING");
        if (id === "static-quality")
          insist(
            completion.schema === "agent-platform.trusted-static-quality.v1" &&
              Number(completion.tests) >= policyScope.testFiles.length &&
              Array.isArray(completion.checks) &&
              same(
                completion.checks.map((v) => object(v).id),
                ["format", "typed-lint", "architecture", "workflow-security", "maintained-secrets"],
              ) &&
              completion.checks.every((v) => object(v).status === "pass"),
            "TRUSTED_CHECKER_COMPLETION_REQUIRED",
          );
      }
      for (const probe of p.probes) {
        await superviseSemanticCase(
          probe,
          () =>
            w.run(s, {
              role: "probe",
              imageId,
              dependencyTreeSha256,
              network: "none",
              mounts: frozenMounts,
              argv: [
                "/usr/local/bin/bun",
                "--no-env-file",
                "--no-install",
                "/policy/ci/quality/trusted/driver.ts",
                "probe",
              ],
              stdin: Buffer.from(JSON.stringify(semanticRequest(probe))),
            }),
          (result, actual) => {
            report.probes.push({
              id: probe.id,
              subject: s,
              inputSha256: semanticRequestDigest(probe),
              actual,
              observation: result.observation,
            });
          },
        );
      }
      insist(
        (await treeDigest(info.Mountpoint)) === report.preparation.at(-1)!.dependencyTreeSha256,
        "FROZEN_DEPENDENCY_TREE_CHANGED",
      );
    }
    report.status = "pass";
    qualifyTrustedExecution(p, report, Date.now());
    await verifyCurrentTarget(reader, target);
    await verifyPolicyRef(reader, p);
  } catch (error) {
    report.status = error instanceof SourceAssertionBoundaryHold ? "held" : "fail";
    await w.record({
      phase: "failure",
      code:
        error instanceof Error && /^TRUSTED_CI_[A-Z0-9_]+$/.test(error.message)
          ? error.message
          : "TRUSTED_CI_CONTROLLER_FAILED",
      originalFailureSha256: sha256(error instanceof Error ? error.message : String(error)),
    });
    throw error;
  } finally {
    await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n", {
      flag: "wx",
      mode: 0o644,
    });
  }
}
if (import.meta.main)
  controller().catch(() => {
    console.error("TRUSTED_CI_CONTROLLER_FAILED");
    process.exitCode = 1;
  });
