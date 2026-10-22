import { prepareProbeEnvironment } from "../probe-environment";
import { loadBehaviorModule, scenarioModule } from "../scenario-adapters";
import { resolve } from "node:path";
import { callBehavior } from "../semantic";
Object.assign(process.env, await prepareProbeEnvironment());
const bytes = await Bun.stdin.bytes();
if (bytes.length > 128000) throw Error("fixture input bound");
const request = JSON.parse(new TextDecoder().decode(bytes)) as Record<string, unknown>;
const frame = await callBehavior(request, () =>
  request.module === scenarioModule
    ? loadBehaviorModule(process.argv[3] ?? resolve(import.meta.dir, "../../../.."), scenarioModule)
    : import(process.argv[2]!),
);
process.stdout.write(JSON.stringify(frame) + "\n");
