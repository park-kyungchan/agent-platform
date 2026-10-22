import { readFile } from "node:fs/promises";
import {
  withQualifiedSource,
  type NativeSourceReadOwner,
} from "../packages/work-continuity/src/coordination/source-runtime.ts";
import type { SourceBindingInput } from "../packages/work-continuity/src/coordination/source-contract.ts";
export async function runSourceBinding(args: string[], owner?: NativeSourceReadOwner) {
  if (args.length !== 2 || args[0] !== "verify") throw Error("SOURCE_BINDING_COMMAND_REQUIRED");
  if (!owner) throw Error("SOURCE_AUTHENTICATED_NATIVE_OWNER_REQUIRED");
  const bytes = await readFile(args[1]!);
  if (bytes.byteLength > 2097152) throw Error("SOURCE_REQUEST_BOUND");
  const value: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  return withQualifiedSource(value as SourceBindingInput, owner, (source) => source.proof);
}
if (import.meta.main) throw Error("SOURCE_AUTHENTICATED_NATIVE_OWNER_REQUIRED");
