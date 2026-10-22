/** Native source-owner command library. JSON requests never instantiate owners. */
import { readFile } from "node:fs/promises";
import {
  materializeSourceDistribution,
  type NativeSourceDistributionOwner,
} from "../packages/work-continuity/src/coordination/source-distribution.ts";
import {
  compileSourceBootstrap,
  type NativeBootstrapBuildOwner,
  type BootstrapBuildRequest,
} from "../packages/work-continuity/src/coordination/source-bootstrap.ts";
export async function runSourceDistribution(
  args: string[],
  owner?: NativeSourceDistributionOwner,
  buildOwner?: NativeBootstrapBuildOwner,
) {
  if (args.length !== 2 || !["materialize", "bootstrap"].includes(args[0]!))
    throw Error("SOURCE_DISTRIBUTION_COMMAND_REQUIRED");
  if ((args[0] === "materialize" && !owner) || (args[0] === "bootstrap" && !buildOwner))
    throw Error("SOURCE_NATIVE_OWNER_REQUIRED");
  const bytes = await readFile(args[1]!);
  if (bytes.byteLength > 2097152) throw Error("SOURCE_REQUEST_BOUND");
  const request: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  if (args[0] === "bootstrap")
    return compileSourceBootstrap(request as BootstrapBuildRequest, buildOwner!);
  return materializeSourceDistribution(request, owner!);
}
if (import.meta.main) throw Error("SOURCE_NATIVE_OWNER_REQUIRED");
