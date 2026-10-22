import { strict as assert } from "node:assert";
import { writeFile } from "node:fs/promises";

const modulePath = process.argv[2],
  witnessPath = process.argv[3];
if (!modulePath || !witnessPath) throw Error("Bounded source assertion fixture paths required");
const subject = (await import(modulePath)) as { answer: () => number };
assert.equal(subject.answer(), 42, "P assertion must execute after candidate import");
await writeFile(witnessPath, "P assertion finished\n", { flag: "wx" });
console.log(
  JSON.stringify({
    schema: "agent-platform.source-ci.v1",
    status: "pass",
    canonicalTestFiles: 1,
    qualityTestFiles: 1,
    commands: [{ exitCode: 0 }],
  }),
);
