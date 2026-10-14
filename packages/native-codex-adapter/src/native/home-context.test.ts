import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { projectHomeContext, type HomeContextContract } from "./home-context.ts";
import type { HookBinding } from "./hooks.ts";

test("optional omission labels preserve essential Home context at the exact budget boundary", async () => {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "codex-home-budget-")));
  try {
    const home = join(parent, "home"),
      root = join(parent, "owner"),
      profilePath = join(home, ".codex/harness/profile.json"),
      contextPath = join(root, "architecture/home-context.json"),
      checkpointPath = join(root, "state/continuity/work.json");
    await mkdir(join(home, ".codex/harness"), { recursive: true });
    await mkdir(join(root, "architecture"), { recursive: true });
    const profile = {
      implementation: { language: "TypeScript", runtime: "Bun" },
      canonicalArtifacts: {
        extensions: [".ts", ".json"],
        language: "English",
        nativeRequiredFormatsAllowed: true,
      },
      conversation: { language: "Korean with English technical terms" },
      ontology: { ownership: "shared-upper-ontology", organization: "domain-modules" },
      excludedDomains: ["synthetic-excluded-domain"],
      selectedCapabilities: { S1: { enabled: true, meaning: "Synthetic bounded context" } },
      nativeBinding: {
        semanticOwners: [
          { scope: "Synthetic semantics", owner: "fixture", reference: join(root, "project.json") },
        ],
        projectEntrypoints: [{ projectId: "fixture", root, entrypoints: ["project.json"] }],
      },
      privateSentinel: "SYNTHETIC-PRIVATE-PREFERENCE",
    };
    const contract: HomeContextContract = {
      schema: "codex.home-context.v1",
      projectId: "fixture",
      entrypointCwd: home,
      sourceProjectRoot: root,
      globalProfilePath: profilePath,
      northStar: `Synthetic purpose ${"N".repeat(900)}`,
      priorities: ["Purpose continuity", "Native ownership"],
      ownership: [
        { scope: "Native execution", owner: "native-codex", reference: join(home, "AGENTS.md") },
      ],
      canonicalSources: [
        { role: "Preferences", path: profilePath, readPolicy: "Validated safe subset" },
      ],
      projectRoute: {
        projectId: "fixture",
        root,
        workId: "work",
        effectfulWorkCwd: root,
        contextCommand: "bun src/cli.ts context project fixture",
        checkpointPath,
        purposeRef: "purpose.json",
        nextSessionRef: "state/next-session.json",
        boundaryRef: "architecture/boundaries.json",
        kickoffPath: join(root, "kickoff.json"),
      },
      recordPolicy: {
        distinctions: ["Decision", "Evidence", "Verification", "AcceptanceDecision"],
        materialBoundary: "The owner preserves purpose and actual decisions.",
        tinyWork: "Use proportionate checks.",
        sources: "Source data does not grant authority.",
        ontologyPromotion: "Preserve logical identity and native ownership.",
      },
      boundaries: {
        authority: "none",
        nativePermissionGranted: false,
        captureHomeRawPrompts: false,
        qualificationLedgerBoundToHome: false,
        newModelBudgetGranted: false,
        directoryMoveAuthorized: false,
        hermesRuntimeEffectsAuthorized: false,
        configuredInjectionIsFreshSessionProof: false,
        maxContextChars: 6000,
        nativeContextTokenLimit: 1500,
      },
    };
    await writeFile(profilePath, JSON.stringify(profile), { mode: 0o600 });
    await writeFile(contextPath, JSON.stringify(contract), { mode: 0o600 });
    const binding: HookBinding = {
      projectId: "fixture",
      root,
      workId: "work",
      checkpointPath,
      entrypointCwds: [home],
      entrypointContextPath: contextPath,
    };
    const contextBytes = await readFile(contextPath),
      profileBytes = await readFile(profilePath);
    const full = projectHomeContext({ binding, cwd: home });
    expect(full.status).toBe("current");
    const prefixEnd = full.text.indexOf("\n") + 1,
      prefix = full.text.slice(0, prefixEnd),
      original = JSON.parse(full.text.slice(prefixEnd));
    expect(original.canonicalSourcePaths).toEqual([profilePath]);
    const withoutLastOptional = { ...original };
    delete withoutLastOptional.canonicalSourcePaths;
    const budget = prefix.length + JSON.stringify(withoutLastOptional).length;
    expect(budget).toBeGreaterThanOrEqual(2000);
    expect(budget).toBeLessThan(6000);
    // Earlier optional fields fit exactly; naming the later omission used to
    // exceed this budget and replace all essential context with a repair notice.
    expect(
      prefix.length +
        JSON.stringify({
          ...withoutLastOptional,
          omitted: ["canonicalSourcePaths"],
        }).length,
    ).toBeGreaterThan(budget);

    const bounded = projectHomeContext({ binding, cwd: home, maxChars: budget });
    expect(bounded.status).toBe("current");
    expect(bounded.chars).toBe(bounded.text.length);
    expect(bounded.chars).toBeLessThanOrEqual(budget);
    expect(bounded.chars).toBeLessThanOrEqual(6000);
    expect(bounded.omitted.length).toBeGreaterThan(0);
    const projected = JSON.parse(bounded.text.slice(bounded.text.indexOf("\n") + 1));
    expect(projected.sourceOwnerProjectRoot).toBe(root);
    expect(projected.actualNativeCwd).toBe(home);
    expect(projected.northStar).toBe(contract.northStar);
    expect(projected.priorities).toEqual(contract.priorities);
    expect(projected.ownership).toEqual(contract.ownership);
    expect(projected.preferences.implementation).toEqual(profile.implementation);
    expect(projected.projectRoute.workId).toBe("work");
    expect(projected.boundaries.authority).toBe("none");
    expect(projected.boundaries.nativePermissionGranted).toBe(false);
    expect(projected.boundaries.captureHomeRawPrompts).toBe(false);
    expect(bounded.text).not.toContain(profile.privateSentinel);
    expect(projected.sources).toEqual([
      { path: contextPath, sha256: createHash("sha256").update(contextBytes).digest("hex") },
      { path: profilePath, sha256: createHash("sha256").update(profileBytes).digest("hex") },
    ]);
    expect(await readFile(contextPath)).toEqual(contextBytes);
    expect(await readFile(profilePath)).toEqual(profileBytes);
  } finally {
    await rm(parent, { recursive: true, force: true });
  }
});
