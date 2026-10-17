import { readNativeAdapterSourceClosure } from "./provider-closure.ts";
import { constants, openSync, closeSync, fstatSync, readFileSync, realpathSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import type { HookBindings, HookInput } from "./hooks.ts";

// Reviewed owner facade: a builtin guard precedes the first common implementation import.
// Its native command path/arguments stay unchanged; native /hooks owns exact definition trust.
/** Admission barrier only: the canonical common parser/composer remains the semantic owner. */
export function admitsCurrentHomeContext(input: HookInput, registry: HookBindings): boolean {
  if (!["SessionStart", "UserPromptSubmit"].includes(input.hook_event_name) ||
      registry.bindings.length !== 1 || registry.contextSelections?.length !== 1) return false;
  const binding = registry.bindings[0]!, selected = registry.contextSelections[0]!;
  if (selected.schema !== "codex.session-git-work-selection.v1" ||
      binding.projectId !== "codex-harness-engineering" ||
      binding.workId !== "ontology-driven-harness-operation" ||
      binding.allowedCwds !== undefined || binding.qualificationLedgerPath !== undefined ||
      binding.childReservationTokens !== undefined || binding.entrypointCwds?.length !== 1 ||
      !binding.entrypointContextPath || input.cwd !== binding.entrypointCwds[0] ||
      input.cwd === binding.root || resolve(input.cwd) !== input.cwd ||
      realpathSync(input.cwd) !== input.cwd || realpathSync(binding.root) !== binding.root ||
      selected.cwd !== input.cwd || selected.ownerRoot !== binding.root ||
      selected.projectId !== binding.projectId || selected.workId !== binding.workId ||
      selected.taskId !== "CODEX-NATIVE-ADOPTION-RECOVERY" ||
      !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(selected.snapshot.commit) ||
      selected.snapshot.projectId !== binding.projectId || selected.snapshot.workId !== binding.workId ||
      selected.nativeSessionId !== input.session_id ||
      selected.nativeAgentId !== input.agent_id || selected.nativeAgentType !== input.agent_type)
    return false;
  return true;
}

export async function runNativeHook(inputText: string, bindingsPath: string, options: { contextOnly?: boolean; bindingsSha256?: string } = {}): Promise<string> {
  if (!bindingsPath || Buffer.byteLength(inputText) > 8 * 1024 * 1024)
    throw Error("BOUNDED_NATIVE_HOOK_INPUT_REQUIRED");
  const source = await readNativeAdapterSourceClosure();
  await source.verify();
  const value: unknown = JSON.parse(inputText);
  // Decline all non-context events before implementation import or any legacy capture.
  if (options.contextOnly && (!value || typeof value !== "object" ||
      !["SessionStart", "UserPromptSubmit"].includes(String((value as Record<string, unknown>).hook_event_name)))) return "";
  let retainedBindings: unknown;
  if (options.contextOnly) {
    if (!options.bindingsSha256 || !/^[a-f0-9]{64}$/.test(options.bindingsSha256) ||
        resolve(bindingsPath) !== bindingsPath || realpathSync(bindingsPath) !== bindingsPath)
      throw Error("PINNED_CONTEXT_BINDINGS_REQUIRED");
    const fd = openSync(bindingsPath, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    try {
      const before = fstatSync(fd);
      if (!before.isFile() || before.size > 1024 * 1024) throw Error("BOUNDED_CONTEXT_BINDINGS_REQUIRED");
      const bytes = readFileSync(fd), after = fstatSync(fd);
      if (bytes.length !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs ||
          createHash("sha256").update(bytes).digest("hex") !== options.bindingsSha256)
        throw Error("CONTEXT_BINDINGS_DRIFT");
      retainedBindings = JSON.parse(bytes.toString());
    } finally { closeSync(fd); }
  }
  const { handleHook, readHookBindings, parseHookInput, resolveHookBinding, resolveSessionGitWorkSelection } = await import("./hooks.ts");
  await source.verify();
  const bindings = readHookBindings(bindingsPath);
  if (options.contextOnly) {
    if (JSON.stringify(bindings) !== JSON.stringify(retainedBindings)) throw Error("CONTEXT_BINDINGS_DRIFT");
    const input = parseHookInput(value);
    if (!admitsCurrentHomeContext(input, bindings)) return "";
    const binding = resolveHookBinding(input.cwd, bindings);
    if (!binding || !resolveSessionGitWorkSelection(input, bindings, binding)) return "";
  }
  const result = await handleHook(value, { bindings });
  await source.verify();
  return result.stdout;
}
async function readNativeInput(): Promise<string> {
  const maxBytes = 8 * 1024 * 1024, buffer = Buffer.allocUnsafe(maxBytes);
  let bytes = 0;
  for await (const chunk of Bun.stdin.stream()) {
    if (chunk.byteLength > maxBytes - bytes) throw Error("BOUNDED_NATIVE_HOOK_INPUT_REQUIRED");
    buffer.set(chunk, bytes); bytes += chunk.byteLength;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, bytes));
}
if (import.meta.main) {
  try {
    const args = Bun.argv.slice(2), index = args.indexOf("--bindings");
    if (index < 0 || !args[index + 1]) throw Error("EXPLICIT_BINDINGS_REQUIRED");
    const stdout = await runNativeHook(await readNativeInput(), args[index + 1]!, { contextOnly: args.includes("--context-only"), bindingsSha256: args[args.indexOf("--bindings-sha256") + 1] });
    if (stdout) process.stdout.write(stdout + "\n");
  } catch {
    // No private paths/event payload/errors and no exit2 continuation request.
    process.stdout.write(JSON.stringify({ systemMessage: "Codex context held: native event or fixed source closure could not be verified." }) + "\n");
  }
  process.exitCode = 0;
}
