import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ActiveRuntime } from "./upstream";
import type { ResearchRequest } from "./generated/research-request";
import Ajv2020 from "ajv/dist/2020.js";
import bundleSchema from "../schemas/capture-bundle.schema.json";
import type { CaptureBundle, Attempt as CaptureAttempt, Artifact as CaptureArtifact, Diagnostic as CaptureDiagnostic, Redirect as RedirectObservation, Headers } from "./generated/capture-bundle";
export type { CaptureBundle, Attempt as CaptureAttempt, Artifact as CaptureArtifact, Diagnostic as CaptureDiagnostic, Redirect as RedirectObservation } from "./generated/capture-bundle";

export type CaptureMode = "static" | "dynamic";
export type HeaderObservation = NonNullable<Headers>;

const bundleValidator = new Ajv2020({ allErrors: true, strict: true }).compile<CaptureBundle>(bundleSchema);
export function assertCaptureBundle(value: unknown): asserts value is CaptureBundle {
  if (!bundleValidator(value)) throw new Error("Invalid capture bundle: " + JSON.stringify(bundleValidator.errors));
  for (const [index, attempt] of value.attempts.entries()) {
    if (attempt.attempt !== index + 1 || attempt.outcome === "failure" && !attempt.error) throw new Error("Capture attempt ordering or diagnostic contract failed");
  }
  for (const artifact of value.artifacts) if (!value.attempts.some(attempt => attempt.attempt === artifact.attempt && attempt.mode === artifact.mode)) throw new Error("Artifact references an unknown capture attempt");
}

const credentialHeader = /^(authorization|proxy-authorization|cookie|set-cookie|x-api-key|api-key)$/i;
export function redactHeaders(headers: HeaderObservation | null): HeaderObservation | null {
  return headers && Object.fromEntries(Object.entries(headers).map(([key, value]) => [key, credentialHeader.test(key) ? "[REDACTED]" : value]));
}
export function sha256(bytes: string | Uint8Array): string { return new Bun.CryptoHasher("sha256").update(bytes).digest("hex"); }
interface BridgeMetadata { event: "metadata"; finalUrl: string; status: number; requestHeaders: HeaderObservation | null; responseHeaders: HeaderObservation | null; redirects: RedirectObservation[]; observedAt: string }
interface BridgeArtifact { event: "artifact"; kind: CaptureArtifact["kind"]; base64: string; provenance: CaptureArtifact["provenance"]; url: string; observedAt: string; complete: boolean }
interface BridgeDiagnostic extends CaptureDiagnostic { event: "diagnostic" }
type BridgeEvent = BridgeMetadata | BridgeArtifact | BridgeDiagnostic | { event: "finished"; observedAt: string };

/** Executes exactly one Scrapling attempt. NDJSON checkpoints survive process timeout. */
export async function collectAttempt(request: ResearchRequest, mode: CaptureMode, attempt: number, runtime: ActiveRuntime, root: string, runDirectory: string): Promise<{ attempt: CaptureAttempt; artifacts: CaptureArtifact[]; content: string }> {
  const startedAt = new Date().toISOString();
  const outputPath = join(runDirectory, `attempt-${attempt}.txt`);
  const stdoutPath = join(runDirectory, `attempt-${attempt}.stdout.txt`);
  const stderrPath = join(runDirectory, `attempt-${attempt}.stderr.txt`);
  const timeoutMs = request.timeoutMs ?? 45000;
  const diagnostics: CaptureDiagnostic[] = [];
  const artifacts: CaptureArtifact[] = [];
  const sanitizedEvents: unknown[] = [];
  let metadata: BridgeMetadata | null = null;
  let stdout = "", stderr = "", exitCode: number | null = null, killed = false;
  try {
    const child = Bun.spawn([runtime.pythonExecutable, join(root, "src/scrapling-bridge.py")], {
      cwd: root, stdin: "pipe", stdout: "pipe", stderr: "pipe", env: { ...Bun.env, PYTHONUNBUFFERED: "1", PYTHONDONTWRITEBYTECODE: "1" },
    });
    child.stdin.write(JSON.stringify({ ...request, mode, timeoutMs, browserExecutable: runtime.browserExecutable }));
    child.stdin.end();
    const deadline = setTimeout(() => { killed = true; child.kill("SIGKILL"); }, timeoutMs * (mode === "dynamic" ? 2 : 1) + 5000);
    try { [stdout, stderr, exitCode] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]); }
    finally { clearTimeout(deadline); }
  } catch (error) { diagnostics.push({ code: "adapter-process-error", message: error instanceof Error ? error.message : String(error), observedAt: new Date().toISOString() }); }
  if (killed) diagnostics.push({ code: "adapter-timeout", message: "Bridge process exceeded the capture deadline", observedAt: new Date().toISOString() });
  for (const line of stdout.split("\n").filter(Boolean)) {
    let event: BridgeEvent;
    try { event = JSON.parse(line) as BridgeEvent; }
    catch { diagnostics.push({ code: "incomplete-bridge-event", message: "Bridge output included an incomplete NDJSON checkpoint", observedAt: new Date().toISOString() }); continue; }
    if (event.event === "metadata") {
      metadata = { ...event, requestHeaders: redactHeaders(event.requestHeaders), responseHeaders: redactHeaders(event.responseHeaders),
        redirects: event.redirects.map(item => ({ ...item, requestHeaders: redactHeaders(item.requestHeaders), responseHeaders: redactHeaders(item.responseHeaders) })) };
      sanitizedEvents.push(metadata);
    } else if (event.event === "artifact") {
      const bytes = Buffer.from(event.base64, "base64");
      const path = event.kind === "extracted-text" ? outputPath : join(runDirectory, `attempt-${attempt}-${event.kind}-${artifacts.length + 1}.${event.kind === "rendered-dom" ? "html" : "bin"}`);
      await writeFile(path, bytes, { flag: "wx" });
      const artifact: CaptureArtifact = { kind: event.kind, path, sha256: sha256(bytes), byteLength: bytes.byteLength, attempt, mode,
        mediaType: event.kind === "extracted-text" ? "text/plain; charset=utf-8" : event.kind === "rendered-dom" ? "text/html; charset=utf-8" : metadata?.responseHeaders?.["content-type"] ?? "application/octet-stream",
        provenance: event.provenance, url: event.url, observedAt: event.observedAt, complete: event.complete };
      artifacts.push(artifact); sanitizedEvents.push({ event: "artifact", ...artifact });
    } else if (event.event === "diagnostic") { const { event: _, ...diagnostic } = event; diagnostics.push(diagnostic); sanitizedEvents.push(event); }
    else sanitizedEvents.push(event);
  }
  await writeFile(stdoutPath, sanitizedEvents.map(event => JSON.stringify(event)).join("\n") + "\n", { flag: "wx" });
  await writeFile(stderrPath, stderr, { flag: "wx" });
  const extraction = artifacts.find(item => item.kind === "extracted-text");
  const content = extraction ? await readFile(extraction.path, "utf8") : "";
  const extractionPassed = Boolean(content.trim());
  const matched = request.expectedText && extraction ? content.includes(request.expectedText) : null;
  const errors = diagnostics.filter(item => !["response-body-unavailable"].includes(item.code)).map(item => item.message);
  if (exitCode !== 0) errors.push(`Scrapling ${mode} bridge exited ${exitCode}`);
  if (!extractionPassed) errors.push("Scrapling returned empty selected content");
  if (matched === false) errors.push("Expected text was absent from extracted content");
  if (metadata && metadata.status >= 400) errors.push(`HTTP response status ${metadata.status}`);
  if (mode === "static" && request.mode !== "static" && request.waitSelector) errors.push("Dynamic readiness must be verified in a rendered capture");
  const unavailable: CaptureAttempt["unavailable"] = [];
  if (mode === "static") unavailable.push({ field: "requestHeaders.wireCompleteness", reason: "Scrapling exposes request header metadata; additional headers generated internally by the HTTP client may not be included" });
  if (!metadata) for (const field of ["status", "finalUrl", "requestHeaders", "responseHeaders", "redirects"]) unavailable.push({ field, reason: "No completed response metadata was observed by Scrapling" });
  else for (const field of ["requestHeaders", "responseHeaders"] as const) if (!metadata[field]) unavailable.push({ field, reason: "The upstream response did not expose this metadata" });
  if (!artifacts.some(item => item.kind === "response-content")) unavailable.push({ field: "response-content", reason: "Response-content bytes were not exposed before the capture terminated; no separate fetch was substituted" });
  if (artifacts.some(item => item.kind === "response-content" && !item.complete)) unavailable.push({ field: "response-content.completeness", reason: "The HTTP client exposed partial response content in a failed request exception" });
  const dom = artifacts.find(item => item.kind === "rendered-dom");
  if (dom && metadata && dom.url !== metadata.finalUrl) unavailable.push({ field: "rendered-dom.navigation-linkage", reason: "Rendered page URL differs from the last observed navigation response URL; exact DOM-to-response linkage remains unverified" });
  if (mode === "static") unavailable.push({ field: "rendered-dom", reason: "A static capture does not execute a browser" });
  else if (!artifacts.some(item => item.kind === "rendered-dom")) unavailable.push({ field: "rendered-dom", reason: "The browser page was unavailable before DOM serialization" });
  return { attempt: { attempt, mode, outcome: errors.length ? "failure" : "success", exitCode, matched, extractionPassed,
    ...(errors.length ? { error: errors.join("; ") } : {}), outputPath, stdoutPath, stderrPath, startedAt, finishedAt: new Date().toISOString(),
    status: metadata?.status ?? null, finalUrl: metadata?.finalUrl ?? null, requestHeaders: metadata?.requestHeaders ?? null,
    responseHeaders: metadata?.responseHeaders ?? null, redirects: metadata?.redirects ?? null, unavailable, diagnostics }, artifacts, content };
}
