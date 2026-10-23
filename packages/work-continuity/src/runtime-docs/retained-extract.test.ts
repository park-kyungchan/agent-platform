import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createDocumentationReader, safeDocumentationUrl } from "./index.ts";
import type { DocumentationRegistry, DocumentationRetainedExtract } from "./types.ts";

const registry = (await Bun.file(
  new URL("../../../../architecture/runtime-documentation-routing/registry.json", import.meta.url),
).json()) as DocumentationRegistry;
const sha = (value: string | Uint8Array) => createHash("sha256").update(value).digest("hex");
const jsonSha = (value: unknown) => sha(JSON.stringify(value));
const request = { surface: "typesafe.ai", topic: "primitives" };
const body = "# Retained source\nNamed state\nTyped question\nUnrelated section\n";
const extract: DocumentationRetainedExtract = {
  fileName: "primitives.md",
  extractSha256: sha(body),
  extractBytes: Buffer.byteLength(body),
  lineRanges: [[2, 3]],
  retrievedAt: "2026-10-19T00:30:57.215566+00:00",
  etag: null,
  lastModified: null,
};
function fixture(topic = "primitives", replacement = extract) {
  const input = structuredClone(registry);
  const route = input.routes.find((r) => r.surface === "typesafe.ai" && r.topic === topic)!;
  route.retainedExtract = structuredClone(replacement);
  return createDocumentationReader(input);
}
async function inDirectory(run: (directory: string) => Promise<void>) {
  const directory = await mkdtemp(join(tmpdir(), "runtime-docs-extract-"));
  try {
    await run(directory);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

describe("retained official concept routes", () => {
  test("preserves all original routes, sources, captures and provenance", () => {
    expect(jsonSha(registry.routes.slice(0, 98))).toBe(
      "da961c064785a28810eff1715ccb7a699709194b3fbb8828cc3a0f068483e1a2",
    );
    expect(
      jsonSha(
        Object.fromEntries(
          Object.entries(registry.sources).filter(([id]) => !id.startsWith("jev-docs-20261019-")),
        ),
      ),
    ).toBe("2481c4560b460bfa3a5a2be9e99205d74f0267dca4244247ef4b8af07969d728");
    expect(jsonSha(registry.captures)).toBe(
      "2150f18bdeb5ee63d4d3bcf45434a0865d64cac1d6141e3483f94aa3468c35fc",
    );
    expect(jsonSha(registry.provenance)).toBe(
      "9070b52bb9c9acddd5cf15c2ea0b5a12ce0451a6934212fca88ba6cadf988877",
    );
    expect(
      jsonSha(
        Object.fromEntries(
          Object.entries(registry.surfaces).filter(
            ([id]) => !["typesafe.ai", "vercel.ai-gateway"].includes(id),
          ),
        ),
      ),
    ).toBe("f489984249402c7f23233618667e38c559699d0b99903bca0ee1dfc77a238289");
  });
  test("keeps stable concepts distinct from applicable current facts", () => {
    const reader = createDocumentationReader(registry);
    for (const route of registry.routes.slice(98)) {
      const plan = reader.select({ surface: route.surface, topic: route.topic });
      expect(plan.retrieval).toBe(
        route.freshnessPolicy === "general" ? "READ_SELECTED_PAGE" : "REVALIDATE_SELECTED_PAGE",
      );
      expect(plan.primary?.documentationVersion).toBeUndefined();
      expect(plan.limits).toEqual({ maxPages: 3, maxExcerptChars: 6000, maxDiscoveryRequests: 2 });
      expect(plan.executionAuthority).toBe("none");
      expect(plan.nativePermissionGranted).toBe(false);
    }
    const price = reader.select({ surface: "typesafe.ai", topic: "price" });
    expect(price.primary?.topic).toBe("direct-pricing");
    expect(price.retrieval).toBe("REVALIDATE_SELECTED_PAGE");
    expect(price.primary?.notes).toContain("does not establish Gateway prices");
    for (const topic of ["unregistered schema field"]) {
      const plan = reader.select({ surface: "typesafe.ai", topic });
      expect(plan.retrieval).toBe("DISCOVER");
      expect(plan.discovery).toEqual(["https://docs.typesafe.ai/llms.txt"]);
    }
    const conflict = reader.select(
      { surface: "typesafe.ai", topic: "sdk", freshness: "installed", targetVersion: "1.14" },
      { installedVersion: "1.13" },
    );
    expect(conflict.retrieval).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
    expect(conflict.evidenceGaps).toContain("requested-version-differs-from-installed-version");
  });
  test("accepts only the new official hosts on their own subject surfaces", () => {
    expect(safeDocumentationUrl("https://docs.typesafe.ai/primitives")).toBe(true);
    expect(safeDocumentationUrl("https://vercel.com/docs/ai-gateway")).toBe(true);
    for (const url of [
      "https://docs.typesafe.ai.evil.invalid/primitives",
      "http://docs.typesafe.ai/primitives",
      "https://user@docs.typesafe.ai/primitives",
    ])
      expect(safeDocumentationUrl(url)).toBe(false);
    const input = structuredClone(registry);
    const route = input.routes.find((r) => r.id === "typesafe.ai.primitives")!;
    route.canonicalUrl = route.preferredUrl = "https://vercel.com/docs/ai-gateway";
    expect(() => createDocumentationReader(input)).toThrow("UNSAFE_ROUTE");
  });
});

describe("canonical bounded local extract read", () => {
  test("hashes actual bytes and exposes only the selected section as extract evidence", async () => {
    await inDirectory(async (directory) => {
      await writeFile(join(directory, extract.fileName), body);
      const reader = fixture();
      const result = await reader.readSelectedExtract(request, { directory });
      expect(result.plan.retrieval).toBe("REUSE_MATCHING_PAGE");
      expect(result.excerpt).toEqual({
        kind: "retained-markdown-extract",
        canonicalUrl: "https://docs.typesafe.ai/primitives",
        extractSha256: sha(body),
        excerptSha256: sha("Named state\nTyped question"),
        text: "Named state\nTyped question",
        retrievedAt: extract.retrievedAt,
        etag: null,
        lastModified: null,
      });
      expect(result.plan.evidenceGaps).toEqual([]);
      expect(result.plan.executionAuthority).toBe("none");
      const source = reader.registry.sources[result.plan.primary!.sourceId!]!;
      expect(source.rawBytesVerified).toBe(false);
      expect(source.bodySha256).toBeNull();
    });
  });
  test("requires exact current-task extract evidence for latest, schema and formulas", async () => {
    await inDirectory(async (directory) => {
      await writeFile(join(directory, extract.fileName), body);
      for (const topic of ["primitives", "api", "confidence-formulas"]) {
        const reader = fixture(topic);
        const selected = { ...request, topic, freshness: "latest" };
        const canonicalUrl = reader.select(selected).primary!.canonicalUrl;
        const observations = {
          cache: {
            canonicalUrl,
            bodySha256: extract.extractSha256,
            independentlyConfirmedBodySha256: extract.extractSha256,
            revalidatedInCurrentTask: true,
          },
        };
        const absent = await reader.readSelectedExtract(selected, { directory, observations });
        expect(absent.excerpt).toBeNull();
        expect(absent.plan.retrieval).toBe("REVALIDATE_SELECTED_PAGE");
        for (const revalidation of [
          {
            canonicalUrl: "https://docs.typesafe.ai/other",
            extractSha256: extract.extractSha256,
            observedInCurrentTask: true as const,
          },
          { canonicalUrl, extractSha256: "a".repeat(64), observedInCurrentTask: true as const },
          {
            canonicalUrl,
            extractSha256: extract.extractSha256,
            observedInCurrentTask: true as const,
            targetVersion: "unrequested",
          },
        ]) {
          const wrong = await reader.readSelectedExtract(selected, { directory, revalidation });
          expect(wrong.excerpt).toBeNull();
          expect(wrong.plan.retrieval).toBe("REVALIDATE_SELECTED_PAGE");
        }
        const current = await reader.readSelectedExtract(selected, {
          directory,
          revalidation: {
            canonicalUrl,
            extractSha256: extract.extractSha256,
            observedInCurrentTask: true,
          },
        });
        expect(current.plan.retrieval).toBe("REUSE_MATCHING_PAGE");
        expect(current.excerpt?.text).toBe("Named state\nTyped question");
      }
    });
  });
  test("does not infer installed/version compatibility from a retrieval date", async () => {
    await inDirectory(async (directory) => {
      await writeFile(join(directory, extract.fileName), body);
      const reader = fixture();
      const selected = { ...request, freshness: "installed", targetVersion: "1.14" };
      const missing = await reader.readSelectedExtract(selected, { directory });
      expect(missing.plan.retrieval).toBe("VERIFY_INSTALLED_VERSION_AND_READ");
      const observations = { installedVersion: "1.14" };
      const unproven = await reader.readSelectedExtract(selected, { directory, observations });
      expect(unproven.plan.retrieval).toBe("READ_VERSION_MATCHED_SOURCE");
      expect(unproven.excerpt).toBeNull();
      const current = await reader.readSelectedExtract(selected, {
        directory,
        observations,
        revalidation: {
          canonicalUrl: "https://docs.typesafe.ai/primitives",
          extractSha256: extract.extractSha256,
          observedInCurrentTask: true,
          targetVersion: "1.14",
        },
      });
      expect(current.plan.retrieval).toBe("REUSE_MATCHING_PAGE");
      expect(current.plan.primary?.documentationVersion).toBeUndefined();
      const conflicting = await reader.readSelectedExtract(
        { ...request, targetVersion: "1.14" },
        {
          directory,
          observations: { installedVersion: "1.13" },
          revalidation: {
            canonicalUrl: "https://docs.typesafe.ai/primitives",
            extractSha256: extract.extractSha256,
            observedInCurrentTask: true,
            targetVersion: "1.14",
          },
        },
      );
      expect(conflicting.excerpt).toBeNull();
      expect(conflicting.plan.evidenceGaps).toContain(
        "requested-version-differs-from-installed-version",
      );
    });
  });
  test("returns bounded lookup gaps for missing, tampered, nonregular and symlink files", async () => {
    await inDirectory(async (directory) => {
      const reader = fixture();
      const file = join(directory, extract.fileName);
      expect((await reader.readSelectedExtract(request, { directory })).excerpt).toBeNull();
      await writeFile(file, body.replace("Named", "Other"));
      const tampered = await reader.readSelectedExtract(request, { directory });
      expect(tampered.excerpt).toBeNull();
      expect(tampered.plan.evidenceGaps).toContain("retained-extract-hash-mismatch");
      await writeFile(file, body + "x");
      expect(
        (await reader.readSelectedExtract(request, { directory })).plan.evidenceGaps,
      ).toContain("retained-extract-file-or-byte-budget-mismatch");
      await rm(file);
      await mkdir(file);
      expect((await reader.readSelectedExtract(request, { directory })).excerpt).toBeNull();
      await rm(file, { recursive: true });
      await writeFile(join(directory, "other.md"), body);
      await symlink(join(directory, "other.md"), file);
      const linked = await reader.readSelectedExtract(request, { directory });
      expect(linked.excerpt).toBeNull();
      expect(linked.plan.retrieval).toBe("READ_SELECTED_PAGE");
    });
  });
  test("refuses missing sections and excerpts over the unchanged 6000 character bound", async () => {
    await inDirectory(async (directory) => {
      await writeFile(join(directory, extract.fileName), body);
      const missing = await fixture("primitives", {
        ...extract,
        lineRanges: [[100, 101]],
      }).readSelectedExtract(request, { directory });
      expect(missing.plan.evidenceGaps).toContain("retained-extract-section-missing");
      const longBody = "😀".repeat(6001);
      await writeFile(join(directory, extract.fileName), longBody);
      const long = await fixture("primitives", {
        ...extract,
        extractSha256: sha(longBody),
        extractBytes: Buffer.byteLength(longBody),
        lineRanges: [[1, 1]],
      }).readSelectedExtract(request, { directory });
      expect(long.excerpt).toBeNull();
      expect(long.plan.evidenceGaps).toContain("retained-extract-excerpt-budget");
    });
  });
  test("does not read an unrelated file for unknown topics or unretained model/SDK routes", async () => {
    const reader = createDocumentationReader(registry);
    for (const topic of ["unknown", "sdk"]) {
      const result = await reader.readSelectedExtract(
        { ...request, topic },
        { directory: "/missing" },
      );
      expect(result.excerpt).toBeNull();
      expect(result.plan.retrieval).toBe(
        topic === "unknown" ? "DISCOVER" : "REVALIDATE_SELECTED_PAGE",
      );
    }
  });
  test("retains absent validators and rejects unsafe bindings or invented validators", () => {
    for (const change of [
      { fileName: "../private.md" },
      { fileName: "/private.md" },
      { extractBytes: 262145 },
      { extractSha256: "not-a-digest" },
      {
        lineRanges: [
          [2, 3],
          [3, 4],
        ],
      },
      { lineRanges: [[0, 1]] },
      { etag: '"invented"' },
      { lastModified: "2026-10-19" },
    ]) {
      expect(() =>
        fixture("primitives", { ...extract, ...change } as DocumentationRetainedExtract),
      ).toThrow();
    }
    const gateway = registry.routes.find((r) => r.surface === "vercel.ai-gateway")!;
    expect(gateway.retainedExtract?.retrievedAt).toBeNull();
    expect(gateway.retainedExtract?.etag).toBeNull();
    expect(gateway.retainedExtract?.lastModified).toBeNull();
  });
});
