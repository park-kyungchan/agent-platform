import type {
  LinkRecord,
  ObjectRecord,
  JsonObject,
  JsonValue,
} from "@palantirkc/ontology/operational";
import type {
  EvaluatorProperties as PersistedEvaluatorProperties,
  VerificationUnitProperties as PersistedVerificationUnitProperties,
  EvalMetric,
} from "@palantirkc/ontology/domains/evidence";

/** Eval configuration is JSON so every execution can preserve its complete input. */
export type { JsonValue, JsonObject } from "@palantirkc/ontology/operational";
export type Verdict = "supported" | "refuted" | "insufficient";
export type VerificationClass =
  "deterministic-verified" | "source-supported" | "model-judged" | "human-reviewed";
export type TemporalScope = "fixed-revision" | "current-state";
export type Fingerprints = Record<string, string>;

export type EvalObject = ObjectRecord;

export interface LocalActionRequest {
  actionType: string;
  projectId: string;
  idempotencyKey: string;
  input: JsonObject;
  expectedVersions?: Record<string, number>;
  actionTypeVersion?: string;
}

export interface FoundryEvalService {
  store?: {
    root?: string;
    readBlob(sha256: string): Promise<Uint8Array>;
    history?(id: string): Promise<{ sequence: number }[]>;
  };
  getObject(id: string): EvalObject | undefined | Promise<EvalObject | undefined>;
  queryObjects(query: {
    projectId: string;
    objectType?: string;
    search?: string;
    includeShared?: boolean;
  }): EvalObject[] | Promise<EvalObject[]>;
  traverseLinks?(
    id: string,
    depth?: number,
  ): { links: LinkRecord[] } | Promise<{ links: LinkRecord[] }>;
  putObjects(objects: EvalObject[], links?: LinkRecord[]): Promise<unknown>;
  runAction(request: LocalActionRequest, options?: { simulate?: boolean }): Promise<unknown>;
}

export type ExpectedPredicate = JsonObject & {
  operator: "equals" | "contains" | "nonempty" | "gte" | "lte" | "exists";
  path?: string;
  value?: JsonValue;
};

export interface VerificationUnitProperties extends PersistedVerificationUnitProperties {
  expectedPredicate: ExpectedPredicate;
  method: string;
  temporal?: TemporalScope;
  freshnessHours?: number;
  fingerprints?: Fingerprints;
  sourceRefs?: string[];
  objectiveVersion?: string;
}

export type Metric = EvalMetric;

export interface Evaluation {
  verdict: Verdict;
  verificationClass: VerificationClass;
  status: "completed" | "error" | "budget-exceeded";
  passed: boolean | null;
  metrics: Metric[];
  diagnostics: string[];
}

export interface EvaluatorProperties extends PersistedEvaluatorProperties {
  method:
    | "deterministic"
    | "source-supported"
    | "model-judged"
    | "human-reviewed"
    | "local-action-simulation"
    | "native-codex";
  objectiveVersion?: string;
  thresholdVersion?: string;
}

export interface SuiteBudget extends JsonObject {
  maxRuns: number;
  maxDurationMs: number;
  maxTotalTokens: number;
}

export interface Freshness {
  state: "fresh" | "stale" | "unknown";
  temporal: TemporalScope;
  checkedAt: string;
  expiresAt: string | null;
  changedFingerprints: string[];
  reasons: string[];
}

export function newEvalObject(
  objectType: string,
  projectId: string,
  properties: JsonObject,
  options: { id?: string; scope?: "project" | "shared"; now?: string } = {},
): EvalObject {
  const now = options.now ?? new Date().toISOString();
  return {
    schemaVersion: "1.0.0",
    kind: "object",
    id: options.id ?? `${objectType.toLowerCase()}-${crypto.randomUUID()}`,
    objectType,
    objectTypeVersion: "1.0.0",
    version: 1,
    projectId,
    scope: options.scope ?? "project",
    createdAt: now,
    updatedAt: now,
    properties,
  };
}

export function requireObjectProperties<T>(object: EvalObject, objectType: string): T {
  if (object.objectType !== objectType)
    throw new Error(`Expected ${objectType}, received ${object.objectType}: ${object.id}`);
  return object.properties as unknown as T;
}

/** Convert diagnostics and subprocess output into a JSON snapshot without undefined fields. */
export function jsonSnapshot(value: unknown): JsonValue {
  const encoded = JSON.stringify(value);
  if (encoded === undefined) return null;
  return JSON.parse(encoded) as JsonValue;
}
