import ts from "typescript";

/** A compatibility entrypoint may only re-export an admitted canonical package export. */
export function validateCompatibilityForwarder(
  path: string,
  source: string,
  canonicalExports: ReadonlySet<string>,
): void {
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  if (
    !file.statements.length ||
    file.statements.some(
      (node) =>
        !ts.isExportDeclaration(node) ||
        !node.moduleSpecifier ||
        !ts.isStringLiteralLike(node.moduleSpecifier) ||
        !canonicalExports.has(node.moduleSpecifier.text) ||
        Boolean(node.attributes),
    )
  )
    throw Error("Compatibility source must only forward canonical public exports: " + path);
}
