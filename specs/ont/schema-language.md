# Schema language

Purpose: The schema language, schema install and embedding of the engine in other programs.

Moved from crates/ont-core/README.md (whole file); crates/ont/README.md ("schema install", "Embedding") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.


Consumers: `ont-store`, the `ont` command and domain crates that pin this repository.
Public contract: Id/Namespace/Scope, observation knowledge/lineage, opaque Session,
Authority/Admission, Snapshot and restricted Query/Response (`Get`, `Traverse`, `Decide`). No internal-platform
dependency, host user mapping, transport or Academy parser.
Read and derive grants are independently required. Service keeps Admission alive
through serialization; raw Snapshot and Authority construction are trusted APIs.

`Scope` is a closed `Education { namespace, class_key, student_key }` or
`Program { namespace, program_key }` enum. `Scope::education` and `Scope::program`
validate nonempty exact keys and the same namespace bounds as `Id`; `namespace()`
borrows the namespace for either variant. No whitespace or UTF-8 normalization is
performed. Education JSON retains namespace/class_key/student_key order; Program
JSON uses namespace/domain/program_key order with `domain: "program"`. Decoding
rejects duplicate, mixed, unknown, missing, null or invalid fields. Grants compare
the entire enum value and object type. This is a Rust source API change requiring
consumers to recompile; legacy Education wire bytes remain compatible. Existing
schema-v1 stores refuse Program intake; Program storage is a separate change.
## schema install

- `ont [--store PATH] schema install --from <repository checkout> --commit <40 lowercase hex> --path <path inside the repository>` reads the file with `git show <commit>:<path>` (`/usr/bin/git`, argv only, cleared environment, at most 16 MiB, a 10-second wait). It refuses a non-hex or uppercase commit and an absolute or `..` path with `invalid_request`, a commit that is not an ancestor of the checkout's `refs/remotes/origin/main` with `commit_not_merged`, a missing path at that commit with `schema_path_not_found`, and a missing `/usr/bin/git` or a failed ancestor check other than status 1 with `git_unavailable`. It then does what the file form does (check, version rule, backup before an accepted install that changes stored bytes when the store already holds an install, install) and records the provenance `(repository, commit, path)`: repository is `remote.origin.url` with any userinfo removed, else the checkout path.
- The caller must run `git fetch origin` before the commit form: `ont schema install --from` does not fetch, and its ancestor check uses the local `refs/remotes/origin/main`. When root reads a checkout owned by another user, Git must trust that exact checkout through `safe.directory` in root's protected Git configuration; otherwise the check refuses with `git_unavailable`.
- The file form `ont [--store PATH] schema install <file>` stays for temporary stores; on the default store path (`/var/lib/ontology-platform/ont/ontology.sqlite`) and aliases through file symlinks, hard links, symlinked parent directories, `..`, repeated slashes, trailing slashes or relative paths, it is refused with `install_requires_commit`.
- The installed schema bytes have one home: the `bytes` column of the store table `schema_installs` (with `domain`, `version`, `sha256`, `installed_at`, `source_repository`, `source_commit`, `source_path`). There is no second copy under `/opt/ontology-platform/schema/`. Drift is detected at load: the latest row's bytes are parsed and their sha256 must equal the row's `sha256` (`schema_pin_mismatch` otherwise), and the same bytes are the ones every process reads, so there is no external file that could drift from the record.
- `describe` and the MCP `ont_describe` show, for each installed domain, `version`, `sha256` and, when recorded, `repository`, `commit` and `path`.

## Embedding
A domain server uses the library without the command: `Context::new(store_path, now)` is the supported constructor (`Context` is `#[non_exhaustive]`; `with_snapshot_bounds` is the documented test constructor) (then set its fields: the store path, a trusted `now`, `client`, `agent`, `session`, `role`), then call the public functions `ont::action::list_actions`, `ont::action::validate_action`, `ont::action::apply_action`, `ont::get::get`, `ont::search::search` and `ont::log::log` with their public request types (`ListActionsRequest`, `ValidateActionRequest`, `ApplyActionRequest`, `GetRequest`, `SearchRequest`, `LogRequest`). Install the server's own schema into its own store file first (for example with `ont_store::Store::install_schema`).
A read-only open that needs a format or projection upgrade refuses with `store_upgrade_required`; the owner's first writable open performs the upgrade.
Create a private store and install its schema with `ont::schema::install` and `InstallRequest`: for a non-default store path whose parent exists, it creates the missing file with mode 0600 before SQLite opens it.
A consumer pins `ont-core`, `ont-store` and `ont` to one Git `rev` of this repository.
- The uid rule is the caller's: the command checks real = effective uid 0 before binding principal `agent`; a library caller must do the same check itself (see `ont::check_uid`) before it trusts a `Context`. Attribution in the action log is self-reported; it is not authentication.
- The store file has one owner process family: every process opens the file itself, SQLite's write lock admits one writer at a time, and no second process family may write the SQLite file, `edits.jsonl`, or the schema installs directly.

