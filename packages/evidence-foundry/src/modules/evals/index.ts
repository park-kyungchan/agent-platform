import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { join } from "node:path";
import type { LinkRecord } from "@palantirkc/ontology/operational";
import type { EvalDatasetProperties as PersistedDatasetProperties, EvalSuiteProperties as PersistedSuiteProperties } from "@palantirkc/ontology/domains/evidence";
import { importNativeCodexResult, parseNativeCodexResult, runNativeCodex, validateBudget } from "../../adapters/native-codex";
import type { NativeCodexResult, NativeCodexRunner } from "../../adapters/native-codex";
import { jsonSnapshot, newEvalObject, requireObjectProperties } from "./types";
import type { Evaluation, EvalObject, EvaluatorProperties, ExpectedPredicate, Fingerprints, FoundryEvalService, JsonObject, JsonValue, LocalActionRequest, Metric, SuiteBudget, VerificationClass, VerificationUnitProperties, Verdict } from "./types";

export * from "./types";

export interface EvalDatasetProperties extends PersistedDatasetProperties { revision?: string }
export interface EvalSuiteProperties extends PersistedSuiteProperties {
  budget?: SuiteBudget;
  revision?: string;
}

export interface RunSuiteOptions {
  now?: string;
  currentFingerprints?: Fingerprints;
  nativeRunner?: NativeCodexRunner;
  runDirectory?: string;
}

export interface EvalSuiteResult {
  run: EvalObject;
  caseResults: EvalObject[];
  commit: unknown;
}

function canonical(value: unknown): string {
  if (value === undefined) return "undefined";
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`).join(",")}}`;
}

export function fingerprint(value: unknown): string { return createHash("sha256").update(canonical(value)).digest("hex"); }
export function normalizeFingerprints(values: Fingerprints): Fingerprints { return Object.fromEntries(Object.entries(values).map(([key, value]) => [key, /^[a-f0-9]{64}$/.test(value) ? value : fingerprint(value)])); }

export function resolvePath(input: unknown, path?: string): unknown {
  if (!path) return input;
  return path.split(".").reduce<unknown>((value, key) => value !== null && typeof value === "object" && Object.hasOwn(value, key) ? (value as Record<string, unknown>)[key] : undefined, input);
}

function isNonempty(value: unknown): boolean {
  if (typeof value === "string" || Array.isArray(value)) return value.length > 0;
  if (value !== null && typeof value === "object") return Object.keys(value).length > 0;
  return value !== undefined && value !== null;
}

function checkPredicate(input: unknown, expected: ExpectedPredicate): { value: unknown; passed: boolean | null; error?: string } {
  const value = resolvePath(input, expected.path);
  if (!["equals", "contains", "nonempty", "gte", "lte", "exists"].includes(expected.operator)) return { value, passed: null, error: "Unknown expected predicate operator" };
  if (expected.operator === "exists") return { value, passed: value !== undefined && value !== null };
  if (value === undefined) return { value, passed: null, error: `Input path was not observed: ${expected.path ?? "root"}` };
  switch (expected.operator) {
    case "equals":
      return expected.value === undefined ? { value, passed: null, error: "Equals predicate requires a value" } : { value, passed: canonical(value) === canonical(expected.value) };
    case "contains":
      if (typeof value === "string" && typeof expected.value === "string") return { value, passed: value.includes(expected.value) };
      if (Array.isArray(value)) return { value, passed: value.some(item => canonical(item) === canonical(expected.value)) };
      return { value, passed: null, error: "Contains predicate requires text or an array" };
    case "nonempty": return { value, passed: isNonempty(value) };
    case "gte": case "lte":
      if (typeof value !== "number" || typeof expected.value !== "number" || !Number.isFinite(value) || !Number.isFinite(expected.value)) return { value, passed: null, error: "Numeric predicate requires finite numeric input and threshold" };
      return { value, passed: expected.operator === "gte" ? value >= expected.value : value <= expected.value };
  }
}

function evaluationClass(method: EvaluatorProperties["method"]): VerificationClass {
  if (method === "source-supported") return "source-supported";
  if (method === "model-judged" || method === "native-codex") return "model-judged";
  if (method === "human-reviewed") return "human-reviewed";
  return "deterministic-verified";
}

function insufficient(method: EvaluatorProperties["method"], diagnostic: string, status: Evaluation["status"] = "error"): Evaluation {
  return { verdict: "insufficient", verificationClass: evaluationClass(method), status, passed: null, metrics: [], diagnostics: [diagnostic] };
}

/** Evaluates one predicate only. Its objective and input are an immutable scoped snapshot. */
export function evaluateUnit(unit: VerificationUnitProperties, input: unknown, evaluator: EvaluatorProperties, context: { sourceAvailable?: boolean; nativeResult?: NativeCodexResult; forceModelJudged?: boolean } = {}): Evaluation {
  if (!unit.objective?.trim() || !unit.scope?.trim() || !unit.expectedPredicate || !unit.inputSnapshot || typeof unit.method !== "string") return insufficient(evaluator.method, "Verification unit requires one objective, scope, input snapshot, expected predicate, and method");
  if (!["deterministic", "source-supported", "model-judged", "human-reviewed", "local-action-simulation", "native-codex"].includes(evaluator.method)) return insufficient(evaluator.method, "Unknown evaluator method");
  if (context.nativeResult?.status !== undefined && context.nativeResult.status !== "completed") {
    const result = insufficient(evaluator.method, context.nativeResult.diagnostics.join("; ") || "Native execution did not complete", context.nativeResult.status === "budget-exceeded" ? "budget-exceeded" : "error");
    result.verificationClass = "model-judged";
    return result;
  }
  if (evaluator.method === "source-supported" && !context.sourceAvailable) return insufficient(evaluator.method, "Source support requires available referenced SourceCapture evidence");
  const objectiveVersion = unit.objectiveVersion ?? evaluator.objectiveVersion ?? "1.0.0";
  const thresholdVersion = unit.thresholdVersion ?? evaluator.thresholdVersion ?? "1.0.0";
  const checked = checkPredicate(input, unit.expectedPredicate);
  const metrics: Metric[] = [
    { name: "extractionNonempty", value: isNonempty(checked.value) ? 1 : 0, objectiveVersion, thresholdVersion, threshold: { operator: "gte", value: 1 }, passed: isNonempty(checked.value) },
    { name: "expectedPredicate", value: checked.passed === null ? null : typeof checked.value === "number" ? checked.value : checked.passed ? 1 : 0, objectiveVersion, thresholdVersion, threshold: jsonSnapshot(unit.expectedPredicate) as JsonObject, passed: checked.passed },
  ];
  let verificationClass = context.forceModelJudged ? "model-judged" : evaluationClass(evaluator.method);
  let verdict: Verdict = checked.passed === null ? "insufficient" : checked.passed ? "supported" : "refuted";
  let passed = checked.passed;
  const diagnostics = checked.error ? [checked.error] : [];
  if (evaluator.method === "model-judged" || evaluator.method === "native-codex") {
    verificationClass = "model-judged";
    const judgment = input !== null && typeof input === "object" ? (input as Record<string, unknown>).judgment ?? input : undefined;
    const reported = judgment !== null && typeof judgment === "object" ? (judgment as Record<string, unknown>).verdict : undefined;
    verdict = reported === "supported" || reported === "refuted" || reported === "insufficient" ? reported : "insufficient";
    passed = verdict === "insufficient" ? null : verdict === "supported";
    if (verdict === "insufficient") diagnostics.push("A model judgment with an explicit verdict is required; it cannot establish deterministic verification");
  }
  if (evaluator.method === "human-reviewed") {
    const review = input !== null && typeof input === "object" ? (input as Record<string, unknown>).review : undefined;
    const authority = review !== null && typeof review === "object" ? review as Record<string, unknown> : undefined;
    if (!authority || typeof authority.actorId !== "string" || typeof authority.reviewedAt !== "string" || !Number.isFinite(Date.parse(authority.reviewedAt))) {
      verdict = "insufficient"; passed = null; diagnostics.push("Human review requires an actorId and valid reviewedAt timestamp");
    }
  }
  return { verdict, verificationClass, status: checked.error && verificationClass !== "model-judged" ? "error" : "completed", passed, metrics, diagnostics };
}

function link(linkType: string, from: string, to: string, projectId: string, now: string): LinkRecord {
  return { schemaVersion: "1.0.0", kind: "link", id: `${linkType}-${crypto.randomUUID()}`, linkType, linkTypeVersion: "1.0.0", version: 1, from, to, projectId, scope: "project", createdAt: now };
}

async function required(foundry: FoundryEvalService, id: string, type: string): Promise<EvalObject> {
  const object = await foundry.getObject(id);
  if (!object || object.objectType !== type) throw new Error(`Missing ${type}: ${id}`);
  return object;
}

async function sourceInput(foundry: FoundryEvalService, unit: VerificationUnitProperties, sources: (EvalObject | undefined)[]): Promise<unknown> {
  const artifactSha256 = unit.inputSnapshot.artifactSha256;
  if (!unit.sourceRefs?.length || typeof artifactSha256 !== "string") throw new Error("Source support requires sourceRefs and a pinned inputSnapshot.artifactSha256");
  if (unit.expectedPredicate.path !== "text" && unit.expectedPredicate.path !== "value") throw new Error("Source-supported predicates must address captured text or value");
  const matched = sources.some(source => source?.objectType === "SourceCapture" && source.properties.outcome === "success" && Array.isArray(source.properties.artifacts) && source.properties.artifacts.some(artifact => artifact !== null && typeof artifact === "object" && !Array.isArray(artifact) && artifact.sha256 === artifactSha256 && typeof artifact.byteLength === "number" && artifact.byteLength > 0));
  if (!matched) throw new Error("Pinned artifact was not present in successful SourceCapture evidence");
  let text: string;
  if (foundry.store) text = new TextDecoder("utf-8", { fatal: true }).decode(await foundry.store.readBlob(artifactSha256));
  else {
    const captured = unit.inputSnapshot.text ?? unit.inputSnapshot.value;
    if (typeof captured !== "string" || createHash("sha256").update(captured).digest("hex") !== artifactSha256) throw new Error("Captured text bytes could not be resolved or did not match their pinned artifact digest");
    text = captured;
  }
  return { ...unit.inputSnapshot, text, value: text };
}

export async function collectFingerprints(foundry: FoundryEvalService, unit: EvalObject, evaluator?: EvalObject, extra: EvalObject[] = [], provided: Fingerprints = {}, sourceSnapshots?: Map<string, EvalObject | undefined>): Promise<Fingerprints> {
  const properties = requireObjectProperties<VerificationUnitProperties>(unit, "VerificationUnit");
  const values: Fingerprints = { ...(properties.fingerprints ?? {}), ...normalizeFingerprints(provided), "runtime:bun": fingerprint({ runtime: "bun", version: Bun.version }) };
  for (const object of [unit, ...(evaluator ? [evaluator] : []), ...extra]) values[`object:${object.id}`] = fingerprint(object);
  values[`snapshot:${unit.id}`] = fingerprint({ inputSnapshot: properties.inputSnapshot, expectedPredicate: properties.expectedPredicate });
  for (const sourceId of properties.sourceRefs ?? []) {
    const source = sourceSnapshots?.has(sourceId) ? sourceSnapshots.get(sourceId) : await foundry.getObject(sourceId);
    values[`source:${sourceId}`] = source ? fingerprint(source.properties) : fingerprint({ missingSource: sourceId });
  }
  return values;
}

/** Versioned suite runner. Case metrics, judgments, diagnostics and acceptance are independent. */
export async function runSuite(foundry: FoundryEvalService, suiteId: string, options: RunSuiteOptions = {}): Promise<EvalSuiteResult> {
  const suite = await required(foundry, suiteId, "EvalSuite");
  const properties = requireObjectProperties<EvalSuiteProperties>(suite, "EvalSuite");
  const claimsAtStart = await foundry.queryObjects({ projectId: suite.projectId, objectType: "Claim", includeShared: true });
  const startedAt = options.now ?? new Date().toISOString();
  const startedClock = performance.now();
  const initialRun = newEvalObject("EvalRun", suite.projectId, { suiteRef: suite.id, status: "running", startedAt, suiteVersion: String(suite.version), suiteSnapshot: jsonSnapshot(suite), targetSnapshot: properties.target }, { now: startedAt });
  await foundry.putObjects([initialRun], [link("run-suite", initialRun.id, suite.id, suite.projectId, startedAt)]);
  const caseResults: EvalObject[] = [];
  const links: LinkRecord[] = [];
  const sourceLinks = new Set<string>();
  const diagnostics: string[] = [];
  let dataset: EvalObject | undefined;
  let nativeRuns = 0;
  let totalTokens = 0;
  try {
    dataset = await required(foundry, properties.datasetRef, "EvalDataset");
    const datasetProperties = requireObjectProperties<EvalDatasetProperties>(dataset, "EvalDataset");
    const iterations = properties.iterations ?? 1;
    if (!Number.isSafeInteger(iterations) || iterations < 1) throw new Error("Suite iterations must be a positive integer");
    if (!Array.isArray(datasetProperties.caseRefs) || !datasetProperties.caseRefs.length || !Array.isArray(properties.evaluatorRefs) || !properties.evaluatorRefs.length) throw new Error("Suite requires a nonempty versioned dataset and evaluators");
    const evaluators = await Promise.all(properties.evaluatorRefs.map(id => required(foundry, id, "Evaluator")));
    for (const unitId of datasetProperties.caseRefs) {
      const unitObject = await required(foundry, unitId, "VerificationUnit");
      const unit = requireObjectProperties<VerificationUnitProperties>(unitObject, "VerificationUnit");
      if (foundry.traverseLinks) for (const existing of (await foundry.traverseLinks(unitObject.id)).links) if (existing.linkType === "capture-unit" && existing.to === unitObject.id) sourceLinks.add(`${existing.from}:${unitObject.id}`);
      for (let iteration = 1; iteration <= iterations; iteration++) {
        for (const evaluatorObject of evaluators) {
          const evaluator = { ...requireObjectProperties<EvaluatorProperties>(evaluatorObject, "Evaluator"), ...(properties.objectiveVersion ? { objectiveVersion: properties.objectiveVersion } : {}), ...(properties.thresholdVersion ? { thresholdVersion: properties.thresholdVersion } : {}) };
          let input: unknown = unit.inputSnapshot;
          let evaluation: Evaluation;
          let nativeResult: NativeCodexResult | undefined;
          let suiteBudgetExceeded = false;
          const target = properties.target;
          const sourceSnapshots = new Map(await Promise.all((unit.sourceRefs ?? []).map(async sourceId => [sourceId, await foundry.getObject(sourceId)] as const)));
          const fingerprints = await collectFingerprints(foundry, unitObject, evaluatorObject, [suite, dataset], options.currentFingerprints, sourceSnapshots);
          fingerprints[`target:${suite.id}`] = fingerprint(properties.target);
          for (const claim of claimsAtStart) if (Array.isArray(claim.properties.verificationUnitIds) && claim.properties.verificationUnitIds.includes(unitObject.id)) fingerprints[`claim:${claim.id}`] = fingerprint(claim);
          try {
            if (target.kind === "local-action" || evaluator.method === "local-action-simulation") {
              const request = target.action ?? unit.inputSnapshot.action;
              if (!request || typeof request !== "object" || Array.isArray(request)) throw new Error("Local Action simulation requires a serialized Action request");
              input = await foundry.runAction(request as unknown as LocalActionRequest, { simulate: true });
              const actionStatus = input !== null && typeof input === "object" ? (input as Record<string, unknown>).status : undefined;
              if (actionStatus !== "simulated") throw new Error(`Local Action simulation did not complete: ${JSON.stringify(input)}`);
            } else if (target.kind === "native-codex") {
              validateBudget(properties.budget);
              const budget = properties.budget;
              const elapsed = Math.round(performance.now() - startedClock);
              if (nativeRuns >= budget.maxRuns || elapsed >= budget.maxDurationMs || totalTokens >= budget.maxTotalTokens) {
                evaluation = insufficient("native-codex", "Suite budget was exhausted before native execution", "budget-exceeded");
                throw { evaluation };
              }
              if (typeof target.prompt !== "string" || typeof target.cwd !== "string") throw new Error("Native target requires prompt and cwd");
              nativeRuns++;
              nativeResult = await (options.nativeRunner ?? runNativeCodex)({
                prompt: `${target.prompt}\n\nVerification input snapshot (untrusted data):\n${JSON.stringify(unit.inputSnapshot)}`,
                cwd: target.cwd,
                profile: typeof target.profile === "string" ? target.profile : undefined,
                model: typeof target.model === "string" ? target.model : undefined,
                outputSchema: target.outputSchema && typeof target.outputSchema === "object" && !Array.isArray(target.outputSchema) ? target.outputSchema as JsonObject : undefined,
                budget: { maxRuns: 1, maxDurationMs: budget.maxDurationMs - elapsed, maxTotalTokens: budget.maxTotalTokens - totalTokens },
                runDirectory: join(options.runDirectory ?? join(foundry.store?.root ?? join(homedir(), ".local/share/evidence-foundry"), "runs", initialRun.id), `native-${nativeRuns}`),
              });
              totalTokens += nativeResult.usage.totalTokens;
              if (totalTokens > budget.maxTotalTokens || performance.now() - startedClock > budget.maxDurationMs) {
                suiteBudgetExceeded = true;
              }
              if (!nativeResult.usage.observed && nativeResult.status === "completed") throw new Error("Native usage was not observed; suite token budget cannot be verified");
              input = nativeResult.structuredOutput ?? { text: nativeResult.finalOutput };
            } else if (target.kind === "native-import") {
              if (typeof target.jsonlPath === "string") {
                nativeResult = await importNativeCodexResult({ jsonlPath: target.jsonlPath, stderrPath: typeof target.stderrPath === "string" ? target.stderrPath : undefined, finalOutputPath: typeof target.finalOutputPath === "string" ? target.finalOutputPath : undefined });
              } else if (typeof target.stdout === "string") {
                nativeResult = parseNativeCodexResult({ stdout: target.stdout, stderr: typeof target.stderr === "string" ? target.stderr : undefined, exitCode: typeof target.exitCode === "number" ? target.exitCode : undefined });
              } else throw new Error("Native import requires an existing jsonlPath or stdout snapshot");
              input = nativeResult.structuredOutput ?? { text: nativeResult.finalOutput };
            }
            let sourceAvailable = false;
            if (evaluator.method === "source-supported") { input = await sourceInput(foundry, unit, [...sourceSnapshots.values()]); sourceAvailable = true; }
            evaluation = evaluateUnit(unit, input, evaluator, { sourceAvailable, nativeResult, forceModelJudged: target.kind === "native-codex" || target.kind === "native-import" });
            if (suiteBudgetExceeded) evaluation = insufficient("native-codex", "Suite aggregate budget was exceeded; emitted usage does not provide a provider hard token cap", "budget-exceeded");
          } catch (error) {
            evaluation = error !== null && typeof error === "object" && "evaluation" in error ? (error as { evaluation: Evaluation }).evaluation : insufficient(target.kind === "native-codex" ? "native-codex" : evaluator.method, String(error));
          }
          const verifiedAt = options.now ?? new Date().toISOString();
          let observedAt = typeof unit.inputSnapshot.observedAt === "string" ? unit.inputSnapshot.observedAt : verifiedAt;
          if (evaluator.method === "source-supported" && unit.sourceRefs?.length) {
            const observations = [...sourceSnapshots.values()].map(source => source?.properties.retrievedAt).filter((time): time is string => typeof time === "string" && Number.isFinite(Date.parse(time)));
            if (observations.length) observedAt = observations.sort((a, b) => Date.parse(a) - Date.parse(b))[0]!;
          }
          const result = newEvalObject("CaseResult", suite.projectId, {
            runRef: initialRun.id, unitRef: unitObject.id, evaluatorRef: evaluatorObject.id, iteration,
            ...jsonSnapshot(evaluation) as JsonObject,
            verifiedAt, observedAt, recordedAt: new Date().toISOString(), fingerprints, temporal: unit.temporal ?? "current-state", freshnessHours: unit.freshnessHours ?? 24,
            inputSnapshot: jsonSnapshot(input), unitVersion: unitObject.version, evaluatorVersion: String(evaluatorObject.version),
            evidenceRefs: unit.sourceRefs ?? [],
            unitSnapshot: jsonSnapshot(unitObject), evaluatorSnapshot: jsonSnapshot(evaluatorObject),
            ...(nativeResult ? { nativeResult: jsonSnapshot(nativeResult), suiteBudgetExceeded } : {}),
          }, { now: verifiedAt });
          caseResults.push(result);
          links.push(link("run-result", initialRun.id, result.id, suite.projectId, verifiedAt), link("result-unit", result.id, unitObject.id, suite.projectId, verifiedAt), link("result-evaluator", result.id, evaluatorObject.id, suite.projectId, verifiedAt));
          for (const sourceId of unit.sourceRefs ?? []) {
            const key = `${sourceId}:${unitObject.id}`;
            if (foundry.traverseLinks && !sourceLinks.has(key) && (await foundry.getObject(sourceId))?.objectType === "SourceCapture") {
              sourceLinks.add(key); links.push(link("capture-unit", sourceId, unitObject.id, suite.projectId, verifiedAt));
            }
          }
        }
      }
    }
    const claims = await foundry.queryObjects({ projectId: suite.projectId, objectType: "Claim", includeShared: true });
    for (const claim of claims) {
      const unitIds = claim.properties.verificationUnitIds;
      if (!Array.isArray(unitIds)) continue;
      for (const result of caseResults) if (unitIds.includes(result.properties.unitRef as JsonValue)) links.push(link("claim-result", claim.id, result.id, suite.projectId, result.createdAt));
    }
  } catch (error) { diagnostics.push(String(error)); }
  const budgetExceeded = caseResults.some(result => result.properties.status === "budget-exceeded");
  const errors = caseResults.filter(result => result.properties.status === "error").length;
  const status = budgetExceeded ? "budget-exceeded" : diagnostics.length || errors ? "failed" : "completed";
  const completedAt = options.now ?? new Date().toISOString();
  const run: EvalObject = { ...initialRun, version: 2, updatedAt: completedAt, properties: {
    ...initialRun.properties, status, completedAt, finishedAt: completedAt, results: caseResults.map(result => result.id), diagnostics,
    ...(dataset ? { datasetVersion: String(dataset.version), datasetSnapshot: jsonSnapshot(dataset) } : {}),
    metrics: {
      caseCount: caseResults.length, supported: caseResults.filter(result => result.properties.verdict === "supported").length,
      refuted: caseResults.filter(result => result.properties.verdict === "refuted").length,
      insufficient: caseResults.filter(result => result.properties.verdict === "insufficient").length,
      passed: caseResults.filter(result => result.properties.passed === true).length,
      failed: caseResults.filter(result => result.properties.passed === false).length, errors,
    },
    usage: { nativeRuns, totalTokens, elapsedMs: Math.round(performance.now() - startedClock) },
    budgetSnapshot: jsonSnapshot(properties.budget),
  } };
  try {
    const commit = await foundry.putObjects([run, ...caseResults], links);
    return { run, caseResults, commit };
  } catch (error) {
    const failedRun: EvalObject = { ...run, properties: { ...run.properties, status: "failed", results: [], diagnostics: [...diagnostics, `Final eval batch could not be committed: ${String(error)}`], diagnosticCaseResults: jsonSnapshot(caseResults) } };
    const commit = await foundry.putObjects([failedRun]);
    return { run: failedRun, caseResults: [], commit };
  }
}
