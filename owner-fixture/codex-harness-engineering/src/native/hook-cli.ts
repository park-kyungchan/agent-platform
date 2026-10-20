import { readNativeAdapterSourceClosure } from "./provider-closure.ts";
import { constants, openSync, closeSync, fstatSync, readFileSync, realpathSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve, dirname } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { HookBindings, HookInput } from "./hooks.ts";
import type { HostAdmissionDescriptor } from "./host-candidate.ts";

function admitsHostDescriptor(registry: HookBindings): boolean {
  const descriptor = (registry as HookBindings & {hostAdmission?: HostAdmissionDescriptor}).hostAdmission;
  const binding = registry.bindings[0], selected = registry.contextSelections?.[0];
  if (!descriptor || !binding || !selected ||
      Object.keys(descriptor).sort().join('|') !== 'authority|codeOwnerRoot|entrypointCwd|nativeCodexHome|nativePermissionGranted|schema|selection' ||
      descriptor.schema !== 'codex.host-context-admission.v1' || descriptor.authority !== 'none' ||
      descriptor.nativePermissionGranted !== false ||
      descriptor.codeOwnerRoot !== resolve(import.meta.dir, '../..') || binding.root !== descriptor.codeOwnerRoot ||
      !binding.nativeCodexHome || descriptor.nativeCodexHome !== binding.nativeCodexHome ||
      realpathSync(binding.nativeCodexHome) !== binding.nativeCodexHome ||
      descriptor.entrypointCwd !== binding.entrypointCwds?.[0] ||
      binding.gitWorkContextVersion !== 'v2' || binding.gitWorkMetadataEncoding !== 'metadata.v1' ||
      selected.schema !== 'codex.session-git-work-selection.v1' || !selected.taskId ||
      selected.latestRelevant !== 'current' || !isDeepStrictEqual(descriptor.selection, selected)) return false;
  return true;
}

function hostRegistryPathMatches(bindingsPath: string, registry: HookBindings): boolean {
  return /^host-candidate-[a-zA-Z0-9][a-zA-Z0-9-]{0,63}$/.test(dirname(bindingsPath).split('/').at(-1)!) &&
    dirname(dirname(bindingsPath)) === resolve(import.meta.dir, '../../.private') &&
    bindingsPath === dirname(bindingsPath)+'/bindings.json' &&
    registry.bindings[0]?.entrypointContextPath === dirname(bindingsPath)+'/home-context.json' &&
    registry.bindings[0]?.checkpointPath === dirname(bindingsPath)+'/checkpoint-not-used.json';
}
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
function hasHostDescriptor(value: unknown): boolean {
  return record(value) && (Object.hasOwn(value, 'hostAdmission') ||
    (Array.isArray(value.bindings) && value.bindings.some(binding => record(binding) && Object.hasOwn(binding, 'nativeCodexHome'))));
}
/** Builtins only. Canonical parsing still follows, but a foreign first event imports no common code. */
function admitsPinnedHostInput(value: unknown, retained: unknown, bindingsPath: string): boolean {
  try {
    if (!record(value) || !record(retained) || retained.schemaVersion !== '1.0.0' ||
        !Array.isArray(retained.bindings) || !Array.isArray(retained.contextSelections) ||
        retained.bindings.length !== 1 || retained.contextSelections.length !== 1 ||
        !record(retained.bindings[0]) || !record(retained.contextSelections[0]) ||
        !record(retained.contextSelections[0].snapshot) ||
        typeof value.cwd !== 'string' || typeof value.session_id !== 'string' || !value.session_id ||
        value.session_id.length > 1024 || /[\u0000-\u001f]/.test(value.session_id) ||
        !['SessionStart','UserPromptSubmit'].includes(String(value.hook_event_name)) ||
        (value.hook_event_name === 'SessionStart' && !['startup','resume','compact','clear','fork'].includes(String(value.source)))) return false;
    const text = (item: unknown) => typeof item === 'string' && item.length > 0 && item.length <= 1024 && !/[\u0000-\u001f]/.test(item);
    const selected = retained.contextSelections[0];
    // This additive Host route admits only an explicitly selected native child.
    // A root fork or a parent selection is never a wildcard for its descendants.
    if (value.hook_event_name === 'SessionStart' && value.source === 'fork' &&
        (!text(value.agent_id) || !text(selected.nativeAgentId))) return false;
    if (!['projectId','workId','taskId','nativeSessionId'].every(key => text(selected[key])) ||
        !['agent_id','agent_type'].every(key => value[key] === undefined || text(value[key])) ||
        !['nativeAgentId','nativeAgentType'].every(key => selected[key] === undefined || text(selected[key]))) return false;
    const registry = retained as unknown as HookBindings;
    return hostRegistryPathMatches(bindingsPath, registry) && admitsCurrentHomeContext(value as unknown as HookInput, registry);
  } catch { return false; }
}

// Reviewed owner facade: a builtin guard precedes the first common implementation import.
// Its native command path/arguments stay unchanged; native /hooks owns exact definition trust.
/** Admission barrier only: the canonical common parser/composer remains the semantic owner. */
export function admitsCurrentHomeContext(input: HookInput, registry: HookBindings): boolean {
  if (!["SessionStart", "UserPromptSubmit"].includes(input.hook_event_name) ||
      registry.bindings.length !== 1 || registry.contextSelections?.length !== 1) return false;
  const binding = registry.bindings[0]!, selected = registry.contextSelections[0]!;
  const host = binding.nativeCodexHome !== undefined || Object.hasOwn(registry, 'hostAdmission');
  if (host && input.hook_event_name === 'SessionStart' && input.source === 'fork' &&
      (!input.agent_id || !selected.nativeAgentId)) return false;
  if (selected.schema !== "codex.session-git-work-selection.v1" ||
      (host ? !admitsHostDescriptor(registry) :
        binding.projectId !== "codex-harness-engineering" || binding.workId !== "ontology-driven-harness-operation") ||
      binding.allowedCwds !== undefined || binding.qualificationLedgerPath !== undefined ||
      binding.childReservationTokens !== undefined || binding.entrypointCwds?.length !== 1 ||
      !binding.entrypointContextPath || input.cwd !== binding.entrypointCwds[0] ||
      input.cwd === binding.root || resolve(input.cwd) !== input.cwd ||
      realpathSync(input.cwd) !== input.cwd || realpathSync(binding.root) !== binding.root ||
      selected.cwd !== input.cwd || selected.ownerRoot !== binding.root ||
      selected.projectId !== binding.projectId || selected.workId !== binding.workId ||
      (!host && selected.taskId !== "CODEX-NATIVE-ADOPTION-RECOVERY") ||
      !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(selected.snapshot.commit) ||
      selected.snapshot.projectId !== binding.projectId || selected.snapshot.workId !== binding.workId ||
      selected.nativeSessionId !== input.session_id ||
      selected.nativeAgentId !== input.agent_id || selected.nativeAgentType !== input.agent_type)
    return false;
  return true;
}

export async function runNativeHook(inputText: string, bindingsPath: string, options: { contextOnly?: boolean; bindingsSha256?: string; selectedSessionOnly?: boolean } = {}): Promise<string> {
  if (!bindingsPath || Buffer.byteLength(inputText) > 8 * 1024 * 1024)
    throw Error("BOUNDED_NATIVE_HOOK_INPUT_REQUIRED");
  if (options.contextOnly && options.selectedSessionOnly)
    throw Error("DISTINCT_CONTEXT_ONLY_AND_SELECTED_SESSION_MODES_REQUIRED");
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
    if ((hasHostDescriptor(retainedBindings) || dirname(bindingsPath).split('/').at(-1)!.startsWith('host-candidate-')) && !admitsPinnedHostInput(value, retainedBindings, bindingsPath)) return '';
  }
  const { handleHook, readHookBindings, parseHookInput, resolveHookBinding, resolveSessionGitWorkSelection } = await import("./hooks.ts");
  await source.verify();
  const bindings = readHookBindings(bindingsPath);
  if (options.contextOnly) {
    if (JSON.stringify(bindings) !== JSON.stringify(retainedBindings)) throw Error("CONTEXT_BINDINGS_DRIFT");
    const input = parseHookInput(value);
    if (hasHostDescriptor(bindings) && !hostRegistryPathMatches(bindingsPath, bindings))
      throw Error('OWNER_LOCAL_HOST_REGISTRY_REQUIRED');
    if (!admitsCurrentHomeContext(input, bindings)) return "";
    const binding = resolveHookBinding(input.cwd, bindings);
    if (!binding || !resolveSessionGitWorkSelection(input, bindings, binding)) return "";
  }
  const result = await handleHook(value, { bindings, selectedSessionOnly: options.selectedSessionOnly });
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
    const stdout = await runNativeHook(await readNativeInput(), args[index + 1]!, { contextOnly: args.includes("--context-only"), selectedSessionOnly: args.includes("--selected-session-only"), bindingsSha256: args[args.indexOf("--bindings-sha256") + 1] });
    if (stdout) process.stdout.write(stdout + "\n");
  } catch {
    // No private paths/event payload/errors and no exit2 continuation request.
    process.stdout.write(JSON.stringify({ systemMessage: "Codex context held: native event or fixed source closure could not be verified." }) + "\n");
  }
  process.exitCode = 0;
}
