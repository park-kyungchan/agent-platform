import type { GitWorkAlias, TypedLegacyAddress, Graph } from "./generated/v3.ts";
import { assertV3, canonicalV3 } from "./v3-codec.ts";
import { readExactV3Source, type ExactV3SourceReader } from "./external-artifacts.ts";
import { assertRecord } from "./records.ts";

const key = (a: TypedLegacyAddress): string => canonicalV3(a);
const family = (a: TypedLegacyAddress): boolean => a.kind === "work" ? a.scope.kind === "work" : a.kind === "task" || a.kind === "run" ? a.scope.kind === "task" : ["work", "task"].includes(a.scope.kind);
export function validateGitWorkAliases(aliases: GitWorkAlias[]): void {
  if (aliases.length > 2048) throw Error("V3_ALIAS_BOUND");
  const map = new Map<string, GitWorkAlias>();
  for (const a of aliases) {
    assertV3("GitWorkAlias", a);
    if (!family(a.from) || !family(a.to) || a.from.kind !== a.to.kind || canonicalV3(a.from.scope) !== canonicalV3(a.to.scope)) throw Error("V3_ALIAS_WRONG_KIND_OR_LOGICAL_IDENTITY");
    if (a.from.key === a.to.key || !a.namespace.trim() || !a.reason.trim()) throw Error("V3_ALIAS_REQUIRES_ACTUAL_LOCATOR_CHANGE");
    if (map.has(key(a.from))) throw Error("V3_ALIAS_AMBIGUOUS");
    map.set(key(a.from), a);
  }
  for (const a of aliases) {
    const seen = new Set<string>(); let address = a.from;
    while (map.has(key(address))) { const id = key(address); if (seen.has(id)) throw Error("V3_ALIAS_CYCLE"); seen.add(id); address = map.get(id)!.to; }
  }
}
export function resolveGitWorkAlias(aliases: GitWorkAlias[], address: TypedLegacyAddress): TypedLegacyAddress {
  validateGitWorkAliases(aliases); let current = address;
  for (let depth = 0; depth <= 2048; depth++) { const next = aliases.find((a) => key(a.from) === key(current)); if (!next) return structuredClone(current); current = next.to; }
  throw Error("V3_ALIAS_BOUND");
}
/** Actual source proof for the supported legacy locator adapter, not name guessing. */
export async function qualifyGitWorkAliases(aliases: GitWorkAlias[], reader: ExactV3SourceReader): Promise<void> {
  validateGitWorkAliases(aliases);
  for (const a of aliases) {
    const source = await readExactV3Source(reader, a.provenance); const record = assertRecord(source.document);
    const scope = a.from.scope;
    if (!("projectId" in scope) || !("workId" in scope) || record.projectId !== scope.projectId || record.workId !== scope.workId) throw Error("V3_ALIAS_SOURCE_SCOPE_MISMATCH");
    const issue = a.from.kind === "issue" && record.kind === "work";
    if (!issue && record.kind !== a.from.kind || a.from.key !== source.path + (issue ? "#/issueId" : "") || scope.kind === "task" && (record.kind !== "task" || record.id !== scope.taskId)) throw Error("V3_ALIAS_SOURCE_KIND_OR_LOCATOR_MISMATCH");
  }
}
