import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { compile, compileFromFile } from "json-schema-to-typescript";
import packageManifest from "../package.json" with { type: "json" };

const generatorVersion = packageManifest.devDependencies["json-schema-to-typescript"];
for (const [schemaPath, outputPath] of [
  ["schemas/artifact.schema.json", "src/generated/contracts.ts"],
  ["schemas/operational.schema.json", "src/generated/operational.ts"],
  ["schemas/evidence.schema.json", "src/generated/evidence.ts"],
] as const) {
const input = await readFile(schemaPath);
const schemaSha256 = createHash("sha256").update(input).digest("hex");
let generated = await compileFromFile(schemaPath, {
  bannerComment: `/**\n * Generated from ${schemaPath}; do not edit.\n * json-schema-to-typescript ${generatorVersion}\n * Input SHA-256: ${schemaSha256}\n */`,
  unreachableDefinitions: true,
  unknownAny: true,
  format: true,
});
// The generator skips unreachable primitive definitions. Generate the domain enum
// from its canonical schema definition instead of maintaining a handwritten twin.
if (schemaPath === "schemas/evidence.schema.json") {
  const schema = JSON.parse(input.toString()) as { $defs: { DomainObjectType: Record<string, unknown> } };
  generated += "\n" + await compile(schema.$defs.DomainObjectType, "DomainObjectType", { bannerComment: "", format: true });
}
if (process.argv.includes("--check")) {
  const existing = await readFile(outputPath, "utf8");
  if (existing !== generated) throw new Error("Generated declarations are stale. Run bun run generate.");
  console.log(JSON.stringify({ status: "pass", generatorVersion, schemaSha256 }));
} else {
  await mkdir("src/generated", { recursive: true });
  await writeFile(outputPath, generated);
  console.log(JSON.stringify({ status: "generated", generatorVersion, schemaSha256, outputPath }));
}
}
