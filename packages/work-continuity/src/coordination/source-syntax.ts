/** Restricted direct-load ESM contract, not a JavaScript sandbox or a complete
 * JavaScript loader scanner. Parse the entire original module before import or
 * any transpiler/compiler/macro operation.
 * Only static imports/reexports, consts and expression-returning functions are
 * admitted. Expressions have lexical bindings, literal data, calls and named
 * members; no ambient globals, computed members, reflection or code loaders.
 * General runtime modules need a separately qualified finite owner bundle. */
type Token = { kind: "name" | "number" | "string" | "punctuation" | "end"; value: string };
const forbiddenNames = new Set([
  "constructor",
  "prototype",
  "__proto__",
  "require",
  "module",
  "exports",
  "eval",
  "Function",
  "AsyncFunction",
  "GeneratorFunction",
  "createRequire",
  "getBuiltinModule",
  "_load",
  "_compile",
  "globalThis",
  "global",
  "process",
  "Bun",
  "Deno",
  "System",
  "window",
  "self",
  "this",
  "super",
  "Reflect",
  "Proxy",
]);
function unsupported(): never {
  throw Error("SOURCE_COMPUTED_LOADER_UNSUPPORTED_RESTRICTED_MODULE_REQUIRED");
}
export function restrictedSourceImports(text: string): string[] {
  if (Buffer.byteLength(text) > 2097152) throw Error("SOURCE_EXECUTABLE_SYNTAX_BOUND");
  const tokens: Token[] = [];
  let at = 0;
  while (at < text.length) {
    const c = text[at]!;
    if (/\s/u.test(c)) {
      at++;
      continue;
    }
    if (text.startsWith("//", at)) {
      at += 2;
      // JavaScript LineTerminator: LF, CR, line separator and paragraph separator.
      // Keep original bytes intact; tokenize every character after the comment.
      while (at < text.length && !/[\n\r\u2028\u2029]/u.test(text[at]!)) at++;
      continue;
    }
    if (text.startsWith("/*", at)) {
      const end = text.indexOf("*/", at + 2);
      if (end < 0) unsupported();
      at = end + 2;
      continue;
    }
    if (c === '"' || c === "'") {
      const quote = c;
      let value = "",
        closed = false;
      at++;
      while (at < text.length) {
        const ch = text[at++]!;
        if (ch === quote) {
          closed = true;
          break;
        }
        if (ch === "\n" || ch === "\r") unsupported();
        if (ch !== "\\") {
          value += ch;
          continue;
        }
        const escape = text[at++]!;
        const simple: Record<string, string> = {
          n: "\n",
          r: "\r",
          t: "\t",
          b: "\b",
          f: "\f",
          "\\": "\\",
          '"': '"',
          "'": "'",
        };
        if (Object.hasOwn(simple, escape)) value += simple[escape];
        else if (escape === "u" && /^[a-fA-F0-9]{4}$/u.test(text.slice(at, at + 4))) {
          value += String.fromCharCode(parseInt(text.slice(at, at + 4), 16));
          at += 4;
        } else unsupported();
      }
      if (!closed) unsupported();
      tokens.push({ kind: "string", value });
      continue;
    }
    const name = /^[A-Za-z_$][A-Za-z0-9_$]*/u.exec(text.slice(at));
    if (name) {
      tokens.push({ kind: "name", value: name[0] });
      at += name[0].length;
      continue;
    }
    const number = /^(?:[0-9]+(?:\.[0-9]+)?)(?:[eE][+-]?[0-9]+)?/u.exec(text.slice(at));
    if (number) {
      tokens.push({ kind: "number", value: number[0] });
      at += number[0].length;
      continue;
    }
    const operator = ["===", "!==", "==", "!=", "<=", ">=", "&&", "||", "??", "**"].find((v) =>
      text.startsWith(v, at),
    );
    if (operator) {
      tokens.push({ kind: "punctuation", value: operator });
      at += operator.length;
      continue;
    }
    if (!";,.(){}:=+-*/%!?<>".includes(c)) unsupported();
    tokens.push({ kind: "punctuation", value: c });
    at++;
  }
  tokens.push({ kind: "end", value: "" });
  let cursor = 0,
    depth = 0;
  const peek = () => tokens[cursor]!;
  const take = (value: string) => {
    if (!["name", "punctuation"].includes(peek().kind) || peek().value !== value) return false;
    cursor++;
    return true;
  };
  const need = (value: string) => {
    if (!take(value)) unsupported();
  };
  const name = () => {
    const token = tokens[cursor++]!;
    if (token.kind !== "name" || forbiddenNames.has(token.value)) unsupported();
    return token.value;
  };
  const bindings = new Set<string>(),
    imports: string[] = [];
  const bind = (value: string) => {
    if (bindings.has(value)) unsupported();
    bindings.add(value);
  };
  const from = () => {
    const token = tokens[cursor++]!;
    if (token.kind !== "string" || !token.value) unsupported();
    imports.push(token.value);
  };
  const precedence: Record<string, number> = {
    "??": 1,
    "||": 2,
    "&&": 3,
    "==": 4,
    "!=": 4,
    "===": 4,
    "!==": 4,
    "<": 5,
    ">": 5,
    "<=": 5,
    ">=": 5,
    "+": 6,
    "-": 6,
    "*": 7,
    "/": 7,
    "%": 7,
    "**": 8,
  };
  function expression(locals: ReadonlySet<string>, minimum = 0): void {
    if (++depth > 128) unsupported();
    try {
      const token = peek();
      if (token.kind === "punctuation" && ["!", "+", "-"].includes(token.value)) {
        cursor++;
        expression(locals, 9);
      } else if (take("(")) {
        expression(locals);
        need(")");
      } else if (take("{")) {
        if (!take("}"))
          do {
            if (peek().kind === "string") {
              if (forbiddenNames.has(peek().value)) unsupported();
              cursor++;
            } else name();
            need(":");
            expression(locals);
            if (take("}")) break;
            need(",");
            if (take("}")) break;
          } while (true);
      } else if (token.kind === "string" || token.kind === "number") cursor++;
      else if (["true", "false", "null", "undefined"].includes(token.value)) cursor++;
      else {
        const identifier = name();
        if (!bindings.has(identifier) && !locals.has(identifier)) unsupported();
      }
      while (true) {
        if (take(".")) name();
        else if (take("(")) {
          if (!take(")"))
            do {
              expression(locals);
              if (take(")")) break;
              need(",");
            } while (true);
        } else break;
      }
      while (Object.hasOwn(precedence, peek().value) && precedence[peek().value]! >= minimum) {
        const value = tokens[cursor++]!.value;
        expression(locals, precedence[value]! + 1);
      }
      if (minimum === 0 && take("?")) {
        expression(locals);
        need(":");
        expression(locals);
      }
    } finally {
      depth--;
    }
  }
  function importClause() {
    if (peek().kind === "string") {
      from();
      return;
    }
    if (peek().kind === "name") {
      bind(name());
      if (!take(",")) {
        need("from");
        from();
        return;
      }
    }
    if (take("*")) {
      need("as");
      bind(name());
    } else {
      need("{");
      if (!take("}"))
        do {
          const original = name();
          bind(take("as") ? name() : original);
          if (take("}")) break;
          need(",");
          if (take("}")) break;
        } while (true);
    }
    need("from");
    from();
  }
  while (peek().kind !== "end") {
    if (take(";")) continue;
    if (take("import")) importClause();
    else {
      const exported = take("export");
      if (exported && take("*")) {
        need("from");
        from();
      } else if (exported && take("{")) {
        const names: string[] = [];
        if (!take("}"))
          do {
            names.push(name());
            if (take("as")) name();
            if (take("}")) break;
            need(",");
            if (take("}")) break;
          } while (true);
        if (take("from")) from();
        else if (names.some((n) => !bindings.has(n))) unsupported();
      } else if (take("const")) {
        do {
          const identifier = name();
          need("=");
          expression(new Set());
          bind(identifier);
        } while (take(","));
      } else if (take("function")) {
        bind(name());
        need("(");
        const params = new Set<string>();
        if (!take(")"))
          do {
            const parameter = name();
            if (params.has(parameter)) unsupported();
            params.add(parameter);
            if (take(")")) break;
            need(",");
          } while (true);
        need("{");
        need("return");
        expression(params);
        take(";");
        need("}");
      } else if (exported && take("default")) expression(new Set());
      else unsupported();
    }
    // Explicit module separators, except at EOF or after a complete function.
    if (!take(";") && peek().kind !== "end" && tokens[cursor - 1]?.value !== "}") unsupported();
  }
  return imports;
}

/** These builtins export data/pure path/URL operations, not module loaders.
 * Everything else, including node:module, vm, workers and Bun, is unsupported
 * on this direct-load route, even under a local import alias. */
export const restrictedSourceBuiltins: readonly string[] = Object.freeze([
  "node:path",
  "node:path/posix",
  "node:path/win32",
  "node:url",
]);
