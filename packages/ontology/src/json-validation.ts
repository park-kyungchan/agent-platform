import type { ValidationIssue } from "./validation.js";

/** JSON validation is ordinary infrastructure; it has no Ontology identity. */
export function jsonIssue(
  value: unknown,
  path = "",
  parents = new WeakSet<object>(),
): ValidationIssue | undefined {
  if (value === null || typeof value === "string" || typeof value === "boolean") return;
  if (typeof value === "number" && Number.isFinite(value)) return;
  if (typeof value !== "object")
    return { code: "non-json-value", path, message: "Value must be finite, plain JSON." };
  if (parents.has(value))
    return { code: "cyclic-json", path, message: "JSON cannot contain an object cycle." };
  if (
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) !== Object.prototype &&
    Object.getPrototypeOf(value) !== null
  ) {
    return { code: "non-json-object", path, message: "JSON objects must be plain objects." };
  }
  parents.add(value);
  for (const [key, nested] of Object.entries(value)) {
    const issue = jsonIssue(nested, `${path}/${key}`, parents);
    if (issue) return issue;
  }
  parents.delete(value);
}
