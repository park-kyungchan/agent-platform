// Public derivative: retained packet-reader and local discovery result contracts.
// Structural evidence only. No annotation format or native permission is invented.
import { object, integer, nonempty, sha } from "./source-coverage.ts";
import { dataSnapshot } from "./data-snapshot.ts";
export function ownList(value: any, valid: (item: any) => boolean): boolean {
  try {
    value = dataSnapshot(value);
  } catch {
    return false;
  }
  if (!Array.isArray(value)) return false;
  for (let i = 0; i < value.length; i++)
    if (!Object.hasOwn(value, i) || !valid(value[i])) return false;
  return true;
}
function fields(value: any, required: string[], optional: string[] = []): boolean {
  return (
    object(value) &&
    (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null) &&
    required.every((k) => Object.hasOwn(value, k)) &&
    Reflect.ownKeys(value).every(
      (k) => typeof k === "string" && [...required, ...optional].includes(k),
    )
  );
}
const strings = (v: any) => ownList(v, nonempty);
function readerParameters(value: any): boolean {
  if (
    !fields(value, ["type", "properties", "additionalProperties"], ["required"]) ||
    value.type !== "object" ||
    value.additionalProperties !== false ||
    (Object.hasOwn(value, "required") &&
      (!Array.isArray(value.required) || value.required.length !== 0)) ||
    !fields(value.properties, ["document_id", "offset", "limit"])
  )
    return false;
  const p = value.properties;
  return (
    fields(p.document_id, ["type", "enum"]) &&
    p.document_id.type === "string" &&
    strings(p.document_id.enum) &&
    p.document_id.enum.length > 0 &&
    fields(p.offset, ["type", "minimum"]) &&
    p.offset.type === "integer" &&
    p.offset.minimum === 0 &&
    fields(p.limit, ["type", "minimum", "maximum"]) &&
    p.limit.type === "integer" &&
    p.limit.minimum === 1 &&
    p.limit.maximum === 8000
  );
}
export function readerResult(args: any, value: any): boolean {
  try {
    args = dataSnapshot(args);
    value = dataSnapshot(value);
  } catch {
    return false;
  }
  const identity = ["review_id", "packet_sha256"];
  if (!object(args) || !object(value) || !nonempty(value.review_id) || !sha(value.packet_sha256))
    return false;
  if (!Object.keys(args).length)
    return (
      fields(value, [...identity, "documents"]) &&
      ownList(
        value.documents,
        (d) =>
          fields(d, ["id", "source", "sha256", "total_chars"]) &&
          nonempty(d.id) &&
          nonempty(d.source) &&
          sha(d.sha256) &&
          integer(d.total_chars),
      )
    );
  return (
    fields(value, [
      ...identity,
      "document_id",
      "source",
      "source_sha256",
      "offset",
      "content",
      "total_chars",
      "has_more",
    ]) &&
    nonempty(value.document_id) &&
    nonempty(value.source) &&
    sha(value.source_sha256) &&
    integer(value.offset) &&
    typeof value.content === "string" &&
    integer(value.total_chars) &&
    typeof value.has_more === "boolean"
  );
}
export function discoveryResult(name: string, args: any, value: any, reader: string): boolean {
  try {
    args = dataSnapshot(args);
    value = dataSnapshot(value);
  } catch {
    return false;
  }
  if (name === "tool_describe") {
    const names = typeof args.names === "string" ? [args.names] : args.names;
    return (
      fields(args, ["names"]) &&
      strings(names) &&
      names.length > 0 &&
      names.every((n: string) => n === reader) &&
      fields(value, ["tools"]) &&
      fields(value.tools, [reader]) &&
      fields(value.tools[reader], ["description", "parameters"]) &&
      nonempty(value.tools[reader].description) &&
      readerParameters(value.tools[reader].parameters)
    );
  }
  const queries = typeof args.queries === "string" ? [args.queries] : args.queries;
  if (
    name !== "tool_search" ||
    !fields(args, ["queries"], ["limit"]) ||
    !strings(queries) ||
    !queries.length ||
    !fields(value, ["queries", "total_available", "results", "tools"]) ||
    !integer(value.total_available) ||
    !strings(value.queries) ||
    JSON.stringify(value.queries) !== JSON.stringify(queries.map((q: string) => q.trim())) ||
    !ownList(
      value.results,
      (group) =>
        fields(group, ["query", "matches"], ["available_sources", "hint"]) &&
        nonempty(group.query) &&
        ownList(group.matches, (n) => n === reader) &&
        (!Object.hasOwn(group, "hint") || nonempty(group.hint)) &&
        (!Object.hasOwn(group, "available_sources") ||
          ownList(
            group.available_sources,
            (s) => fields(s, ["name", "tool_count"]) && nonempty(s.name) && integer(s.tool_count),
          )),
    ) ||
    value.results.length !== queries.length ||
    !object(value.tools)
  )
    return false;
  const found = new Set<string>();
  for (let i = 0; i < value.results.length; i++) {
    if (value.results[i].query !== value.queries[i]) return false;
    for (const match of value.results[i].matches) found.add(match);
  }
  const names = Object.keys(value.tools);
  return (
    names.length === found.size &&
    value.total_available >= names.length &&
    names.every(
      (n) =>
        found.has(n) &&
        fields(value.tools[n], ["source", "source_name", "description", "required"]) &&
        nonempty(value.tools[n].source) &&
        nonempty(value.tools[n].source_name) &&
        typeof value.tools[n].description === "string" &&
        strings(value.tools[n].required),
    )
  );
}
