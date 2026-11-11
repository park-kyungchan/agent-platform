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
Version 1 is preserved in Git at
`f2d1b030bd1a0a66928ef57d9365027526e664da:ontology/work.schema.json`,
sha256 `4573ab180f63889029ae98c892c996e810f9d8140cf26726ee6d5fffbfc10605`.
Version 2 retains those source definitions, types and links without changing
meaning. It adds five types keyed by the creating action id:

| Type | Records | Actions |
| --- | --- | --- |
| `hold` | A blocked effect and its lifting condition/reason | `set_hold`, `lift_hold` |
| `decision` | One current decision per task/topic, with its predecessor | `record_decision`, `supersede_decision` |
| `kpi` | Definition, target and latest recorded value/evidence | `define_kpi`, `set_kpi_target` |
| `measurement` | Immutable value, observation time and evidence history | `record_measurement` (also updates `kpi`) |
| `worker_run` | Runtime, assignment, progress and closing result | `start_worker_run`, `note_worker_run`, `close_worker_run` |

Reverse links from tasks are `holds`, `decisions`, `kpis`, `worker_runs`;
from a KPI, `measurements`; from an earlier decision, `successors`.
`agent` reads every type and may apply every declared action. Actors on this
host share the OS account and are self-reported, including client, agent and
session; this attribution is not authentication.

Action definitions describe every parameter and give actionable criterion
codes/messages. All listed action parameters are required, including reason,
undo, target and session; use an empty string when one has no content. Their
object properties are optional until the relevant action supplies them.
A new KPI has no value, observation time or evidence until its first measurement.
Measurement recording updates the KPI with the last applied measurement; it
does not choose the greatest timestamp. Retrying an identical request with the
same `request_id`, client and agent returns `already_applied` without new edits.
Use a distinct `request_id` per intended change. Two rules in one action are
supported and atomic (decision replacement and measurement recording).

### Language limits and explicit adaptations
- View arguments compare only with string properties, not references. Each
  task-owned operational type therefore also stores `task_directory` (at most
  300 bytes). Its action parameter is checked against the referenced task's
  `directory`; it cannot be supplied independently. Every view takes `task` as
  that **full directory**, for example
  `/home/palantirkc/work/workspace-strategy-reassessment-20261109`, rather than
  the task key. Typed reference arguments would remove this mirror and parameter.
- Rule values substitute parameters, but cannot copy a referenced object's
  properties. `supersede_decision` therefore also requires `task`,
  `task_directory`, `topic`, checked against the old decision before its status
  changes; it records `supersedes` as the old decision reference. Allowing
  `$decision.task`, `$decision.topic` and `$decision.task_directory` in rule
  values would remove these extra parameters. The extra `decision_supersedes`
  link makes the predecessor/successor relationship navigable.
- No application-time expression is available to rule values. The caller passes
  `observed_at`, `started_at`, `closed_at` as strings (40 bytes maximum); use
  normalized UTC ISO 8601 values so lexical ordering matches time ordering.
  A trusted `$now` rule expression would remove caller-supplied application
  times; typed timestamp parsing would validate observation times separately.
- The requested views use equality filters and lexical ordering. Typed date or
  number comparison would be better for KPI value versus target or observation
  age in `kpi_board`, elapsed runtime in `live_workers`, and a closing date
  window in `closed_workers`. Add typed arguments and date/number conversions
  (and property comparisons for value versus target) to express those views.
  `active_holds.until` is intentionally free text, so a time-based expiry view
  would also need a separate typed expiry field. `current_decisions` needs no
  numeric or date comparison.
- Properties have no description field in this schema language; type descriptions
  and action descriptions carry the explanations. Adding property descriptions
  would make them available directly in schema introspection.

### Install and rehearse
The file install route is for disposable stores only. This executable rehearsal
requires Bash, Python 3 and Git, takes one `ont` binary path, creates its own
`mktemp -d` store, verifies the frozen v1 bytes from Git, installs v1 and then the
worktree's v2 using `schema install <file>`, indexes only `task_states`, exercises
every action and its criteria, checks retry, history and task isolation, prints
all five rendered blocks, and removes the store even on failure:

```sh
ontology/rehearsal.sh /path/to/ont
```

Supply a build with actions and `render`. The host installation is a separate
operation: after review and merge, fetch this repository and use
`ont --store <authorized-store> schema install --from <checkout> --commit <merged-40-hex> --path ontology/work.schema.json`.
The temporary-file route does not establish deployment or native CLI loading.

### Generated views
All views filter on `task_directory` using the full-directory `task` argument:
`kpi_board` orders by KPI name; `active_holds` by effect; `current_decisions` by
topic; `live_workers` by start time; `closed_workers` by close time. The last
four also filter by their declared status. Each view uses the language's default
200-row limit (maximum 1000), with an explicit omitted-row count, and the engine's
10,000-head bound; argument text uses ASCII letters, digits and `._:/-` only.

With an already installed schema in a disposable store, render through stdin:

```sh
set -o pipefail
printf '%s\n' '{"op":"render","view":"kpi_board","args":{"task":"/home/palantirkc/work/workspace-strategy-reassessment-20261109"}}' | ont --store "$scratch/store.sqlite" render
```

The response's `markdown` field contains the generated block. To update an
existing document block use `render --write <file> --section kpi_board --arg
task=<full-directory> --preimage-dir <existing-directory>` with the same explicit
store; `render --check <file>` checks drift. Render after writers finish for a
coherent whole block: rendering is not one snapshot across pages.
