# Store

Purpose: The store file and its opening rules.

Moved from crates/ont/README.md (opening text); crates/ont-store/README.md (whole file) at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.


- R1: The `ont` command: `schema check|install`, `describe`, `get`, `backup`, `index`, `search`, `around`, `aggregate`, `log`, `action list|validate|apply` over a format 4 store (format 3 upgrades only on a writable open; a read refuses with `store_upgrade_required`).

- R2: `describe` lists readable views with their `name`, `args` and `object_type`, including through `ont_describe`.
- R3: Empty or whitespace-only stdin defaults to `{}` for `describe` and `action list`; commands with required fields report the first missing field.
- R4: Reference search filters require a full Id, obtainable by searching the referenced type.
- R5: Refusals may include a top-level `hint`; submission hints use fixed recovery text selected by failed criterion codes (unknown codes get a generic repair), without request values.
- R6: Store creation and existing-store opens refuse symbolic links at the store file, its containing directory, or that directory's parent (exactly two directory levels; aliases above them are allowed), SQLite sidecars (`-journal`, `-wal`, `-shm`), `edits.jsonl` or its lock with `store_path_is_symlink` and field `--store`.
- R7: SQLite opens use `SQLITE_OPEN_NOFOLLOW` against a swapped final database path; the parent path is canonicalized after the guard so host aliases above the two checked directory levels can open normally.
- R8: Parent and adjacent file checks are pre-open checks: concurrent replacement after inspection remains outside this guarantee; edit-log descriptors separately use `O_NOFOLLOW`.
- R9: Default-store aliases retain `install_requires_commit`.
- R10: Git provenance strips userinfo only when its host boundary is certain.
- R11: Raw-slash credentials and ambiguous port/path-`@` forms use the checkout path as provenance instead; `strip_credentials` returns `None` for ambiguity.
- R12: Scp-like userinfo requires exactly one `@` and no `/` before the first `:`; a `:` before the first `@` falls back rather than assuming a password boundary.
- R13: Ordinary ports and local paths remain intact.


Consumers: the `ont` command and domain crates that pin this repository.
- R14: Public contract: isolated memory/file Store, trusted atomic seed/append_batch, transaction-bound ReadSnapshot.
- R15: Uses rusqlite 0.32.1 with bundled libsqlite3-sys 0.30.1 (SQLite source 3.46.0); no system install.
- R16: Schema version 2 (`user_version` 2) for the program projection, application ID 1330533430, composite BINARY keys, FULL synchronous, DELETE journal, 250ms busy timeout, foreign keys.
- R17: Unknown existing databases refuse; Not an Academy journal, Hindsight database or authorization entrypoint.

- R18: The program projection is schema version 2; the first-version (Education) store mode no longer exists, and a version 1 file refuses at open like any other unknown version.
- R19: `Store::open_program` / `Store::memory_program` explicitly select a separate Program-only schema-v2 projection.
- R20: V2 uses the same application ID and object, observation and link tables; it installs no action tables.
- R21: Each mode validates the selected scope domain, required keys and exact Id namespace during intake and snapshot reads, including link endpoints.
- R22: A failed seed/append rolls back the entire transaction.

- R23: Opening the other mode's existing version, a future version or an unknown database refuses with `store_failure` before reading object rows.
- R24: No mode upgrades, downgrades, rewrites scopes or eagerly repairs corrupt rows.
Use only an explicitly task-owned new path for Program projections.
- R25: Older binaries reject v2 at open.
- R56: New stores have 16,384-byte SQLite pages.

## Schema mode (format 3)

- R26: `Store::open_schema(path)` / `Store::memory_schema()` explicitly select format 3 (`user_version` 3, the same `application_id` as formats 1 and 2, checked the same way; a busy lock at open refuses with `store_busy`).
- R27: Format 1 or 2 files refuse with `store_failure`, and a format 3 file refuses in the program mode; no mode converts another.
- R28: Its tables are `objects`, `observations` (the v1 columns plus `ingested_at`, `schema_domain`, `schema_version` and `change_kind`), `links` (created, unused), `schema_installs` and `action_log` (see `schema-v3.sql`).
- R29: Opening an existing store: `open_schema` is the only open that creates or initialises a file.
- R30: `open_schema_existing` (read-only) and `open_schema_existing_writable` (read-write, for `Store::write`) have no create flag and never initialise: a missing path is `store_not_found`, an empty or foreign file is `store_failure` with its bytes unchanged.

- R31: An observation and an action-log row refer to the install they were written under by `(schema_domain, schema_version)`, a foreign key to `schema_installs(domain, version)`; the pin is read from that install row (one home for the pin).
- R32: Schema change in v1: installing a newer version does not re-validate stored observations; an old observation keeps its own pin; a type removed by the newer version is still readable by `head` and `history` and refuses `unknown_type` for ingest and `scan_heads`.

- R33: `install_schema(bytes, InstallArgs { installed_at, source_repository, source_commit })` keeps, per install, the domain, version, sha256, the exact schema bytes, the caller-given install time and the optional source repository/commit.
- R34: A domain's version only grows; the same (domain, version) with a different sha256 refuses with `schema_version_conflict`; installing the exact bytes of the latest installed version again is a no-op; a candidate is checked (including the type-name clash) against the set read inside its IMMEDIATE transaction before it is written.
- R35: Opening the store rebuilds the `SchemaSet` from each domain's latest stored bytes and re-verifies each pin; stored bytes that no longer hash to their row's sha256 refuse the open with `schema_pin_mismatch`.
- R36: The cached set is never trusted across a transaction boundary: every transaction (read or write) compares the latest install rows with the cache and rebuilds inside that transaction when they differ, and the cache is replaced only after a successful commit.
- R37: `schema_set()` and `schema_pins()` refresh the same way.

- R38: One public write transaction covers observations and the action log: `Store::write(now, |tx| ...)` runs the closure in one IMMEDIATE transaction with a `SchemaWrite` handle (`append`, `renew`, `log_action`, `head`, `schema_set`), commits when the closure returns `Ok` and rolls back otherwise.
- R39: `append_schema(items, ingested_at)` and `append_action_log(entry)` are thin callers of it.
- R40: Ingest rules (per item, inside the transaction): the object type must be declared and its `fields` must pass `SchemaSet::validate_fields`; the scope must be `Scope::Domain` of the declaring domain, in that schema's namespace, with a key set equal to the domain's `scope_keys` exactly (an empty scope key or value is `scope_keys_mismatch`); the object `Id.namespace` must equal the schema's namespace; and every `ref` value's `Id.namespace` must equal the namespace of the schema that declares its target type.
- R41: A reference to an object that does not exist in the store is accepted: sources are indexed independently, and the reference is only a value.
- R42: The legacy `seed`/`append_batch` refuse with `schema_violation` in this mode (the program mode has no dedicated wrong-mode refusal besides `store_failure` at open, so that code is kept).
- R43: Refusals are `StoreRefusal { code, violation }`: validation failures carry code `schema_violation` with the violation code and pointer available; a moved head carries `revision_conflict`; a repeated revision carries `duplicate_revision`.
- R44: Change rules: `initial` only for an object with no observation (or after a retraction head, which is treated as having no head; the full history is kept), and it must not name a superseded head; `update`, `refresh`, `correction` and `retraction` only when `supersedes` equals the current head; `refresh` must carry fields equal to the head's fields.
- R45: A `retraction` observation has empty `fields`, so field validation is skipped for it.
- R46: `action_log.action_id` is UNIQUE (a duplicate refuses with `duplicate_action_id`).

- R47: Reads: `head(id)` returns the head and reports a retracted head with `retracted: true` instead of as absent; an objects row whose head observation is missing, and decoded rows that fail validation, are `store_failure`, not absence.
- R48: `history(id)` returns every observation in append order with its `ingested_at` and its own pin.
- R49: `scan_heads(object_type, limit, after)` returns non-retracted heads in primary-key order `(instance, account, native_key)`, continuing from the last returned `Id`; a limit above 10,000 refuses with `limit_too_large`.
- R50: Every integer crossing into SQLite goes through `i64::try_from` (`value_out_of_range`).

- R51: The busy timeout is 250 ms for the program mode (unchanged) and 10 s for mode `Schema` at open (`open_schema_with_busy_timeout` selects another value).
- R52: A writer that cannot take the lock within it refuses with `store_busy`; it is not retried automatically.

- R53: `Store::backup_to(target)` runs `VACUUM INTO` with a bound parameter and refuses an existing target with `backup_target_exists`; the caller chooses the path, verifies the copy and rotates old copies.
- R71: `ont backup` follows the normal rotation (newest three kept).
- R54: `log_action` refuses an entry whose `domain` differs from its schema domain (`action_domain_mismatch`) or whose schema version is not the current pin of that domain in the transaction's set (`action_schema_mismatch`);
- R55: a duplicate `action_id` is recognised by the unique-constraint extended result code.
