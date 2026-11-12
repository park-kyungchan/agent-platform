# Engineering memory originals
A record is one durable explanation with dated sources and one subject.
Git owns its original bytes and history; records are outside installed `content/`.
Exclude transcripts, STATE/status copies, KPI values, worker runs, receipts,
credentials and student data. The private Academy archive stays outside this repo.
Checks neither authorize effects nor adopt proposed rules.

## Header grammar
`records/<record_id>.md` is UTF-8 without BOM: first line `---`, a JSON object,
a line `---`, then Markdown. Split on line feed only; no carriage returns anywhere.
Use unique JSON keys and exactly these fields:
`schema_version` (integer 2 for current records; 1 only for legacy/migration input), `record_id`, `kind`, `title`, `subject`, `scope`,
`owner_ref`, `sources`. `title` and `scope` are nonempty strings.
`record_id` matches the filename: lowercase ASCII letters/digits separated by
single hyphens; keep it stable through edits and topic renames.
`subject` is exactly `{ "type": "platform", "id": "agent-platform" }`, with
nonempty id and type from platform, module, host, task, procedure, user, term,
memory-map. Profiles use user; explanations use term; navigation uses memory-map.
`owner_ref` is a store object Id (`object_type`, nonempty string-valued `namespace`,
`native_key`), or `{ "repository": "agent-platform", "path": "path/to/source" }`,
or `"none"`. Only `none` requires/allows the nonempty string `owner_ref_reason`;
a cross-task rationale without a store decision uses reason `spans tasks`.
`sources` is a nonempty array of exactly `{ "date": "YYYY-MM-DD", "locator": "..." }`
objects; each date is a real calendar date and each locator a nonempty string.
Schema 2 also requires `abstract` (nonempty), `horizon` (`mid-term` or `long-term`),
`review_on` (real ISO date), `recall_terms`, `derived_from`, and `codified_in`.
`recall_terms` has 1..8 distinct nonempty literal strings. The complete original
must fit in 4,096 UTF-8 bytes. The publisher must later verify each term in an
extracted fact or its entities; this byte limit is no token/semantic guarantee.
`derived_from` entries contain exactly `record_id` and nonempty `evidence_ref`;
targets must exist locally, without cycles or upward abstraction links.
Incidents have none; lessons require incidents; patterns require at least two
distinct incident chains through incidents/lessons. A Lead with a reviewer checks
actual independence. Proposals target patterns, lessons or incidents; without a
pattern target, Supporting records must contain `Critical exception: <text>` at
the start of a line. Only presence is checked, never justification.
Profiles/glossary have no abstraction levels. `codified_in` entries have exactly
nonempty `repository`, `path`, `commit` (40 hex digits), `anchor`; these link to a
rule rather than repeat it. No reverse relation store is maintained.
No status, manual revision counter or supersession field belongs in the file.

One original explains one assertion/mechanism in English, naming its subject,
condition and distinguishing identifiers; keep Korean proper nouns beside English
aliases. Source admission pins repository commit/path or evidence hash and excludes
private lesson bodies. A syntax checker cannot establish truth or detect every
private name. Initial review intervals <=30/180 days for mid/long-term are curator
policy. Overdue means review is due, not automatic invalidity or expiry.

## Kinds and required body sections
Each required section is a unique, nonempty second-level heading (`## Name`).

| Kind | One-sentence test | Required sections (exact English names) |
| --- | --- | --- |
| rationale | Explains why an owned decision was made, referencing its store Id; only a cross-task decision without one states the decision here. | Decision reference; Question; Reasoning; Tradeoffs; Alternatives; Assumptions; Counterevidence; Revisit trigger |
| incident | Records one identified occurrence, its verified or explicitly unknown cause, and actual verification. | Occurrence; Expected and actual behavior; Boundary; Impact; Cause; Mitigation; Verification |
| lesson | Derives a bounded prevention rule from identified incidents and explains when it works. | Incident references; Prevention rule; Conditions and exclusions; Why it worked; Counterexample; Verification |
| pattern | Shows a shared mechanism across at least two independent incident/lesson chains, not repeated reports. | Independent occurrences; Common mechanism; Competing explanation; Scope limits; Codification candidate; Recurrence evidence |
| direction-proposal | Proposes a structural change with alternatives including retaining the design, a reversible trial and acceptance/stop conditions. | Supporting records; Contradicted assumption; Alternatives; Expected effects; Reversible trial; Acceptance and stop conditions; Decision owner; Proposal reference |
| user-profile | States an explicitly evidenced user preference within a named scope, without inferred ability or blanket permission. | Preference; Explicit user evidence; Applicability; Valid from; Revisit trigger; Confidence |
| glossary | Preserves the explanation and feedback evidence, links GLOSSARY.md without copying its canonical definition, and never infers mastery. | Term; Canonical glossary reference; Explanation; Concrete input and executor; Result; Explanation evidence; Scope; User feedback |

Unknown feedback stays explicitly unknown. Promotion creates a linked higher-kind
record; it never rewrites an incident into a general rule. Corrections edit the
same original; distinct successors link through the store.

## Authorship and publication
The task's Lead writes and commits its records in the task's closure pull request.
Independent review of a record is required only for `direction-proposal`.
The file is the original; the server holds a retrieval copy written only from a
commit on `main`. Use record_id as document_id. Schema 1 requires explicit source
conversion before publication. Publication bytes remove ONLY `review_on`, emit
`title`, `subject`, `abstract` first, then remaining header keys sorted; nested
object keys are sorted, arrays retain order. Use compact JSON, `ensure_ascii=false`,
UTF-8 and `---\n<JSON>\n---\n<body>` with unchanged body bytes. SHA-256 hashes those
projected bytes outside the original. A review_on-only edit changes no retrieval
bytes/tags; every other record/subject/tag change needs the next maintenance window.
Status stays in its owning store; rebuilds make engineering retrieval unavailable.
No real work record is written yet: select the first records only after the
held-out questions are sealed under design section 10. This packet adds none.

## Subjects and tags
`subjects.json` is the sole vocabulary: exactly `schema_version:1` and `nodes`.
Each node has `type`, `id`, `parent` (null or `{type,id}`), nonempty `display_name`,
and distinct literal string `aliases`. Only memory-map:engineering is root;
maximum depth is four edges. platform/host/user attach to root; module to platform;
procedure to platform/module/host; task to root/platform/module/procedure/host;
term to root/platform/module/procedure. Reject duplicate nodes, unknown parents,
cycles, disconnected nodes, invalid parent types and unknown record subjects.
Subject ids use `[a-z0-9][a-z0-9._-]{0,127}`, never paths. Keep ids through rename
or reparent; user nodes imply no people hierarchy. Only the root is populated yet.
Tags are ordered: `subject:<type>:<id>`, `within:<type>:<id>` from self through root,
`kind:<kind>`, `horizon:<value>`. No authored status/parent/tag lists. Reparenting
requires a full rebuild before retrieval. Exact recall uses subject; explicit
descendants uses within, with `tags_match:"all_strict"` and optional literal
horizon tag; default includes both horizons. No wildcard or prefix inference.

## Check
`python3 -I -B ci/check-memory-records.py` checks every file under `memory/records/`.
An absent/empty record directory passes with zero files; keep fixtures only under
`ci/fixtures/memory-records/`. It is a syntax/shape check, not a factual judge.
`python3 -I -B -m unittest discover -s ci -p test_check_memory_records.py` runs tests.

Default checking requires schema 2 and loads `memory/subjects.json`; an explicit
positional directory is legacy/migration input (schema 1 accepted). `--subjects PATH`
selects a vocabulary; schema 2 always needs one. `--commit C` reads pinned Git blobs
and vocabulary, using the same validator, never working file bytes. Commit paths
are repository-relative; symlink/nonregular/nested originals are rejected.
Success is `memory records: <count> files, 0 errors`; failure is one
`<code>@<path>: <cause>` line per error, exit 1, without record bodies.
`--due YYYY-MM-DD` instead emits a JSON array sorted by record_id containing
`{"horizon":"...","record_id":"...","review_on":"..."}` for dates <= the input.
`--tree` emits a JSON array sorted by (type,id), containing vocabulary nodes plus
`depth`, only for schema-2 records and their ancestors. Empty views emit `[]`.
Both views validate the full input first, are mutually exclusive, and store nothing.
Python helpers for later packets: `load(root, subjects, commit, publication)` returns
`(count, records, nodes, errors)`, with no records returned on failure; `records`
maps record_id to `header`, exact `body` bytes, parsed `sections`, `path`.
`publication_bytes(record)` refuses schema 1; `publication_hash(record)` returns
64 lowercase SHA-256 hex characters. `derived_tags(header,nodes)` returns the ordered
list; `recall_tags(subject,descendants=False,horizon=None)` returns tags/all_strict.
Use helpers only after successful validation; no publisher or service is added here.

## Publisher runbook (V4-2a/V4-2b; offline implementation, not activated)
`python3 -I -B scripts/publish-memory.py publish --credential-fd 3` and
`python3 -I -B scripts/publish-memory.py verify --credential-fd 3` use only
`http://127.0.0.1:8888` and bank `coding-agent::engineering`. Neither accepts an
address/bank override. The caller opens an anonymous pipe/private inherited FD
IN PROCESS and passes only its integer number (`pass_fds` with Python subprocess).
Write exact credential bytes without a newline, close the writing end, and never
put them in shell text, argv, environment, config, a token file or debug output.
Do not invoke these commands against the service without an executing task grant.
Every request has a <=15s timeout, the run a 900s budget, and mutations have >=1s
start gaps. No request is automatically retried. A timeout or lost acknowledgement
means unresolved work: keep consumers stopped and inspect the exact document and
complete native parent/child operations read-only before any further mutation.

The Lead checks holds (including pending retraction/source-removal holds), reviewed
main ancestry, private-source admission and sealed held-out count/hashes before
selecting records. The publisher parses no authority file. It pins local `main`,
`memory/subjects.json`, `memory/engineering.json`, publication bytes/hashes and
complete derived tags using the checker's imported helpers. It never publishes
working files. Run from a checkout sharing the reviewed local main reference;
missing config on main refuses, including before this change is merged.
A nonblocking flock in Git's common directory (`publish-memory.lock`, mode 0600,
empty) spans pinning through exit and is shared by linked worktrees. Other hosts
and mount namespaces are unsupported. All four subcommands use the same `locked()` helper.

W1 inspects version/config, terminal parent/child operations, full lists and
page-inclusive models. W2-W5 bank export/restore/delete/create/absence and native
MCP denial remain UQ1/UQ2: `publish` refuses before any mutation, for existing AND
absent banks. V4-5 must measure these before replacing the refusals. The measured
`retain_once()` helper generates a fresh UUID per dispatch and never resends;
it cannot be reached through production `publish` while those barriers remain.
`verify` implements W7 set/byte/tag/recall-term/profile/trace checks and repeats
source/profile/operation checks. It emits counts, hashes and hashes of observed
extraction provider/model pairs only. It does not certify factual meaning, privacy or answer
quality. The Lead with a reviewer still checks subject/condition, aliases and
administrative assertions. No embedding/reranker runtime revision/cap is inferred.
W8 still requires full fixed/fresh-held-out acceptance at activation/API changes,
or the fixed canary subset for routine rebuilds (V4-3). W9 tools enablement and
fresh-session scope/negative-mutation/lesson-route qualification remain UQ2/UQ3
(V4-5), followed by the Lead's X7 measurement/readback and fresh consumer sessions.
A successful `verify` alone never exposes a bank or authorizes W8/W9. The script
never changes CLI settings, starts sessions, creates recovery archives, restores,
or deletes/creates a bank in this packet. Those unmeasured operations have no
speculative wire implementation. No live request belongs in CI.


### Retraction and restore (V4-2b)
`python3 -I -B scripts/publish-memory.py retract --record-id ID --credential-fd 3`
is the fixed-bank safety route. Before invoking it the Lead stops/drains affected
consumers and records the pending source-removal hold. The script names these
obligations in its output; it cannot prove consumer shutdown or inspect the hold.
It pins main, checks version/profile, terminal parent/child operations and zero
models/observations, then GETs the exact document. Its original_text hash must
match the pinned record's publication hash: Git is then the recovery copy.
A mismatch or missing record stops with `bank_copy_not_in_source` before DELETE.
`--no-recovery-copy` explicitly states that the owner recovered the bank copy
outside this script or that it must not be retained. No body is saved or printed.

One DELETE is followed by exact document GET404, exhaustive per-document facts
with total zero, and chunks GET404 (HC U5). A lost answer always stops with
`delete_unresolved_route_unavailable_no_resend`, after reporting boolean/unknown
readbacks; never rerun DELETE as a timeout recovery step. Failed readback stops.
The script repeats zero-model/profile/terminal-work checks and, for each pinned
recall term, sends a recall query with the checker's exact subject tag selector
and all_strict matching (HC U10). No returned result may name the withdrawn id.
There is one read per recall term, filtered by the record's subject tags; no separate read by subject alone is sent.
Only these clean reads permit `retracted_on_listed_reads`; arbitrary index
completeness remains unknown. Missing pinned source yields
`listed_reads=not_evaluated` and `deleted_unproven`, even with the override.
Every result leaves the route unavailable and consumers stopped. Only after proof
may the owning closure PR remove the original and verify removal on reviewed main.
The Lead keeps the source-removal hold until acceptance; a later rebuild must
exclude the id. Neither this command nor a recovery copy authorizes republishing it.

`python3 -I -B scripts/publish-memory.py restore --recovery TASK_ARCHIVE.zip --recovery-sha256 HEX --credential-fd 3`
requires a regular file opened without following a symbolic link and its matching
SHA-256 before any request. It checks pinned source and W1 preconditions through
measured reads; a read404 cannot establish an absent restore target. It then
refuses with `uq1_restore_unmeasured`. Native archive format, absent-target proof
and import semantics remain UQ1 for V4-5. No import, lifecycle or polling code
exists here. The Lead must keep restored reads denied until corrected current
source passes rebuild and qualification. V4-3 may use only the listed-read claim;
V4-5 must measure missing dependencies before replacing UQ refusals.
