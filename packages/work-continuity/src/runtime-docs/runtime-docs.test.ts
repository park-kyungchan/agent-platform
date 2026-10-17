import { describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createDocumentationReader,
  readDocumentationRegistry,
  safeDocumentationUrl,
} from "./index.ts";
import type { DocumentationRegistry } from "./types.ts";

const path = new URL(
  "../../../../architecture/runtime-documentation-routing/registry.json",
  import.meta.url,
);
const registry = (await Bun.file(path).json()) as DocumentationRegistry;
const digest = "a".repeat(64);

describe("portable docs-only Runtime Index", () => {
  test("selects all86 registered routes and their aliases without execution admission", () => {
    const reader = createDocumentationReader(registry);
    expect(reader.registry.routes).toHaveLength(86);
    for (const route of registry.routes) {
      for (const topic of [route.topic, ...route.aliases]) {
        const plan = reader.select({ surface: route.surface, topic });
        expect(plan.status).toBe("SELECTED");
        expect(plan.primary?.id).toBe(route.id);
        expect(plan.executionAuthority).toBe("none");
        expect(plan.nativePermissionGranted).toBe(false);
        expect(Object.hasOwn(plan, "execution")).toBe(false);
      }
    }
  });
  test("TUI and slash aliases select partial frontend evidence without native admission", () => {
    const reader = createDocumentationReader(registry);
    for (const [topic, id, url] of [
      ["terminal-ui", "hermes.agent.tui", "user-guide/tui"],
      ["터미널UI".normalize("NFD"), "hermes.agent.tui", "user-guide/tui"],
      ["slash", "hermes.agent.slash-commands", "reference/slash-commands"],
      ["슬래시명령".normalize("NFD"), "hermes.agent.slash-commands", "reference/slash-commands"],
    ]) {
      const plan = reader.select({ surface: "hermes.agent", topic });
      expect(plan.status).toBe("SELECTED");
      expect(plan.primary?.id).toBe(id);
      expect(plan.primary?.canonicalUrl).toBe(`https://hermes-agent.nousresearch.com/docs/${url}`);
      expect(plan.primary?.evidenceStatus).toBe("page-opened-limited-content");
      expect(plan.primary?.documentationVersion).toBeUndefined();
      expect(plan.primary?.notes).toContain("TUI /sessions lists live sessions");
      expect(plan.evidenceGaps).toContain("route-body-not-fully-reviewed");
      expect(plan.retrieval).toBe("REVALIDATE_SELECTED_PAGE");
      expect(plan.executionAuthority).toBe("none");
      expect(plan.nativePermissionGranted).toBe(false);
      expect(Object.hasOwn(plan, "execution")).toBe(false);
      const source = reader.registry.sources[plan.primary!.sourceId!];
      expect(source?.checkedOn).toBe("2026-10-17");
      expect(source?.rawBytesVerified).toBe(false);
      expect(source?.bodySha256).toBeNull();
    }
  });
  test("new route evidence leaves Oct14 capture and family provenance historical", () => {
    const reader = createDocumentationReader(registry);
    expect(reader.registry.provenance.sourceDirectory).toBe(
      "discovery/runtime-documentation-routing/20261014/reviewed",
    );
    expect(reader.registry.provenance.sourceArtifacts).toHaveLength(6);
    expect(reader.registry.provenance.sourceArtifacts[0]).toEqual({
      path: "routing.json",
      sha256: "968068af391a6d6948ba047933172fb5030c3fcc430a3a66199a3e4e567ba48a",
    });
    expect(reader.registry.captures).toHaveLength(28);
    expect(
      reader.registry.captures.some((capture) => capture.retrievedAt.startsWith("2026-10-17")),
    ).toBe(false);
    const historical = reader.select({ surface: "hermes.agent", topic: "sessions" }).primary!;
    expect(historical.checkedOn).toBe("2026-10-14");
    expect(historical.evidenceStatus).toBe("user-supplied-index-family");
    expect(historical.sourceId).toBe("hermes-index");
    expect(reader.registry.sources[historical.sourceId!]!.checkedOn).toBe("2026-10-14");
    expect(reader.registry.limitations.join(" ")).toContain(
      "No native Scrapling capture or Evidence Foundry blob admission is claimed",
    );
  });
  test("TUI version observations direct retrieval without certifying documentation compatibility", () => {
    const reader = createDocumentationReader(registry);
    expect(
      reader.select({ surface: "hermes.agent", topic: "tui", freshness: "installed" }).retrieval,
    ).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
    const installed = reader.select(
      { surface: "hermes.agent", topic: "slash", freshness: "installed" },
      { installedVersion: "0.22.0" },
    );
    expect(installed.retrieval).toBe("REVALIDATE_SELECTED_PAGE");
    expect(installed.primary?.documentationVersion).toBeUndefined();
    expect(installed.evidenceGaps).toContain(
      "confirm-version-matched-release-or-source-if-current-page-cannot-substantiate-this-version",
    );
    const conflict = reader.select(
      { surface: "hermes.agent", topic: "tui", freshness: "installed", targetVersion: "0.22.0" },
      { installedVersion: "0.21.0" },
    );
    expect(conflict.retrieval).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
    expect(conflict.evidenceGaps).toContain("requested-version-differs-from-installed-version");
    expect(conflict.nativePermissionGranted).toBe(false);
  });
  test("TUI routes do not infer a subject from its host or steal historical aliases", () => {
    const plan = createDocumentationReader(registry).select({
      surface: "hermes",
      topic: "tui",
      executionHost: "hostinger.container",
    });
    expect(plan.status).toBe("NEEDS_DISAMBIGUATION");
    expect(plan.primary).toBeNull();
    expect(plan.discovery).toContain("hermes.agent");
    const collision = structuredClone(registry);
    collision.routes.find((route) => route.id === "hermes.agent.tui")!.aliases.push("sessions");
    expect(() => createDocumentationReader(collision)).toThrow("AMBIGUOUS_TOPIC_ALIAS");
  });
  test("TUI cache reuse needs this page and current-task revalidation without promoting evidence", () => {
    const reader = createDocumentationReader(registry);
    const request = { surface: "hermes.agent", topic: "tui", freshness: "latest" };
    const cache = {
      canonicalUrl: reader.select(request).primary!.canonicalUrl,
      bodySha256: digest,
      independentlyConfirmedBodySha256: digest,
      revalidatedInCurrentTask: false,
    };
    expect(reader.select(request, { cache }).retrieval).toBe("REVALIDATE_SELECTED_PAGE");
    expect(
      reader.select(request, {
        cache: {
          ...cache,
          canonicalUrl: registry.sources["hermes-index"]!.url,
          revalidatedInCurrentTask: true,
        },
      }).retrieval,
    ).toBe("REVALIDATE_SELECTED_PAGE");
    const revalidated = reader.select(request, {
      cache: { ...cache, revalidatedInCurrentTask: true },
    });
    expect(revalidated.retrieval).toBe("REUSE_MATCHING_PAGE");
    expect(revalidated.evidenceGaps).toContain("route-body-not-fully-reviewed");
    expect(revalidated.primary?.evidenceStatus).toBe("page-opened-limited-content");
  });
  test("normalizes Unicode topic aliases and explicit surface names", () => {
    const reader = createDocumentationReader(registry);
    expect(
      reader.select({ surface: " CODEX.CLI ", topic: "훅".normalize("NFD") }).primary?.id,
    ).toBe("codex.cli.hooks");
  });
  test("host companions do not replace the question subject or infer a version", () => {
    const plan = createDocumentationReader(registry).select({
      surface: "codex.cli",
      topic: "hooks",
      executionHost: "github.actions.hosted",
    });
    expect(plan.primary?.surface).toBe("codex.cli");
    expect(plan.secondary.map((route) => route.id)).toEqual([
      "github.actions.workflow",
      "github.actions.security",
    ]);
    expect(plan.request.targetVersion).toBeUndefined();
    expect(plan.secondary.length + 1).toBeLessThanOrEqual(plan.limits.maxPages);
  });
  test("missing subjects and topics return bounded discovery without a native HOLD", () => {
    const reader = createDocumentationReader(registry);
    const ambiguous = reader.select({ surface: "github", topic: "permissions" });
    expect(ambiguous.status).toBe("NEEDS_DISAMBIGUATION");
    expect(ambiguous.discovery.every((surface) => surface.startsWith("github."))).toBe(true);
    const missing = reader.select({ surface: "codex.cli", topic: "unknown topic" });
    expect(missing.status).toBe("NEEDS_DISCOVERY");
    expect(missing.discovery).toEqual([registry.surfaces["codex.cli"]!.index]);
    expect(Object.hasOwn(missing, "execution")).toBe(false);
  });
  test("rejects authority, account and runtime-action fields from task requests", () => {
    const reader = createDocumentationReader(registry);
    for (const key of [
      "intent",
      "effect",
      "trusted",
      "scopeAllowed",
      "nativeApprovalVerified",
      "billingIdentityConfirmed",
      "executionAuthority",
      "nativePermissionGranted",
      "authenticationVerified",
    ]) {
      expect(() => reader.select({ surface: "codex.cli", topic: "hooks", [key]: true })).toThrow(
        "UNTRUSTED_OR_UNKNOWN_FIELD",
      );
    }
    for (const request of [
      null,
      [],
      { surface: "codex.cli", topic: "\n" },
      { surface: "codex.cli", topic: "hooks", freshness: [] },
      { surface: "codex.cli", topic: "hooks", executionHost: "vps.unknown" },
    ]) {
      expect(() => reader.select(request)).toThrow();
    }
  });
  test("clones and freezes the registry once; later caller mutation cannot alter selection", () => {
    const input = structuredClone(registry);
    const reader = createDocumentationReader(input);
    const selected = reader.select({ surface: "codex.cli", topic: "hooks" });
    const original = selected.primary!.canonicalUrl;
    input.routes.find((route) => route.id === "codex.cli.hooks")!.canonicalUrl =
      "https://evil.example/";
    input.routes.length = 0;
    expect(reader.select({ surface: "codex.cli", topic: "hooks" }).primary?.canonicalUrl).toBe(
      original,
    );
    expect(reader.registry.routes).toHaveLength(86);
    expect(Object.isFrozen(reader.registry.routes)).toBe(true);
    expect(Object.isFrozen(selected.primary)).toBe(true);
    expect(() =>
      Object.assign(selected.primary!, { canonicalUrl: "https://evil.example/" }),
    ).toThrow();
    expect(reader.registrySha256).toMatch(/^[a-f0-9]{64}$/);
  });
  test("rejects unsafe official URLs and cross-document representations", () => {
    for (const url of [
      "http://docs.github.com/en/actions",
      "https://docs.github.com.evil.example/en/actions",
      "https://user@docs.github.com/en/actions",
      "https://docs.github.com:443/en/actions",
      "https://docs.github.com/en/%0aactions",
      "https://docs.github.com\\@evil.example/x",
      "https:////docs.github.com/en/actions",
      "file:///etc/passwd",
    ])
      expect(safeDocumentationUrl(url)).toBe(false);
    const bad = structuredClone(registry);
    bad.routes.find((route) => route.id === "github.actions.security")!.preferredUrl =
      "https://docs.github.com/api/article/body?pathname=/en/actions/other-page";
    expect(() => createDocumentationReader(bad)).toThrow("DOCUMENT_REPRESENTATION_MISMATCH");
    const duplicateQuery = structuredClone(registry);
    duplicateQuery.routes.find((route) => route.id === "github.actions.security")!.preferredUrl +=
      "&pathname=/different";
    expect(() => createDocumentationReader(duplicateQuery)).toThrow(
      "DOCUMENT_REPRESENTATION_MISMATCH",
    );
  });
  test("rejects same-origin but wrong-surface URLs, ambiguous aliases and unsafe provenance", () => {
    const wrongSurface = structuredClone(registry);
    wrongSurface.routes[0]!.canonicalUrl = "https://docs.github.com/en/actions";
    expect(() => createDocumentationReader(wrongSurface)).toThrow("UNSAFE_ROUTE");
    const aliases = structuredClone(registry);
    const hooks = aliases.routes.find((route) => route.id === "codex.cli.hooks")!;
    hooks.aliases.push(
      aliases.routes.find((route) => route.surface === "codex.cli" && route.id !== hooks.id)!.topic,
    );
    expect(() => createDocumentationReader(aliases)).toThrow("AMBIGUOUS_TOPIC_ALIAS");
    const provenance = structuredClone(registry);
    provenance.provenance.sourceDirectory = "/home/private";
    expect(() => createDocumentationReader(provenance)).toThrow("INVALID_PROVENANCE");
  });
  test("installed version gaps affect documentation retrieval only", () => {
    const reader = createDocumentationReader(registry);
    const request = { surface: "codex.cli", topic: "hooks", freshness: "installed" };
    expect(reader.select(request).retrieval).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
    const conflict = reader.select(
      { ...request, targetVersion: "0.162.1" },
      { installedVersion: "0.161.0" },
    );
    expect(conflict.retrieval).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
    expect(conflict.evidenceGaps).toContain("requested-version-differs-from-installed-version");
    expect(conflict.nativePermissionGranted).toBe(false);
    expect(Object.hasOwn(conflict, "execution")).toBe(false);
  });
  test("explicit documentation versions cannot stand in for installed compatibility", () => {
    const plan = createDocumentationReader(registry).select(
      { surface: "muse.code", topic: "protocol", freshness: "installed" },
      { installedVersion: "1.4.7" },
    );
    expect(plan.primary?.documentationVersion).toBe("1.4");
    expect(plan.retrieval).toBe("READ_VERSION_MATCHED_SOURCE");
    expect(plan.evidenceGaps).toContain("documentation-version-differs-from-applicable-version");
  });
  test("cache reuse binds canonical page, exact applicable version and confirmed body", () => {
    const reader = createDocumentationReader(registry);
    const url = reader.select({ surface: "codex.cli", topic: "hooks" }).primary!.canonicalUrl;
    const request = { surface: "codex.cli", topic: "hooks", freshness: "installed" };
    const cache = {
      canonicalUrl: url,
      bodySha256: digest,
      independentlyConfirmedBodySha256: digest,
      revalidatedInCurrentTask: false,
      targetVersion: "0.162.1",
    };
    expect(reader.select(request, { installedVersion: "0.162.1", cache }).retrieval).toBe(
      "REUSE_MATCHING_PAGE",
    );
    expect(reader.select(request, { installedVersion: "0.161.0", cache }).retrieval).toBe(
      "READ_SELECTED_PAGE",
    );
    expect(
      reader.select(request, {
        installedVersion: "0.162.1",
        cache: { ...cache, independentlyConfirmedBodySha256: "b".repeat(64) },
      }).retrieval,
    ).toBe("READ_SELECTED_PAGE");
    expect(
      reader.select({ ...request, freshness: "latest", targetVersion: "0.162.1" }, { cache })
        .retrieval,
    ).toBe("REVALIDATE_SELECTED_PAGE");
    expect(
      reader.select(
        { ...request, freshness: "latest", targetVersion: "0.162.1" },
        { cache: { ...cache, revalidatedInCurrentTask: true } },
      ).retrieval,
    ).toBe("REUSE_MATCHING_PAGE");
  });
  test("sensitive topics require current-task revalidation and gaps retain limited evidence", () => {
    const reader = createDocumentationReader(registry);
    expect(reader.select({ surface: "codex.cli", topic: "permissions" }).retrieval).toBe(
      "REVALIDATE_SELECTED_PAGE",
    );
    expect(reader.select({ surface: "codex.cli", topic: "hooks" }).evidenceGaps).toContain(
      "route-body-not-fully-reviewed",
    );
    expect(
      reader.registry.captures.filter((capture) => !capture.expectedPredicateMatched).length,
    ).toBeGreaterThan(0);
  });
  test("portable default reader returns identical snapshot identities across loads", async () => {
    const a = await readDocumentationRegistry(),
      b = await readDocumentationRegistry(path);
    expect(a.registrySha256).toBe(b.registrySha256);
    expect(a.select({ surface: "hermes.agent", topic: "tools" })).toEqual(
      b.select({ surface: "hermes.agent", topic: "tools" }),
    );
    expect(JSON.stringify(a.registry)).not.toMatch(
      /\/home\/|billingSnapshot|declaredUserContext|localAdapter/,
    );
  });
  test("file reader rejects oversized data and directories before JSON selection", async () => {
    const dir = await mkdtemp(join(tmpdir(), "runtime-docs-"));
    try {
      const file = join(dir, "oversized.json");
      await writeFile(file, " ".repeat(262_145));
      await expect(readDocumentationRegistry(file)).rejects.toThrow("REGISTRY_FILE_BUDGET");
      await expect(readDocumentationRegistry(dir)).rejects.toThrow("REGISTRY_FILE_BUDGET");
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
