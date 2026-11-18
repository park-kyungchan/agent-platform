# Render

Purpose: Generated blocks in Markdown files.

Moved from crates/ont/README.md ("Generated blocks") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## Generated blocks

A schema's optional `views` map declares a table over a type in the same domain.
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

`filter` accepts the search grammar; a string `$title` takes the string argument
`title`, declared in `args: ["title"]`. `limit` defaults to 200 (maximum 1000).
`order` and `columns` must be nonempty. Order ties use the object key. Retracted
objects are omitted; stale values have ` (stale)` appended. Unresolved knowledge
states appear as `(missing)`, `(null)`, `(blank)`, `(unknown)`, `(not_performed)`.
Cells escape pipes, line breaks and HTML comment delimiters. Rendering uses
search admission and a 10,000-candidate whole-render bound, with no hidden-object totals.

Three forms (use `--store` with a test store; omitting it selects the host store):

```sh
printf '%s\n' '{"op":"render","view":"items","args":{}}' | ont --store /tmp/demo/store.sqlite render
ont --store /tmp/demo/store.sqlite render --write /tmp/demo/BOARD.md --section named_items --arg title=equal --preimage-dir /tmp/demo/preimages
ont --store /tmp/demo/store.sqlite render --check /tmp/demo/BOARD.md
```

`--write` requires an existing regular file with exactly one matching, closed
block. It saves the previous bytes in the existing preimage directory with mode
0600 and a SHA-256 suffix, then renames a temporary file over the target, keeping
its mode and all bytes outside the block. An identical block causes no write and
answers `changed:false` with `preimage:null`. Argument values must be nonempty
and contain only ASCII letters, digits, `.`, `_`, `:`, `/` or `-`.
After these argument checks and before reading any objects, render checks each
supplied argument against every string property it is compared with, including
arguments in an `in` list. The index-time field validator enforces the value
type's allowed values, pattern, minimum and maximum, and the property's
`max_bytes`. This applies to standard input, `--write`, `--check` and the library
render function. A `prefix` leaf is exempt because its operand is part of a
value; reference properties keep their existing native-key rules below.
A constraint refusal exits 2 with `code: invalid_argument`, `field: /args/<name>`,
`argument` naming the argument, `property` naming the compared property, and
`rule: "argument must satisfy the compared property's value constraints"`.
The refusal contains no rejected value. Arguments are checked in ascending name
order; for each argument the first violating leaf in the filter's written order
is reported. Opening-line arguments are sorted by name. A check reports the
opening line of each drifting, unknown or unclosed block and exits 2; a clean
check exits 0. A block with a constraint-refused argument is a drift entry with
`refusal_code: invalid_argument`.

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

An argument-bearing opening line has the form
`<!-- ont:render named_items title=equal schema=board@1 -->`.
More matching rows than the limit add a final table row containing
`(+N more; limit L)` with the exact omitted count. No timestamp is generated.

Rendering reads schema metadata and every page in one SQLite read snapshot.
Concurrent writes cannot change later pages, counts, ordering or values in that
block; their commits may wait for the bounded render to finish. The remaining
10,000-candidate allowance is passed to each page before loading payloads, with
no raw-head preflight. The `(stale)` suffix follows the trusted clock.

`--write` holds an exclusive advisory lock on the persistent
`<target file name>.ont-render.lock` sidecar beside the target, from before the
read through rename. Other render writers wait at most 250 ms and then refuse
with `render_busy`; an inaccessible or invalid sidecar is `render_lock_unavailable`.
Render leaves `<target>.ont-render.lock` beside the target; inside a Git worktree
it appears as an untracked file and may be deleted when no render runs.
Render never removes the sidecar itself, so rename does not change the lock inode.
Other editors must respect the same lock to be serialized. Targets with multiple hard
links are refused as `render_target_invalid`.

Fenced examples are prose: an opener has at most three leading spaces and a run
of at least three backticks or tildes. A closer has at most three leading spaces,
the same character repeated at least as many times, and only spaces after it;
backtick opener info strings cannot contain backticks. Markers inside the fence
are ignored by both check and write. An unclosed fence makes check exit 2 with
`refusal_code: render_fence_unclosed` at the opening fence's line (empty view and
arguments); write refuses `render_fence_unclosed` without changing the target.
Cells escape backslashes before pipes, line breaks and comment delimiters.
In a `render_drift` entry, `refusal_code` is present when rendering was refused
(for example `store_busy`, `store_not_found`, `unavailable`); pure byte drift
has no `refusal_code`. Argument placeholders are accepted only as complete
string comparison values on string properties (including string-based value
types), or individual elements of `in` on those properties; no coercion or
placeholders inside arrays/structs are supported. An `eq` filter on a single
reference property also accepts an argument, for example
`"filter":{"eq":["task","$task"]},"args":["task"]`. Its argument is the
referenced object's native key (such as `/synthetic/task-1`), recorded unchanged
in the generated block's opening line. Rendering builds the full reference Id
for search; no copied key property is needed and the target need not exist.
Reference arguments support `eq` only. Their characters follow the same ASCII
argument rules above; an unrepresentable key refuses at render time with
`reference_argument_unrepresentable`.
Unresolved sort values compare as JSON null; nonscalar values compare by compact
JSON text. In ascending boolean order, this places `false`, unresolved, `true`;
other mixed comparisons follow JSON text order. Order ties use the key.

