import { createHash } from "node:crypto";

/** Fixed transport for finite behavior calls, not a test-completion protocol. */
export interface SemanticCase {
  id: string;
  family: string;
  callers: string[];
  module: string;
  exportName: string;
  args: unknown[];
  expected:
    | { kind: "return"; value: unknown }
    | { kind: "undefined" }
    | { kind: "throw"; name: string; message: string }
    | {
        kind: "predicates";
        values: { pointer: string; equals: unknown }[];
        relations?: { left: string; right: string; equal: boolean }[];
        shapes?: { pointer: string; kind: "uuid-v4" | "sha256" | "git-oid" | "nonempty-array" }[];
      };
}
function canonical(value: unknown): string {
  if (value === null || typeof value === "string" || typeof value === "boolean")
    return JSON.stringify(value);
  if (typeof value === "number" && Number.isSafeInteger(value)) return String(value);
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (!value || typeof value !== "object" || Object.getPrototypeOf(value) !== Object.prototype)
    throw Error("TRUSTED_CI_SEMANTIC_JSON_REQUIRED");
  return (
    "{" +
    Object.keys(value)
      .sort()
      .map((key) => JSON.stringify(key) + ":" + canonical((value as Record<string, unknown>)[key]))
      .join(",") +
    "}"
  );
}
const hash = (value: unknown): string =>
  createHash("sha256").update(canonical(value)).digest("hex");
export function semanticRequest(c: SemanticCase) {
  return {
    schema: "agent-platform.behavior-call.v1",
    caseId: c.id,
    module: c.module,
    exportName: c.exportName,
    args: c.args,
  };
}
export const semanticRequestDigest = (c: SemanticCase): string => hash(semanticRequest(c));
export const semanticCaseDigest = (c: SemanticCase): string => hash(c);
export function assertedSemanticCase(c: SemanticCase) {
  return { kind: "externally-compared", caseSha256: hash(c), expected: c.expected };
}
function pointer(value: unknown, path: string): unknown {
  if (path === "") return value;
  if (!path.startsWith("/")) throw Error("Invalid fixed predicate pointer");
  for (const part of path.slice(1).split("/")) {
    const key = part.replaceAll("~1", "/").replaceAll("~0", "~");
    if (!value || typeof value !== "object" || !Object.hasOwn(value, key))
      throw Error("Missing predicate field");
    value = (value as Record<string, unknown>)[key];
  }
  return value;
}
/** Runs only on P's report host. Never imports candidate code or accepts a worker verdict. */
export function compareSemanticResult(c: SemanticCase, bytes: Uint8Array): unknown {
  const stdoutSha256 = createHash("sha256").update(bytes).digest("hex");
  try {
    if (!bytes.length || bytes.length > 128000) throw Error("bounded result required");
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    const frame = JSON.parse(text) as Record<string, unknown>;
    if (
      Object.keys(frame).sort().join() !== "caseId,outcome,requestSha256,schema" ||
      frame.schema !== "agent-platform.behavior-result.v1" ||
      frame.caseId !== c.id ||
      frame.requestSha256 !== semanticRequestDigest(c) ||
      JSON.stringify(frame) + "\n" !== text
    )
      throw Error("exact behavior frame required");
    const actual = frame.outcome as Record<string, unknown>;
    if (c.expected.kind === "predicates") {
      if (
        Object.keys(actual).sort().join() !== "kind,value" ||
        actual.kind !== "return" ||
        !c.expected.values.length ||
        !c.expected.values.every(
          (p) => canonical(pointer(actual.value, p.pointer)) === canonical(p.equals),
        ) ||
        !(c.expected.relations ?? []).every(
          (p) =>
            (canonical(pointer(actual.value, p.left)) ===
              canonical(pointer(actual.value, p.right))) ===
            p.equal,
        ) ||
        !(c.expected.shapes ?? []).every((p) => {
          const value = pointer(actual.value, p.pointer);
          if (p.kind === "nonempty-array") return Array.isArray(value) && value.length > 0;
          return (
            typeof value === "string" &&
            (p.kind === "uuid-v4"
              ? /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(value)
              : p.kind === "sha256"
                ? /^[a-f0-9]{64}$/.test(value)
                : /^[a-f0-9]{40}$/.test(value))
          );
        })
      )
        throw Error("behavior predicate mismatch");
    } else if (canonical(actual) !== canonical(c.expected)) throw Error("behavior mismatch");
    return assertedSemanticCase(c);
  } catch {
    return { kind: "behavior-mismatch", caseSha256: hash(c), stdoutSha256 };
  }
}
/** Small data codec for existing APIs taking Sets/bytes. No callbacks, paths or executable input. */
export function decodeBehaviorArgument(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(decodeBehaviorArgument);
  if (value && typeof value === "object") {
    const row = value as Record<string, unknown>;
    if (Object.keys(row).length === 1 && Array.isArray(row.$set))
      return new Set(row.$set.map(decodeBehaviorArgument));
    if (Object.keys(row).length === 1 && Array.isArray(row.$bytes)) {
      if (!row.$bytes.every((x) => Number.isInteger(x) && Number(x) >= 0 && Number(x) <= 255))
        throw Error("TRUSTED_CI_BEHAVIOR_BYTE_INPUT");
      return new Uint8Array(row.$bytes as number[]);
    }
    return Object.fromEntries(Object.entries(row).map(([k, v]) => [k, decodeBehaviorArgument(v)]));
  }
  return value;
}
/** Worker performs one call; assertions and expected values are absent from its request. */
export async function callBehavior(
  request: Record<string, unknown>,
  load: () => Promise<Record<string, unknown>>,
) {
  if (
    request.schema !== "agent-platform.behavior-call.v1" ||
    typeof request.caseId !== "string" ||
    typeof request.exportName !== "string" ||
    !Array.isArray(request.args) ||
    request.args.length > 8
  )
    throw Error("TRUSTED_CI_FIXED_BEHAVIOR_REQUEST_REQUIRED");
  const module = await load(); // Import failure is an interface failure, never an expected API throw.
  const fn = module[request.exportName];
  if (typeof fn !== "function") throw Error("TRUSTED_CI_REQUIRED_CANDIDATE_EXPORT_MISSING");
  const args = request.args.map(decodeBehaviorArgument);
  let outcome: unknown;
  try {
    const value: unknown = await fn(...args);
    outcome = value === undefined ? { kind: "undefined" } : { kind: "return", value };
  } catch (error) {
    if (!(error instanceof Error)) throw Error("TRUSTED_CI_NON_ERROR_THROW_UNSUPPORTED");
    outcome = { kind: "throw", name: error.name, message: error.message };
  }
  // Serialization failures occur outside the API-throw handler.
  canonical(outcome);
  const frame = {
    schema: "agent-platform.behavior-result.v1",
    caseId: request.caseId,
    requestSha256: hash(request),
    outcome,
  };
  if (Buffer.byteLength(JSON.stringify(frame)) > 128000)
    throw Error("TRUSTED_CI_BEHAVIOR_RESULT_LIMIT");
  return frame;
}

export interface BehaviorRun {
  stdout: Uint8Array;
  observation: { exitCode: number; timedOut: boolean; oomKilled: boolean; stdoutSha256: string };
}
/** Same supervisor in controller and real subprocess counterexamples. Record before rejection. */
export async function superviseSemanticCase<T extends BehaviorRun>(
  c: SemanticCase,
  run: () => Promise<T>,
  record: (result: T, actual: unknown) => void | Promise<void>,
): Promise<void> {
  const result = await run();
  const actual = compareSemanticResult(c, result.stdout);
  await record(result, actual);
  if (
    result.observation.exitCode !== 0 ||
    result.observation.timedOut ||
    result.observation.oomKilled ||
    result.observation.stdoutSha256 !== createHash("sha256").update(result.stdout).digest("hex") ||
    canonical(actual) !== canonical(assertedSemanticCase(c))
  )
    throw Error("TRUSTED_CI_REQUIRED_SEMANTIC_CASE_FAILED");
}
