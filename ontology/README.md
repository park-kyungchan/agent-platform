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
are ordered arrays of repository-relative patterns. `ap work start` requires a
column-1 `Write set:` line for worktree packets, split at commas and trimmed.
It accepts 1 to 64 members whose path part is at most 297 bytes (at most 300
bytes including optional trailing `/**`): relative paths without empty,
`.` or `..` components or backslashes, optionally ending in `/**`, with no other
`*`, `?` or `[` wildcard. No worktree means no write_set parameter.
`write_pattern` checks each member against
`^[A-Za-z0-9._][A-Za-z0-9._/+@-]{0,296}(/\*\*)?$`; the schema has no
member-count bound, so only the caller enforces the count of 64.
Members use ASCII letters, digits and `._/+@-`, begin with a letter, digit, `.`
or `_`, and may have a final `/**`; duplicate members and multiple column-1
`Write set:` lines are refused. The tracked-path character check finds 0
incompatible paths in `agent-platform`; the Lead measured 0 in `ontology-platform`
and 27 non-ASCII tracked paths in one directory of `education-platform`, which
packets name by the directory and `/**`.
No command yet consumes these stored review or write-set records.

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
`status` remains the running/closed statement used by views and other criteria.
For a new type, a uniqueness criterion must link through a reference that is
set only while the object is open and cleared by its closing action (LENS F43).
Version 6 and earlier version 7 objects without `running_in` can still be noted
and closed, but do not block a new start with the same name/head/kind; no migration
is performed. The duplicate criteria basis contains only running objects, including after a later schema install; the engine still reads every closed object as a reverse-link candidate because a cleared `running_in` is `missing`, which its candidate index treats as unknown, until engine item G3.
