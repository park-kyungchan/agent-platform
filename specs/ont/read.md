# Read

Purpose: Search, linked object-set reads and count aggregation.

Moved from crates/ont/README.md ("search", "Linked object-set reads", "Count aggregation") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## search
- R1: `qualified` covers the page returned (the scanned heads of this request), not the whole set when `continuation` is present.
- R2: `stale` and `partial` count scanned admitted heads; `stale_sources` lists the sources of every stale scanned head.
- R3: `scanned` now counts admitted candidate heads evaluated on this page; hidden and retracted heads consume neither the scan budget nor lookahead.


- R4: All four readers use the same visible candidate access and one read snapshot per operation.
- R5: The renderer shares its allowance across all pages; `around` shares its per-type allowance across all hops. Numeric limits:

| Reader | Candidate/head allowance | Output/other allowance |
| --- | --- | --- |
- R6: | `search` | 10,000 admitted candidates per page | 1-1,000 rows, default 100 |
- R7: | `around` | 10,000 unique admitted heads per type across all hops; candidates at first hop | 1-3 hops, 10,000 reference entries, 1-1,000 rows (default 100) |
- R8: | `aggregate` | 10,000 admitted candidates per operation | 2 group keys, 1 count metric, 1,000 buckets, 1,024 bytes/group value |
- R9: | `render` | 10,000 admitted candidates across all pages | 1-1,000 displayed rows, default 200; exact omitted count |
- R10: | One observation | `ont_core::MAX_OBSERVATION_BYTES` = 65,536 canonical UTF-8 JSON bytes | `ont_core::MAX_OBSERVATION_ARRAY_ITEMS` = 1,024 items in every array, including nested arrays |
- R11: | `get` history | `ont_store::schema_mode::MAX_HISTORY_OBSERVATIONS` = 100 observations | Newest 100 rows in append order; `basis.older_omitted` counts older rows from SQL metadata after admission; no continuation |

- R12: All observation writers, including renewal, share `ont_core::observation_payload` before writing any row of that observation.
- R13: Either bound refuses `observation_too_large`.
- R14: Indexed ingest rolls back the entire source transaction (reported as `source_failed` with that code); action apply rolls back all edits and the action-log row in its existing IMMEDIATE transaction.
- R15: Other sources in an index run remain independent.

- R16: Schema-mode loads check SQL `octet_length(payload)` from column metadata before loading the body: `length(TEXT)` counts characters, while a cast to BLOB first materializes the body.
This check counts UTF-8 bytes without that allocation.
- R17: A short-circuit SQL guard also checks every array before returning the payload.
- R18: Legacy over-bound heads are excluded by `search`, `around` and `aggregate`: `basis.oversized_excluded` counts admitted excluded heads, forces `qualified:false`, and each consumes the scan allowance (`around` counts each unique head once).
- R19: Hidden heads affect no visible count.
- R20: `render` refuses `observation_too_large` if any page excludes an oversized head, instead of publishing an incomplete block.
- R21: `get` admits metadata before counting or loading history.
- R22: Its newest-100 window includes the head; `basis.oversized_excluded` counts over-bound nonhead rows left out of that window, without backfilling from older rows.
- R23: This count and `basis.older_omitted` are disjoint.
- R24: An over-bound head still refuses `observation_too_large`; a valid head superseding a bad old row is readable.
The task Lead or store operator must repair an over-bound head by hand before `get` can read that object.
- R25: The engine can no longer write an over-bound observation.
- R26: An identical-schema projection rebuild writes uncertain `?` postings for every property of these unreadable legacy heads, so equality-filtered reads also count their exclusion without loading them; reverse `linked` reads retain their refusal.
- R27: These bounds change neither schema format nor store format 4.

A binary older than this change does not read `-`: on a projection written by this code its `search`, `aggregate` and `around` over a property without a value (an `eq` or `in` on it, an empty `in`, or a property the filter only uses) load fewer heads, so `scanned` and `unresolved_excluded` are smaller, `qualified` can be true where this code answers false, and a `continuation` can be absent; rows and criteria verdicts are equal; run one binary per store.
- R28: A store written before this change keeps its earlier `?` postings for fields without a value until its projection is rebuilt (a schema install that changes the schema): until then a reverse link loads those heads as before; heads written afterwards post `-`.

Candidate selection and search pagination:

- R29: `limit`: 1 to 1,000 matching returned objects, default 100. Continuation means another admitted candidate exists, not necessarily another matching row.
- R30: Scan budget: 10,000 admitted candidates evaluated per page; an additional admitted candidate refuses `result_too_large` unless the row limit was reached.
- R31: Candidates: the first `eq` or `in` leaf reachable through `all` nodes supplies a necessary property condition. `any`, `none`, other operators and no filter provide no condition and retain an unfiltered scan. Empty `in` is supported.
- R32: Unresolved used properties, historical schema pins, partial coverage and stale heads remain candidates even if the selected equality cannot match, preserving their existing qualification/exclusion effects. `scanned` counts candidates; aggregate `oldest_observed_at` is the minimum timestamp of loadable admitted candidates, not excluded fresh/complete heads. Other basis fields keep their definitions.
- R33: Filter bounds remain depth 8, 64 leaves, 100 `in` values; request bytes remain bounded as documented above. No payload is loaded before metadata admission.

- R34: The store maintains a format-4 `head_candidates` projection transactionally.
- R35: `visibility` stores each matching grant's canonical namespace/domain/scope-key predicate; `scope_keys` stores the actual object's key/value map. Omitted grant keys keep native wildcard semantics. The primary key is `(object_type,visibility,property,value,instance,account,native_key)`.
- R36: Current read admission (type, namespace, domain and scope-key equality) is fully represented by these partitions; Authority/session checks still run before payload loading. No inaccessible head needs a residual per-head work budget.

- R37: Heads whose observation cannot be read at rebuild, or with `fresh_until >= NEVER_STALE` (253402300799) have no expiry postings;
- R38: candidate clocks above `NEVER_STALE` refuse with `clock_out_of_range`.

- R39: Equality postings (`v<value>`), uncertain postings (`?` for an unposted value or an observation too large to read at rebuild, `-` for no value in a present observation), and 16 hexadecimal expiry prefixes per finite-expiry live head permit at most 240 stale equality streams per readable scope.
- R40: Every stream uses an indexed keyset seek after the previous object key and returns at most 256 metadata rows.
- R41: A bounded merge retains only the first 256 unique keys.
- R42: There is no UNION materialization or SQL sort; work is bounded by stream count times admitted candidates plus fixed seek overhead, independently of hidden heads.
- R43: With 100,000 fresh superseded and 201 current heads, only current/uncertain/stale postings are read and 201 payloads are loaded.
- R44: Unsupported filter forms also use the visibility-aware admission/lookahead loop: intentionally, hidden tails no longer create continuations or consume the old raw-head allowance.

- R45: A read-only open of format 3 or an older projection refuses with `store_upgrade_required`; the owner must upgrade through a writable open.
- R46: DDL, backfill and the format-4 marker commit atomically; two openers serialize.
- R47: Backfill/schema upgrades read live payloads in 256-head pages.
- R48: Repeated opens do not rewrite the projection.
- R49: Appends replace that head's postings in the write transaction; backfills do not replace them.
- R50: A changed candidate semantics version rebuilds the projection once at first open, linear in heads, including existing format-4 stores.
- R51: An old projection on unwritable storage refuses read-only opens until a writable open upgrades it.
- R52: Engines with different semantics versions that alternate on one store each rebuild at open, linear in heads.

- R53: Binaries older than format 4 reject it on both read-only and writable opens.
- R54: A format-4 binary without the semantics marker can still write; its checkpoint makes the present engine rebuild at its next open.
Before migrating from format 3, quiesce old writers and keep a consistent store copy and its matching binary. Rollback to a pre-format-4 binary requires restoring that pre-migration store copy, with any subsequent edits handled by the operator; that binary cannot open format 4. No db56e16 projection was installed outside disposable tests, so there is one migration path from format 3; disposable predecessor projections rebuild in that same migration. Do not manually lower the format marker.

- R55: Database triggers on every INSERT/UPDATE/DELETE of `objects`, `observations` and `schema_installs` advance a mutation generation, even through an older connection opened before migration.
These tables contain every candidate input: identity/scope/head, observation values/freshness/pins, and schema bytes including grants. `links` and `action_log` are not inputs to candidate membership.
- R56: Native projecting writes record `projected` and `semantics_projected` together in their transaction; partial writes refuse if the starting projection is stale.
- R57: Each candidate page compares generations and semantics versions inside its snapshot and refuses `projection_stale` on mismatch.
- R58: Searches do not acquire a write lock or start a rebuild.

Repair explicitly with `ont --store STORE schema install FILE`, using the exact installed schema bytes (or its `--from CHECKOUT --commit SHA --path PATH` form).
- R59: An identical install repairs all domains under IMMEDIATE; `changed:false` still refers to the schema pin.
- R60: Rebuild and checkpoint commit together; interruption rolls back and leaves the old correct projection or the stale refusal.
- R61: SQLite's schema cookie invalidates pre-migration prepared statements so their next execution runs the newly installed triggers; the held-connection regression executes and asserts this reprepare.
- R62: The format barrier remains in place.

- R63: The compatibility guarantee covers every binary of this repository, opened before or after migration.
- R64: Trigger definitions are checked at open and before candidate reads, cached against SQLite's schema cookie; missing/changed triggers refuse until identical-schema repair recreates them and rebuilds atomically.
- R65: Arbitrary SQL modification (dropping triggers, disabling them on a connection, or editing tables) is outside what the store can detect; presence checks do not detect a connection-local trigger switch.

## Linked object-set reads

- R66: `ont around` and `ont_search` with `"op":"search_around"` take `{"from":{"object_type":"hold","where":{"eq":["status","active"]}},"path":["task"],"limit":100}`.
- R67: `from` uses the search filter grammar; each of 1 to 3 path entries is a forward or reverse link name from `describe`.
- R68: The whole path is checked before loading heads.
- R69: One SQLite snapshot and one native admission cover the request; metadata admission precedes every payload load, including reverse scans.
- R70: Hidden and missing targets both contribute to `dangling`, with identical answers. Each hop removes duplicate IDs. The final set is sorted by ID, capped at `limit` (default 100, maximum 1,000); `matched` counts the full final set, with no continuation token.
- R71: The first hop uses the filter candidate condition and its unresolved basis. Reverse scans use unfiltered visible candidates; named forward targets use metadata admission.
- R72: At most 10,000 unique admitted heads per type may be loaded across all hops; otherwise the operation refuses `result_too_large`.
- R73: Basis `scanned`, `stale` and `partial` count unique admitted heads used by starting/reverse scans or forward lookups.
- R74: Unresolved filter comparisons contribute `unresolved_excluded`; unresolved used filter/link properties also prevent qualification. `qualified` requires no stale, partial, unresolved-used/excluded or dangling input.
- R75: Stale source entries name the `ont index --source NAME` refresh command.

- R76: Traversal charges at most 10,000 reference entries across all hops, including repeated IDs and missing targets.
- R77: Each admitted source's entire reference array is charged before iteration or target lookups, also in reverse scans; hidden source heads consume no entries.
- R78: Exceeding `MAX_REFERENCE_ENTRIES` refuses `result_too_large` with `max_reference_entries: 10000`.
- R79: Loaded heads are shared without copying their reference arrays before this check.

## Count aggregation

- R80: `ont aggregate` and `ont_search` with `"op":"aggregate"` accept `{"object_type":"instruction_conflict","group_by":["status"],"metrics":["count"]}` and optional `where` using the search filter grammar.
- R81: Only `metrics:["count"]` is supported; other metrics refuse `unsupported_metric` at `/metrics`.
- R82: The answer has `kind: aggregation`, `groups:[{key:{status:"open"},count:0},...]` and basis `scanned`, `unresolved_excluded`, `stale`, `partial`, `oldest_observed_at` (null for no admitted heads), `qualified`, `stale_sources`.
- R83: Read and derive grants are required on the type and on each candidate object's scope, before loading payloads.
- R84: Hidden objects affect no counts.
- R85: Bounds are 10,000 admitted candidates, 2 distinct group properties, 1 metric, 1,000 buckets including enum zero buckets, and 1,024 compact JSON bytes per group value.
- R86: Enum keys expand all declared combinations, including on an empty store when all keys are enums; mixed keys expand for observed non-enum combinations.
- R87: `group_by` may be omitted for zero keys.
- R88: `where` uses search membership, including `unresolved`, `any` and `none`; missing filter values are evaluated by that grammar.
- R89: Historical incompatible filter values are excluded and counted.
- R90: Only matching rows need resolved group values; oversized or incompatible historical group values are excluded and counted, never a request-wide refusal.
- R91: Any exclusion, stale/partial head or unresolved used filter value prevents qualification.

- R92: Group properties must have a declared worst-case compact JSON size <=1,024 bytes; otherwise preflight refuses `unsupported_group_property`, naming the property and a usable declaration, independently of the stored rows:

| Declared type | Size guarantee and admission |
| --- | --- |
- R93: | Enum value type | Largest encoded member; all members must fit. |
- R94: | Boolean | 5 bytes; admitted. |
- R95: | Integer, timestamp | 20 bytes; admitted. |
- R96: | Date | 12 bytes; admitted. |
- R97: | Decimal, including non-enum value types | Unbounded spelling without `max_bytes`; admitted with `max_bytes <=1022` (plus two quotes). |
- R98: | String, including non-enum value types | Unbounded without `max_bytes`; admitted with `max_bytes <=170` (up to six encoded bytes per input byte, plus two quotes). Pattern-derived bounds are not inferred. |
- R99: | Reference | Id fields have length limits, but their worst-case escaped representation exceeds 1,024 bytes; refused. |
- R100: | Array | The global 1,024-item cap does not establish a fitting 1,024-byte group bound; refused. |
- R101: | Struct | Admitted if field-name overhead plus every field's recursively declared bound fits; inner strings have no `max_bytes` declaration. |

- R102: Bucket order compares keys in `group_by` order: enum components follow declared enum order, other components sort by canonical encoded JSON.
- R103: JSON object fields are recursively sorted for identity, independently of map construction order.
