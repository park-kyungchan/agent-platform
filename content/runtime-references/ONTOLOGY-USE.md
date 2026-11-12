# Using the Ontology work store

## 1. Homes
The placement rule is `CONTEXT-GOVERNANCE.md`; the store implements these operational types, not a second source or memory bank.

| Fact kind | Home |
| --- | --- |
| Current blocked effects, task choices, a task's KPI with its target, measured values/history, worker progress/results | Store `hold`, `decision`, `kpi`, `measurement`, `worker_run`, when the task's STATE names that object type in use; STATE then points to the store and does not copy it |
| Task owner, authority, next action and evidence pointers; holds, decisions, measurements and worker progress of a task whose STATE does not name that object type in use | Task `STATE.md` |
| Durable rationale, lessons, corrections, preferences and explained terms | Hindsight when the task authorizes it |
| Schemas, action contracts, implemented behaviour and KPI definitions | Owning repository source; `kpi.definition` records the task's operational definition |
| Task files, packets, reports, binaries and instructions | Original files; store `task`, `packet`, `report`, `installed_artifact`, `instruction_file` are indexed observations |

## 2. Authority
Every agent may read: `help`, `describe`, `get`, `search`, `around`, `aggregate`, `action list`, `action validate`, `render` to standard output and `render --check`.
Only the Lead of the task owning the objects writes: `action apply` and `render --write`. A worker writes its report; the Lead records it. A reviewer never writes.
Only a Lead authorized by the task's STATE runs `index`, `schema install`, `backup` or anything changing the installed binary. The same rules apply to the tools of `ont mcp`. An example grants no authority.
The command authenticates nobody: every agent on this host shares the same OS user. Client, agent and session are self-reported attribution. These restrictions are rules of work.
Run `ont help` for command forms; read `ontology/work.schema.json` and `ontology/README.md` in `agent-platform` for the contract. A schema author also reads `crates/ont/README.md` in `ontology-platform`. `describe` reads installed object types and lists each view with its arguments; `action list` explains parameters and criteria. Both read empty standard input as `{}`.

## 3. Object Ids
An Id is JSON with `object_type`, `namespace` and `native_key`. Use the Id returned by `search`, including its namespace; a title is not an Id.

| Object type | Native key |
| --- | --- |
| `task` | Task directory name below `/home/palantirkc/work/`, for example `ws-example` |
| `packet`, `report` | File path relative to that work root, for example `ws-example/TASK-example.md` or `ws-example/REPORT-example.md` |
| `installed_artifact`, `instruction_file` | Absolute file path |
| `hold`, `decision`, `kpi`, `measurement`, `worker_run`, `thread`, `plan` | Generated at creation: in the apply answer it is the `id` of the `result.edits[]` entry whose `change` is `initial` (its `native_key` equals `result.action_id`); later take `result.objects[].id` from search; do not invent a key |

## 4. Read recipes
These examples use the made-up task `ws-example`. Replace its Id with the owning task's search result. Shell pipelines use `set -o pipefail`.
Search reference filters take a full Id; view arguments take only a native key. In shell, keep JSON variables quoted.

```sh
set -o pipefail
TASK_ID='{"object_type":"task","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"ws-example"}'
# Active holds: effect and until.
printf '{"object_type":"hold","where":{"all":[{"eq":["task",%s]},{"eq":["status","active"]}]}}\n' "$TASK_ID" | ont search
# Current decisions: topic, text, decided_by, reason and undo.
printf '{"object_type":"decision","where":{"all":[{"eq":["task",%s]},{"eq":["status","current"]}]}}\n' "$TASK_ID" | ont search
# KPI values with their latest measurement: name, target, value, observed_at and evidence.
printf '{"object_type":"kpi","where":{"all":[{"eq":["task",%s]}]}}\n' "$TASK_ID" | ont search
# Live worker runs: name, tier, runtime, session, packet, started_at and last_note.
printf '{"object_type":"worker_run","where":{"all":[{"eq":["task",%s]},{"eq":["status","running"]}]}}\n' "$TASK_ID" | ont search
# One object by Id, including observation history.
printf '{"id":%s}\n' "$TASK_ID" | ont get
# Rendered text of a view. Views of the work schema, each with the argument task: active_holds, current_decisions, kpi_board, live_workers, closed_workers.
printf '%s\n' '{"view":"active_holds","args":{"task":"ws-example"}}' | ont render
```

The engine contract and detailed bounds are in `ontology-platform/crates/ont/README.md`; these are the limits relevant to task reads:

| Reader | Candidate allowance | Output and other limits |
| --- | --- | --- |
| `search` | 10,000 per page | 1–1,000 rows, default 100 |
| `around` | 10,000 unique admitted heads per type across all hops; candidates at first hop | 1–3 hops, 10,000 reference entries, 1–1,000 rows, default 100 |
| `aggregate` | 10,000 per operation | 2 group keys, 1 count metric, 1,000 buckets, 1,024 bytes per group value |
| `render` | 10,000 across all pages | 1–1,000 displayed rows, default 200; exact omitted count |

The scan limit counts admitted filter candidates, not every head. The first `eq` or `in` reachable through `all` supplies a necessary property condition; `any`, `none`, other operators or no filter leave an unfiltered scan. Unresolved properties, historical schema pins, partial coverage and stale heads remain candidates to preserve qualification/exclusion effects. On `result_too_large`, narrow the request with a necessary task/status equality or split it into disjoint requests; a smaller display limit does not reset the whole-operation allowance of `aggregate`, `around` or `render`.

`aggregate` requires `read` and `derive` on the type and each candidate's scope. `describe` identifies the installed schema pin and readable types; the matching work schema grants `derive` on `hold`, `decision`, `kpi`, `measurement` and `worker_run`. Recheck the matching schema when its pin changes; readable types alone do not prove `derive`. For example, count running workers without fetching their fields:

```sh
printf '%s\n' '{"object_type":"worker_run","where":{"eq":["status","running"]},"metrics":["count"]}' | ont aggregate
```

The answer has `result.kind:"aggregation"`, `result.groups:[{"key":{},"count":N}]` and `result.basis`; require `qualified:true` for a complete count.
For search, values are `result.objects[].head.fields.<property>.value`; identity is `result.objects[].id`. `result.basis.qualified` false means the answer is incomplete (`stale`, `partial` or `unresolved_excluded` say why); pass each non-null `result.continuation` back as `continuation` with the same query until null, even after an empty page. A continuation promises another candidate, not another matching row.
For get, `result.objects[].head` is a revision string: choose the `observations[]` entry whose `revision` equals it, then read `fields.<property>.value`. Render returns text in top-level `markdown`, with `rows` and `truncated`.
The KPI's latest value is updated atomically with its measurement history. Equal observation times allow corrections; the last applied value wins. An unset optional property has no value; views show `(missing)`. Packet/report references may be dangling; use `get` to check existence.

## 5. Lead writes
Send stdin JSON to `ont action apply`: `action_type`, `parameters` and a stable `request_id`. Reference parameters are full Ids. `ont action validate` accepts the same body; apply can pass its `result.basis` as `expected` revisions to detect concurrent changes.
A required parameter has no `?` below. Numbers after strings are maximum UTF-8 byte lengths; optional omission/null skips assignment and does not clear a stored scalar. Application times are generated automatically.

| Action | What it records | Parameters and limits |
| --- | --- | --- |
| `open_thread` | Task's work thread and first consumer/use | task, key:120, consumer:1000, consumer_kind:`domain_route`/`harness`/`none`, first_use:1000; unique task/key |
| `amend_thread` | Consumer/use amendment, previous values in history | thread, consumer:1000, consumer_kind, first_use:1000, reason:600 |
| `open_plan` | One plan per thread, opening estimates immutable | thread, first_lines:integer, limit_lines:integer (Lead supplies 1.5 times first_lines), current_lines:integer, amended_by_family:120 |
| `amend_plan` | Current size/latest writer; clears scope_reviewed, preserves reviewed_lines | plan, current_lines:integer, amended_by_family:120 |
| `record_scope_review` | Scope/design review covering a stated size | plan, reviewed_lines:integer, reviewer_family:120, review_report:1000; reviewed_lines below current_lines refuses `reviewed_lines_below_current`; equal latest-writer family refuses `scope_review_same_family`; families are self-declared |
| `set_hold` | Active blocked effect | task, effect:300, until:300 (lifting condition in prose); effect unique among active holds |
| `lift_hold` | Lifting an active hold | hold, reason?:300 |
| `record_decision` | Current task/topic choice | task, topic:120, text:1000, decided_by:`user`/`lead`, reason?:600, undo?:300, user_words?:1000, user_form?:`decision`/`question` (both required for user, user_words must be nonempty; question refuses `user_question_is_not_a_decision`); one current per topic |
| `supersede_decision` | Replacement and predecessor | decision, text:1000, decided_by:`user`/`lead`, reason?:600, undo?:300, user_words?:1000, user_form?:`decision`/`question` (both required for user, user_words must be nonempty; question refused); inherits task/topic |
| `define_kpi` | Named task KPI | task, name:120, definition:600, target?:200; name unique within task |
| `record_measurement` | Immutable observation and latest KPI | kpi, value:200, observed_at:40, evidence:300; `observed_at` is a real UTC time written exactly as `YYYY-MM-DDTHH:MM:SSZ` (`date -u +%Y-%m-%dT%H:%M:%SZ`), not older than history or later than application time |
| `set_kpi_target` | Desired KPI value | kpi, target:200 |
| `start_worker_run` | Dispatch | task, name:80, tier:`mechanical`/`bounded`/`open`/`review`, runtime:120, packet, thread, plan, layer:`engine`/`domain`/`harness`, session?:200; thread must belong to task (`thread_of_other_task`); unknown plan refuses `plan_missing` before `plan_of_other_thread`; plan must belong to thread; engine needs domain_route; kind none/empty first_use refused; `plan_over_limit` passes only for current_lines at most limit_lines or present reviewed_lines, or tier review; scope_reviewed is not read; name unique among running workers |
| `note_worker_run` | Progress, separate from final result | worker_run, note:600, report?; run must be running |
| `close_worker_run` | Final outcome and close time | worker_run, result:600, report?; run must be running |

`request_id` is 1–64 ASCII letters, digits, `.`, `_` or `-`. Optional attribution: `agent` and `session` are top-level fields of the body beside `request_id` (those characters plus `:`, 1–64), and the client is the command option `--client NAME`.
Retry an uncertain apply with the same client, agent, request_id and body: it returns `result.outcome=already_applied` without new edits. Changed content with that identity refuses `request_id_reused`; a new identity is a new action. Reconcile the target before retrying.
For the board practice below, set THREAD_ID and PLAN_ID to the thread and its plan full Ids, PACKET_ID and REPORT_ID to full file Id JSON, WORKER_ID to the start action's worker Id, KPI_ID to the KPI Id, and OBSERVED_AT to the actual observation time. The file references use work-relative keys even before indexing. `render --write` replaces an existing block; the first block of a section is created by pasting the `markdown` of `ont render` for that view into the board file once. The example board has one block.

```sh
printf '{"action_type":"start_worker_run","request_id":"example-dispatch","parameters":{"task":%s,"name":"example-worker","tier":"bounded","runtime":"codex","packet":%s,"thread":%s,"plan":%s,"layer":"domain"}}\n' "$TASK_ID" "$PACKET_ID" "$THREAD_ID" "$PLAN_ID" | ont action apply
printf '{"action_type":"note_worker_run","request_id":"example-progress","parameters":{"worker_run":%s,"note":"Recipes verified"}}\n' "$WORKER_ID" | ont action apply
printf '{"action_type":"close_worker_run","request_id":"example-final-result","parameters":{"worker_run":%s,"result":"Verification complete","report":%s}}\n' "$WORKER_ID" "$REPORT_ID" | ont action apply
printf '{"action_type":"record_measurement","request_id":"example-observation","parameters":{"kpi":%s,"value":"2","observed_at":"%s","evidence":"ws-example/REPORT-example.md"}}\n' "$KPI_ID" "$OBSERVED_AT" | ont action apply
ont render --write "$BOARD" --section active_holds --arg task=ws-example --preimage-dir "$PREIMAGES"; ont render --check "$BOARD" --expect-blocks 1
```

One render operation reads its schema and all pages in one snapshot; separate render operations can observe different commits. Check `truncated` and omitted rows. `render --write` keeps a preimage; `render --check FILE --expect-blocks N` checks drift and the expected number of generated blocks.

## 6. Refusals
Read `code`, `field`, `rule` and `hint` when present; repair that field or precondition. Do not repeat a refused call unchanged. Submission refusals instead list details in `failed[]`, `failures[].code` and `failures[].message`.

| First-use refusal | Cause and repair |
| --- | --- |
| `projection_stale` | Projected generations differ or required triggers are missing/changed; reads refuse instead of using an invalid projection. Only the STATE-authorized Lead repairs it by installing the exact installed schema bytes again; `changed:false` can still mean the projection was rebuilt. |
| `wrong_value_type`, field `/where` | A search reference filter received a native-key string; pass the full task Id |
| `submission_criteria` / `decision_already_current` | A current decision already owns the topic; use `supersede_decision` with its Id |
| `submission_criteria` / `older_measurement` | History contains a later observation; use the actual latest observation, equal-time correction allowed |

## 7. Recovery pointers
A session whose `ont` MCP server started before an engine installation must reconnect that server: the old server refuses the new store format. See the engine README for compatibility and identical-schema repair; reconnecting does not authorize a write or repair.
Binary installation and rollback belong to `ontology-platform/scripts/install-ont.sh` (`install` and `rollback`); its source describes verification and recovery.
Store backups exist under the store's `backup/` directory. Only the task's STATE-authorized Lead may restore one; consult that task's recovery evidence and the store owner's documentation. Rebuilding indexed files does not restore Ontology-owned action history.
