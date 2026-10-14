import { readNativeAdapterSourceClosure } from "./provider-closure.ts";

// Reviewed owner facade: a builtin guard precedes the first common implementation import.
// Its native command path/arguments stay unchanged; native /hooks owns exact definition trust.
export async function runNativeHook(inputText: string, bindingsPath: string): Promise<string> {
  if (!bindingsPath || Buffer.byteLength(inputText) > 8 * 1024 * 1024)
    throw Error("BOUNDED_NATIVE_HOOK_INPUT_REQUIRED");
  const source = await readNativeAdapterSourceClosure();
  await source.verify();
  const { handleHook, readHookBindings } = await import("./hooks.ts");
  await source.verify();
  const bindings = readHookBindings(bindingsPath);
  const result = await handleHook(JSON.parse(inputText), { bindings });
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
    const stdout = await runNativeHook(await readNativeInput(), args[index + 1]!);
    if (stdout) process.stdout.write(stdout + "\n");
  } catch {
    // No private paths/event payload/errors and no exit2 continuation request.
    process.stdout.write(JSON.stringify({ systemMessage: "Codex context held: native event or fixed source closure could not be verified." }) + "\n");
  }
  process.exitCode = 0;
}
