# Render

Purpose: Generated blocks in Markdown files.

Moved from crates/ont/README.md ("Generated blocks") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## Generated blocks

- R1: A schema's optional `views` map declares a table over a type in the same domain.
For example, the synthetic `render-board.json` test fixture declares:

```json
"items": {
  "object_type": "item",
  "order": [{"property": "rank", "direction": "asc"}],
  "columns": [
    {"header": "Key", "property": "$key"},
    {"header": "Title", "property": "title"},
    {"header": "Rank", "property": "rank"}
  ]
}
```

- R2: `filter` accepts the search grammar; a string `$title` takes the string argument `title`, declared in `args: ["title"]`.
- R3: `limit` defaults to 200 (maximum 1000).
- R4: `order` and `columns` must be nonempty.
- R5: Order ties use the object key.
- R6: Retracted objects are omitted; stale values have ` (stale)` appended.
- R7: Unresolved knowledge states appear as `(missing)`, `(null)`, `(blank)`, `(unknown)`, `(not_performed)`.
- R8: Cells escape pipes, line breaks and HTML comment delimiters.
- R9: Rendering uses search admission and a 10,000-candidate whole-render bound, with no hidden-object totals.

Three forms (use `--store` with a test store; omitting it selects the host store):

```sh
printf '%s\n' '{"op":"render","view":"items","args":{}}' | ont --store /tmp/demo/store.sqlite render
ont --store /tmp/demo/store.sqlite render --write /tmp/demo/BOARD.md --section named_items --arg title=equal --preimage-dir /tmp/demo/preimages
ont --store /tmp/demo/store.sqlite render --check /tmp/demo/BOARD.md
```

- R10: `--write` requires an existing regular file with exactly one matching, closed block.
- R11: It saves the previous bytes in the existing preimage directory with mode 0600 and a SHA-256 suffix, then renames a temporary file over the target, keeping its mode and all bytes outside the block.
- R12: An identical block causes no write and answers `changed:false` with `preimage:null`.
- R13: Argument values must be nonempty and contain only ASCII letters, digits, `.`, `_`, `:`, `/` or `-`.
- R14: After these argument checks and before reading any objects, render checks each supplied argument against every string property it is compared with, including arguments in an `in` list.
- R15: The index-time field validator enforces the value type's allowed values, pattern, minimum and maximum, and the property's `max_bytes`.
- R16: This applies to standard input, `--write`, `--check` and the library render function.
- R17: A `prefix` leaf is exempt because its operand is part of a value; reference properties keep their existing native-key rules below.
- R18: A constraint refusal exits 2 with `code: invalid_argument`, `field: /args/<name>`, `argument` naming the argument, `property` naming the compared property, and `rule: "argument must satisfy the compared property's value constraints"`.
- R19: The refusal contains no rejected value.
- R20: Arguments are checked in ascending name order; for each argument the first violating leaf in the filter's written order is reported.
- R21: Opening-line arguments are sorted by name.
- R22: A check reports the opening line of each drifting, unknown or unclosed block and exits 2; a clean check exits 0.
- R23: A block with a constraint-refused argument is a drift entry with `refusal_code: invalid_argument`.

The `deterministic_order_ties_arguments_and_actions` test renders this exact block:

```markdown
<!-- ont:render items schema=board@1 -->
| Key | Title | Rank |
| --- | --- | --- |
| a | equal | 2 |
| b | equal | 2 |
| z | last | 10 |
<!-- /ont:render -->
```

- R24: An argument-bearing opening line has the form `<!-- ont:render named_items title=equal schema=board@1 -->`.
- R25: More matching rows than the limit add a final table row containing `(+N more; limit L)` with the exact omitted count.
- R26: No timestamp is generated.

- R27: Rendering reads schema metadata and every page in one SQLite read snapshot.
- R28: Concurrent writes cannot change later pages, counts, ordering or values in that block; their commits may wait for the bounded render to finish.
- R29: The remaining 10,000-candidate allowance is passed to each page before loading payloads, with no raw-head preflight.
- R30: The `(stale)` suffix follows the trusted clock.

- R31: `--write` holds an exclusive advisory lock on the persistent `<target file name>.ont-render.lock` sidecar beside the target, from before the read through rename.
- R32: Other render writers wait at most 250 ms and then refuse with `render_busy`; an inaccessible or invalid sidecar is `render_lock_unavailable`.
- R33: Render leaves `<target>.ont-render.lock` beside the target; inside a Git worktree it appears as an untracked file and may be deleted when no render runs.
- R34: Render never removes the sidecar itself, so rename does not change the lock inode.
- R35: Other editors must respect the same lock to be serialized.
- R36: Targets with multiple hard links are refused as `render_target_invalid`.

- R37: Fenced examples are prose: an opener has at most three leading spaces and a run of at least three backticks or tildes. A closer has at most three leading spaces, the same character repeated at least as many times, and only spaces after it; backtick opener info strings cannot contain backticks.
- R38: Markers inside the fence are ignored by both check and write.
- R39: An unclosed fence makes check exit 2 with `refusal_code: render_fence_unclosed` at the opening fence's line (empty view and arguments); write refuses `render_fence_unclosed` without changing the target.
- R40: Cells escape backslashes before pipes, line breaks and comment delimiters.
- R41: In a `render_drift` entry, `refusal_code` is present when rendering was refused (for example `store_busy`, `store_not_found`, `unavailable`); pure byte drift has no `refusal_code`.
- R42: Argument placeholders are accepted only as complete string comparison values on string properties (including string-based value types), or individual elements of `in` on those properties; no coercion or placeholders inside arrays/structs are supported.
- R43: An `eq` filter on a single reference property also accepts an argument, for example `"filter":{"eq":["task","$task"]},"args":["task"]`.
- R44: Its argument is the referenced object's native key (such as `/synthetic/task-1`), recorded unchanged in the generated block's opening line.
- R45: Rendering builds the full reference Id for search; no copied key property is needed and the target need not exist.
- R46: Reference arguments support `eq` only.
- R47: Their characters follow the same ASCII argument rules above; an unrepresentable key refuses at render time with `reference_argument_unrepresentable`.
- R48: Unresolved sort values compare as JSON null; nonscalar values compare by compact JSON text.
- R49: In ascending boolean order, this places `false`, unresolved, `true`; other mixed comparisons follow JSON text order.
- R50: Order ties use the key.

