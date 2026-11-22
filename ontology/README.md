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
- any directory whose name begins with `edu-` (the Academy's entries, read only by the Academy's own task) at
  depth 1, 2 and 3: `/home/palantirkc/work/edu-*`, `/home/palantirkc/work/*/edu-*` and
  `/home/palantirkc/work/*/*/edu-*`, in the lists of `task_states`, `packets` and `reports` (version 16).
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
root), checks the frozen v1 pin, installs v1, indexes the task, packet and report fixture files
(below), installs v2, creates and lifts a v2 hold, then installs v4 and verifies the task
and v2 hold revisions are unchanged. It exercises the version 2 action/time,
retry, reference, escaping and optional-cell cases; prints six rendered blocks
(one memory-index block and five program-state blocks); and invokes the
exhaustive memory-index walk with two records. Its private stores are removed
on exit. Output sizes and time-comparison answers go to stderr. A walk failure
fails the rehearsal. Native calls must use the authorized disposable wrapper.

The index sources of `ontology/work.schema.json` name the work directory of the host, and the rehearsal reads
no file there. The directory `ontology/fixtures/work/` holds one-line files (`# <an invented title>`) at the
paths the rehearsal reads or asserts, relative to the work root: two `STATE.md`, one `TASK-O13.md` and one
`REPORT-O13.md`; no other file may lie there, because the rehearsal asserts other keys absent. The rehearsal
copies them to `work/` under its own temporary root (taken through `realpath`, as the engine refuses a root
reached through a symbolic link) before the first index, and installs every schema file of its first two stores
(each pinned version read from Git, after the assertion of its digest, and the repository's file) as a copy in
which every string that begins with `/home/palantirkc/work/` in two groups begins with that root instead: the
paths of `sources` (`glob`, `exclude`) and the arguments of the `transform` of a `primary_key` or of a property
of an object type (`replace_prefix`, `segment_after`), which form the keys of packets and reports. Descriptions
and the paths of other directories are not touched, and a parameter that only holds a path as text stays as it
is. For each copy the rehearsal asserts that replacing the root back gives the original file, but for the
`paths` of `installed_binaries` where the install rows need them moved (below), and that the count of strings
changed equals the count under the root in the copy; it prints `redirect_copy=` with the two counts of each
copy (127 source paths and 5 transform arguments in every version so far), and after the last install of those
two stores `rehearsal_sources=<n> redirected, 0 under the host's work directory`, `<n>` being the strings of both
groups over all copies. A string of either group left under the host's work directory fails the rehearsal. This
proves that the objects indexed from the fixtures have the keys they have on the host, and that the calls of the
engine name no path of the host's work directory (`strace` of the run shows none beyond the repository
checkout). The store of `ontology/memory-index-walk.py` installs the repository's file unchanged and indexes
nothing: it is outside the redirect and the trace. The rehearsal does not index the `instructions` source; the paths of
`installed_binaries` are moved to files of its own directory by the same copy. The schema file itself keeps the
host's paths in its sources.

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
and provenance. No real record
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
install rows run on the copy of this file that also moves those paths (two
files of the rehearsal's own directory), the one copy of "Install and rehearse"; the rehearsal
asserts that every other member is equal to the file's, whose version it keeps (above the installed 9). It prints the basis of the search of
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
between the two completes it. A row is already done when both answered `already_applied`, when the
record ended `legacy_already_carried`, or (since version 11) when the record answered `already_applied` and
the aside was refused `request_id_reused`: the row was triaged with the old verb after an earlier run carried
it and set it aside, so the reason differs under the same request id. The script changes no decision, prints one line
`{"carried": n, "already": m}` and exits 0 only when every row ended applied or already done.

The contract every later version keeps: the script stays in the tree; it names the highest version it knows
and holds one step per version (a version without work is an empty step); for a pair it runs the steps of
every version above `before` up to `after`; on a pair it does not know (`after` above the highest version, or
`before` not below `after`) it changes nothing, prints `{"status":"unknown_pair"}` and exits 2; and it
carries no row twice: a second run with the same arguments changes nothing but for rows the old actions
added in between. The file is committed executable (mode `100755`): delivery skips one that is not.

### Install over version 9 and the rehearsal
The rehearsal now installs version 9 from Git by its commit and digest, and then the repository's file
(as the copy of "Install and rehearse", which also moves the paths of `installed_binaries`) over it; every object and view asserted
under version 9 reads the same, and `ont_describe` and `ont_action_list` stay within the MCP limit of
262,144 bytes. On a second store it starts at version 2 with a row of the user without words, installs the
pinned versions up to 9, records two rows of the user with words (one triaged) and a Lead row, installs the
repository's file, and runs `ontology/migrate`: 9 10 twice (2 carried, then 2 already), again after a record
applied by hand without its aside (1 carried, 2 already), three unknown pairs (exit 2, the store unchanged),
and the pair the repository's file names. Then every action of this section is applied and every refusal
above is asserted, each before the action it guards. The rows that need the role of the actor
(`assume_intent`, `confirm_intent_delegated`, `end_delegation`, and the read back with `ont log`) run with the
engine of `ontology/ENGINE-PIN`, which knows the role and has the verb `log`. Since version 11 the second store
installs version 10 from Git instead of the repository's file (section "Version 11"), runs the migration
again after a carried row was triaged with the old verb (0 carried, 3 already), and names the pairs it uses
by number.

## Version 11: units, runs, findings
Version 10 remains pinned at
`e8e0286d81a5020aed14d9232156cc8c1de0a5e4:ontology/work.schema.json`,
sha256 `d10e4f5dec9aadcf3dbc9d4be63655196743c09e17adf4e3b3041d3d847af388`.
Version 11 only adds to versions 9 and 10. Every value type, source, object type, link type, action type and
view reads the same in canonical JSON, except: `review_kind` gains `page` and `plan`; `request_route` gains
`unit` and `finding`; `review`, `hold` and `request` gain optional properties; `close_review` and
`close_intent` gain one criterion each; `route_request` and `set_request_aside` also clear the new references
`unit` and `finding`; the one grant gains the three new types (read and derive) and the new actions.

The new types are the agents' side of the life cycle of work:

1. A `unit` is one piece of work that ends alone (for code: one pull request). Its page is its properties.
   It is added `draft` in a lane: `small` (one request of the user, one repository, class `plain`) or
   `objective` (part of an open intent). It becomes `ready`, then `running` under the order the criteria
   hold (reviews of the plan and of the page, dependencies merged, no active hold), is `accepted` on a head a
   review accepted, and ends `merged` (with a repository) or `done` (without one), or `dropped`.
2. A `run` is one stage of one agent: who ran, on what model, how long, how many tokens.
3. A `finding` is something that should change the system: it is opened, seen again, planned into a unit or
   an intent, dismissed or closed.
4. A review of a page (kind `page`), of a plan (kind `plan`) and of a unit's head (kind `packet`) is the
   `review` of version 7; a hold on a unit or an intent is the `hold` of version 2.

All three new types are Ontology-owned and written only by these actions. `unit` and `finding` are keyed by
the native key of a required reference parameter (`unit` of `add_unit`, `finding` of `open_finding`), as
`intent` is; they have no `title` and are shown by that key. `run` is keyed by the action id, with the title
`role`. A time is a stored UTC time of one-second resolution; "later" is strictly later. A scalar cannot be
cleared, so a property said to be "valid only while" a state keeps its last value as history after it.

### `unit`: one piece of work that ends alone
The page, set by `add_unit` and changed only by `amend_unit`:

| Property | Type | Meaning |
| --- | --- | --- |
| `lane` | `small`, `objective` | never changes |
| `class` | `plain`, `contract`, `effect` | `contract` and `effect` need a page review before `ready` and the Lead's approval of the head before the end |
| `intent` | `intent`, optional | required in the lane `objective`; never changes |
| `origin` | `request`, optional | required in the lane `small`; never changes |
| `objective` | string, 2,000 | the state after the merge, as sentences that can be checked |
| `interface` (20,000), `invariants` (3,000), `not_built` (1,500), `notes` (3,000), `verifier_questions` (4,000), `report` (1,000) | string, optional | what the Lead fixes; what must stay true; what is not built; notes for the worker; the verifier's questions; what the worker's report holds |
| `acceptance` | array of `{check, kind}`, optional | `check` nonempty; `kind` `fails_on_base` or `invariant` |
| `write_set`, `read_first`, `specs` | array of string, optional | paths the unit may change; files read first; design sources |
| `repository` (200), `branch` (200) | string, optional | `owner/name`; a unit without one is a document, a measurement or an effect on the host |
| `implementer`, `verifier` | `{runtime, model, effort}`, optional | the tiers |
| `estimate_lines` | integer, optional | |
| `depends_on` | array of `unit`, optional | units that must be `merged` or `done` before this one starts |

The state, set only by the actions below:

| Property | Type | Meaning |
| --- | --- | --- |
| `state` | `draft`, `ready`, `running`, `accepted`, `stopped`, `merged`, `done`, `dropped` | |
| `added_at`, `amended_at` | UTC time | `amended_at` is the time of `add_unit` or of the last `amend_unit`: a review counts only when it STARTED later |
| `ready_at`, `started_at`, `ended_at` | UTC time, optional | `ended_at`: the stop, the merge, the close or the drop |
| `pull_request` | integer, optional | |
| `head`, `approved_head`, `merged_commit` | commit or digest, optional | `head` is a commit, or for a unit without a repository the SHA-256 of its evidence; `approved_head` is valid only while it equals `head` |
| `rounds` | integer | 0 from `add_unit`; the caller of `record_head` passes the new number |
| `writers` | array of string | the sessions that wrote to the unit's branch, empty from `add_unit` on; none of them closes a review of the unit |
| `stop_cause` (100), `stop_detail` (1,000) | string, optional | valid only while `state` is `stopped` |
| `waits_on` | string, 300, optional | the constraint a ready unit waits on, as the dispatch pass last saw it |
| `evidence` | string, 1,500, optional | what shows a unit without a pull request is done |
| `delivered_lines` | integer, optional | |
| `drop_reason` | string, 600, optional | |

The parameter `intent` of `ready_unit`, `start_unit`, `resume_unit`, `reopen_unit`, `merge_unit` and
`close_unit` is the unit's own intent, given again because a criterion reads an intent's status and its
reviews and holds only from a parameter. In the lane `objective` it is required (`intent_parameter_required`)
and must be the unit's (`intent_differs`); in the lane `small` it is left out. "Held" means an `active` hold
names the unit, or names the given intent.

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `add_unit` | `unit`, `lane`, `class`, `objective`; optional every other page property | a `draft` unit, `rounds` 0, `writers` empty, `added_at` and `amended_at` | lane `objective`: `intent_required`; `intent_not_open_for_units` (the intent is not `confirmed` or `assumed`). Lane `small`: `origin_required`; `origin_not_current`; `small_must_be_plain`; `small_needs_one_repository_and_a_write_set` (no or an empty `repository`, no or an empty `write_set`); `request_has_a_live_unit` (another unit of this origin is not `merged`, `done` or `dropped`). Both: `objective_empty`; `depends_on_itself`; a key used before: `revision_conflict` from the engine |
| `amend_unit` | `unit`; optional every page property but `lane`, `intent` and `origin` | given members replace their values; `state` `draft`, `amended_at` | `unit_not_amendable` (not `draft`, `ready` or `stopped`); `objective_empty` (given and empty); `small_must_be_plain` (lane `small` and another class given); `depends_on_itself`; a member `lane`, `intent` or `origin`: `invalid_parameter` from the engine |
| `ready_unit` | `unit`; optional `intent` | `draft` becomes `ready`, `ready_at` | `unit_not_draft`; `write_set_missing` (absent or empty); `acceptance_missing` (absent or empty); `tiers_missing` (no `implementer` or no `verifier`); `dependency_dropped`; `intent_not_open_for_units`; `intent_differs`; `intent_parameter_required`; `plan_not_reviewed` (lane `objective`: no review of kind `plan` of the intent is `closed` with verdict `accept`); `page_not_reviewed` (class `contract` or `effect`: no review of kind `page` of the unit is `closed` with verdict `accept` and STARTED later than `amended_at`) |
| `start_unit` | `unit`; optional `intent` | `ready` becomes `running`, `started_at` | `unit_not_ready`; `intent_differs`; `intent_parameter_required`; `intent_not_open_for_units`; `dependency_not_merged` (a unit of `depends_on` is not `merged` or `done`); `held`; `intent_not_confirmed` (class `contract` or `effect` and the intent is not `confirmed`) |
| `record_head` | `unit`, `head`, `writers`; optional `pull_request`, `rounds`, `delivered_lines` | sets them; `writers` is the whole new list | `unit_not_running` (not `running` or `stopped`) |
| `stop_unit` | `unit`, `cause`; optional `detail` | `running` becomes `stopped`: `stop_cause`, `stop_detail`, `ended_at` | `unit_not_running`; `cause_empty` |
| `resume_unit` | `unit`; optional `intent` | `stopped` becomes `running` on the same page (an amendment goes through `ready_unit` and `start_unit`) | `unit_not_stopped`; `intent_differs`; `intent_parameter_required`; `held`; `intent_not_confirmed` |
| `accept_unit` | `unit`, `head` | `running` becomes `accepted` | `unit_not_running`; `head_differs` (not the unit's `head`); `head_not_reviewed` (no review of kind `packet` of the unit and of that head is `closed` with verdict `accept` and STARTED later than `amended_at`) |
| `approve_unit` | `unit`, `head`, `reason` | `approved_head`; the reason stays in the action log | `unit_not_accepted`; `not_the_lead` (the actor's role is not `lead`); `head_differs`; `reason_empty` |
| `merge_unit` | `unit`, `head`, `merged_commit`; optional `intent` | `accepted` becomes `merged`: `merged_commit`, `ended_at` | `unit_not_accepted`; `head_differs`; `head_not_reviewed`; `intent_differs`; `intent_parameter_required`; `held`; `not_approved` (class `contract` or `effect`, and `approved_head` is absent or not `head`); `no_repository` (it ends by `close_unit`) |
| `close_unit` | `unit`, `evidence`; optional `intent` | a unit without a repository: `accepted` becomes `done`: `evidence`, `ended_at` | `unit_has_a_repository`; `unit_not_accepted`; `evidence_empty`; `head_not_reviewed` (read from the unit's own `head`, so a revoked review stops it); `intent_differs`; `intent_parameter_required`; `held`; `not_approved` |
| `drop_unit` | `unit`, `reason` | any state but `merged`, `done` and `dropped` becomes `dropped`: `drop_reason`, `ended_at` | `unit_ended`; `reason_empty` |
| `note_unit_wait` | `unit`, `waits_on` | `waits_on` | `unit_not_ready` |
| `reopen_unit` | `unit`, `cause`; optional `intent` | `accepted` becomes `running`; the cause stays in the action log | `unit_not_accepted`; `cause_empty`; `intent_differs`; `intent_parameter_required`; `held`; `intent_not_confirmed` |

`reopen_unit` is the way back when `main` moved under an accepted head, the merge on the hosting service
failed, the Lead rejected the diff or the accepting review was revoked. The come-back is `record_head` of the
new head, `carry_review` or a verifier's round, `accept_unit`, and for the classes `contract` and `effect` a new
`approve_unit`. A unit that is `accepted` under an active hold stays so until the hold is lifted or the unit is
dropped: `reopen_unit` is held too.

Held by `ap` and not by this schema, each by the command named:
- every unit of `depends_on` exists (`ap unit add`, `ap unit amend`): a reference to a unit that does not exist
  is no candidate of `linked`, so the engine would let such a unit start;
- `record_head` is given every writer (`ap unit run`);
- the `evidence` given to `close_unit` is the text whose SHA-256 is the unit's `head` (`ap unit run`);
- `merge_unit` and `close_unit` are validated (`ont action validate`) before the merge on the hosting service
  or the effect, so that a hold stops the merge itself and not only its record (`ap unit run`): applied after
  it, a refusal leaves the unit `accepted` until its cause is gone;
- each `fails_on_base` check fails on the base (`ap unit run`);
- the write set and the protected paths against the diff (`ap unit run`);
- the required check on the hosting service (`ap unit run`);
- no overlap of write sets between running units (`ap unit run`);
- the host's limits (`ap unit run`);
- the patch of a unit is unchanged between the two heads of a `carry_review` (`ap unit run`);
- a `lane` `small` unit keeps a nonempty `repository` after an amendment (`ap unit amend`; the store holds the
  write set again at `ready_unit`).

### Reviews of units, pages and plans
`review` gains the optional properties `unit` (`unit`), `intent` (`intent`), `round` (integer) and
`carried_from` (`review`). Its property `task` stays required and is given by the caller: which task a command
gives is that command's rule, not this schema's. `head` stays required too: for the kind `page` it is the
SHA-256 of the rendered page, for the kind `plan` the SHA-256 of the plan as the command renders it. Delivery
(`accepted` in `crates/ap/src/deliver.rs`) reads a review by `kind`, `head` and `status`, then `verdict`,
`repository`, `report`, `report_sha256` and `closed_at`: a review made by the new actions has each of them.

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `close_review` (changed) | as before | as before | as before, and `review_names_a_unit` (the review has a `unit`: close it with `close_unit_review`) |
| `start_unit_review` | the parameters of `start_review`; optional `unit`, `intent`, `round` | as `start_review`, with the three members | `unknown_task`; `duplicate_running_review`; `subject_required` (kind `page` without `unit`, kind `plan` without `intent`); `unknown_unit`; `unknown_intent` |
| `close_unit_review` | the parameters of `close_review`; optional `unit` | as `close_review` | `review_not_running`; `report_required`; `unit_differs` (the review names a unit and the parameter is absent or another); `session_required` (a review of a unit names the closing session); `writer_cannot_review` (the session is one of the unit's `writers`) |
| `carry_review` | `review`, `unit`, `head` | a `closed` review of kind `packet` of the new `head`, verdict `accept`, `started_at` and `closed_at` now, `carried_from` the given one, and its `task`, `model`, `effort`, `prompt`, `prompt_sha256`, `repository`, `unit`, `report`, `report_sha256` | `review_not_accepting` (not `closed`, verdict `accept`, kind `packet`); `review_incomplete` (no `repository`, `unit`, `report` or `report_sha256`); `review_older_than_amendment` (it did not start later than the unit's `amended_at`); `unit_differs`; `head_unchanged` |

A review closed before this version, and one made by `start_review` and `close_review` after it, is read as
before. The other members of a carried review (`pull_request`, `base`, `runs`, `account`, `session`) are read
through `carried_from`: a rule cannot copy a member its source lacks.

### `run`: one stage of one agent
| Property | Type | Meaning |
| --- | --- | --- |
| `unit`, `intent` | `unit`, `intent`; optional | what the run worked on |
| `role` (40), `runtime` (40), `model` (80) | string | |
| `effort` (20), `account` (200), `session` (200) | string, optional | |
| `status` | `running`, `closed` | |
| `state` (40), `served_by` (80) | string, optional | the end state the run record names; the model that served, when it differs |
| `started_at`, `ended_at` | UTC time; `ended_at` optional | |
| `input_tokens`, `cache_read_tokens`, `cache_write_tokens`, `output_tokens` | integer, optional | the four counts of a run record, kept apart because the price table prices them apart |
| `limit` | string, 300, optional | the limit reading as the run record holds it |
| `record`, `stream` | file path, optional | the run record; the raw stream |
| `running_in` | `unit`, optional | set while `running`, cleared at the close, as `review.running_in` is |

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `start_run` | `role`, `runtime`, `model`; optional `unit`, `intent`, `effort`, `account`, `session`, `record`, `stream` | a `running` run, `started_at`, `running_in` the unit | `unknown_unit`; `unknown_intent` |
| `close_run` | `run`, `state`; optional `session`, `served_by`, the four token counts, `limit`, `record`, `stream` | `closed`, `ended_at`; `running_in` cleared | `run_not_running` |

### `finding`: something that should change the system
| Property | Type | Meaning |
| --- | --- | --- |
| `kind` | `incident`, `lesson`, `drift`, `security`, `note` | |
| `detector` (60), `summary` (600) | string | what found it; what it is |
| `evidence` | string, 2,000, optional | the last evidence |
| `severity` | `info`, `act`, `urgent` | |
| `state` | `open`, `planned`, `dismissed`, `closed` | |
| `first_seen_at`, `last_seen_at` | UTC time | |
| `unit`, `intent` | `unit`, `intent`; optional | what it was planned into |
| `reason` (600), `ended_at` | optional | of a dismissal or a close |

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `open_finding` | `finding`, `kind`, `detector`, `summary`, `severity`; optional `evidence` | an `open` finding, both times now | a key used before: `revision_conflict` from the engine, after which the caller applies `see_finding_again` or `reopen_finding` (the command forms the key from the detector and a signature) |
| `see_finding_again` | `finding`; optional `evidence`, `severity` | `last_seen_at` | `finding_not_live` (not `open` or `planned`) |
| `reopen_finding` | `finding`, `evidence` | `dismissed` or `closed` becomes `open` | `finding_live` |
| `plan_finding_as_unit` | `finding`, `unit` | `planned`, `unit`; `intent` cleared | `finding_not_open`; `unknown_unit` |
| `plan_finding_as_intent` | `finding`, `intent` | `planned`, `intent`; `unit` cleared | `finding_not_open`; `unknown_intent` |
| `dismiss_finding`, `close_finding` | `finding`, `reason` | `dismissed` or `closed`, `reason`, `ended_at` | `finding_not_live`; `reason_empty` |

### Holds on a unit or an intent, and version 10's actions
`hold` gains the optional properties `unit` (`unit`), `intent` (`intent`) and `scope` (string, 200).
`set_hold` and `lift_hold` do not change. `set_hold_on` takes the parameters of `set_hold` (its `task` is given
by the caller) and optionally `unit`, `intent`, `scope`; it is refused as `set_hold` is (`unknown_task`,
`duplicate_active_hold`), and `unknown_unit`, `unknown_intent`.

`request` gains the optional references `unit` and `finding`; `route_request` and `set_request_aside` also
clear them.

| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `close_intent` (changed) | as before | as before | as before, and `unit_not_ended` (a unit of the intent is not `merged`, `done` or `dropped`) |
| `route_request_to_unit` | `request`, `unit` | `route` `unit`, `unit`, `routed_at`; `intent` and `finding` cleared | `request_not_current`; `request_confirms_an_intent`; `unknown_unit` |
| `route_request_to_finding` | `request`, `finding` | `route` `finding`, `finding`, `routed_at`; `intent` and `unit` cleared | `request_not_current`; `request_confirms_an_intent`; `unknown_finding` |

### Links, grants and views
Links, each named forward and reverse: `unit_intent` / `intent_units`, `unit_origin` / `request_units`,
`unit_depends_on` / `unit_dependents` (`many_to_many`: it is backed by an array), `review_unit` /
`unit_reviews`, `review_intent` / `intent_reviews`, `review_carried_from` / `review_carries`, `run_unit` /
`unit_runs`, `run_intent` / `intent_runs`, `run_running_in` / `unit_running_runs`, `finding_unit` /
`unit_findings`, `finding_intent` / `intent_findings`, `hold_unit` / `unit_holds`, `hold_intent` /
`intent_holds`, `request_unit` / `unit_requests`, `request_finding` / `finding_requests`.

Views: `units_of_intent` (argument `intent`, the intent's key), `units_live` (`ready`, `running`, `accepted` or
`stopped`), `runs_running`, `findings_open` (`open` or `planned`, newest `last_seen_at` first), `holds_on_work`
(active holds that name a unit or an intent), `reviews_of_unit` and `runs_of_unit` (argument `unit`, the unit's
key). A search of reviews by `unit` is not qualified on this engine (a filter on an optional property), so
the commands read a unit's reviews and runs through these two views.

### Size
Every new type, property, parameter and action carries a description, every criterion a message.
The engine bounds each description of a property or a parameter to 300 bytes; the MCP answers are bounded to
262,144 bytes, and the rehearsal asserts both `ont_describe` and `ont_action_list` stay at or under it on both
stores. Measured on this file at the engine of `ontology/ENGINE-PIN`: `ont_describe` 214,024 bytes (48,120
left), `ont_action_list` 140,647 bytes. These sizes were measured on the repository's file, with the paths of the host. The
rehearsal installs the redirected copy, whose `ont_describe` text is longer by the difference between the
length of its temporary root and that of the host's work directory, once for each of the 132 redirected strings
(with the default `/tmp`: 214,420 bytes on the second store, 396 more). The bytes that the rows `v9-mcp-sizes` and
`v10-mcp-sizes` and the line `v11_mcp_bytes=` print are those of the copy, and the bound of 262,144 bytes is
asserted on it. The texts that versions 10 and 11 add (the descriptions and messages
of their new types, properties, parameters, actions and criteria) are 564 texts of 38,285 bytes, a mean of 68
bytes; the bound this version keeps is a mean of at most 160. A later version that adds as much again does not
fit: it shortens texts or splits the domain.

### The migration and the rehearsal
`ontology/migrate` knows version 11; its step is empty. The step of version 10 also counts a row as already
done when its record answered `already_applied` and its aside was refused `request_id_reused`: the row was
triaged with the old verb after its carry, so the reason of a second aside differs under the same request id.

The second store of the rehearsal installs version 10 from Git by its commit and digest (above) and runs
the walk of version 10 on it; then it installs version 11 (since version 12 from Git by its commit and
digest, section "Version 12"): every object read under version 10 reads the same, `ontology/migrate` runs
10 11 (exit 0, the store unchanged) and 9 11 (the carry of version 10: every row already done), and
`confirm_intent`, `close_intent` and `route_request` act on objects made under version 10. Then the walk of version 11 applies every action of this section and asserts
the refusals above, each before the action it guards, and shows that a review made by the new actions (and
one made by `carry_review`) is found by the search delivery uses. `approve_unit` runs with the role `lead`;
with an engine without the verb `log` in `ont --help` (which does not know the role), `approve_unit` and every
later step on that unit are skipped, with one line on standard error that names them.

## Version 12: the page digest
Version 11 remains pinned at
`eb4963d5fa0ff697a9f6743df38a03bbd20b233d:ontology/work.schema.json`,
sha256 `62062852a76a173dc7a51625b1ccfdd806432d086a7350b9b3d2a12b6aeb1d34`.
Version 12 changes one action. Every value type, source, object type, link type, view and every other action
reads the same in canonical JSON as at version 11; `head_not_reviewed` of `accept_unit`, `merge_unit` and
`close_unit`, and `review_older_than_amendment` of `carry_review` (reviews of kind `packet`), are unchanged.

`ready_unit` takes one new optional parameter `page_digest` (value type `sha256_hex`: 64 lowercase hexadecimal
characters, another string is `invalid_parameter`). Its criterion `page_not_reviewed` holds for a unit of class
`contract` or `effect` only when a review of kind `page` linked by `unit_reviews` is `closed` with verdict
`accept` and its `head` equals `$page_digest`, whenever that review started: the clause that compared the
review's `started_at` with the unit's `amended_at` is removed from this criterion only. The `where` checks
`{"present": "$page_digest"}` before the comparison, so such a unit without `page_digest` is refused
`page_not_reviewed` (without the guard the engine answers `criterion_not_evaluable` for a unit that has an
accepted review and no `page_digest`). A unit of class `plain` needs no `page_digest`.

The digest is the one `ap unit page <key> --digest` prints (`specs/ap/unit.md` R34): the SHA-256 of the page
part as built for the reviewer without its line `Depends on: ...`. Neither the dependencies, the tiers
(`implementer`, `verifier`) nor `estimate_lines` enter it, so an amendment of only these after an accept keeps
the accept; an amendment of any other page property makes another digest and needs a new look. `ap unit ready`
gives `page_digest` for every unit, in any state and of any class, so that a unit that is not a draft is
refused `unit_not_draft` first (`page_not_reviewed` beside it when the digest has no accepted review).

### What the store does not check
- That `page_digest` is the digest of the unit as stored: the engine does not render a page. `ap` computes it
  from the same read it applies `expected` with; a caller that applies `ready_unit` by hand with another
  digest is answered by what reviews exist of that digest.
- A page review accepted before delivery for a unit with dependencies has the old digest (computed with the
  `Depends on:` line) and no longer counts: such a page is looked at again after delivery. A page review of a
  unit without dependencies keeps its digest.

### The migration and the rehearsal
`ontology/migrate` knows version 12; its step is empty (no data migration). `ap deliver` installs this file by
its existing schema step, and runs that step before it installs `ap`: between the two installs the old `ap`
sends `ready_unit` without `page_digest`, and a `contract` or `effect` unit is refused `page_not_reviewed` (it
fails safe). An `ap` rolled back below this change cannot make such a unit ready while the store is at version
12; a plain unit is not affected.

The second store of the rehearsal installs version 11 from Git by its commit and digest (above) instead of the
repository's file, and runs the walk of version 11 on it. Then it adds, still under version 11, a unit of class
`contract` with a dependency and a page review accepted after its last amendment, and installs the repository's
file over the objects made under version 11: every object read under version 11 (tasks, decisions, requests,
intents, criteria, units, reviews, runs, findings, holds) and ten views read the same; the schema differs from
version 11 only in `ready_unit` and the version; `ont_describe` and `ont_action_list` stay within 262,144 bytes
(the line `v12_mcp_bytes=`); `ontology/migrate` runs 11 12 (exit 0, the store unchanged), 9 12 (every row
already done) and 12 13 (exit 2), and the pairs refused on the version 10 part are (9, 13), (10, 10) and
(10, 9). The walk of version 12, each refusal before the action it guards: `ready_unit` without `page_digest`
on that unit, which has an accepted page review, refused `page_not_reviewed`; with a `page_digest` no accepted
review of the unit has, refused `page_not_reviewed`; with a digest in capitals, `invalid_parameter`; then
`amend_unit` changing only `depends_on`, `estimate_lines` and `implementer` (later than the review's start),
and `ready_unit` with the same `page_digest`, applied; again, `unit_not_draft`, and with another digest
`unit_not_draft` and `page_not_reviewed`; `start_unit` of it. On a unit of class `effect` made under version
12, a running page review and one closed `repair` of the digest given do not count, an accepting one does; a
plain unit becomes ready without `page_digest`.

Version 12 stays pinned as stated at the head of section "Version 13"; the migration and the rehearsal as they stand now are described in section "Version 13", "The migration and the rehearsal".

## Version 13: principles
Version 12 remains pinned at
`60b52b1ccc99f50ac04fba7f3c472d5dc16261f9:ontology/work.schema.json`,
sha256 `2310273cccfceae52d352ba112340ef695b5f959a48b89b6b2453173c7cf4ee1`.
Version 13 only adds to version 12. Every value type, source, object type, link type, action type and view reads
the same in canonical JSON, except that `intent` gains one optional property, `principles`; the one grant gains
the type `principle` (in `read` and in `derive`, as the other types of the domain) and the four new actions.

A `principle` is a want of the user that outlives one intent: a pillar, or a want under a pillar. It is
Ontology-owned and written only by the actions below. It is keyed by the native key of the required reference
parameter `principle` of `record_principle` (lower-case letters, digits and `-`, at most 60 bytes), as `intent` is
keyed by the `intent` of `draft_intent`; it has no `title` and is shown by that key. A used key is refused by the
engine (`revision_conflict`).

| Property | Type | Meaning |
| --- | --- | --- |
| `order_key` | string, 1 to 40 bytes | the place in the hierarchy: two-digit segments joined by `.`, such as `01` for a pillar and `01.02` for a want under it |
| `statement` | string, 1,000 | the want |
| `parent` | `principle`, optional | the pillar or want it is under; none for a pillar |
| `requests` | array of `request` | the requests that state it, at least one |
| `status` | `current`, `retired` (value type `principle_status`) | |
| `recorded_at`, `amended_at` | UTC time | `amended_at` is the time of `record_principle` or of the last `amend_principle` |
| `retired_at`, `retired_reason` (600) | UTC time, string, optional | set by `retire_principle` |

`intent.principles` (array of `principle`, optional) names the principles an intent obeys.

### Links
| Link | From | To | Cardinality | Reverse |
| --- | --- | --- | --- | --- |
| `principle_parent` | `principle.parent` | `principle` | `many_to_one` | `principle_children` |
| `principle_requests` | `principle.requests` | `request` | `many_to_many` | `request_principles` |
| `intent_principles` | `intent.principles` | `principle` | `many_to_many` | `principle_intents` |

`ont around` walks them: from a principle by `principle_requests` to the requests that state it, from an intent by
`intent_principles` to the principles set on it.

### Actions
| Action | Parameters | Effect | Refusals |
| --- | --- | --- | --- |
| `record_principle` | `principle`, `order_key`, `statement`, `requests`; optional `parent` | a `current` principle, `recorded_at` and `amended_at` the time of the action | `statement_empty`; `order_key_empty`; `parent_not_current` (a `parent` given that is absent or retired); `requests_empty`; a key used before: `revision_conflict` from the engine |
| `amend_principle` | `principle`; optional `order_key`, `statement`, `requests`, `parent` | the members given replace their values; `amended_at` | `principle_not_current` (absent or retired); `statement_empty` and `requests_empty` (given and empty); `parent_not_current` |
| `retire_principle` | `principle`, `reason` (600) | `status` `retired`, `retired_at`, `retired_reason` | `principle_not_current` |
| `set_intent_principles` | `intent`, `principles` | the intent's `principles`, given whole; replaces the list | `intent_not_open` (absent, or `met` or `retired`) |

`requests_empty` is a criterion `{"ne": ["$requests", []]}`, which the engine at the pin evaluates on an array
parameter, so it has its rehearsal rows. `order_key_empty` of `amend_principle` is not a criterion: an `order_key`
given and empty is not refused (see below).

### The view
`principles` (no arguments) lists the principles with `status` `current`, in `order_key` ascending order, at most
1,000 rows, with the columns `Order` (`order_key`), `Key`, `Parent`, `Statement` and `Requests`. A pillar has no
parent, so its `Parent` cell is the engine's text for an absent reference, `(missing)`. The engine renders flat
tables: there is no tree and no recursion, and `order_key` gives the order, so a Lead reads the hierarchy off the
keys.

### The grant
The grant of the domain reads `principle` and derives it as the other types are, and lists the four actions.

### What the store does not check
- That `order_key` has the form of two-digit segments joined by `.`: the engine orders it as a string and does not
  check the form: `10` sorts before `9`, so a caller writes two digits. `amend_principle` has no criterion on an
  `order_key` given as the empty string (it refuses an empty `statement` and an empty `requests` given;
  `record_principle` refuses an empty `order_key`).
- That a principle's `order_key` begins with its parent's, and that the parents do not form a cycle: an amendment
  can name a principle under itself or under one of its descendants.
- That a parent stays current: retiring a principle leaves its children current, under a retired parent.
- That a request listed in `requests` is current, or that its words say what the statement says: only that the
  listed objects are references.
- A `parent` cannot be cleared by an amendment (an ordinary scalar or a reference has no clear by value).
- That a principle named by `set_intent_principles` exists, is current, or is listed once: it stores a principle
  that does not exist, a retired principle and an empty list, as given (its only refusal is `intent_not_open`).
- That an intent drops a principle when it is retired: the intent keeps it in `principles`, `ont around` still
  reaches it by `intent_principles`, and the view `principles` no longer shows its row.

### The migration and the rehearsal
`ontology/migrate` knows version 13; its step is empty (no data migration), so a store at version 12 carries on
with `ontology/migrate` 12 13 changing nothing. The line `HIGHEST = 13` names it; `13 14` and `9 14` are pairs it
does not know (exit 2). `12 13`, which `ontology/migrate` refused (exit 2) while its highest version was 12, is
now a known pair; the pairs refused on the version 10 part of the rehearsal are (9, 14), (10, 10) and (10, 9)
(they were (9, 13), (10, 10) and (10, 9) at version 12).

The second store of the rehearsal installs version 12 from Git by its commit and digest (above) instead of the
repository's file, and runs the walk of version 12 on it. Then it installs the repository's file over the objects
made under version 12: every object read under version 12 (tasks, decisions, requests, intents, criteria, units,
reviews, runs, findings, holds) and ten views read the same; the schema differs from version 12 only by the new
type, links, actions, view, value type and grant, and the property `intent.principles`; `ont_describe` and
`ont_action_list` stay within 262,144 bytes (the line `v13_mcp_bytes=`); `ontology/migrate` runs 12 13 and 11 13
(exit 0, the store unchanged), 9 13 (every row already done) and 13 14 (exit 2). The walk of version 13, each
refusal before the action it guards: two requests are recorded; `record_principle` is refused `statement_empty`,
`order_key_empty`, `requests_empty` and `parent_not_current` (an absent parent, and the principle's own key), a
pillar and a want under it are recorded, and a used key is refused `revision_conflict`; the view `principles`
renders the two rows in `order_key` order with the pillar's key in the child's `Parent` cell and `(missing)` in
the pillar's; `ont around` from the child by `principle_requests` reaches its request, from the request by
`request_principles`, from the child by `principle_parent` and from the pillar by `principle_children` reach their
ends; `set_intent_principles` is refused `intent_not_open` for an absent intent and a met one, and applied on an
open one, from which `ont around` by `intent_principles` reaches the principles set on it, and from a principle by
`principle_intents` reaches the intent; `amend_principle` is refused `principle_not_current`, `statement_empty`,
`requests_empty` and `parent_not_current`, and applied with all members and with one; `retire_principle` is
applied, then refused `principle_not_current` as an amendment and a second retirement are, and a retired
principle as a `parent` is refused `parent_not_current`; the view then holds the pillar only. The last line of
standard output of the rehearsal's walk of version 13 is `v13 principles: ok`.

Version 13 stays pinned as stated at the head of section "Version 14: routes"; the migration and the rehearsal as they stand now are described in section "Version 14: routes", "The migration and the rehearsal".

## Version 14: routes
Version 13 remains pinned at
`4338bf4b9369c55dd0180f91ca39f430fac178d6:ontology/work.schema.json`,
sha256 `fdbb0f838035b28c41b81cc08ddacd6ea2e69480156dbe58fafa9409792b825c`.
Version 14 only adds to version 13. Every value type, link type, action type, view and source of version 13 reads
the same in canonical JSON; two object types and their two sources, the function `route` and the two types in the
grant's `read` are added.

The routes answer where a fact lives, so a reader asks once instead of reading files. `ap route index`
(`specs/ap/route.md`) writes the records, on the timer `ap-route.timer` every ten minutes (`deploy/README.md`,
"Routes"); `ont fn route` answers from the current objects.

### `section`: one heading of a Markdown file
One object per heading of each `*.md` file at `origin/main` of agent-platform, ontology-platform and
agent-vps-operations, and of the installed shared instructions under `/root/.agents`, with the text above a file's
first heading (or a whole file without a heading) as one more. The Academy's repositories are not indexed. Keyed by
the hex SHA-256 of the repository (or `instructions`), the path, the heading path and the ordinal among equal heading
paths of the file, so a key stays when lines move; titled by `heading`.

| Property | Type | Meaning |
| --- | --- | --- |
| `repository` | string, optional | `owner/name`; none for the instructions |
| `path` | string | the path in the repository, or the absolute path of an installed instruction file |
| `heading` | string | the heading path, parent headings joined by ` > `; the file's name for the text above the first heading |
| `first_line`, `last_line` | integer | the heading line, and the line before the next heading of the same or a higher level |
| `commit` | string, optional | the last commit at `origin/main` that changed the file; none for the instructions |
| `digest` | string | SHA-256 of the file's bytes when it was indexed |
| `text` | string, 4,000 | the words of the section's lines, each run of white space one space |

### `component`: one component of a repository's map
One object per component of each repository's `map.json` at `origin/main`, keyed by the hex SHA-256 of the
repository, `map.json` and the name; titled by `name`. Properties `repository`, `name`, `purpose`, `paths`, `entry`,
`spec`, `tests` (each an array of strings) and `commit`, the last commit that changed `map.json`.

### Sources, the grant and the function
The sources `sections` and `components` are of kind `records` (`specs/ont/index.md` R14 to R29 of the engine):
every line of `/var/lib/agent-platform/route/*.sections.jsonl` and `*.components.jsonl` is one record, its time the
member `observed_at` (the producer's run time), its type the member `type`; `complete_listing` is set, so a section
whose file or heading is gone is retracted, and `fresh_for_s` is 3600. Each source is owned by its one type. The
grant of the domain reads `section` and `component`; they are source-owned, so it derives neither.

The function `route` (`specs/ont/fn.md` R1 of the engine) has the parameters `words` (string, required) and `limit`
(integer, optional), the program `/usr/local/bin/ap`, `timeout_ms` 2000 and `answer_bytes` 65536. Its answer is
`specs/ap/route.md` R15 to R21.

### What the store does not check
- That a record's lines and text are those of the file at the commit it names: the engine reads the records as the
  producer wrote them. A repository whose fetch fails keeps its earlier records, which turn stale after an hour.
- That `limit` is from 1 to 20: the parameter is an integer; the program refuses another value (`function_failed`
  to the caller).
- That `repository` and `commit` are absent exactly for the instructions, and that a component's arrays hold paths.
- That the program at `/usr/local/bin/ap` answers the function: an `ap` older than this change refuses an empty
  argument list, and the call is `function_failed`.

### The migration and the rehearsal
`ontology/migrate` knows version 14; its step is empty (no data migration), so a store at version 13 carries on with
`ontology/migrate` 13 14 changing nothing. The line `HIGHEST = 14` names it; `14 15` and `9 15` are pairs it does not
know (exit 2). The pairs refused on the version 10 part of the rehearsal are (9, 15), (10, 10) and (10, 9). The
engine at `ontology/ENGINE-PIN` must know functions and the kind `records` before this schema is installed: an older
engine refuses it.

The second store of the rehearsal installs version 13 from Git by its commit and digest (above) instead of the
repository's file, and runs the walk of version 13 on it. Then it installs the repository's file over the objects
made under version 13, with the globs of the two sources rewritten to a directory of the rehearsal's own root:
every object read under version 13 (tasks, decisions, requests, intents, criteria, units, reviews, runs, findings,
holds, principles) and ten views read the same; the schema differs from version 13 only by the two types, the two
sources, the function and the grant's `read`; `ont describe` lists the function `route` with its two parameters and
without its program; `ont_describe` and `ont_action_list` stay within 262,144 bytes (the line `v14_mcp_bytes=`);
`ontology/migrate` runs 13 14, 12 14 (exit 0, the store unchanged), 9 14 (every row already done) and 14 15 (exit 2).
The walk of version 14: an index of the two sources while their directory is absent fails both with
`glob_root_missing`; two sections (one of a repository, one of the instructions without `repository` and `commit`)
and a component are written and indexed (`initial`), read back by `ont search` with every property, indexed again
unchanged, and a changed text is an `update`; a line of the other type fails the source `record_type_mismatch`, a
text over 4,000 bytes `value_constraint_violation`, and neither changes a head; a file removed retracts its section,
and an empty directory retracts every section and component. The last line of standard output of the rehearsal is
`v14 routes: ok`.

Version 14 stays pinned as stated at the head of section "Version 15: the horizon"; the migration and the rehearsal as they stand now are described in section "Version 15: the horizon", "The migration and the rehearsal".

## Version 15: the horizon
Version 14 remains pinned at
`d0d469fc8b9cfa390c82b27acffbedd26388d342:ontology/work.schema.json`,
sha256 `b0af778a40e7bdb87219ea91c41720e5866ddcd0d7f5910f106d2698edae36b7`.
Version 15 only adds to version 14: the function `horizon`. Every other member of the schema reads the same in
canonical JSON.

The horizon answers which draft units of an intent may have their pages written now, so that the dispatch pass starts
their writers (`specs/ap/work.md` R23 to R25, `specs/ap/unit-write.md`) and a Lead asks once instead of reading each
unit's dependencies.

### The function
The function `horizon` (`specs/ont/fn.md` R1 of the engine) has the parameter `intent` (string, required: the
intent's key), the program `/usr/local/bin/ap`, `timeout_ms` 30000 and `answer_bytes` 65536. Its answer is
`specs/ap/work.md` R23: `{"intent", "units": [{"unit", "class", "page"}]}`, the draft units of the intent whose every
dependency is `merged` or `done`, `ready`, `running`, `accepted` or `stopped`, or a draft with a review of kind `page`
closed `accept` at its page digest.

### What the store does not check
- That the program at `/usr/local/bin/ap` answers the function: an `ap` older than this change refuses an empty
  argument list, and the call is `function_failed`.
- That the answer is current when it is read: it is computed from the objects at the call.

### The migration and the rehearsal
`ontology/migrate` knows version 15; its step is empty (no data migration), so a store at version 14 carries on with
`ontology/migrate` 14 15 changing nothing. The line `HIGHEST = 15` names it; `15 16` and `9 16` are pairs it does not
know (exit 2). The pairs refused on the version 10 part of the rehearsal are (9, 16), (10, 10) and (10, 9).

The second store of the rehearsal installs version 14 from Git by its commit and digest (above) instead of the
repository's file, with the globs of the two sources rewritten to a directory of the rehearsal's own root, and runs
the walk of version 14 on it. Then it installs the repository's file over the objects made under version 14, the
globs rewritten the same way: every object read under version 14 (tasks, decisions, requests, intents, criteria,
units, reviews, runs, findings, holds, principles, sections, components) and the ten views of version 14 read the
same; the schema differs from version 14 only by `version` and the function; `ont describe` lists the function
`horizon` with its one parameter and without its program, beside `route`; `ontology/migrate` runs 14 15 (exit 0, the
store unchanged) and 15 16 (exit 2). The last line of standard output of the rehearsal is `v15 horizon: ok`.

Version 15 stays pinned as stated at the head of section "Version 16: clauses, the wave and the stored page digest"; the migration and the rehearsal as they stand now are described in section "Version 16: clauses, the wave and the stored page digest", "The migration and the rehearsal".

## Version 16: clauses, the wave and the stored page digest
Version 15 remains pinned at
`8663375dde9d0557fbde136730766ef1f1d02210:ontology/work.schema.json`,
sha256 `4fc42799c7cbd47c9e53bd2c3b1e6e2389d915e8aa79f2b501034f3793a1574a`.
Version 16 changes what points 1 to 6 below name; every other value type, link type, view, source, object type and
action of version 15 reads the same in canonical JSON.

A clause is one numbered sentence of a specification, so that an implementer and a page reviewer ask what binds a
change once, with the tests that cite it and the units that name it. `ap clause index` (`specs/ap/clause.md`) writes
the records on the timer `ap-clause.timer`; `ont fn clause` answers. A unit stores the digest of its page as `ap`
computed it, so that `ready_unit` compares a page review's `head` with what the store holds and not with a caller's
parameter.

1. The object type `clause`, owned by the new source `clauses`, without a `title`, keyed by `<owner>/<name>:<path>#R<n>`
   (`park-kyungchan/agent-platform:specs/ap/unit.md#R11`). Its properties, each from the record's member of its name and
   each required: `repository`, `path`, `rule` (`R<n>`) and `commit` (strings), `line` and `tests_total` (integers),
   `digest` (`sha256_hex`), `text` (string, at most 4,000 bytes) and `tests` (array of strings, each `<path>:<line>`).
   The source `clauses` is of kind `records`: every line of `/var/lib/agent-platform/clause/*.clauses.jsonl`, its time
   the member `observed_at`, its type the member `type`, `complete_listing` set, `fresh_for_s` 3600. The grant's `read`
   gains `clause`; it is source-owned, so the grant does not derive it.
2. The function `clause`: the parameter `key` (string, required), the program `/usr/local/bin/ap`, `timeout_ms` 10000
   and `answer_bytes` 65536. Its answer is `specs/ap/clause.md` R7 to R9.
3. `unit` gains three optional properties: `clauses` (an array of references to `clause`), `wave` (integer) and
   `page_digest` (`sha256_hex`), and the link type `unit_clauses` (`unit` by `clauses` to `clause`, many to many, its
   reverse `clause_units`). The property `unit.write_set` and the parameter `write_set` of `add_unit` and `amend_unit`
   are arrays of the value type `write_pattern` (an entry such as `a/*` is `invalid_parameter`).
4. `add_unit` gains the optional parameters `clauses`, `wave` and `page_digest`; `amend_unit` gains `clauses` and `wave`
   (optional) and `page_digest` (required: an amendment without it is the engine's `invalid_parameter`). Each rule sets
   the three properties from them; a parameter not given leaves its property as it is.
5. `ready_unit`. `page_not_reviewed` holds for a unit of class `contract` or `effect` only when `$unit.page_digest` is
   present (a `present` guard before the comparison) and a review of kind `page` linked by `unit_reviews` is `closed`
   with verdict `accept` and has `$unit.page_digest` as its `head`. The parameter `page_digest` stays optional; the new
   criterion `page_digest_differs`, directly before `page_not_reviewed`, refuses when the parameter and
   `$unit.page_digest` are both present and not equal. The `where` of `plan_not_reviewed` gains
   `{"gt": ["started_at", "$unit.added_at"]}`: a plan review counts for a unit only when it started after the unit was
   added.
6. The sources `task_states`, `packets` and `reports`: each `exclude` gains `/home/palantirkc/work/edu-*`,
   `/home/palantirkc/work/*/edu-*` and `/home/palantirkc/work/*/*/edu-*` (section "Directories the sources must never
   read").

The engine's codes the walk below reads: `amend_unit` without `page_digest` is `invalid_parameter` (field
`/parameters/page_digest`); `add_unit` with the `write_set` entry `a/*` is `invalid_parameter` (field
`/parameters/write_set`); an action whose rule writes a stored unit whose `write_set` holds such an entry
(`ready_unit`, `drop_unit` and `stop_unit` among them) is `invalid_parameter` (field `/parameters`, property
`write_set`); `amend_unit` of a `running` unit is `unit_not_amendable`.

### What the store does not check
- That a `page_digest` given to `add_unit` or `amend_unit` is the digest of the page: the engine renders no page; `ap`
  computes it (`specs/ap/unit.md` R42).
- That a unit of class `contract` or `effect` has a stored digest: one without (added by another caller than `ap`, or
  stored before version 16) is not made ready until an amendment stores one. After delivery a `draft` unit of those
  classes with an accepted page review needs one `ap unit amend` (of `estimate_lines` to its own value, for one).
- That a stored `write_set` is of the form: a unit stored before version 16 whose `write_set` holds a text that is no
  `write_pattern` reads as before and is changed by no action. A `draft`, `ready` or `stopped` one is changed again once
  `amend_unit` gives it a `write_set` of the form; a `running` or `accepted` one never (`unit_not_amendable`), so before
  delivery the Lead reads that no `running` or `accepted` unit holds such an entry.
- That `wave` is from 1 to 99: it is any integer (`ap` bounds it, `specs/ap/unit.md` R41).
- That a clause a unit names exists: a reference to a key without a current clause is stored, and `ont around` from a
  clause reaches only the units that name it.
- That a record's lines, text and tests are those of the file at the commit it names: the engine reads the records as
  `ap clause index` wrote them. A repository whose fetch fails keeps its earlier records, which turn stale after an
  hour.

### The migration and the rehearsal
`ontology/migrate` knows version 16; its step is empty (no data migration): it gives no stored unit a digest, a wave or
clauses and rewrites no stored `write_set`. The line `HIGHEST = 16` names it; `16 17` and `9 17` are pairs it does not
know (exit 2). The pairs refused on the version 10 part of the rehearsal are (9, 17), (10, 10) and (10, 9). Between the
installs of the schema and of `ap`, an `ap` of the version before is refused by `amend_unit` (no `page_digest`) and adds
units without a stored digest, which `ready_unit` then refuses `page_not_reviewed` when they are of class `contract` or
`effect`: both fail safe.

The second store of the rehearsal installs version 15 from Git by its commit and digest (above) instead of the
repository's file, with the globs of `sections` and `components` rewritten to a directory of the rehearsal's own root,
and runs the walk of version 15 on it. Still under version 15 it adds two units of class `contract`, each with an
accepted page review, and a `draft` and a `running` unit whose `write_set` is `["a/*"]`. Then it installs the
repository's file over the objects made under version 15, the glob of `clauses` rewritten to that root too: every
object read under version 15 and the ten views read the same; the schema differs from version 15 only by points 1 to 6
(each is taken off a copy and the rest compared); `ont describe` lists the function `clause` with its one parameter,
beside `horizon` and `route`; `ont_describe` and `ont_action_list` stay within 262,144 bytes (the line
`v16_mcp_bytes=`); `ontology/migrate` runs 15 16 (exit 0, the store unchanged) and 16 17 (exit 2). The walk of version
16, each refusal before the action it guards:

- (a) on the first `contract` unit, after a plan review of its intent that started after the unit was added:
  `ready_unit` with the reviewed digest, `page_not_reviewed` (no stored digest); `amend_unit` of `estimate_lines`
  without `page_digest`, `invalid_parameter`; with it, applied; `ready_unit` with another digest,
  `page_digest_differs`; with the stored one, applied.
- (b) on the second: `amend_unit` of the objective with a new digest, then `ready_unit` with the old digest
  (`page_digest_differs` and `page_not_reviewed`) and with the new one (`page_not_reviewed`).
- (c) a `plain` unit added without `page_digest` becomes ready.
- (d) on an intent of its own: a unit added after the intent's only accepted plan review, `plan_not_reviewed`, and
  ready once a later plan review is closed `accept`; a unit added before the first review is ready without a new one.
- (e) `add_unit` with the `write_set` entry `a/*`, `invalid_parameter`; with `a/**` and `a/b.rs`, applied; the `draft`
  and the `running` unit of version 15 read the same; `drop_unit` of the draft and `stop_unit` of the running one are
  each `invalid_parameter`; `amend_unit` with a `write_set` of the form applies to the draft and is refused
  `unit_not_amendable` for the running one.
- (f) an index of `clauses` while its directory is absent fails `glob_root_missing`; two clause lines are indexed
  (`initial`) and read back by `ont search` and by `ont get` under the key of point 1 with every property; a changed
  text is an `update`; a unit added with `clauses` naming one of them and a key without a clause is stored, and
  `ont around` from the clause by `clause_units` reaches it; a removed file retracts its clause.
- (g) `wave` set by `add_unit` and changed by `amend_unit` reads back.

The last line of standard output of the rehearsal is `v16 clauses: ok`.
