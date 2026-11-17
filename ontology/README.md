# ontology
`work.schema.json` declares the `work` domain for the `ont` command of the
`ontology-platform` repository: object types built from files of this host's
daily work, plus Ontology-owned program state. Files remain authoritative for
source-owned types; actions alone write the Ontology-owned types.

Declared in version 1: `task` (STATE.md files), `packet` (TASK*.md at directory
depth 1 to 3), `report` (REPORT*.md at the same depths), `installed_artifact`
(four paths under `/usr/local/bin` are declared; only those that exist become
objects) and `instruction_file` (shared instructions under `/root/.agents` and
their sources under `content/`). An absent file has no object. Version 1 has no
action or function. Packets and reports are keyed by their path below the work root.

Links: `packet_task` and `report_task` (the first directory below the work root
equals the task key), `report_packet` (`REPORT-<id>.md` points to
`TASK-<id>.md` in the same directory) and the reference `source_copy` (an
installed instruction file points to its source under `content/`).
A reference has two special states. "No value": the rule could not build a key
(a `REPORT.md` has no packet id; a source file has no source copy). "Dangling":
the key is stored but no such object exists (the packet has another name, the
directory has no STATE.md, the skill source is in another repository).
`report.packet` is exact, with no fallback in version 1.

## Who may change it
Only a pull request to this repository, reviewed and merged like any source.
Nothing here is installed by merging; installation is a separate, authorized step.

## Check
`ont schema check ontology/work.schema.json` must exit 0, `"clean":true`.

## Directories the sources must never read
Each `markdown_fields` source carries an `exclude` list. A pattern prunes a
directory only when it matches the directory itself, so it is written per depth:
- `ws-daniel-prayer-login`, `problem-edit/context`, `problem-edit/runs`,
  `ws-replan/preparation/slices`: private or custody content.
- any directory named `wt`, `node_modules`, `.git`, or whose name starts with
  `target`, at depth 1, 2 and 3; any directory or file whose name contains
  `private`, `custody`, `student`, `oracle`, `secret` or `credential`, at every depth.
The list exists because a glob under `work/` would otherwise read those paths.
Before changing a glob or the list, enumerate matched names (no content) and
show none lies under an excluded path.

## Version 2: program state
Version 1 remains pinned in Git at
`f2d1b030bd1a0a66928ef57d9365027526e664da:ontology/work.schema.json`,
sha256 `4573ab180f63889029ae98c892c996e810f9d8140cf26726ee6d5fffbfc10605`.
Version 2 retains the original types, sources and links in meaning and adds
property descriptions. It adds five Ontology-owned types, keyed by action id:

| Type | Records | Actions |
| --- | --- | --- |
| `thread` (v5) | Task/key, consumer, consumer_kind (`domain_route`/`harness`/`none`), first_use; prior values in history | `open_thread`, `amend_thread` (requires reason) |
| `plan` (v6) | One per thread; immutable first_lines/limit_lines, current_lines, amended_by_family, scope_reviewed, optional reviewed_lines, reviewer_family, review_report | `open_plan`, `amend_plan`, `record_scope_review` |
| `hold` | Blocked effect, lifting condition/reason, `set_at` | `set_hold`, `lift_hold` |
| `decision` | Current choice per task/topic, predecessor, `decided_at` | `record_decision`, `supersede_decision` |
| `kpi` | Definition, target and latest recorded value/evidence | `define_kpi`, `set_kpi_target` |
| `measurement` | Immutable value, `observed_at`, evidence, `recorded_at` | `record_measurement` (also updates `kpi`) |
| `worker_run` | Runtime, assignment, thread, plan, layer, progress, result, start/close time | `start_worker_run`, `note_worker_run`, `close_worker_run` |

Task references are authoritative; there is no copied task-directory property.
Reverse links from a task are `holds`, `decisions`, `kpis`, `worker_runs`;
from a KPI, `measurements`; from an earlier decision, `successors`.
`supersede_decision` takes only the old decision reference and the new text,
`decided_by`, optional reason and undo; user attribution also requires nonempty quoted `user_words` and `user_form:decision`. A question refuses `user_question_is_not_a_decision`. Rules read `$decision.task` and
`$decision.topic` from the original snapshot even after marking it superseded;
the successor stores `supersedes` as that old decision reference. Two rules in
one action are atomic, including measurement creation and updating the KPI.

`agent` reads every type and may apply every action, including permission to
read properties through references. Actors on this host share the OS account;
client, agent and session attribution is self-reported, not authentication.
Rule and criterion reference reads require fresh, readable objects; missing or
unreadable references refuse, and stale source values refuse `rule_value_stale`.
Validate before apply and pass its basis as expected revisions to detect changes.

### Version 6 dispatch parameters
`open_thread(task,key,consumer,consumer_kind,first_use)` creates one task/key;
`amend_thread(thread,consumer,consumer_kind,first_use,reason)` preserves its history.
`open_plan(thread,first_lines,limit_lines,current_lines,amended_by_family)` sets
opening estimates once; Lead supplies limit_lines as 1.5 times first_lines.
`amend_plan(plan,current_lines,amended_by_family)` clears scope_reviewed but preserves reviewed_lines.
`record_scope_review(plan,reviewed_lines,reviewer_family,review_report)` records the
size justified by scope/design review; reviewed_lines below current_lines refuses
`reviewed_lines_below_current`; under version 6 equal reviewer/latest-writer families refuse `scope_review_same_family`; version 7 removes that comparison.
Families are self-declared caller values, not authenticated identities.
`start_worker_run` additionally requires thread, plan and layer (`engine`/`domain`/`harness`).
The thread must belong to the task (`thread_of_other_task`); the plan must
belong to the thread (`plan_of_other_thread`); missing plan input
uses native `invalid_parameter`; unknown plan refuses `plan_missing` before
`plan_of_other_thread` (the other plan criteria also fail).
Engine work without consumer_kind domain_route refuses `engine_without_domain_consumer`.
Consumer kind none or empty first_use refuses `missing_first_use`; whitespace-only
text is not tested, and parameter definitions have no minimum-length field.
current_lines must be at most limit_lines or present reviewed_lines, or tier must
be review; otherwise `plan_over_limit`. The legacy scope_reviewed flag is not read.
A version 5 over-limit plan without reviewed_lines needs a size-bearing review.
An amendment within reviewed_lines needs no new review. Stored numbers are read
through the required plan reference; ordered present guards the optional integer. No arithmetic,
review-content inspection or view is added by these criteria.
`record_decision` and `supersede_decision` accept optional user_words:1000 and
user_form (`decision`/`question`); user attribution requires both and nonempty user_words, otherwise
`user_decision_evidence_required`. Lead decisions may quote questions in reason.

### Times and optional values
`set_at`, `decided_at`, `started_at`, `closed_at` and `recorded_at` are UTC
application-time strings generated by `$now`, never supplied as parameters.
`record_measurement` alone takes `observed_at`: when the observation happened,
which may differ from when it was recorded. Supply normalized UTC ISO 8601,
for example `2026-11-11T00:00:00Z`. Time properties are strings, capped at 40
bytes. Caller-supplied `observed_at` uses `utc_observed_time`: fixed-width UTC
with month 01..12, each month's maximum day (February allows 29), hour 00..23
and minute/second 00..59. Leap-year validation is intentionally not required;
February 29 in a non-leap year is still admitted. The trusted `$now` destinations
keep the canonical-shape `utc_time` pattern, as required by the rule language.
Measurement
recording refuses `older_measurement` if any recorded observation time is later
than the input. Equal times are accepted for corrections; among equal times the
last applied value is current. Thus the board never moves to an older observation.
The rehearsal prints `validate` answers proving string time ordering.

`record_measurement` has two independent submission guards:
- `older_measurement` rejects an observation older than any recorded measurement,
  preventing a late stale value from moving the board backward.
- `future_observation` requires `{"le":["$observed_at","$now"]}`: give when the
  value was observed, no later than the trusted application time. It prevents
  future-dated immutable history from blocking later real observations.

The input's calendar-plausible fixed-width UTC form makes the comparison
chronological. In comparisons with `$now`, a bare word is a literal; use the
`$observed_at` parameter reference. A missing `$` matching a parameter name
refuses schema checking. Validation checks its own clock and does not promise
acceptance at a later apply; apply checks its clock inside the transaction.
Equality with application time is accepted. The rehearsal refuses both one hour
ahead and year 9999 without changing store, log or board bytes, then applies the
boundary time and a later real observation to show the board still advances.
Retry with the same `request_id`, client, agent and parameters returns
`already_applied` and preserves the original application time and object edits.

Decision reason/undo, hold lifting reason, an initial KPI target, worker session
and worker report are optional. `lift_hold.reason` supplies `hold.lift_reason`.
Omission/null skips optional assignments, preserving existing values on modify.
`set_kpi_target.target`, `note_worker_run.note` and `close_worker_run.result` are
required. A note writes `last_note`, separate from final `result`, and may also
attach a report; close preserves that progress note and requires its own final
outcome. The version 2 optional assignments need no presence guard; review criteria in
version 7 guard optional inputs with `present`. Ordinary scalar values cannot be cleared with null; this would require
a scalar-clear rule.

An action takes the full object Id returned by `search`; a view's task argument
takes only the native key. The four task-creating actions refuse absent or
retracted tasks as `unknown_task`; search for a live task before applying them.

### Worker file references and rendered cells
Worker assignments and reports use references `packet` and `report` to the
version 1 file object types; no path is copied into the worker. Use the Id from
`search`. Before a file is indexed, build the Id with its file type, the domain
namespace and the file path **relative to `/home/palantirkc/work/`**, for example:

```json
{"object_type":"packet","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"ontology-operational-base/TASK-O13.md"}
```

For a report, use `"object_type":"report"` and a report key such as
`ontology-operational-base/REPORT-O13.md`. A file reference may remain unresolved when its key or namespace is wrong;
`get` on the full Id shows whether the file object exists. Running
`ont index --source packets --source reports` can resolve a correct key for an
admitted file, but cannot fix an incorrect key or make an object for a file that
still does not exist or is excluded. A dangling
reference is still a value, not a missing property. Reference cells render the
native key alone whether or not the target is indexed: for example
`ontology-operational-base/TASK-O13.md` and
`ontology-operational-base/TASK-O13-unindexed.md`. They are readable locators
relative to the work root, not full Id JSON or document titles.

### Missing cells and retry identity
`(missing)` means a property was never set: the KPI Current/Observed/Evidence
cells before its first measurement, Target when no target was supplied,
live-worker Session or Note before one is supplied, or closed-worker Report
when no report was attached. These are intentional optional states, not errors.
Required active hold, decision and final result cells do not normally show it.

Keep the same `request_id`, client and agent when retrying an uncertain apply.
A new request id (or a different client, including CLI versus MCP) identifies a
new action and writes a second measurement. Equal time and value leave the
board unchanged while history grows. Reusing the same id with changed content
refuses `request_id_reused`. The rehearsal checks both cases, plus escaping a
pipe as `\|` and a line break as a space inside a rendered cell.

Descriptions on properties and parameters explain expected content, topic
uniqueness, time formats and evidence paths; each stays within 300 UTF-8 bytes.
The rehearsal measures native `describe` output and MCP `ont_describe` and
`ont_action_list` text sizes, asserting the MCP 262,144-byte response limit.

### Install and rehearse
Use an `ont` build with reference view arguments, reference property rule values,
`$now`, optional-value skipping and descriptions (ontology-platform PR #33), plus criterion `$now` (PR #34).
The file install route is only for disposable stores. The rehearsal requires
Bash, Python 3 and Git and takes one binary path:

```sh
ontology/rehearsal.sh /path/to/ont
```

It creates a store under `mktemp -d` (set `TMPDIR` to the authorized scratch
root), checks the frozen v1 pin, installs v1, indexes task/packet/report files,
installs v2, creates and lifts a v2 hold, then installs v4 and verifies the task
and v2 hold revisions are unchanged. It exercises the version 2 action/time,
retry, reference, escaping and optional-cell cases; prints six rendered blocks
(one memory-index block and five program-state blocks); and invokes the
exhaustive memory-index walk with two records. Its private stores are removed
on exit. Output sizes and time-comparison answers go to stderr. A walk failure
fails the rehearsal. Native calls must use the authorized disposable wrapper.

Host installation is a separate authorized operation after review and merge:
fetch this repository, then use `ont --store <authorized-store> schema install
--from <checkout> --commit <merged-40-hex> --path ontology/work.schema.json`.
The temporary-store check establishes neither deployment nor native CLI loading.

### Generated views and remaining limits
Every program-state view takes `task` as the task's native key, for example
`workspace-strategy-reassessment-20261109`, and filters directly on the `task`
reference with `{"eq":["task","$task"]}`. Render through stdin:

```sh
set -o pipefail
printf '%s\n' '{"op":"render","view":"kpi_board","args":{"task":"workspace-strategy-reassessment-20261109"}}' | ont --store "$scratch/store.sqlite" render
```

The response's `markdown` field is the generated block. `kpi_board` shows
Observed from `observed_at`, orders by name; `active_holds` filters active and
orders by effect; `current_decisions` filters current and orders by topic;
`live_workers` filters running, orders by started_at and adds Note from
`last_note`; `closed_workers` filters closed and orders by closed_at descending
so the newest runs remain within the default row limit. Packet and Report
columns display references; closed-worker Result displays the final result.
To update an existing block, use `render --write <file> --section kpi_board
--arg task=<native-key> --preimage-dir <existing-directory>` with an explicit
store; `render --check <file>` checks drift.

Reference view arguments support equality only; safe argument characters remain
ASCII letters, digits and `._:/-`. An unrepresentable native key refuses
`reference_argument_unrepresentable`. Each view defaults to 200 rows with an
omitted-row count and a 10,000-head engine bound. Rendering is not one snapshot
across pages: finish writers first for a coherent block. Typed numeric/date
arguments, conversions and property comparison would enable KPI progress or
observation-age filtering, elapsed worker time and closed-date windows. A
hold's `until` is intentionally prose; expiry comparisons need a separate typed
expiry field. `current_decisions` needs no numeric/date comparison.

## Version 3: memory retrieval index
Version 2 remains pinned at
`2b9417c8bc96b0cbeb4ab7913aa44cb49ad8d604:ontology/work.schema.json`,
sha256 `27de5fd02c20299897720ab34543e04e6db685438c3dd7409691cb8ddae11f24`.
Every existing definition is byte-equal; grants gain only additive entries.
Version 3 adds a cross-task `memory_record` keyed by the stable record id.
Its only properties are `document_id`, `status`, optional `supersedes` and
`body_hash` (64 lowercase hex SHA-256). Git owns all title/kind/subject/body
and provenance; see [../memory/README.md](../memory/README.md). No real record
is populated yet. The writer supplies document_id equal to the record key and
only selects originals from `main`; these are writer contracts.
`scope_keys` remains empty in namespace `vps://palantirkc` / `root`, with no task
reference. `agent` has the same read/action grants as other Lead-written types.

### Transitions (amendments A1 and A2)
`superseded` is final: never reuse that record id. A wanted-again text needs a
new original file/id. No action clears a predecessor reference. The links are
`memory_predecessor` and reverse `memory_successors`.

| Action | Parameters | Admitted transition / refusals |
| --- | --- | --- |
| `mark_memory_record_pending` | `record`, `document_id`, `body_hash`, optional `supersedes` | Creates a record or returns pending/ready to pending with a new hash; refuses superseded as `memory_record_already_superseded` and self-supersession as `memory_successor_is_self`. Omitted predecessor preserves its value. |
| `mark_memory_record_ready` | `record` | Pending to ready; refuses missing/nonpending (`memory_record_not_pending`), a ready predecessor (`memory_predecessor_ready`) or a pending/ready successor (`memory_record_has_successor`). |
| `mark_memory_record_superseded` | `record`, `successor` | Pending/ready to superseded; refuses missing/final record (`memory_record_already_superseded`), missing/nonready successor (`memory_successor_not_ready`) or missing/unequal successor predecessor (`memory_successor_not_linked`). |
| `withdraw_memory_record` | `record` | Pending/ready to superseded without requiring a successor; refuses missing/final record (`memory_record_already_superseded`); preserves every reference. |

Required engine: `ontology-platform` commit
`a1195da6ac03219cfb05a109829cc6347df7a429` or a compatible descendant.
The criteria use ordered `all`: target-existence guards precede head reads,
and `present` guards optional parameter/property comparisons. The pending self
guard is `none(all(present($supersedes),eq($record,$supersedes)))`; it refuses
a new id naming itself. The successor backlink comparison follows
`present($successor.supersedes)` behind its target-existence guard, so a missing
backlink is an evaluated false criterion. `any` and `none` evaluate every operand;
each comparison needing a guard stays inside its own ordered `all`.

`current_memory_records` takes no arguments and selects pending/ready records,
showing Record id (`$key`), Document id, Body hash and Status in one read.
A truncated answer means **current memory unavailable**. The native default
is 200 rows with a 10,000-head bound; paging/complete reads belong to P1b.
A successor withdrawn before its predecessor becomes superseded no longer
blocks that still-current predecessor. An already superseded id stays final.
The store guarantees exclusion only for an adjacent predecessor/successor pair:
both cannot be ready. It gives no exclusion guarantee across a longer chain.
Allowed example: pending A; pending B superseding A; pending C superseding B;
ready C; withdraw B; ready A. A and C are both current. Withdrawal ends B only;
A and C are judged by their own adjacent pairs, even though retained links join
the three records.

Both replacement sequences are admitted:

1. Ready predecessor: create the pending successor, withdraw the predecessor,
   then mark the successor ready. Between withdrawal and readiness neither is
   ready; the successor remains pending and therefore remains in the view.
2. Pending predecessor: create the pending successor, mark the successor ready,
   then mark the predecessor superseded naming the successor.

The writer must avoid a second pending/ready successor for one predecessor
(A1). It must replace only a ready predecessor and never create a successor of
a pending predecessor (A2). These are writer duties, not store admission rules;
the store permits forks and the second replacement sequence above.

### Exhaustive native-store walk
Run only through an approved disposable wrapper, with a scratch root:

```sh
python3 -I -B ontology/memory-index-walk.py /path/to/wrapper /path/to/scratch
python3 -I -B ontology/memory-index-walk.py /path/to/wrapper /path/to/scratch --records 2
python3 -I -B ontology/memory-index-walk.py /path/to/wrapper /path/to/scratch --records 3 --depth 4
```

The breadth-first walk explores every action/record/predecessor/successor choice,
including omitted/self predecessors and absent targets. Each action runs on a
copy of its source store; each new abstract state retains its shortest history.
States contain every record's status and predecessor. Document ids equal keys;
one fixed valid hash represents all hashes because no criterion reads them.
The script checks that abstraction premise against the schema. Bounds are
2,000 states, 100,000 native calls and 1,150 seconds, with 20-second call timeouts.
A bound stops with its own message. If three records exceed 20 minutes, report a
complete two-record walk and a stated-depth three-record walk instead.

- I1: superseded in a source state remains superseded in every outgoing state.
- I2: a record and its predecessor are never both ready.
- I3: a pending/ready record never names itself as predecessor.
- I4: every pending record can reach ready or superseded; reverse graph search
  checks this, with actual withdrawal witnesses at a bounded-depth frontier.
- I5: the view equals exactly all pending/ready records, including id/hash/status.

A failure prints the shortest action sequence and exits nonzero; I4 runs after
graph closure, so an earlier violation leaves I4 unverified. Artifacts include
states, native requests/answers, transitions and a summary with applied/refused
counts, bounds, elapsed time and exit. No operational store is selected by default.
An edge counts as refused only with exit 2, status `refused`, code
`submission_criteria`, and exactly the failed-code set the independent oracle
expects. Evaluation/parameter/rule errors and every unexpected code fail with
the shortest path. The summary includes refusal totals per expected code (one
edge may fail several criteria), unexpected-code count and completed depth.
At closure, the walk self-test follows native graph edges for both replacement
sequences and, with three records, the allowed withdrawn-middle example above.

## Version 4: derive grants
Version 3 remains pinned at
`e01ac49b6b62a0511d5bf7979cc3e1d8a1fcaa28:ontology/work.schema.json`,
sha256 `acd5b124d11f2cfb1b82161774b70e060318fd3607f9fb24ef122e12d249e96d`.
Every existing definition is byte-equal. Apart from the version number, the
only change is the one `agent` grant's additive `derive` list:
`hold`, `decision`, `kpi`, `measurement`, `worker_run`. These are the types
read by the five program-state views, plus `measurement`. Read and action
grants are unchanged; `memory_record` has no derive grant.

With an engine supporting count aggregation (ontology-platform PR #41), the
rehearsal runs this request against its disposable store:

```sh
set -o pipefail
printf '%s\n' '{"object_type":"worker_run","group_by":["status"],"metrics":["count"]}' | ont --store "$scratch/store.sqlite" aggregate
```

The observed result shape is below (inside the CLI's `status: ok`, `result`
envelope). `oldest_observed_at` is the rehearsal fixture's timestamp:

```json
{"basis":{"oldest_observed_at":1794362461,"partial":0,"qualified":true,"scanned":4,"stale":0,"stale_sources":[],"unresolved_excluded":0},"groups":[{"count":2,"key":{"status":"running"}},{"count":2,"key":{"status":"closed"}}],"kind":"aggregation"}
```

The rehearsal asserts both exact enum groups and separately checks that
`{"object_type":"memory_record","metrics":["count"]}` refuses with
`derive_forbidden`. This source change grants no live-store installation;
installation remains a separate authorized operation.

## Version 7: reviews and write sets
`review` is the sole record of a model's verdict on an exact head: task, kind
(`packet`, `integrated`, `design`, `integration`), optional repository/pull_request,
head and optional base (40-hex commit or 64-hex design digest), optional worker
runs, model, effort, prompt path/digest, optional report path/digest, verdict,
account, session, status and application times. Keys come from `$action_id`.
`file_path` checks an absolute printable ASCII path without spaces, 2 to 400 bytes;
callers keep paths below `/home/palantirkc/work/` and supply their
SHA-256 digests (`sha256_hex`). `memory_body_hash` remains unchanged.
`start_review` refuses `unknown_task` and `duplicate_running_review` for an
existing running review of the same task, head and kind. `close_review` refuses
`review_not_running`, and `report_required` unless both report and digest are
supplied or verdict is `abandoned`. `reviews(task)` renders newest starts first.
`agent` may read/derive reviews and apply both actions.

`worker_run.write_set` and the optional `start_worker_run.write_set` parameter
are ordered arrays of repository-relative patterns: relative paths without
empty, `.` or `..` components or backslashes, optionally ending in `/**`, with
no other `*`, `?` or `[` wildcard. `write_pattern` checks each member against
`^[A-Za-z0-9._][A-Za-z0-9._/+@-]{0,296}(/\*\*)?$`; the schema has no
member-count bound.
Members use ASCII letters, digits and `._/+@-`, begin with a letter, digit, `.`
or `_`, and may have a final `/**`. The tracked-path character check finds 0
incompatible paths in `agent-platform`; the Lead measured 0 in `ontology-platform`
and 27 non-ASCII tracked paths in one directory of `education-platform`, which
packets name by the directory and `/**`.
`ap unit` checks a unit's changed paths against the write set of its page, not
against stored records (see [One unit](../crates/ap/README.md#one-unit)).

`record_scope_review` retains every existing family field and parameter but
no longer compares families. Its optional `review` reference must be closed
(`review_not_closed` otherwise) and is stored in `plan.review`. All version 6
properties remain declared; existing objects need no migration. The rehearsal
runs the old family refusals on version 6 before installing version 7, then
checks review transitions, write-set dispatch and existing-object compatibility.

`review.running_in` is set by `start_review`, cleared by `close_review`, followed
only by the duplicate criterion, and never set by a caller.
`worker_run.running_in` is set by `start_worker_run`, cleared by `close_worker_run`,
followed only by the duplicate criterion, and never set by a caller.
An applied `close_worker_run` or `close_review` answers two edits of the same object.
`status` remains the running/closed statement used by views and other criteria.
For a new type, a uniqueness criterion must link through a reference that is
set only while the object is open and cleared by its closing action (LENS F43).
Version 6 and earlier version 7 objects without `running_in` can still be noted
and closed, but do not block a new start with the same name/head/kind; no migration
is performed. The duplicate criteria basis contains only running objects, including after a later schema install; the engine still reads every closed object as a reverse-link candidate because a cleared `running_in` is `missing`, which its candidate index treats as unknown, until engine item G3.

## Version 8: install attempts and revoked reviews
Version 7 remains pinned at
`ccb15142dbe19f2e04ad1c5d445048336db60d08:ontology/work.schema.json`,
sha256 `6f6d2acc4a45ccc7e2f25268d458f9b6fabc5ab10fb85231d0fa81eaf68373f4`.
Every definition of version 7 reads the same except two additions to `review`
and one new view: `review_status` gains `revoked`, `review` gains the optional
`revoked_at` (`utc_time`) and `revoke_reason` (at most 100 bytes), and the view
`installs` is added. No existing view changes.

`install` is the record of one install attempt on one `installed_artifact`:
started before an installer runs, closed after a read of the target, so that a
dead run is found and "once" is decided by the store. It is Ontology-owned, keyed
by `$action_id`, titled by `commit`. Its properties: `target` (the
`installed_artifact`), `kind` (`forward`, `rollback`), `repository`
(`owner/name`), `commit` (installed, or rolled back to), `attempt` (1 for the
first on this target and commit), `from` (what a read of the target showed
before), optional `from_sha256` (the digest of the target file before: the name
of the copy the installer keeps is made of it, and a dead run leaves no other
handle), `by` (a timer's invocation or a Lead's session), optional `head` (the
accepted head whose merge is `commit`), `review` (its accepting review),
`reviews` (every accepting review of the installed range), `past` (commits a
Lead stepped over, 40 hex each joined by one space, at most 2,049 bytes),
`reason`, `rolls_back` (for kind `rollback`: the attempt whose install it
undoes); then `status` (`running`, `closed`), `started_at`, `running_in`, and
after the close `outcome` (`installed`, `not_started`, `failed_restored`,
`failed`), `to` (what the read after the installers showed),
`rollback_binary` (the copy of the file the installer kept) and
`rollback_content` (the tarball of the content it kept), both `file_path`,
`code` and `closed_at`. Links: `install_target` (reverse `installs`),
`install_running_target` (reverse `running_installs`), `install_review`
(reverse `review_installs`), `install_reviews` (reverse `range_installs`),
`install_rolls_back` (reverse `rolled_back_by`).

| Action | Parameters | Refusals, in this order |
| --- | --- | --- |
| `start_install` | `target`, `kind`, `repository`, `commit`, `attempt`, `from`, `by`; optional `head`, `review`, `reviews`, `past`, `reason`, `from_sha256`, `rolls_back` | `unknown_target` (no such installed file); `install_running` (an `install` of this target is running); `review_not_accepting` unless `kind` is `rollback` or `review` and `head` are given, the review exists and is closed, of kind `packet`, with verdict `accept`, of `repository` and of exactly `head`; `unknown_rolled_back` (`rolls_back` names no `install`) |
| `close_install` | `install`, `outcome`, `to`; optional `rollback_binary`, `rollback_content`, `code` | `install_not_running` |
| `revoke_review` | `review`, `reason` (at most 100 bytes) | `review_not_closed`: a running review, and one already revoked |

`start_install` creates the object with every parameter, `status: running`,
`started_at: $now` and `running_in: $target`; `close_install` sets the five
values, `status: closed`, `closed_at: $now` and clears `running_in`, and answers
two edits of the same object. `running_in` follows the rule of version 7 for a
new type: set by the start, cleared by the close, followed only by the duplicate
criterion. A second running attempt of one target is refused; another target is
admitted; a closed attempt cannot be closed again; the same request id with the
same parameters answers `already_applied` and creates no second object.
`reviews`, `past` and `rolls_back` are the caller's words: no criterion reads
the elements of `reviews` or `past`, and `rolls_back` is only checked to exist;
the store holds the rule for `review` alone. A search has no order, so the newest
attempt of a target is found by reading its attempts and ordering them by
`started_at` (one attempt runs per target at a time). The view `installs` takes
no argument and shows one row per attempt, newest `started_at` first, with the
columns Started, Kind, Commit, Attempt, Status, Outcome, To and By.

`revoke_review` sets `status: revoked`, `revoked_at: $now` and
`revoke_reason`; `verdict`, report and `closed_at` stay. A revoke of a review
that is still `running` applies nothing (`ap review revoke` posts the failure as
before); that review can still be closed. Every reader of a closed review reads
`status` `closed`, so none finds a revoked one: `record_scope_review` refuses
it with `review_not_closed`, `start_install` with `review_not_accepting`, and
the come-back of `ap unit` does not reuse it (see [One review](../crates/ap/README.md#one-review)).
`agent` reads and derives `install` and applies the three actions. The caller of
`start_install` and `close_install` is `ap deliver`, with the client and agent
`ap-deliver` (see [Delivery](../crates/ap/README.md#delivery)).

Version 8 remains pinned at
`64fb1f59c6ff426807886d5a10397f5be3e496fa:ontology/work.schema.json`,
sha256 `8b4b4236d9e5cff8ce3286211682cc977f4cdf1fdbfb36b42aefa08021e04d23`.
The rehearsal runs the rows of version 7 on the pinned version 7 file, starts a
review and closes two reviews with `accept` there, installs the pinned version 8
file, closes the running review and revokes one closed review; it then installs
version 9 (below) and later names the other review as the `review` of a
forward attempt. `installed_artifact` is owned by a source of four
paths under `/usr/local/bin` that need not exist where the rehearsal runs, so the
install rows run on a copy that differs from this file only in those paths (two
files of the rehearsal's own directory) and in its version (10, above the installed 9); the rehearsal
asserts that every other member is equal. It prints the basis of the search of
`ap unit` made before this change, for the review closed under version 7
(`v8_upgrade_search_basis=`): on this engine a filter on an optional property
(`repository`, `verdict`, `report_sha256`) answers `qualified: false` as soon as
one scanned review lacks that property (a running or an abandoned review,
whatever its schema version), while `unresolved_excluded` stays 0, so the basis
does not name the object that made it unqualified. The read a caller can rely on
is the row `v8-upgrade-closed-review-qualified`: a search by properties that
every review has (`kind`, `head`, `status`, and a task) is qualified, and
`verdict`, `repository` and the report's digest are compared on the objects of
the answer. `ap unit` and `ap deliver` read an accepting review only this way,
through one function (`accepting` in `crates/ap/src/review.rs`).

## Version 9: the objective, the backlog item, the order of a task and the triage of the user's rows
Version 8 remains pinned at
`64fb1f59c6ff426807886d5a10397f5be3e496fa:ontology/work.schema.json`,
sha256 `8b4b4236d9e5cff8ce3286211682cc977f4cdf1fdbfb36b42aefa08021e04d23`.
Version 9 only adds. Every object type, action type, link type, view, value type
and grant of version 8 reads the same; `decision` gains five optional
properties, the grants gain the three new types and the fourteen new actions, and
the version number changes. The rehearsal installs version 9 over the store that
the rows of version 8 leave, compares 19 named objects (listed under "Install over
version 8" below) and every view before and after, and then applies `record_decision` and refuses a duplicate
of a review started under version 8 (`duplicate_running_review`).

A "current row of the user with words" below is a `decision` of the named task
with `status` `current`, `decided_by` `user` and a nonempty `user_words`. Every
"later than" compares two stored times of one-second resolution: a row recorded
in the same second as the stated or confirmed time is not later (the
rehearsal moves its own clock by one second before each such row). All the types
are Ontology-owned and written only by the actions; they are keyed by the action
id, except `board`, keyed by its task's key.

### `objective`: the state the user wants
Properties: `task`, `name` (at most 60 bytes), `statement` (600), optional `check`
(400, a command whose exit 0 shows the objective holds), `rank` (integer),
`status` (`stated`, `confirmed`, `retired`), `source` (a `decision`), `stated_at`,
`confirmed_by` (a `decision`) and `confirmed_at`, `retire_reason` and
`retired_at`, and the last check: `last_exit`, `last_line` (200),
`last_checked_at`. Links: `objective_task` (reverse `objectives`),
`objective_source` (`sourced_objectives`), `objective_confirmed_by`
(`confirmed_objectives`).

| Action | Parameters | Refusals |
| --- | --- | --- |
| `state_objective` | `task`, `name`, `statement`, `rank`, `source`; optional `check` | `unknown_task`; `source_not_user_words` (the source is not a current row of the user with words of this task); `name_used` (any objective of the task ever had the name, a retired one too) |
| `restate_objective` | `objective`, `statement`, `source` | `objective_not_live` (retired); `source_not_user_words`; `confirmed_needs_later_row` (the objective is `confirmed` and the source was not decided later than `confirmed_at`) |
| `confirm_objective` | `objective`, `row` | `objective_not_stated`; `row_not_user_words`; `row_is_source_or_earlier` (the row is the source, or not decided later than `stated_at`) |
| `amend_objective` | `objective`, `decision`; optional `rank`, `check` | `objective_not_live`; `row_not_current` (not a current decision of the task); `nothing_to_amend` (neither `rank` nor `check`) |
| `retire_objective` | `objective`, `reason`; optional `row` | `objective_not_live` (retired already); `confirmed_needs_later_row` (a confirmed objective needs a row of the user with words decided later than `confirmed_at`) |
| `record_check` | `objective`, `exit`, `line` | `objective_not_live`; `objective_without_check` (the objective has no `check`: a result is recorded only where a check command stands) |

`restate_objective` modifies the same object: `statement`, `source`, `stated_at`
`$now`, `status` `stated`, and `confirmed_by` cleared. `confirm_objective` sets
`confirmed`, `confirmed_by` and `confirmed_at`. A time cannot be cleared, so
`confirmed_at` stays after a restate; it means something only while `status` is
`confirmed`, and the criterion of `restate_objective` and `retire_objective`
reads it only for a confirmed objective: an objective that was never confirmed,
or was restated since, is restated with any current row of the user.
`amend_objective` changes `rank` or `check` and nothing else, and does not change
the status. Reading an earlier statement back: `ont get` on the objective prints
`objects[0].observations[]`, one entry per edit, each with its full `fields`; the
statements, with consecutive repeats removed, are the sentences in the order they
were stated (`ont` has no command that prints the action log). The rehearsal reads
them back this way.

### `item`: one piece of work that was asked for
Properties: `task`, `key` (60 bytes), `title` (200), `statement` (400), `objective`,
`origin` (a `decision`), `first_use` (300), `state` (`queued`, `in_work`, `done`,
`dropped`), optional `repository` (200) and `branch` (100), `review`, `evidence`
(600), `release_reason` (400), `drop_reason` (600), and the times `added_at`,
`started_at`, `closed_at`, `dropped_at`. Links: `item_task` (reverse `items`),
`item_objective` (`objective_items`), `item_origin` (`originated_items`),
`item_review` (`closed_items`: the items a review closed).

| Action | Parameters | Refusals |
| --- | --- | --- |
| `add_item` | `task`, `key`, `title`, `statement`, `objective`, `origin`, `first_use`, all required | `unknown_task`; `origin_not_current` (not a current decision of the task); `objective_not_live` (of another task, or retired); `key_used` (any item of the task ever had the key, a dropped one too) |
| `amend_item` | `item`; optional `title`, `statement`, `first_use`, `objective` | `item_closed` (done or dropped); `objective_not_live` (a given objective is of another task or retired) |
| `start_item` | `item`; optional `repository`, `branch` | `item_not_queued`; `item_not_ordered` (the board of the item's task does not name the item); `repository_and_branch` (only one of the two is given, or the item holds a repository and the call gives none) |
| `release_item` | `item`, `reason` | `item_not_in_work` |
| `close_item` | `item`; optional `review`, `evidence` | `item_not_in_work`; `review_or_evidence`; `review_closes_another_item` |
| `drop_item` | `item`, `reason` | `item_done_or_dropped` |

`add_item` sets `state` `queued`. `start_item` sets `in_work`, `started_at` and,
when given, `repository` and `branch`; `release_item` returns to `queued` with
`release_reason`; `close_item` sets `done`, `closed_at` and its `review` or
`evidence`; `drop_item` sets `dropped`, `drop_reason` and `dropped_at`. No action
reopens a done or dropped item: each action refuses every state it does not
start from, and a defect of a done item is a new item. A property that was set
cannot be cleared, so a repository stays on an item after `release_item`; that is
why `start_item` refuses a start without a repository on an item that holds one:
an item that ever had a repository closes only with a review. A new start may
name another repository and branch.

`close_item` takes one of two forms. An item with a repository needs a `review`
that is `closed`, has verdict `accept`, and is of the item's task and the item's
repository, and no `evidence`. An item without a repository needs `evidence` and
no `review`. Any other combination answers `review_or_evidence` (a running,
abandoned, revoked, other-repository or other-task review, a review with
evidence, and neither, are rows of the rehearsal). A review that another item
already names answers `review_closes_another_item`. That is all the store binds
of a review: its task, its repository, `closed` with the verdict `accept`, and
named by no other item. That the review is the one of the item's own pull request
is held by the caller (`ap unit` passes the accepting review of the pull request
it merged); the `review` has no property that names the item in this version.

### `board`: the order of a task's work
Properties: `task`, `order` (an array of references to `item`, kept in the order
given: the rehearsal reads back a list of five, a replacement and an empty list),
`reason` (600) and `set_at`. A board is
keyed by its task's key, which the caller writes in the `board` parameter; there
is at most one per task. Links: `board_task` (reverse `boards`), `board_order`
(reverse `ordering_boards`: the boards that name an item), and, from `decision`, `decision_triage_item`
(`triaged_rows`) and `decision_triage_objective` (`intent_rows`).

| Action | Parameters | Refusals |
| --- | --- | --- |
| `set_order` | `board`, `task`, `order`, `reason` | `unknown_task`; `board_of_another_task` (the board is of another task, or the task has a board and the call names a second one) |

`set_order` creates the board at its first use and replaces the whole list,
`reason` and `set_at` afterwards. It takes no row and the board holds no
reference to one: a property cannot be cleared, so a later order given without a
row would keep naming the earlier row. When a row of the user caused the order,
the `reason` names the topic of that row.

### Triage of the user's rows
`decision` gains the optional properties `triage` (`item`, `intent`,
`not_adopted`, `none`), `triage_item`, `triage_objective`, `triage_reason` and
`triaged_at`. `triage_decision(decision, triage; item, objective, reason)` sets
them once on a row. Any current row decided by the user is triaged, with or
without `user_words`: rows recorded under versions 2 to 4 have none and are still
current (the rehearsal records one under version 2 and triages it after version 9).

| Refusal | When |
| --- | --- |
| `not_a_user_row` | the decision is not current, or not decided by the user |
| `already_triaged` | the row has a `triage` |
| `item_required` | `triage` is `item` and `item` is missing or of another task than the row |
| `objective_required` | `triage` is `intent` and `objective` is missing or of another task than the row |
| `reason_required` | `triage` is `not_adopted` or `none` and `reason` is missing |
| `reference_not_for_triage` | `item` is given with a word other than `item`, or `objective` with a word other than `intent`: a reference is stored only under the word that chose it |

A row of version 8 or earlier has no `triage`; nothing triages it by itself.

### What the store does not check
- The elements of `order`. The engine cannot test each element of an array
  parameter in a criterion, so a done or dropped item, an item of another task, a
  dangling reference and one item named twice are stored, and so is an empty list
  (the rehearsal stores an item of another task, a dangling element and an empty
  list; the engine stores `[A, A, C]` as given). The check lives in `start_item`: it refuses an item that no board of the
  item's own task names (`item_not_ordered`), including an item that only another
  task's board names; a reader that prints the order shows the elements that fail.
- That a row is about the thing it is cited for. A row is checked for its task,
  its status, its author and its words, and by its time where a rule says so; the
  store does not read what it says. The same holds for the `evidence` text of an
  item and for the `check` command and the `line` of an objective.
- That the key of a board equals its task's key: that is the caller's convention;
  only the task, through `board_of_another_task`, is checked.

### Install over version 8 and the rehearsal
`ont schema install` of version 9 over a store at version 8 changes no object:
the rehearsal asserts it by reading 19 objects written under version 8 and
earlier (a task, a packet and a report, one hold, three decisions: a Lead row, its
successor and a row of the user without words recorded under version 2, one KPI,
one worker run, one thread, one plan, and eight reviews: closed, running and
revoked) and the views before and after; measurements, the other holds, workers,
threads and plans and every install record are not in that list, and it
checks that `ont_describe` and `ont_action_list` stay within the MCP limit of
262,144 bytes. Version 9 has no new view or function. Delivery refuses with
`schema_differs` while the store's version differs from the repository's, so the
host's store is installed once, by the authorized step above, and delivery
installs again after it. An installed version cannot be taken back by a revert;
the way back is a later version.

An empty string is no value. Every string parameter of the fourteen actions,
except `line` of `record_check` (a check may print nothing), has the value type
`nonempty_text` (a string of at least one character of any kind: spaces, line
breaks and bytes outside ASCII included), so `""` is refused with
`invalid_parameter` before any rule runs. No refusal code was added for it, and no
rule reads an empty value: an objective never holds an empty `check`, so
`record_check` records only where a check command stands; `close_item` never
closes on an empty `evidence`; `triage_decision` never takes an empty `reason`;
`start_item` never stores an empty `repository` or `branch`.

The rows of version 9 run after the rows of version 8, each refusal before the
action it guards, with the answer's code asserted (a row of several failed
criteria asserts the whole set). They are in `ontology/rehearsal.sh` under the
comment "Version 9".

## Version 10: requests, intents, criteria
Version 9 remains pinned at
`8379ae651929853d6fb2df79559ece4a98c5f551:ontology/work.schema.json`,
sha256 `faba52d93401384cdc6146a7f505c61dafc4a87662deb284dfc689f19a58d298`.
Version 10 only adds: every value type, source, object type, link type, action type and view of version 9
reads the same, byte for byte in canonical JSON, and the one grant gains the three new types (read and
derive) and the seventeen new actions. The new types are the user's side of the life cycle of work:

1. Every message of the user that orders, decides, prefers, corrects, stops, answers or delegates is a
   `request`, recorded in the user's own words (`record_request`). It starts `unrouted`.
2. A request is routed to the `intent` it is part of (`draft_intent` routes the request that started the
   intent, `route_request` any other), or set aside as part of no intent (`set_request_aside`).
3. An intent is drafted, gains its success criteria (`add_criterion`), and is confirmed by the user's own
   word routed to it (`confirm_intent`), or by the Lead under a delegation of the user
   (`confirm_intent_delegated`). The Lead may proceed on a draft before that (`assume_intent`).
4. Each `criterion` is checked (`check_criterion`) or waived by the user (`waive_criterion`); the intent
   closes `met` when none is `open` or `fail` (`close_intent`), or is retired (`retire_intent`).

All three types are Ontology-owned and written only by these actions. `request` and `criterion` are keyed by
the action id; `intent` is keyed by the native key of the parameter `intent` of `draft_intent` (the engine's
`/native_key` form, as `board` is), has no `title` and is shown by that key. A time below is a stored UTC
time of one-second resolution; "later" is strictly later: a request said in the same second as the
intent's last amendment does not confirm it (the rehearsal moves its clock by one second).

### `request`: one message of the user
| Property | Type | Meaning |
| --- | --- | --- |
| `words` | string, 24,000 bytes | the user's own text, in the user's language; no action changes it |
| `words_verified` | boolean | true when a program read the words from a session record; set once, and unverified words become verified only by a `supersede_request` that carries the verified ones |
| `truncated`, `record` (400) | boolean, string; optional | the words were cut; the session record they were read from |
| `reading` | string, 1,000 | what the agent took the message to mean, in English |
| `kind` | `order`, `decision`, `preference`, `correction`, `stop`, `answer`, `delegation` | a `delegation` hands the confirmation of named intents to the Lead for a time the user does not step in |
| `topic` | string, 120 | a label, not unique (view `requests_by_topic`) |
| `said_at`, `recorded_at` | UTC time | when the user said it (never after the recording); when it was recorded (`$now`) |
| `session` (120), `runtime` (40) | string, optional | where it was said |
| `status` | `current`, `superseded` | superseded by a correction or, for a delegation, by `end_delegation` |
| `supersedes` | `request`, optional | the request this one corrects |
| `route` | `unrouted`, `intent`, `aside` | |
| `intent` | `intent`, optional | the intent while `route` is `intent`; cleared by `set_request_aside` |
| `route_reason` | string, 600, optional | valid only while `route` is `aside`: a scalar cannot be cleared, so the reason of an earlier aside stays as history |
| `routed_at` | UTC time, optional | the last route or aside |
| `legacy_decision` | `decision`, optional | the row of version 9 this request was carried from |

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `record_request` | `words`, `words_verified`, `reading`, `kind`, `topic`, `said_at`; optional `truncated`, `record`, `session`, `runtime`, `legacy_decision` | a `current`, `unrouted` request | `words_empty`; `said_in_the_future` (`said_at` after now: a local time written with `Z`); `legacy_not_a_user_row` (a `legacy_decision` is given and is not a decision decided by the user); `legacy_already_carried` (a request already carries that decision) |
| `supersede_request` | `request` and the parameters of `record_request` but `legacy_decision` | the old request `superseded`; a new `current`, `unrouted` one with `supersedes` | `request_not_current`; `words_empty`; `said_in_the_future`; `request_confirms_an_intent` (the request is the `confirmed_by` of an intent: amend that intent first, which clears the confirmation) |
| `route_request` | `request`, `intent` | `route` `intent`, the reference, `routed_at` | `request_not_current`; `unknown_intent`; `intent_closed` (`met` or `retired`); `request_confirms_an_intent` |
| `set_request_aside` | `request`, `reason` | `route` `aside`, `route_reason`, `routed_at`; the reference `intent` cleared | `request_not_current`; `reason_empty`; `request_confirms_an_intent` |
| `end_delegation` | `request`, `stopped_by` | the delegation `superseded`; the intents it confirmed keep it as their history | `not_a_delegation` (not a current request of kind `delegation`); `by_not_the_users_word` (`stopped_by` is not a current `order`, `decision`, `correction` or `stop`); `by_not_later` (`stopped_by` was not said later than the delegation) |

The second parameter of `end_delegation` is `stopped_by`, not `by`: a rule that modifies an object, sets or
clears a reference changes the first parameter, in name order, that refers to the rule's type, and both
parameters refer to `request`. The rehearsal asserts that the action changed the delegation and nothing
else.

### `intent`: one objective of the user
| Property | Type | Meaning |
| --- | --- | --- |
| `outcome` | string, 1,200 | what is true when it is met, as sentences that can be checked |
| `why` (800), `not_included` (800), `constraints` (800) | string, optional | the need; what is not wanted; the limits the work keeps |
| `first_user` | string, 600 | who or what uses the result first, and the real run that shows it |
| `questions` | array of `{question, default, answer}`, optional | open questions; `question` and `default` are nonempty, `answer` is empty until answered: each question has a default by its type |
| `origin` | `request` | the request that started it |
| `status` | `draft`, `assumed`, `confirmed`, `met`, `retired` | |
| `amended_at` | UTC time | the time of `draft_intent` or of the last `amend_intent` |
| `confirmed_by`, `confirmed_at`, `confirmed_delegated` | `request`, UTC time, boolean; optional | the confirming request (a delegation when `confirmed_delegated`); `confirmed_at` is valid only while `status` is `confirmed` or `met` |
| `assumed_reason` | string, 600, optional | valid only while `status` is `assumed` |
| `lead_session` (120), `lead_named_by` | string, `request`; optional | the session that leads it and the user's word that named it |
| `directory` (300), `notes` (4,000) | string, optional | the task directory; the Lead's free text for the next Lead |
| `closed_at`, `closed_reason` (600) | optional | the close as `met` or `retired` |

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `draft_intent` | `intent`, `outcome`, `first_user`, `origin`; optional `why`, `not_included`, `constraints`, `questions`, `directory` | a `draft` with `amended_at`; the origin's `route` `intent`, its `intent` and `routed_at` | `origin_not_current`; `origin_confirms_an_intent`; `outcome_empty`; `first_user_empty`; a key used before: `revision_conflict` from the engine |
| `amend_intent` | `intent`; optional `outcome`, `first_user`, `why`, `not_included`, `constraints`, `questions`, `directory` | given members replace their values; `status` `draft`, `amended_at`, `confirmed_delegated` false, `confirmed_by` cleared | `intent_closed`; `outcome_empty` and `first_user_empty` (given and empty) |
| `note_intent` | `intent`, `notes` | `notes` only | `intent_closed` |
| `name_lead` | `intent`, `session`, `by` | `lead_session`, `lead_named_by` | `intent_closed`; `by_not_the_users_word` (not a current `order`, `decision` or `delegation`); `by_not_routed_here` (`by` is not routed to this intent) |
| `assume_intent` | `intent`, `reason` | `draft` becomes `assumed` with `assumed_reason` | `intent_not_draft`; `not_the_lead` (the actor's role is not `lead`); `reason_empty` |
| `confirm_intent` | `intent`, `by` | `draft` or `assumed` becomes `confirmed`: `confirmed_by`, `confirmed_at`, `confirmed_delegated` false | `intent_not_open`; `by_not_the_users_word` (not a current `order`, `decision` or `answer`); `by_not_routed_here`; `by_older_than_amendment` (`by` was not said later than `amended_at`); `no_criterion` (no criterion of the intent is in a state other than `waived`) |
| `confirm_intent_delegated` | `intent`, `delegation`, `reason` | `draft` or `assumed` becomes `confirmed` by the delegation, `confirmed_delegated` true; the reason stays in the action log | `intent_not_open`; `not_the_lead`; `not_a_delegation`; `reason_empty`; `no_criterion` |
| `close_intent` | `intent`, `reason` | `confirmed` becomes `met`, `closed_at`, `closed_reason` | `intent_not_confirmed`; `criterion_not_passed` (a criterion is `open` or `fail`) |
| `retire_intent` | `intent`, `reason` | any status but `met` and `retired` becomes `retired` | `intent_closed`; `reason_empty` |

A delegation may confirm several intents, so it need not be routed to each. Once it confirmed one, it can no
longer be routed, set aside or superseded (`request_confirms_an_intent`): route it or set it aside before its
first use. `end_delegation` is how the user ends it.

### `criterion`: one success criterion of an intent
| Property | Type | Meaning |
| --- | --- | --- |
| `intent` | `intent` | |
| `statement` | string, 600 | checkable true or false |
| `check` | string, 800 | the command, test, eval with its threshold, or the real run to observe |
| `baseline` | string, 300, optional | for a number: its value before, with source and time |
| `state` | `open`, `pass`, `fail`, `waived` | |
| `evidence` (1,000), `checked_at` | optional | the output of the last check, or the reason of the waiver |
| `waived_by` | `request`, optional | the user's word that waived it |
| `added_at` | UTC time | |

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `add_criterion` | `intent`, `statement`, `check`; optional `baseline` | an `open` criterion | `intent_closed`; `statement_empty`; `check_empty` |
| `check_criterion` | `criterion`, `state`, `evidence` | `state` (`pass` or `fail`), `evidence`, `checked_at` | `state_not_a_result` (`open` or `waived`); `evidence_empty`; `criterion_waived` |
| `waive_criterion` | `criterion`, `by`, `reason` | `waived`, `waived_by`, the reason as `evidence` | `by_not_the_users_word` (not a current `decision`, `order` or `answer`); `by_not_routed_here` (not routed to the criterion's intent); `reason_empty` |

### Links, grants and views
Links, each named forward and reverse: `request_intent` / `intent_requests`, `intent_origin` /
`request_started_intents`, `criterion_intent` / `intent_criteria`, `request_supersedes` /
`request_successors`, `request_legacy_decision` / `decision_carried_requests`, `intent_confirmed_by` /
`request_confirmed_intents`, `intent_lead_named_by` / `request_named_leads`, `criterion_waived_by` /
`request_waived_criteria`. The refusals `request_confirms_an_intent`, `origin_confirms_an_intent`,
`legacy_already_carried`, `no_criterion` and `criterion_not_passed` read them in reverse.

Views: `requests_unrouted` (current requests not routed yet, oldest first), `requests_by_topic` (argument
`topic`: its current requests, newest first, with their route), `intents_open` (`draft`, `assumed` or
`confirmed`, by key), `criteria_of_intent` (argument `intent`, the intent's key).

### What the store does not check
- At most five questions per intent: an array has no length bound in the schema language. `ap` holds it.
- The form of an intent's key (lower-case letters, digits and `-`, at most 60 bytes): the engine checks
  only that a native key is 1 to 512 bytes. `ap` holds it.
- That a request's words are the user's: the engine does not know who calls. `words_verified` says whether
  a program read them from a session record.

### The migration
`ontology/migrate <store> <before> <after>` is the script delivery runs after it installed a new version.
It runs `$ONT_BIN` (default `ont`) with `--store`, every `action apply` with `--client migrate` and without
`ONT_AGENT` and `ONT_ROLE`, so that the action id of a request id is the same whoever runs it.

The step of version 10 carries every current decision of the user with nonempty `user_words` into a
request: it searches decisions by `status` `current` and `decided_by` `user`, following `continuation` and
requiring `basis.qualified`, reads each row with `get` (a search answer carries no property), and stops
with a non-zero exit, having applied nothing, unless the rows it read equal the `aggregate` count of the same
filter. For each row with words: `record_request` (`words` the user's words, `words_verified` false,
`reading` the decision's `text`, `kind` `decision`, its `topic`, `said_at` its `decided_at`,
`legacy_decision` the row; request id `migrate-10-<key of the row>`), then `set_request_aside` with the
reason `carried from version 9`, followed by `, triage <word>` when the row was triaged (request id
`migrate-10-aside-<key>`). The carried requests are current, found by topic, and not in
`requests_unrouted`: the triage of version 9 pointed at objectives and items, which have no counterpart here.
A record that answers `already_applied` is still followed by its aside, so a run that was interrupted
between the two completes it. A row is already done when both answered `already_applied`, or when the
record ended `legacy_already_carried`. The script changes no decision, prints one line
`{"carried": n, "already": m}` and exits 0 only when every row ended applied or already done.

The contract every later version keeps: the script stays in the tree; it names the highest version it knows
and holds one step per version (a version without work is an empty step); for a pair it runs the steps of
every version above `before` up to `after`; on a pair it does not know (`after` above the highest version, or
`before` not below `after`) it changes nothing, prints `{"status":"unknown_pair"}` and exits 2; and it
carries no row twice: a second run with the same arguments changes nothing but for rows the old actions
added in between. The file is committed executable (mode `100755`): delivery skips one that is not.

### Install over version 9 and the rehearsal
The rehearsal now installs version 9 from Git by its commit and digest, and then the repository's file
(as a copy that differs only in the paths of `installed_binaries`) over it; every object and view asserted
under version 9 reads the same, and `ont_describe` and `ont_action_list` stay within the MCP limit of
262,144 bytes. On a second store it starts at version 2 with a row of the user without words, installs the
pinned versions up to 9, records two rows of the user with words (one triaged) and a Lead row, installs the
repository's file, and runs `ontology/migrate`: 9 10 twice (2 carried, then 2 already), again after a record
applied by hand without its aside (1 carried, 2 already), three unknown pairs (exit 2, the store unchanged),
and the pair the repository's file names. Then every action of this section is applied and every refusal
above is asserted, each before the action it guards. The rows that need the role of the actor
(`assume_intent`, `confirm_intent_delegated`, `end_delegation`, and the read back with `ont log`) run with an
engine that has the verb `log`; with another, they are skipped with one line on standard error.
