import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import Ajv from "ajv";
import Ajv2020 from "ajv/dist/2020.js";
import type { JsonObject, JsonValue, SuiteBudget } from "../modules/evals/types";

export const NATIVE_CODEX_DOCUMENTATION = "https://learn.chatgpt.com/docs/non-interactive-mode";

export interface NativeUsage {
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  totalTokens: number;
  observed: boolean;
  raw: JsonObject[];
}

export interface NativeCodexResult {
  adapterVersion: "1.0.0";
  status: "completed" | "failed" | "budget-exceeded";
  events: JsonObject[];
  usage: NativeUsage;
  structuredOutput: JsonValue | null;
  finalOutput: string;
  rawStdout: string;
  rawStderr: string;
  exitCode: number | null;
  elapsedMs: number;
  diagnostics: string[];
  artifactDirectory?: string;
  invocation?: { command: string[]; cwd: string; budget: SuiteBudget; prompt: string; documentation: string };
}

export interface NativeCodexRequest {
  prompt: string;
  cwd: string;
  profile?: string;
  model?: string;
  budget: SuiteBudget;
  outputSchema?: JsonObject;
  runDirectory: string;
  /** Dependency injection for subprocess tests; suite JSON cannot override the executable. */
  executable?: string[];
}

export type NativeCodexRunner = (request: NativeCodexRequest) => Promise<NativeCodexResult>;

export function validateBudget(budget: SuiteBudget | undefined): asserts budget is SuiteBudget {
  if (!budget || ![budget.maxRuns, budget.maxDurationMs, budget.maxTotalTokens].every(value => Number.isSafeInteger(value) && value > 0)) {
    throw new Error("Native Codex requires an explicit positive suite budget: maxRuns, maxDurationMs, maxTotalTokens");
  }
}

function asObject(value: unknown): JsonObject | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as JsonObject : undefined;
}

function positiveNumber(value: JsonValue | undefined): number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : 0;
}

export function parseNativeCodexResult(input: { stdout: string; stderr?: string; finalOutput?: string; exitCode?: number | null; elapsedMs?: number; budget?: SuiteBudget; outputSchema?: JsonObject; terminatedForBudget?: boolean }): NativeCodexResult {
  const diagnostics: string[] = [];
  const events: JsonObject[] = [];
  for (const [index, line] of input.stdout.split(/\r?\n/).entries()) {
    if (!line.trim()) continue;
    try {
      const event = asObject(JSON.parse(line));
      if (!event || typeof event.type !== "string") throw new Error("Event must be an object with a type");
      events.push(event);
    } catch (error) {
      diagnostics.push(`Invalid JSONL event at line ${index + 1}: ${String(error)}`);
    }
  }
  const usage: NativeUsage = { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0, totalTokens: 0, observed: false, raw: [] };
  for (const event of events) {
    const raw = asObject(event.usage);
    if (!raw) continue;
    usage.observed = true;
    usage.raw.push(raw);
    usage.inputTokens += positiveNumber(raw.input_tokens);
    usage.outputTokens += positiveNumber(raw.output_tokens);
    usage.cachedInputTokens += positiveNumber(raw.cached_input_tokens);
  }
  // Cached input and reasoning output are subsets, not additional billable token counts.
  usage.totalTokens = usage.inputTokens + usage.outputTokens;
  let finalOutput = input.finalOutput;
  if (finalOutput === undefined) {
    const messages = events.filter(event => event.type === "item.completed" && asObject(event.item)?.type === "agent_message");
    const finalMessage = asObject(messages.at(-1)?.item)?.text;
    finalOutput = typeof finalMessage === "string" ? finalMessage : "";
  }
  let structuredOutput: JsonValue | null = null;
  if (finalOutput.trim()) {
    try { structuredOutput = JSON.parse(finalOutput) as JsonValue; }
    catch { if (input.outputSchema) diagnostics.push("Final output was not valid JSON"); }
  } else if (input.outputSchema) diagnostics.push("Final structured output is missing");
  if (input.outputSchema && structuredOutput !== null) {
    try {
      const ajv = String(input.outputSchema.$schema ?? "").includes("2020-12") ? new Ajv2020({ strict: false, allErrors: true }) : new Ajv({ strict: false, allErrors: true });
      const validator = ajv.compile(input.outputSchema);
      if (!validator(structuredOutput)) diagnostics.push(`Final output failed its schema: ${JSON.stringify(validator.errors)}`);
    } catch (error) { diagnostics.push(`Invalid output schema: ${String(error)}`); }
  }
  const elapsedMs = input.elapsedMs ?? 0;
  const exitCode = input.exitCode === undefined ? 0 : input.exitCode;
  const failedEvents = events.some(event => event.type === "turn.failed" || event.type === "error");
  const completed = events.some(event => event.type === "turn.completed");
  if (!completed) diagnostics.push("No completed native turn was observed");
  if (failedEvents) diagnostics.push("Native Codex emitted a failure event");
  if (exitCode !== 0) diagnostics.push(`Native process exited with ${String(exitCode)}`);
  if (!usage.observed) diagnostics.push("Native token usage was not observable");
  const exceeded = input.terminatedForBudget || (input.budget && (elapsedMs > input.budget.maxDurationMs || usage.totalTokens > input.budget.maxTotalTokens));
  if (exceeded) diagnostics.push("Explicit suite budget was exceeded; token usage is checked at emitted event boundaries, not a provider hard token cap");
  const status = exceeded ? "budget-exceeded" : !completed || failedEvents || exitCode !== 0 || diagnostics.some(message => /Invalid JSONL|Final output|structured output|output schema/.test(message)) ? "failed" : "completed";
  return { adapterVersion: "1.0.0", status, events, usage, structuredOutput, finalOutput, rawStdout: input.stdout, rawStderr: input.stderr ?? "", exitCode, elapsedMs, diagnostics };
}

/** Import existing traces without starting a model or altering their original source files. */
export async function importNativeCodexResult(input: { jsonlPath: string; stderrPath?: string; finalOutputPath?: string; exitCode?: number | null; elapsedMs?: number; budget?: SuiteBudget; outputSchema?: JsonObject }): Promise<NativeCodexResult> {
  const [stdout, stderr, finalOutput] = await Promise.all([
    readFile(input.jsonlPath, "utf8"),
    input.stderrPath ? readFile(input.stderrPath, "utf8") : undefined,
    input.finalOutputPath ? readFile(input.finalOutputPath, "utf8") : undefined,
  ]);
  const result = parseNativeCodexResult({ ...input, stdout, stderr, finalOutput });
  result.diagnostics.push(`Imported native trace from ${input.jsonlPath}`);
  return result;
}

/** The native process owns the agent loop. This adapter only captures and bounds execution. */
export async function runNativeCodex(request: NativeCodexRequest): Promise<NativeCodexResult> {
  validateBudget(request.budget);
  if (!request.prompt.trim()) throw new Error("Native Codex prompt must not be empty");
  await mkdir(dirname(request.runDirectory), { recursive: true, mode: 0o700 });
  await mkdir(request.runDirectory, { recursive: false, mode: 0o700 });
  const finalPath = join(request.runDirectory, "final-output.json");
  const command = [...(request.executable ?? ["codex"]), "exec", "--json", "--color", "never", "--sandbox", "read-only", "-c", "approval_policy=\"never\"", "--cd", request.cwd, "--output-last-message", finalPath];
  if (request.profile) command.push("--profile", request.profile);
  if (request.model) command.push("--model", request.model);
  if (request.outputSchema) {
    const schemaPath = join(request.runDirectory, "output-schema.json");
    await writeFile(schemaPath, JSON.stringify(request.outputSchema, null, 2), { mode: 0o600 });
    command.push("--output-schema", schemaPath);
  }
  command.push("-");
  const invocation = { command, cwd: request.cwd, budget: request.budget, prompt: request.prompt, documentation: NATIVE_CODEX_DOCUMENTATION };
  await writeFile(join(request.runDirectory, "invocation.json"), JSON.stringify(invocation, null, 2), { mode: 0o600 });
  const started = performance.now();
  let stdout = "";
  let stderr = "";
  let terminatedForBudget = false;
  let exitCode: number | null = null;
  try {
    const nativeProcess = Bun.spawn(command, { cwd: request.cwd, stdin: new TextEncoder().encode(request.prompt), stdout: "pipe", stderr: "pipe", detached: true });
    const terminate = () => {
      terminatedForBudget = true;
      try { globalThis.process.kill(-nativeProcess.pid, "SIGKILL"); }
      catch { try { nativeProcess.kill("SIGKILL"); } catch { /* The process may already have exited. */ } }
    };
    const timer = setTimeout(terminate, request.budget.maxDurationMs);
    let pending = "";
    let observedTokens = 0;
    const captureStdout = async () => {
      const decoder = new TextDecoder();
      for await (const chunk of nativeProcess.stdout) {
        const text = decoder.decode(chunk, { stream: true });
        stdout += text;
        pending += text;
        const lines = pending.split("\n");
        pending = lines.pop() ?? "";
        for (const line of lines) {
          try {
            const event = asObject(JSON.parse(line));
            const usage = asObject(event?.usage);
            if (usage) observedTokens += positiveNumber(usage.input_tokens) + positiveNumber(usage.output_tokens);
          } catch { /* The parser records malformed events after all bytes are preserved. */ }
        }
        if (observedTokens > request.budget.maxTotalTokens) terminate();
      }
      stdout += decoder.decode();
    };
    try {
      await Promise.all([captureStdout(), new Response(nativeProcess.stderr).text().then(text => { stderr = text; }), nativeProcess.exited.then(code => { exitCode = code; })]);
    } finally { clearTimeout(timer); }
  } catch (error) { stderr += `\nNative adapter failure: ${String(error)}`; exitCode = null; }
  const elapsedMs = Math.round(performance.now() - started);
  let finalOutput: string | undefined;
  try { finalOutput = await readFile(finalPath, "utf8"); } catch { /* Events may contain the last message. */ }
  const result = parseNativeCodexResult({ stdout, stderr, finalOutput, exitCode, elapsedMs, budget: request.budget, outputSchema: request.outputSchema, terminatedForBudget });
  result.artifactDirectory = request.runDirectory;
  result.invocation = invocation;
  await Promise.all([
    writeFile(join(request.runDirectory, "events.jsonl"), stdout, { mode: 0o600 }),
    writeFile(join(request.runDirectory, "stderr.log"), stderr, { mode: 0o600 }),
    writeFile(join(request.runDirectory, "result.json"), JSON.stringify(result, null, 2), { mode: 0o600 }),
  ]);
  return result;
}
