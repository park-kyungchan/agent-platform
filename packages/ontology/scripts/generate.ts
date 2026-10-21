import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { compile, compileFromFile } from "json-schema-to-typescript";
import packageManifest from "../package.json" with { type: "json" };
import Ajv from "ajv";
import standaloneCode from "ajv/dist/standalone/index.js";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

/** Additive schema owner: old Ontology schemas/releases never acquire v3 fields. */
async function generateGitWorkV3(): Promise<void> {
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const schemaPath = join(root, "packages/work-continuity/contracts/git-work-v3.schema.json");
  const output = join(root, "packages/work-continuity/src/git-work/generated");
  const input = await readFile(schemaPath);
  const schemaSha256 = createHash("sha256").update(input).digest("hex");
  const schema = JSON.parse(input.toString()) as Parameters<Ajv["addSchema"]>[0];
  const banner = `/** Generated from git-work-v3.schema.json; do not edit. SHA-256: ${schemaSha256} */`;
  const types = await compile(schema, "GitWorkV3Graph", {
    bannerComment: banner,
    unreachableDefinitions: true,
    unknownAny: true,
    format: true,
  });
  const names = ["Graph", "Work", "Task", "Run", "Contract", "CreateRequest", "EffectiveContract", "AllocationBinding", "StoredRecord", "ContextPacket", "CompactContextPacket"];
  const ajv = new Ajv({ strict: true, allErrors: false, code: { source: true, esm: true } });
  ajv.addSchema(schema, "v3");
  const exports = Object.fromEntries(names.map((name) => ["validate" + name, "v3#/$defs/" + name]));
  {
    // Bundle only already pinned Ajv helpers. Consumers need no new dependency/pin.
    const standalone = standaloneCode(ajv, exports);
    const built = await Bun.build({ entrypoints: ["git-work-v3-schema"], target: "bun", packages: "bundle", plugins: [{ name: "owned-v3-schema", setup(build) {
      build.onResolve({ filter: /^git-work-v3-schema$/ }, () => ({ path: "validators", namespace: "owned-v3-schema" }));
      build.onLoad({ filter: /.*/, namespace: "owned-v3-schema" }, () => ({ contents: standalone, loader: "js", resolveDir: join(root, "packages/ontology") }));
    } }] });
    if (!built.success) throw Error("V3_VALIDATOR_BUNDLE_FAILED");
    const runtime = banner + "\n" + (await built.outputs[0]!.text());
    const declarations = banner + "\n" + names.map((name) => `export declare const validate${name}: (value: unknown) => boolean;`).join("\n") + "\n";
    const artifacts = { "v3.ts": types, "v3-validators.js": runtime, "v3-validators.d.ts": declarations };
    if (!process.argv.includes("--check")) await mkdir(output, { recursive: true });
    for (const [name, bytes] of Object.entries(artifacts)) {
      const path = join(output, name);
      if (process.argv.includes("--check")) {
        if ((await readFile(path, "utf8")) !== bytes) throw Error("STALE_GIT_WORK_V3_GENERATED_SOURCE");
      } else await writeFile(path, bytes);
    }
    console.log(JSON.stringify({ status: "pass", schemaSha256, outputs: Object.keys(artifacts) }));
  }
}

if (process.argv.includes("--git-work-v3")) {
  await generateGitWorkV3();
} else {

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
    const schema = JSON.parse(input.toString()) as {
      $defs: { DomainObjectType: Record<string, unknown> };
    };
    generated +=
      "\n" +
      (await compile(schema.$defs.DomainObjectType, "DomainObjectType", {
        bannerComment: "",
        format: true,
      }));
  }
  if (process.argv.includes("--check")) {
    const existing = await readFile(outputPath, "utf8");
    if (existing !== generated)
      throw new Error("Generated declarations are stale. Run bun run generate.");
    console.log(JSON.stringify({ status: "pass", generatorVersion, schemaSha256 }));
  } else {
    await mkdir("src/generated", { recursive: true });
    await writeFile(outputPath, generated);
    console.log(
      JSON.stringify({ status: "generated", generatorVersion, schemaSha256, outputPath }),
    );
  }
}
}
