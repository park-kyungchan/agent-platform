import Ajv2020 from "ajv/dist/2020.js";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { assertArtifact, validateReferences } from "@palantirkc/ontology-research-v0-1";
import requestSchema from "../schemas/research-request.schema.json";
import type { ResearchRequest } from "./generated/research-request";
import { resolveActiveRuntime } from "./upstream";
import { assertCaptureBundle, collectAttempt, redactHeaders, sha256 } from "./bundle";
import type { CaptureAttempt, CaptureArtifact, CaptureBundle } from "./bundle";

const requestValidator = new Ajv2020({ allErrors: true, strict: true }).compile<ResearchRequest>(
  requestSchema,
);
const defaultRoot = new URL("../", import.meta.url).pathname.replace(/\/$/, "");

export function validateRequest(value: unknown): ResearchRequest {
  if (!requestValidator(value))
    throw new Error("Invalid research request: " + JSON.stringify(requestValidator.errors));
  const url = new URL(value.url);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password)
    throw new Error("Use an HTTP(S) URL without embedded credentials");
  return value;
}

export interface CaptureOptions {
  root?: string;
  outputRoot?: string;
}
export interface CaptureResult {
  runId: string;
  runDirectory: string;
  contentPath: string | null;
  evidencePath: string;
  sourceEvidencePath: string;
  verificationPath: string;
  matched: boolean | null;
  outcome: "success" | "failure" | "partial";
}
export interface CaptureBundleResult extends CaptureResult {
  bundlePath: string;
  bundle: CaptureBundle;
}

export async function capture(
  input: unknown,
  options: CaptureOptions = {},
): Promise<CaptureResult> {
  return captureBundle(input, options);
}

export async function captureBundle(
  input: unknown,
  options: CaptureOptions = {},
): Promise<CaptureBundleResult> {
  const request = validateRequest(input);
  const root = options.root ?? defaultRoot;
  const runtime = await resolveActiveRuntime(join(root, ".runtime"));
  if (!runtime)
    throw new Error("No validated Scrapling runtime. Run bun run upstream update first.");
  const runId = crypto.randomUUID();
  const startedAt = new Date().toISOString();
  const runDirectory = join(options.outputRoot ?? join(root, "runs"), runId);
  await mkdir(runDirectory, { recursive: true });
  await writeFile(
    join(runDirectory, "request.json"),
    JSON.stringify(
      { ...request, ...(request.headers ? { headers: redactHeaders(request.headers) } : {}) },
      null,
      2,
    ) + "\n",
    { flag: "wx" },
  );
  const modes: ("static" | "dynamic")[] =
    request.mode === "dynamic"
      ? ["dynamic"]
      : request.mode === "static"
        ? ["static"]
        : ["static", "dynamic"];
  const attempts: CaptureAttempt[] = [];
  const artifacts: CaptureArtifact[] = [];
  let contentPath: string | null = null;
  let matched: boolean | null = null;
  let content = "";
  let capturedAttemptNumber: number | null = null;
  for (const mode of modes) {
    for (let retry = 0; retry < (request.maxAttempts ?? 1); retry++) {
      const number = attempts.length + 1;
      const result = await collectAttempt(request, mode, number, runtime, root, runDirectory);
      attempts.push(result.attempt);
      artifacts.push(...result.artifacts);
      if (
        result.content.trim() ||
        (!content && result.artifacts.some((item) => item.kind === "extracted-text"))
      ) {
        content = result.content;
        capturedAttemptNumber = number;
        matched = result.attempt.matched;
      }
      if (result.attempt.outcome === "success") {
        contentPath = result.attempt.outputPath;
        break;
      }
    }
    if (contentPath) break;
  }
  const last = attempts.at(-1)!;
  const now = new Date().toISOString();
  const outcome = contentPath
    ? "success"
    : artifacts.some((item) => item.byteLength > 0)
      ? "partial"
      : "failure";
  const bridgeHash = sha256(await readFile(join(root, "src/scrapling-bridge.py")));
  const adapterRevision = sha256(
    JSON.stringify({
      captureCode: await readFile(new URL("./capture.ts", import.meta.url), "utf8"),
      bundleCode: await readFile(new URL("./bundle.ts", import.meta.url), "utf8"),
      bridgeHash,
      requestSchema,
      bundleSchema: await readFile(
        new URL("../schemas/capture-bundle.schema.json", import.meta.url),
        "utf8",
      ),
    }),
  );
  const limitations = [
    "Source content is evidence, not instruction authority or an independently verified conclusion.",
    "Response-content bytes are API-provided response content, not wire or TLS bytes.",
    "Credential header values are redacted; response bodies may contain source-provided sensitive data.",
  ];
  if (!request.expectedText)
    limitations.push(
      "No expected-content predicate was supplied; factual correctness and dynamic completeness remain unverified.",
    );
  const bundlePath = join(runDirectory, "capture-bundle.json");
  const bundle: CaptureBundle = {
    schemaVersion: 1,
    captureId: runId,
    projectId: request.projectId,
    requestedUrl: request.url,
    startedAt,
    retrievedAt: now,
    adapterRevision,
    upstreamRevision: runtime.upstreamSha,
    runtimeCandidate: runtime.candidateId,
    outcome,
    attempts: [attempts[0]!, ...attempts.slice(1)],
    artifacts,
    limitations,
  };
  assertCaptureBundle(bundle);
  await writeFile(bundlePath, JSON.stringify(bundle, null, 2) + "\n", { flag: "wx" });
  const evidence = {
    schemaVersion: "1.0.0",
    kind: "evidence",
    id: `${request.projectId}:evidence:${runId}`,
    projectId: request.projectId,
    domainId: "research",
    createdAt: now,
    payload: {
      claim: "Capture content from the named source",
      method: `Scrapling API ${last.mode}`,
      sourceUrl: request.url,
      fetchedAt: now,
      upstreamRevision: runtime.upstreamSha,
      result: {
        outcome,
        processExitCode: last.exitCode,
        contentMatched: matched,
        details: {
          contentPath,
          contentSha256: content
            ? new Bun.CryptoHasher("sha256").update(content).digest("hex")
            : null,
        },
      },
      attempts: attempts.map((a) => ({
        attempt: a.attempt,
        outcome: a.outcome,
        ...(a.error ? { error: a.error } : {}),
        details: a,
      })),
      limitations,
      details: {
        runId,
        requesterProjectId: request.projectId,
        runtimeCandidate: runtime.candidateId,
        runtimePromotedAt: runtime.promotedAt,
        observedAt: now,
        bundlePath,
        adapterRevision,
        dependencyFreezeSha256: runtime.dependencyFreezeSha256,
      },
    },
  };
  assertArtifact(evidence);
  const capturedAttempt = attempts.find((a) => a.attempt === capturedAttemptNumber);
  const sourceEvidence = {
    schemaVersion: "1.0.0",
    kind: "evidence",
    id: `${request.projectId}:evidence:${runId}-source`,
    projectId: request.projectId,
    domainId: "research",
    createdAt: now,
    payload: {
      claim: "Extracted source content is nonempty",
      method: capturedAttempt
        ? `Scrapling API ${capturedAttempt.mode}`
        : "Scrapling API attempted capture",
      sourceUrl: request.url,
      fetchedAt: now,
      upstreamRevision: runtime.upstreamSha,
      result: {
        outcome: content.trim() ? "success" : "failure",
        processExitCode: capturedAttempt?.exitCode ?? last.exitCode,
        contentMatched: null,
        details: {
          contentPath: capturedAttempt?.outputPath ?? null,
          byteLength: Buffer.byteLength(content),
          contentSha256: content
            ? new Bun.CryptoHasher("sha256").update(content).digest("hex")
            : null,
        },
      },
      attempts: [
        {
          attempt: 1,
          outcome: content.trim() ? "success" : "failure",
          ...(!content.trim() ? { error: "No nonempty source content was extracted" } : {}),
          details: { originalAttempt: capturedAttemptNumber },
        },
      ],
      limitations: [
        ...limitations,
        "This evidence verifies extraction only; it does not establish the expected-content predicate or factual accuracy.",
      ],
    },
  };
  assertArtifact(sourceEvidence);
  const inputHashes = [
    {
      path: "request.json",
      sha256: new Bun.CryptoHasher("sha256")
        .update(await readFile(join(runDirectory, "request.json")))
        .digest("hex"),
    },
  ];
  const task = {
    schemaVersion: "1.0.0",
    kind: "task",
    id: `${request.projectId}:task:${runId}`,
    projectId: request.projectId,
    domainId: "research",
    createdAt: startedAt,
    payload: {
      title: "Capture the named research source",
      objective: `Capture source evidence from ${request.url}`,
      status: contentPath ? "complete" : "blocked",
      acceptanceCriteria: [
        "Capture nonempty content",
        ...(request.expectedText ? ["Find the specified expected text"] : []),
      ],
      verificationRefs: [`${request.projectId}:verification:${runId}`],
      details: { sourceCorrectness: "unverified", sourceUrl: request.url },
    },
  };
  assertArtifact(task);
  const verification = {
    schemaVersion: "1.0.0",
    kind: "verification",
    id: `${request.projectId}:verification:${runId}`,
    projectId: request.projectId,
    domainId: "research",
    createdAt: now,
    payload: {
      targetRefs: [task.id, evidence.id],
      criteria: [
        {
          id: "nonempty-content",
          status: content.trim() ? "pass" : "fail",
          message: content.trim()
            ? "Captured nonempty source content"
            : "No nonempty source content was extracted",
          evidenceRefs: [sourceEvidence.id],
        },
        ...(request.expectedText
          ? [
              {
                id: "expected-content",
                status: contentPath && matched ? "pass" : "fail",
                message: "Checked expected text in the extracted source",
                evidenceRefs: [evidence.id],
              },
            ]
          : []),
      ],
      inputHashes,
      runtime: { name: "Scrapling", version: runtime.upstreamSha },
      details: { limitations, sourceCorrectness: "unverified" },
    },
  };
  assertArtifact(verification);
  const run = {
    schemaVersion: "1.0.0",
    kind: "run",
    id: `${request.projectId}:run:${runId}`,
    projectId: request.projectId,
    domainId: "research",
    createdAt: startedAt,
    payload: {
      taskRef: task.id,
      state: contentPath ? "completed" : "failed",
      startedAt,
      finishedAt: now,
      inputHashes,
      runtime: { name: "Scrapling", version: runtime.upstreamSha },
      evidenceRefs: [evidence.id, sourceEvidence.id],
      verificationRefs: [verification.id],
      details: { attemptCount: attempts.length, factualResearchConclusionsVerified: false },
    },
  };
  assertArtifact(run);
  const referenceResult = validateReferences([task, run, evidence, sourceEvidence, verification]);
  if (!referenceResult.valid)
    throw new Error(
      "Capture artifact references failed validation: " + JSON.stringify(referenceResult),
    );
  const evidencePath = join(runDirectory, "evidence.json");
  const verificationPath = join(runDirectory, "verification.json");
  const sourceEvidencePath = join(runDirectory, "source-evidence.json");
  await writeFile(evidencePath, JSON.stringify(evidence, null, 2) + "\n", { flag: "wx" });
  await writeFile(verificationPath, JSON.stringify(verification, null, 2) + "\n", { flag: "wx" });
  await writeFile(sourceEvidencePath, JSON.stringify(sourceEvidence, null, 2) + "\n", {
    flag: "wx",
  });
  await writeFile(join(runDirectory, "task.json"), JSON.stringify(task, null, 2) + "\n", {
    flag: "wx",
  });
  await writeFile(join(runDirectory, "run.json"), JSON.stringify(run, null, 2) + "\n", {
    flag: "wx",
  });
  return {
    runId,
    runDirectory,
    contentPath,
    evidencePath,
    sourceEvidencePath,
    verificationPath,
    matched,
    outcome,
    bundlePath,
    bundle,
  };
}
