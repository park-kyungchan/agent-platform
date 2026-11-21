# Schema language

Purpose: The schema language, schema install and embedding of the engine in other programs.

Moved from crates/ont-core/README.md (whole file); crates/ont/README.md ("schema install", "Embedding") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.


Consumers: `ont-store`, the `ont` command and domain crates that pin this repository.
- R1: Public contract: Id/Namespace/Scope, observation knowledge/lineage, opaque Session, Authority/Admission, Snapshot and restricted Query/Response (`Get`, `Traverse`, `Decide`). No internal-platform dependency, host user mapping, transport or Academy parser.
- R2: Read and derive grants are independently required.
- R3: Service keeps Admission alive through serialization; raw Snapshot and Authority construction are trusted APIs.

- R4: `Scope` is a closed `Education { namespace, class_key, student_key }` or `Program { namespace, program_key }` enum.
- R5: `Scope::education` and `Scope::program` validate nonempty exact keys and the same namespace bounds as `Id`; `namespace()` borrows the namespace for either variant.
- R6: No whitespace or UTF-8 normalization is performed.
- R7: Education JSON retains namespace/class_key/student_key order; Program JSON uses namespace/domain/program_key order with `domain: "program"`.
- R8: Decoding rejects duplicate, mixed, unknown, missing, null or invalid fields.
- R9: Grants compare the entire enum value and object type.
This is a Rust source API change requiring consumers to recompile; legacy Education wire bytes remain compatible.
- R10: Existing schema-v1 stores refuse Program intake; Program storage is a separate change.
## schema install

- R11: `ont [--store PATH] schema install --from <repository checkout> --commit <40 lowercase hex> --path <path inside the repository>` reads the file with `git show <commit>:<path>` (`/usr/bin/git`, argv only, cleared environment, at most 16 MiB, a 10-second wait).
- R12: It refuses a non-hex or uppercase commit and an absolute or `..` path with `invalid_request`, a commit that is not an ancestor of the checkout's `refs/remotes/origin/main` with `commit_not_merged`, a missing path at that commit with `schema_path_not_found`, and a missing `/usr/bin/git` or a failed ancestor check other than status 1 with `git_unavailable`.
- R13: It then does what the file form does (check, version rule, backup before an accepted install that changes stored bytes when the store already holds an install, install) and records the provenance `(repository, commit, path)`: repository is `remote.origin.url` with any userinfo removed, else the checkout path.
- R14: The caller must run `git fetch origin` before the commit form: `ont schema install --from` does not fetch, and its ancestor check uses the local `refs/remotes/origin/main`.
- R15: When root reads a checkout owned by another user, Git must trust that exact checkout through `safe.directory` in root's protected Git configuration; otherwise the check refuses with `git_unavailable`.
- R16: The file form `ont [--store PATH] schema install <file>` stays for temporary stores; on the default store path (`/var/lib/ontology-platform/ont/ontology.sqlite`) and aliases through file symlinks, hard links, symlinked parent directories, `..`, repeated slashes, trailing slashes or relative paths, it is refused with `install_requires_commit`.
- R17: The installed schema bytes have one home: the `bytes` column of the store table `schema_installs` (with `domain`, `version`, `sha256`, `installed_at`, `source_repository`, `source_commit`, `source_path`). There is no second copy under `/opt/ontology-platform/schema/`.
- R18: Drift is detected at load: the latest row's bytes are parsed and their sha256 must equal the row's `sha256` (`schema_pin_mismatch` otherwise), and the same bytes are the ones every process reads, so there is no external file that could drift from the record.
- R19: `describe` and the MCP `ont_describe` show, for each installed domain, `version`, `sha256` and, when recorded, `repository`, `commit` and `path`.

## Embedding
- R20: A domain server uses the library without the command: `Context::new(store_path, now)` is the supported constructor (`Context` is `#[non_exhaustive]`; `with_snapshot_bounds` is the documented test constructor) (then set its fields: the store path, a trusted `now`, `client`, `agent`, `session`, `role`), then call the public functions `ont::action::list_actions`, `ont::action::validate_action`, `ont::action::apply_action`, `ont::get::get`, `ont::search::search` and `ont::log::log` with their public request types (`ListActionsRequest`, `ValidateActionRequest`, `ApplyActionRequest`, `GetRequest`, `SearchRequest`, `LogRequest`). Install the server's own schema into its own store file first (for example with `ont_store::Store::install_schema`).
- R21: A read-only open that needs a format or projection upgrade refuses with `store_upgrade_required`; the owner's first writable open performs the upgrade.
- R22: Create a private store and install its schema with `ont::schema::install` and `InstallRequest`: for a non-default store path whose parent exists, it creates the missing file with mode 0600 before SQLite opens it.
- R23: A consumer pins `ont-core`, `ont-store` and `ont` to one Git `rev` of this repository.
- R24: The uid rule is the caller's: the command checks real = effective uid 0 before binding principal `agent`; a library caller must do the same check itself (see `ont::check_uid`) before it trusts a `Context`.
- R25: Attribution in the action log is self-reported; it is not authentication.
- R26: The store file has one owner process family: every process opens the file itself, SQLite's write lock admits one writer at a time, and no second process family may write the SQLite file, `edits.jsonl`, or the schema installs directly.

## Names across domains

- R27: Among the domains installed in one store, object type names and view names are unique (`duplicate_type_name` at check and at install). Action type names, link type names with their forward and reverse names, and function names are unique inside one schema and may repeat across domains; a name that may repeat is always resolved in one domain (action.md R105 and R107, read.md R104, fn.md R5). `ont render` and the other reads take no `domain`.

## Grants

- R28: An entry of `grants` may hold, beside `principal`, `scope`, `read`, `derive` and `actions`, two optional members: `role`, a string under the schema's name rule (1 to 64 characters, a lowercase letter first, then lowercase letters, digits or underscore), and `functions`, an array of names of functions the schema declares. `ont schema check` and `ont schema install` refuse a `role` against the name rule with `invalid_name` at `/grants/<i>/role`, and a name in `functions` the schema does not declare with `unknown_function` at `/grants/<i>/functions/<j>`, as they refuse `unknown_action` at `/grants/<i>/actions/<j>`. `principal` stays required and as it is: the engine applies the grants whose principal is the word `agent`. The public type `SchemaGrant` of `ont-core` carries `role` and `functions` as public members.
- R29: A grant without `role` is a grant of every call; a grant with `role` is a grant of the calls whose resolved role (action.md R112) is that role. A grant gives nothing outside its schema's domain (action.md R106).
- R30: A domain is role-checked when a grant of its schema names a role or lists a function. A schema none of whose grants does is of the form before this requirement, and its domain answers every call as before it: the role stays the logged claim of action.md R12 to R17 and no roles file is read. A schema without the two members checks, installs and loads as before.

Forward only: an `ont` older than this change refuses a schema whose grant holds `role` or `functions` (`unknown_field`), so the new `ont` is installed before such a schema.
