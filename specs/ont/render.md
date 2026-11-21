# Render

Purpose: Generated blocks in Markdown files.

Moved from crates/ont/README.md ("Generated blocks") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## Generated blocks

- R1: A schema's optional `views` map declares, under each name, a table over a type in the same domain or a document (R51).
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

## Documents

A document is a view of several parts in one generated block: an ordered list of document blocks, each a heading and a table or a list over one object type of the same schema. A "generated block" stays the region of a file between the two marker lines; a part of a document is a "document block".

- R51: An entry of `views` that has the member `blocks` is a document: `{"blocks":[B,...],"args"?:[name,...],"description"?:string}`, where B is `{"heading":string,"table":T}` or `{"heading":string,"list":L}`, T is `{"object_type","filter"?,"order","columns","limit"?}` and L is `{"object_type","filter"?,"order","properties","limit"?}`; `properties` is an array of property names, and every other member of T and L has the form and the meaning R1 to R5 give it in a table view. `args` and `description` are the document's; a document block has neither. An entry without `blocks` is a table view. A document has no one object type, so `describe` (store.md R2) lists the table views and not a document; its name is unique among the views of all installed domains like a table view's.
- R52: `schema check` and `schema install` refuse a document with exit 2 and `code: schema_violation`, a violation naming its code and pointer, with `<v>` the view name, `<i>` the index of a document block and P the pointer `/views/<v>/blocks/<i>/table` or `/views/<v>/blocks/<i>/list`: fewer than 1 or more than 20 document blocks is `invalid_document` at `/views/<v>/blocks`; a heading that is empty, longer than 200 bytes or holds a control character (line breaks among them) is `invalid_document` at `/views/<v>/blocks/<i>/heading`; `properties` with fewer than 1 or more than 4 names is `invalid_document` at `P/properties`, and a name that is not `$key`, `$observed_at` or a declared property of the type is `unknown_property` at `P/properties`; the document's `args` follow a table view's rule, `invalid_schema` at `/views/<v>/args`; inside T and L the checks of a table view hold with their codes at pointers under P (`unknown_type` at `P/object_type`, `invalid_schema` at `P/limit`, `P/order`, `P/columns` and `P/filter`, `unknown_property` at `P/columns` and `P/order`), and a `$name` in a document block's filter must be declared in the document's `args`. An entry with `blocks` and one of `object_type`, `filter`, `order`, `columns`, `limit`, a document block with both or neither of `table` and `list`, and a member no form names are refused with `schema_violation` too.
- R53: A document renders as one generated block: the opening line of R21 and R24 with the document's name and arguments; then, for each document block in the declared order, the line `## ` followed by the heading passed through the cell escape (R8, R40), then its table or its list; one empty line between two document blocks and none before the first or after the last; then the closing line. A table is the header row, the separator row, the object rows and the row of R25, each as a table view prints it (R5 to R8, R40, R48 to R50); with no object it is the header and separator rows.
- R54: A list prints one line per object, in the order `order` gives with ties by key: `- `, then the cell texts of `properties` joined by `: `, a cell text being that of a table cell (R6, R7, R8, R40, the `(stale)` suffix included). With no object it prints the one line `- (none)`; with more objects than `limit` (R3) a last line `- (+N more; limit L)` with the exact omitted count.
- R55: Every document block's object type must be one the principal may read in the schema's domain; else the whole render is refused `unavailable`, before any argument is judged and before any object is read. Each argument the document declares is required, an undeclared one is refused, and R13 holds. R14 to R19 hold over every document block: before any object is read, each supplied argument is checked against every string property it is compared with in any document block, in ascending argument name, then document block order, then the filter's written order (R20), and the refusal's `property` is the compared property of that leaf. R42 to R47 hold for each document block by itself: an argument is substituted by the kind of the property it is compared with there, and an argument that any document block compares with a reference property follows R47.
- R56: The schema metadata and every page of every document block are read in the one read snapshot of R27, and R28 holds for the document.
- R57: The allowance of 10,000 candidates (R9, R29) is the document's: each document block's pages get what the earlier ones left, and a document whose document blocks together need more is refused `result_too_large`; a head that makes a table view refuse makes the document refuse with the same code. No answer holds a part of a document.
- R58: The answer of `render` for a document has the six members of a table view's: `status`, `view`, `schema`, `rows` (the object rows and list items printed over all document blocks; the lines of R25, `- (none)` and `- (+N more; limit L)` are not counted), `truncated` (true when a document block is cut by its limit) and `markdown`. `render --write`, `render --check` and `--expect-blocks` take a document as one generated block: R10 to R12, R21 to R23 and R31 to R41 hold for it, `--section` and the opening line name the document, and `rows` of the write answer is the document's.

With this view in domain `board` version 1:

```json
"overview": {
  "args": ["title"],
  "blocks": [
    {"heading": "Items", "table": {
      "object_type": "item",
      "filter": {"eq": ["title", "$title"]},
      "order": [{"property": "rank", "direction": "asc"}],
      "columns": [{"header": "Key", "property": "$key"}, {"header": "Rank", "property": "rank"}]
    }},
    {"heading": "Notes", "list": {
      "object_type": "note",
      "order": [{"property": "text", "direction": "asc"}],
      "properties": ["$key", "text"]
    }}
  ]
}
```

items `a` and `b` (title `equal`, rank 2) and `z` (title `last`, rank 10), and notes `n1` (text `first`) and `n2` (text `second`), the `document_over_two_types_renders_the_same_bytes_twice` test renders `{"view":"overview","args":{"title":"equal"}}` as `rows` 4, `truncated` false and these exact bytes, with no line feed after the closing line:

```markdown
<!-- ont:render overview title=equal schema=board@1 -->
## Items
| Key | Rank |
| --- | --- |
| a | 2 |
| b | 2 |

## Notes
- n1: first
- n2: second
<!-- /ont:render -->
```

Between the opening and the closing line every line of a document is empty or begins with `## `, `| ` or `- `, so no stored value and no heading opens or closes a generated block or a fence.

Not included: a heading level other than `##`, free text between document blocks, a document block that names another view, a document inside a document, a document block over a link, over `around` or over more than one type, arguments per document block, a template for a list item, counts or computed columns, and `describe` listing a document.
