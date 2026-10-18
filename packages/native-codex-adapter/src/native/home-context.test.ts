import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  projectHomeContext,
  readHomeContextContractValue,
  type HomeContextContract,
  type HomeContextContractV2,
  type SelectedGitWorkContext,
} from "./home-context.ts";
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
    const binding: HookBinding = {
      projectId: "fixture",
      root,
      workId: "work",
      checkpointPath,
      entrypointCwds: [home],
      entrypointContextPath: contextPath,
    };
    // Prospective validation must work before either global preferences or staged
    // contract files exist; the file reader below retains its separate source SHA.
    const prospective = readHomeContextContractValue(contract, binding, home);
    expect(prospective).toEqual(contract);
    expect(() =>
      readHomeContextContractValue({ ...contract, priorities: "invalid" }, binding, home),
    ).toThrow("Invalid bounded context list");
    expect(() =>
      readHomeContextContractValue({ ...contract, northStar: "N".repeat(1601) }, binding, home),
    ).toThrow("Invalid bounded context text");
    expect(() =>
      readHomeContextContractValue(contract, { ...binding, workId: "another-work" }, home),
    ).toThrow("Home context differs from the canonical owner and actual cwd");
    prospective.boundaries.nativePermissionGranted = true;
    expect(contract.boundaries.nativePermissionGranted).toBe(false);
    await writeFile(profilePath, JSON.stringify(profile), { mode: 0o600 });
    await writeFile(contextPath, JSON.stringify(contract), { mode: 0o600 });
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

async function hostHomeFixture() {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "codex-host-home-"))),
    cwd = join(parent, "orchestrator"),
    nativeCodexHome = join(parent, "native-home", ".codex"),
    root = join(parent, "codex-owner"),
    repositoryRoot = join(parent, "agent-platform"),
    checkout = join(parent, "selected-work-checkout"),
    contextPath = join(root, "architecture/home-context.json"),
    profilePath = join(nativeCodexHome, "harness/profile.json");
  await Promise.all([
    mkdir(cwd, { recursive: true }),
    mkdir(join(nativeCodexHome, "harness"), { recursive: true }),
    mkdir(join(root, "architecture"), { recursive: true }),
    mkdir(repositoryRoot, { recursive: true }),
    mkdir(checkout, { recursive: true }),
  ]);
  const binding: HookBinding = {
    projectId: "agent-platform",
    root,
    workId: "continuing-work",
    checkpointPath: join(root, "state/continuity/work.json"),
    entrypointCwds: [cwd],
    entrypointContextPath: contextPath,
    nativeCodexHome,
    gitWorkContextVersion: "v2",
    gitWorkMetadataEncoding: "metadata.v1",
  };
  const contract: HomeContextContractV2 = {
    schema: "codex.home-context.v2",
    projectId: binding.projectId,
    entrypointCwd: cwd,
    nativeCodexHome,
    codeOwner: { projectId: "codex-harness-engineering", root },
    globalProfilePath: profilePath,
    selectedWork: {
      repositoryRoot,
      repositoryId: "fixture/agent-platform",
      commit: "a".repeat(40),
      projectId: binding.projectId,
      workId: binding.workId!,
    },
    northStar: "Preserve the full selected continuing Work purpose and native ownership.",
    priorities: ["Preserve purpose", "Keep code ownership separate from logical Work"],
    ownership: [
      {
        scope: "Native execution",
        owner: "native-codex",
        reference: join(nativeCodexHome, "AGENTS.md"),
      },
    ],
    canonicalSources: [
      { role: "Preferences", path: profilePath, readPolicy: "Validated safe subset" },
    ],
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
      semanticOwners: [],
      projectEntrypoints: [
        { projectId: "codex-harness-engineering", root, entrypoints: ["project.json"] },
      ],
    },
    privateSentinel: "SYNTHETIC-HOST-PRIVATE-PREFERENCE",
  };
  // Home consumes an already admitted common representation. These synthetic
  // semantics exercise preservation without reading any native/user Work store.
  const selectedWork = {
    schema: "common.git-work-context.compact.v1",
    encoding: {
      version: "metadata.v1",
      prefixes: [],
      decoder: "@palantirkc/work-continuity/git-work#decodeCompactGitWorkContext",
    },
    metadata: [
      contract.selectedWork.repositoryId,
      contract.selectedWork.commit,
      contract.selectedWork.projectId,
      contract.selectedWork.workId,
    ],
    status: "source-qualified",
    strings: ["FULL-SYNTHETIC-PURPOSE", "PRESERVED-COMPLETED-EFFECT-FENCE"],
    sources: [],
    normative: [],
    decisions: [],
    continuityBinding: [],
    holdReasons: [],
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  } satisfies SelectedGitWorkContext;
  const receipt = {
    schema: "codex.home-work-checkout-observation.v1",
    repositoryRoot,
    repositoryId: contract.selectedWork.repositoryId,
    commit: contract.selectedWork.commit,
    projectId: binding.projectId,
    workId: binding.workId!,
    cwd: checkout,
    branch: "refs/heads/work/synthetic-continuing-work",
  };
  const save = () => writeFile(contextPath, JSON.stringify(contract), { mode: 0o600 });
  const pinReceipt = async (value: unknown = receipt) => {
    const path = join(root, "architecture/checkout-observation.json"),
      bytes = JSON.stringify(value);
    await writeFile(path, bytes, { mode: 0o600 });
    contract.selectedWork.checkoutObservation = {
      path,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
    await save();
    return path;
  };
  await writeFile(profilePath, JSON.stringify(profile), { mode: 0o600 });
  await save();
  return {
    parent,
    cwd,
    nativeCodexHome,
    root,
    repositoryRoot,
    checkout,
    contextPath,
    profilePath,
    profile,
    binding,
    contract,
    selectedWork,
    receipt,
    save,
    pinReceipt,
    options: { binding, cwd, selectedWork, selectedRepositoryRoot: repositoryRoot },
    cleanup: () => rm(parent, { recursive: true, force: true }),
  };
}

function homeProjection(text: string) {
  return JSON.parse(text.slice(text.indexOf("\n") + 1));
}

test("Home v2 separates native Home, entrypoint, fixed code owner and source-only selected Work", async () => {
  const f = await hostHomeFixture();
  try {
    expect(new Set([f.nativeCodexHome, f.cwd, f.root, f.repositoryRoot]).size).toBe(4);
    expect(readHomeContextContractValue(f.contract, f.binding, f.cwd)).toEqual(f.contract);
    const before = await Promise.all([readFile(f.contextPath), readFile(f.profilePath)]),
      result = projectHomeContext(f.options);
    expect(result.status).toBe("current");
    const projected = homeProjection(result.text);
    expect(projected.codeOwner).toEqual(f.contract.codeOwner);
    expect(projected.actualNativeCwd).toBe(f.cwd);
    expect(projected.nativeCodexHome).toBe(f.nativeCodexHome);
    expect(projected.selectedWork).toEqual(f.selectedWork);
    expect(projected.boundaries.executionReady).toBe(false);
    expect(projected.boundaries.nativePermissionGranted).toBe(false);
    expect(projected.selectedWorkRoute).toEqual({ executionReady: false });
    expect(projected.projectRoute).toBeUndefined();
    expect(result.text).not.toContain("enter sourceOwnerProjectRoot for effectful Work");
    expect(result.text).not.toContain(f.profile.privateSentinel);
    expect(result.chars).toBeLessThanOrEqual(12000);
    expect(await Promise.all([readFile(f.contextPath), readFile(f.profilePath)])).toEqual(before);
  } finally {
    await f.cleanup();
  }
});

test("Home v2 only exposes effectful checkout after the native owner observes the exact pinned receipt", async () => {
  const f = await hostHomeFixture();
  try {
    const receiptPath = await f.pinReceipt(),
      source = await readFile(receiptPath),
      observed: unknown[] = [];
    const result = projectHomeContext({
      ...f.options,
      observeCheckout: (receipt) => {
        observed.push(receipt);
      },
    });
    expect(result.status).toBe("current");
    expect(observed).toEqual([f.receipt]);
    const projected = homeProjection(result.text);
    expect(projected.boundaries.executionReady).toBe(true);
    expect(projected.selectedWorkRoute.effectfulWorkCwd).toBe(f.checkout);
    expect(projected.selectedWorkRoute.effectfulWorkCwd).not.toBe(f.root);
    expect(projected.boundaries.nativePermissionGranted).toBe(false);
    expect(result.nativePermissionGranted).toBe(false);
    expect(await readFile(receiptPath)).toEqual(source);
    expect(projectHomeContext(f.options).status).toBe("repair-required");
    const failed = projectHomeContext({
      ...f.options,
      observeCheckout: () => {
        throw new Error("PRIVATE-OBSERVER-FAILURE");
      },
    });
    expect(failed.status).toBe("repair-required");
    expect(failed.text).not.toContain("PRIVATE-OBSERVER-FAILURE");
  } finally {
    await f.cleanup();
  }
});

test("Home v2 holds a missing selected context without inventing a Work or code-owner execution route", async () => {
  const f = await hostHomeFixture();
  try {
    const result = projectHomeContext({ ...f.options, selectedWork: undefined });
    expect(result.status).toBe("held");
    const projected = homeProjection(result.text);
    expect(projected.boundaries.executionReady).toBe(false);
    expect(projected.selectedWorkRoute).toEqual({ executionReady: false });
    expect(projected.boundaries.nativePermissionGranted).toBe(false);
  } finally {
    await f.cleanup();
  }
});

test("Home v2 preserves a held Work and never observes its checkout for execution", async () => {
  const f = await hostHomeFixture();
  try {
    await f.pinReceipt();
    let observed = false;
    const selectedWork = {
      ...f.selectedWork,
      status: "held" as const,
      holdReasons: ["LATEST_RELEVANT_UNKNOWN"],
    };
    const result = projectHomeContext({
      ...f.options,
      selectedWork,
      observeCheckout: () => {
        observed = true;
      },
    });
    expect(result.status).toBe("held");
    expect(observed).toBe(false);
    const projected = homeProjection(result.text);
    expect(projected.selectedWork).toEqual(selectedWork);
    expect(projected.selectedWorkRoute).toEqual({ executionReady: false });
    expect(projected.boundaries.executionReady).toBe(false);
  } finally {
    await f.cleanup();
  }
});

test("Home v2 holds oversized required semantics and withdraws the checkout execution route", async () => {
  const f = await hostHomeFixture();
  try {
    await f.pinReceipt();
    f.selectedWork.strings = [
      "BEGIN-REQUIRED-PURPOSE " + "P".repeat(13000) + " END-REQUIRED-PURPOSE",
    ];
    const result = projectHomeContext({ ...f.options, observeCheckout: () => {} });
    expect(result.status).toBe("held");
    expect(result.chars).toBeLessThanOrEqual(12000);
    const projected = homeProjection(result.text);
    expect(projected.selectedWork.holdReasons).toEqual(["NATIVE_CONTEXT_OUTPUT_LIMIT"]);
    expect(projected.selectedWork.requiredContextChars).toBeGreaterThan(12000);
    expect(projected.selectedWork.commit).toBe(f.contract.selectedWork.commit);
    expect(projected.selectedWorkRoute).toEqual({ executionReady: false });
    expect(projected.boundaries.executionReady).toBe(false);
    expect(projected.boundaries.nativePermissionGranted).toBe(false);
    expect(result.text).not.toContain("BEGIN-REQUIRED-PURPOSE");
  } finally {
    await f.cleanup();
  }
});

for (const field of ["repositoryId", "commit", "projectId", "workId"] as const) {
  test(`Home v2 rejects a selected context with a different ${field}`, async () => {
    const f = await hostHomeFixture();
    try {
      const selectedWork = structuredClone(f.selectedWork),
        index = ["repositoryId", "commit", "projectId", "workId"].indexOf(field);
      selectedWork.metadata[index] = field === "commit" ? "b".repeat(40) : "foreign-selection";
      expect(projectHomeContext({ ...f.options, selectedWork }).status).toBe("repair-required");
    } finally {
      await f.cleanup();
    }
  });
}

test("Home v2 rejects a different or absent selected repository root", async () => {
  const f = await hostHomeFixture();
  try {
    expect(projectHomeContext({ ...f.options, selectedRepositoryRoot: f.root }).status).toBe(
      "repair-required",
    );
    expect(projectHomeContext({ ...f.options, selectedRepositoryRoot: undefined }).status).toBe(
      "repair-required",
    );
  } finally {
    await f.cleanup();
  }
});

for (const mutation of [
  "native-home",
  "profile",
  "code-owner",
  "logical-project",
  "logical-work",
  "legacy-route",
  "delivery-version",
  "metadata-encoding",
] as const) {
  test(`Home v2 refuses mismatched ${mutation} binding or route`, async () => {
    const f = await hostHomeFixture();
    try {
      switch (mutation) {
        case "native-home":
          f.contract.nativeCodexHome = f.cwd;
          break;
        case "profile":
          f.contract.globalProfilePath = join(f.cwd, ".codex/harness/profile.json");
          break;
        case "code-owner":
          f.contract.codeOwner.root = f.repositoryRoot;
          break;
        case "logical-project":
          f.contract.selectedWork.projectId = "codex-harness-engineering";
          break;
        case "logical-work":
          f.contract.selectedWork.workId = "historical-work";
          break;
        case "legacy-route":
          Object.assign(f.contract, { projectRoute: { effectfulWorkCwd: f.root } });
          break;
        case "delivery-version":
          f.binding.gitWorkContextVersion = "v1";
          break;
        case "metadata-encoding":
          delete f.binding.gitWorkMetadataEncoding;
          break;
      }
      await f.save();
      expect(projectHomeContext(f.options).status).toBe("repair-required");
    } finally {
      await f.cleanup();
    }
  });
}

for (const target of [
  "native-home",
  "profile",
  "receipt",
  "checkout",
  "entrypoint",
  "code-owner",
  "repository",
] as const) {
  test(`Home v2 refuses a symlinked ${target} path`, async () => {
    const f = await hostHomeFixture();
    try {
      const alias =
        target === "receipt"
          ? join(f.root, "architecture/receipt-alias.json")
          : join(f.parent, `${target}-alias`);
      switch (target) {
        case "native-home":
          await symlink(f.nativeCodexHome, alias);
          f.contract.nativeCodexHome = alias;
          f.contract.globalProfilePath = join(alias, "harness/profile.json");
          f.binding.nativeCodexHome = alias;
          break;
        case "profile": {
          const bytes = await readFile(f.profilePath),
            actual = join(f.parent, "actual-profile.json");
          await writeFile(actual, bytes);
          await rm(f.profilePath);
          await symlink(actual, f.profilePath);
          break;
        }
        case "receipt": {
          const path = await f.pinReceipt();
          await symlink(path, alias);
          f.contract.selectedWork.checkoutObservation!.path = alias;
          break;
        }
        case "checkout":
          await symlink(f.checkout, alias);
          await f.pinReceipt({ ...f.receipt, cwd: alias });
          break;
        case "entrypoint":
          await symlink(f.cwd, alias);
          f.contract.entrypointCwd = alias;
          f.binding.entrypointCwds = [alias];
          break;
        case "code-owner":
          await symlink(f.root, alias);
          f.contract.codeOwner.root = alias;
          f.binding.root = alias;
          f.binding.entrypointContextPath = join(alias, "architecture/home-context.json");
          break;
        case "repository":
          await symlink(f.repositoryRoot, alias);
          f.contract.selectedWork.repositoryRoot = alias;
          break;
      }
      await f.save();
      let observed = false;
      const result = projectHomeContext({
        ...f.options,
        ...(target === "entrypoint" ? { cwd: alias } : {}),
        ...(target === "repository" ? { selectedRepositoryRoot: alias } : {}),
        observeCheckout: () => {
          observed = true;
        },
      });
      expect(result.status).toBe("repair-required");
      expect(observed).toBe(false);
    } finally {
      await f.cleanup();
    }
  });
}

for (const field of ["repositoryRoot", "repositoryId", "commit", "projectId", "workId"] as const) {
  test(`Home v2 rejects a pinned checkout receipt with a different ${field} before owner observation`, async () => {
    const f = await hostHomeFixture();
    try {
      await f.pinReceipt({
        ...f.receipt,
        [field]:
          field === "repositoryRoot"
            ? f.root
            : field === "commit"
              ? "b".repeat(40)
              : "foreign-selection",
      });
      let observed = false;
      expect(
        projectHomeContext({
          ...f.options,
          observeCheckout: () => {
            observed = true;
          },
        }).status,
      ).toBe("repair-required");
      expect(observed).toBe(false);
    } finally {
      await f.cleanup();
    }
  });
}

test("Home v2 rejects changed checkout receipt bytes and a receipt outside its fixed code owner", async () => {
  const f = await hostHomeFixture();
  try {
    const path = await f.pinReceipt();
    await writeFile(path, JSON.stringify({ ...f.receipt, branch: "another-branch" }));
    let observed = false;
    const observeCheckout = () => {
      observed = true;
    };
    expect(projectHomeContext({ ...f.options, observeCheckout }).status).toBe("repair-required");
    expect(observed).toBe(false);
    const outside = join(f.parent, "outside-receipt.json"),
      bytes = JSON.stringify(f.receipt);
    await writeFile(outside, bytes);
    f.contract.selectedWork.checkoutObservation = {
      path: outside,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    };
    await f.save();
    expect(projectHomeContext({ ...f.options, observeCheckout }).status).toBe("repair-required");
    expect(observed).toBe(false);
  } finally {
    await f.cleanup();
  }
});

for (const mutation of ["schema", "branch", "extra-key"] as const) {
  test(`Home v2 rejects an invalid pinned checkout receipt ${mutation}`, async () => {
    const f = await hostHomeFixture();
    try {
      await f.pinReceipt({
        ...f.receipt,
        ...(mutation === "schema" ? { schema: "foreign.checkout-observation.v1" } : {}),
        ...(mutation === "branch" ? { branch: "refs/heads/../foreign" } : {}),
        ...(mutation === "extra-key" ? { nativePermissionGranted: true } : {}),
      });
      let observed = false;
      expect(
        projectHomeContext({
          ...f.options,
          observeCheckout: () => {
            observed = true;
          },
        }).status,
      ).toBe("repair-required");
      expect(observed).toBe(false);
    } finally {
      await f.cleanup();
    }
  });
}
