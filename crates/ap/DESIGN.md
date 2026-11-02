# S02 foundation contract v1

Status: foundation only; renderer leaf intentionally unimplemented. The current
candidate is not S02 success. Lead acceptance and independent Muse review must
freeze these expectations before Terra receives implementation authority.

## Authority and observed native surface

Base: `af303d170311a2f11eac9a37125e8a89c1c776b2` (accepted S01).
Task: `/home/palantirkc/work/ws-replan/STATE.md`; Lead owns state, integration and
the single expensive `-j1` build slot. Adopted ZIP SHA256:
`964845e260a25350823fb42d38a60e7cba973f8af607e10b505727d3bf78915c`.
Read PROPOSAL (minimal ap, §§4.3/6/10–12), PORTING_GUIDE and S02/AP-01/06/07
under the extracted ZIP identified by CURRENT-PLAN.md. S01 dispositions are
[`../../docs/rust-migration-dispositions.md`](../../docs/rust-migration-dispositions.md).
These are new conformance requirements, not parity with the retired adapter.

Local source-backed observations on 2026-11-02:

* `/root/.codex/AGENTS.md` is a direct symlink to `/root/.agents/AGENTS.md`.
  Its lstat reports uid 0 / symlink mode 0777; global AGENTS.override.md is absent.
* `/root/.claude/CLAUDE.md` is an ordinary file with exact text
  `# User instructions (Claude Code)\n\n@/root/.agents/AGENTS.md\n` (uid 0, mode 0644).
  These are observations, not new mode requirements or authorization to chmod.
* `/root/.agents/runtime-references/RUNTIME-LOADING.md` §§Current native
  surfaces/Native instruction differences documents Codex 0.165.0 global
  AGENTS/override priority and Claude 2.1.316 global imports. Its official source
  links are <https://learn.chatgpt.com/docs/agent-configuration/agents-md.md> and
  <https://code.claude.com/docs/en/memory>. The prepared installed help is in
  `/home/palantirkc/work/ws-replan/preparation/final-review/runtime/`:
  `codex-help.txt` describes config/strict-config; `claude-help.txt` describes
  native CLAUDE discovery/settings. No source-network call or native invocation
  was required to establish the chosen pointer surfaces.

Only two owned selectors exist: the Codex global instruction link's target and
the exact Claude global canonical import line. The library never renders the
canonical instruction source, a per-CLI copy, a project rule, a settings key,
MCP registration, trust record, lifecycle event or hook command. Native configs
are opaque preservation snapshots. No unmet pane-hook caller was established;
existing Herdr and official Hindsight hooks remain vendor-owned. Muse/agy retain
their native mechanisms pending qualification; this first increment cannot
establish P6 completion or close their consumers. Fresh loading is S03 evidence.

## Public boundary and CLI

`ap::render(&RenderRequest) -> Result<RenderPlan, RenderError>` is the only
library entrypoint. `src/types.rs` is the exact JSON/public Rust schema. All
fields are required, null is not missing, duplicate struct fields and unknown
envelope fields are refused by serde. Unknown native keys occur only inside
opaque `text` strings, which are preserved literally. No environment, filesystem,
clock, network, subprocess, process cwd or native loader discovery in the leaf.

`ap render < request.json` accepts one UTF-8 JSON document on stdin, at most
1,048,576 bytes (including whitespace). No flags, file arguments, NDJSON, stdin
logging or env expansion. `ap --help`/`ap -h` alone prints the literal help in
main.rs. No arguments or any other arguments give usage refusal. JSON success
is one compact document and one LF on stdout; empty stderr; exit 0. Field order
is Rust declaration order, arrays use the ordering below, string escaping is
serde_json 1.0.150. Input JSON object key order is immaterial. Artifact text and
symlink target bytes are exact UTF-8 strings after JSON decoding.

Refusal: empty stdout, compact `{"error":{"code":"...","field":"..."}}`
plus LF on stderr. CLI admission errors (`usage/args`, `invalid_input/stdin`,
`input_too_large/stdin`) exit 2. Renderer refusals exit 3. I/O failure exits 4
with `io_error` and `stdin`/`stdout` field if stderr remains writable. Internal
serialization failure exits 70. The foundation stub returns
`renderer_unimplemented/renderer`, exit 70. Never echo raw parser errors,
paths supplied by a caller, environment values or text payloads into diagnostics.
Public renderer error fields are the static selectors enumerated below.

## Input qualification and ordered algorithm (Terra specification)

Return the FIRST failure in the following sequence. Do not collect errors or
continue producing partial artifacts. Profile directories are explicit native
config directories, not homes to which the library appends a hidden component.
Both initial adapters are required; adapters are not inferred from directory names.

1. `schema_version` must equal 1; otherwise
   `unsupported_schema/schema_version`.
2. Validate `canonical_path`, `codex.config_dir`, `claude.config_dir`, then every
   `preserved[i].path` in input order. An admitted absolute path starts with `/`,
   is not `/`, has only ASCII letters/digits/`/._-`, has no empty component,
   `.` or `..` component, and has no trailing slash. Thus spaces, Unicode,
   environment/tilde references, URL/Windows syntax, NUL and non-normal paths
   are deliberately unsupported, not expanded or normalized. Error:
   `invalid_path/<field>`; use static `preserved.path` for every preserved entry.
3. Derive `codex.config_dir + /AGENTS.md`,
   `codex.config_dir + /AGENTS.override.md`,
   `claude.config_dir + /CLAUDE.md` (these filenames are literal).
   If the two config dirs are equal, or canonical_path equals one of those
   three derived paths, return `path_collision/profiles`. For preserved paths,
   first detect duplicates (input order), giving
   `duplicate_preserved_path/preserved.path`; then reject any collision with
   canonical_path or the three derived paths as `path_collision/preserved.path`.
   Parent/ancestor filesystem aliases are NOT resolved: these are logical
   path proposals. S03 must prove real identity and safe parent topology.
4. Require `codex.instruction_override` to be Missing, including refusal of an
   empty file or unsupported entry: `instruction_conflict/codex.instruction_override`.
   An override would shadow the proposed global AGENTS source. Never delete it.
5. Codex `instructions`:
   * Missing -> desired Symlink with absolute target `canonical_path`.
   * File (including empty) -> `instruction_conflict/codex.instructions`.
     Do not silently adopt, overwrite or copy an unowned instruction document.
   * Unsupported (any reason) -> `unsupported_target/codex.instructions`.
   * Symlink -> validate `target` with the same character alphabet. For this
     field only, relative paths and `.`/`..` components are allowed; no empty
     component, empty string, root-only target or trailing slash. Fail syntax as
     `invalid_path/codex.instructions.target`. Resolve lexically against
     codex.config_dir for a relative target, or `/` for an absolute target;
     consume `.` and pop for `..`. Popping above `/` or resolving to `/` gives
     `unsupported_target/codex.instructions`. A resolved target different from
     canonical_path gives `instruction_conflict/codex.instructions`. A matching
     link is preserved with its original spelling (including relative or dot
     components). The renderer does not follow links or open the canonical file.
6. Claude `instructions`:
   * Missing -> desired File with `@` + canonical_path + LF.
   * Symlink/Unsupported -> `unsupported_target/claude.instructions`. Do not
     convert another pointer topology to a file or follow a chain.
   * File -> reject NUL, lone CR, or a mixture of LF and CRLF line endings as
     `unsupported_text/claude.instructions`. Other UTF-8 text is preserved.
     Determine EOL as CRLF if any CRLF exists, otherwise LF (including empty text).
     Split into logical lines, removing only EOL bytes. An owned import is an
     EXACT line `@` + canonical_path, with no spaces, quoting or suffix.
     More than one exact import, OR any occurrence of canonical_path in any
     non-exact line, gives `instruction_conflict/claude.instructions`. This
     deliberately refuses ambiguous quoted/inline/fenced/prose mentions rather
     than claiming a full Markdown/native import parser. Fence-bearing text
     is unsupported for import ownership: if ANY line, after removing leading
     ASCII spaces and tabs, starts with three backticks or three tildes, return
     that same conflict, whether or not canonical_path is already present.
     This conservative refusal includes closed/unclosed fences and exact imports
     outside them. Otherwise an append could make the next render refuse its own
     output, violating the pure projection requirement. Other imports (different
     paths) and non-fenced Markdown are untouched.
     Exactly one admissible exact import -> preserve all original bytes, even
     an absent final newline. No import -> append one: if nonempty text lacks a
     final EOL, first append the selected EOL; then `@` + canonical_path + EOL.
     Empty text yields only the import and LF. Do not insert a blank line,
     replace another import, trim text or normalize line endings.
7. Emit exactly two artifacts with their derived paths, the literal selectors
   `codex_instruction_link` and `claude_canonical_import`, original `before`,
   computed `after`, and `changed = (before != after)`. Sort artifacts by path
   UTF-8 byte order, not adapter order. Copy all preserved entries exactly and
   sort them by path bytes. No extra artifact, mutation, generated canonical
   bytes or setting is permitted. Output schema_version is 1. The original
   request is not modified.

Ownership is intentionally narrow: an existing matching link or exact import
is a no-op, not proof that ap installed it. Unrelated text, including imports,
is retained. A planned append to an ordinary Claude file requires S03's real
owned-field/conflict handling before any application. S02 does not manufacture
trust or claim that a project override, alternate CODEX_HOME, permissions or
native selection flags will load the result. Caller must supply the affected
profile and S03 must validate it.

## Independent fixed expectations and scope

`conformance/cases.json` supplies explicit inputs and literal expected JSON
plans/refusals, authored before leaf implementation. Synthetic paths and config
bytes contain no credentials or private payload. AP-01 cases preserve unknown
JSON/TOML keys, comments, environment references, unrelated MCP registrations,
Hindsight and matcher-distinct hooks as opaque bytes. No TOML/JSON patch library
is justified by this selector set. AP-06 covers absolute/relative links,
unsupported shape classifications and path/text refusals. AP-07 contributes
only pure repeatability and the projection check: feed after entries into the
same request and expect both changed flags false with identical desired bytes.
Actual install-twice, owner/uid/mode checks, dangling/chain discovery, races,
kernel locks, partial apply, rollback and scoped doctor/fake-hook nonexecution
are S03 obligations. Do not label the pure projection check a real install.

`tests/conformance.rs` exercises public library cases and that projection;
`conformance/verify.py` exercises the actual binary's stdout/stderr/exit status
and strict schema admission. It additionally checks repeated execution and
input-object key reordering. Its self-check must reject changed bytes, missing
fields, changed ordering, changed status, extra stdout and nonempty stderr.
Expected outputs are independent constants: never generate them from render,
accept a subset, trim artifact strings, sort arrays during comparison or bless
the stub. JSON envelope object order alone may vary in the comparator; the CLI
additionally must serialize its already typed plan compactly plus one LF.

## Dependency and build decisions

One crate, edition 2024, Rust 1.95.0, workspace lint inheritance and unsafe
forbidden. serde =1.0.228 (derive/std) and serde_json =1.0.150 (std) were already
cached locally and their feature declarations inspected. Cargo.lock is resolved
offline. No clap, TOML editor, async runtime, filesystem helper or Ontology
dependency. std handles the one-command CLI; transitive versions are lockfile
pinned. No compilation, cargo test/check/clippy, installation or toolchain
download is authorized in this foundation assignment. rustfmt, offline lockfile
resolution/metadata, JSON validation, comparator self-check and diff checks are
bounded permitted checks. First compiler/test success remains unobserved.
