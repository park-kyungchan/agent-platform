import { test, expect } from "bun:test";
import { writeFile, readFile, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import queries from "./fixtures/public/corpus.public.queries.json" with { type: "json" };
import manifest from "./fixtures/public/corpus.public.json" with { type: "json" };
import { publicCorpusReader } from "./fixtures/public-reader.ts";
import { withMemoryQuery, createMemoryQueryView, memoryDigest } from "./v1.ts";
import { retrievalVersion } from "./ranking.ts";

test("fixed independent public Korean corpus measures the declared equivalent lexical exposure and permission controls", async () => {
  const root = new URL("../../../../", import.meta.url).pathname;
  const fixture = join(root, "packages/work-continuity/src/memory/fixtures/public");
  for (const [file, digest] of [
    ["corpus.public.json", "26ed9d6a800ebbdf249b1edf8858cdabe6c2958b67cadd6c463b2cc004e9ccf1"],
    [
      "corpus.public.records.json",
      "d1af44ead38d293bede2a6afa9ab689adf1896155c3a10797c36af56a4e82b99",
    ],
    [
      "corpus.public.queries.json",
      "918aa2d7ad0fb1d32ca0d47531429e872dfc93e00e7d6f8bd1c4ff7c8c0ffe0d",
    ],
  ])
    expect(memoryDigest(await readFile(join(fixture, file!)))).toBe(digest);
  const started = performance.now(),
    observations = [];
  let forbidden = 0,
    top1 = 0,
    relevantReturned = 0,
    totalReturned = 0,
    recallSum = 0,
    recallDenominator = 0,
    noResultCorrect = 0,
    labelChecks = 0,
    labelErrors = 0,
    falseInstructions = 0,
    eligibleInstructionOmissions = 0,
    conflictsCovered = 0,
    gapsCovered = 0,
    exposureMismatches = 0;
  for (const item of queries.queries) {
    const f = publicCorpusReader(item.query, item.queryId === "pub:18" ? 7 : 8);
    let transitionVerified = false;
    if (item.queryId === "pub:18") {
      const warm = createMemoryQueryView({ ...f.request, query: "권한 변경" }, f.owner);
      await warm.withCurrent((value) => value);
      f.state.generation = 8;
      f.trace.splice(0);
      const current = await warm.withCurrent((value) => value);
      expect(current.hits.some((hit) => f.sourceId(hit.source) === "fxs:17")).toBe(false);
      expect(JSON.stringify(current)).not.toContain("SYNTHETIC_DENIED_17");
      expect(JSON.stringify(warm)).not.toContain("SYNTHETIC_DENIED_17");
      warm.close();
      transitionVerified = true;
    }
    const result = await withMemoryQuery(f.request, f.owner, (value) => value);
    const ids = [...new Set(result.hits.map((hit) => f.sourceId(hit.source)))];
    const gold = new Set(
      item.expected.relevanceTiers
        .filter((tier) => tier.grade >= 2)
        .flatMap((tier) => tier.sourceIds),
    );
    const relevant = ids.filter((id) => gold.has(id)).length;
    if (
      item.expected.top1AnyOf.length &&
      (item.expected.top1AnyOf as readonly string[]).includes(ids[0] ?? "")
    )
      top1++;
    if (gold.size) {
      recallSum += relevant / gold.size;
      recallDenominator++;
    }
    relevantReturned += relevant;
    totalReturned += ids.length;
    if (item.expected.expectedNoResult && ids.length === 0) noResultCorrect++;
    const acquiredIds = [
      ...new Set(
        f.trace
          .filter((row) => row.phase === "body" && row.generation === 8)
          .map((row) => row.sourceId),
      ),
    ].sort();
    const parity =
      JSON.stringify(acquiredIds) ===
      JSON.stringify([...item.exposure.candidateEligibleSourceIds].sort());
    if (!parity) exposureMismatches++;
    const prohibited = f.trace.filter(
      (row) =>
        row.generation === 8 &&
        ["original", "body"].includes(row.phase) &&
        ["fxs:17", "fxs:18", "fxs:19"].includes(row.sourceId),
    );
    forbidden += prohibited.length;
    for (const hit of result.hits) {
      const id = f.sourceId(hit.source),
        expected = (
          item.expected.sourceLabels as Record<
            string,
            {
              recordStatus: string;
              authority: string;
              temporalStatus: string;
              currentInstructionEligible: boolean;
            }
          >
        )[id];
      if (expected) {
        labelChecks++;
        if (
          hit.document.record.status !== expected.recordStatus ||
          hit.document.record.authority !== expected.authority ||
          hit.temporalStatus !== expected.temporalStatus
        )
          labelErrors++;
        if (hit.currentInstructionEligible && !expected.currentInstructionEligible)
          falseInstructions++;
        if (expected.currentInstructionEligible && !hit.currentInstructionEligible)
          eligibleInstructionOmissions++;
      }
    }
    let conflictCovered = false;
    if (item.expected.mustSurfaceConflicts.length) {
      const expected = item.expected.mustSurfaceConflicts[0]!;
      conflictCovered = result.conflicts.some((conflict) =>
        expected.state === "unresolved"
          ? conflict.governingConflict && conflict.selectedGoverningValue === null
          : conflict.conflictingEvidence &&
            !conflict.governingConflict &&
            conflict.selectedGoverningValue !== null,
      );
      if (conflictCovered) conflictsCovered++;
    }
    if (
      item.expected.acquisitionGaps.length &&
      result.acquisitionGaps.length &&
      result.sourceCoverage === "partial"
    )
      gapsCovered++;
    observations.push({
      queryId: item.queryId,
      ids,
      gold: [...gold],
      top1AnyOf: item.expected.top1AnyOf,
      acquiredIds,
      exposureParity: parity,
      forbiddenUses: prohibited.length,
      matched: result.counts.matched,
      omissions: result.omissions,
      gaps: result.acquisitionGaps,
      coverage: result.sourceCoverage,
      conflicts: result.conflicts,
      transitionVerified,
      labels: result.hits.map((hit) => ({
        id: f.sourceId(hit.source),
        owner: hit.document.record.owner,
        revision: hit.document.record.revision,
        status: hit.document.record.status,
        authority: hit.document.record.authority,
        temporalStatus: hit.temporalStatus,
        instructionEligible: hit.currentInstructionEligible,
        source: hit.source,
      })),
      trace: f.trace,
    });
  }
  const report = {
    schema: "local.s02-public-relevance-observation.v1",
    corpusId: manifest.corpusId,
    corpusManifestSha256: "26ed9d6a800ebbdf249b1edf8858cdabe6c2958b67cadd6c463b2cc004e9ccf1",
    retrievalVersion,
    predeclaredEquivalentSubset: {
      queryIds: queries.queries.map((item) => item.queryId),
      denominator: 23,
      baselineAdapter: "native public-reader.r1 + current source/permission/status gate",
      candidateAdapter:
        "same exact adapter and permitted bodies; korean-technical-lexical.v1 weighted words/fragments/explicit technical aliases and score-tie Work context",
    },
    historicalApiTarget: {
      supportedQueries: 0,
      unsupportedQueries: 23,
      scores: null,
      reason:
        "These23generic B2 requests do not supply the historical required Work/dataset/Episode/claim-key adapter. Do not fabricate those records or score absence as ranking failure.",
    },
    target: {
      queries: 23,
      top1GoldQueries: 20,
      noResultQueries: 3,
      records: 23,
      permittedAvailableAt8: 20,
    },
    metrics: {
      top1: { numerator: top1, denominator: 20 },
      precisionAtLimit: {
        numerator: relevantReturned,
        denominator: totalReturned,
        value: totalReturned ? relevantReturned / totalReturned : null,
      },
      macroRecallAtLimit: {
        sum: recallSum,
        denominator: recallDenominator,
        value: recallDenominator ? recallSum / recallDenominator : null,
      },
      noResult: { numerator: noResultCorrect, denominator: 3 },
      forbiddenAcquisitionOrReuse: { violations: forbidden, requestDenominator: 23 },
      labels: {
        checks: labelChecks,
        errors: labelErrors,
        falseInstructions,
        eligibleInstructionOmissions,
      },
      conflictCoverage: { numerator: conflictsCovered, denominator: 2 },
      gapQueryCoverage: {
        numerator: gapsCovered,
        denominator: queries.queries.filter((item) => item.expected.acquisitionGaps.length).length,
      },
      exposureMismatches,
    },
    elapsedMs: performance.now() - started,
    cost: null,
    tokens: null,
    observations,
    nativePermissionGranted: false,
    AcceptanceDecision: null,
    limitations: [
      "Public fixed source corpus only; independent validation is separate. No untouched blind-generalization claim from this report.",
      "Query-only index/view created; no persistent snippets/embedding cache exists. Prior native copies remain their actual owner's custody.",
      "Lexical optional-Work adapter comparison is algorithm-only, not historical native B2 adoption.",
    ],
  };
  // Observations are disposable execution evidence, outside canonical source inputs.
  const outputRoot = await mkdtemp(join(tmpdir(), "memory-public-observation-"));
  await writeFile(
    join(outputRoot, "public.measurement." + retrievalVersion + ".json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  // The qualified runner retains this public synthetic report outside the source tree.
  console.log(JSON.stringify({ schema: "local.s02-public-relevance-transport.v1", observation: report }));
  expect(queries.queries).toHaveLength(23);
  expect(forbidden).toBe(0);
  expect(exposureMismatches).toBe(0);
  expect(labelErrors).toBe(0);
  expect(falseInstructions).toBe(0);
}, 30_000);
