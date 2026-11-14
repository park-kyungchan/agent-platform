# Using the Ontology work store

Read this when you need a task's holds, decisions, KPI values or worker runs from the store, or when you are Lead and record one. The store holds these for a task only when that task's STATE names the object type in use; STATE then points to the store and does not copy it. For any other task they are in its STATE's pointers and in Git. Task files, packets, reports, installed binaries and instruction files stay the originals: the store indexes them as observations.

## 1. Who may do what

| Who | May run |
| --- | --- |
| Every agent | The reads: `help`, `describe`, `get`, `search`, `around`, `aggregate`, `action list`, `action validate`, `render` to standard output, `render --check` |
| The Lead of the task that owns the objects | `action apply`, `render --write` |
| A Lead whose STATE authorizes it | `index`, `schema install`, `backup`, anything that changes the installed binary |
| A worker, a reviewer | No write. A worker writes its report and the Lead records it |

- The command authenticates nobody: every agent on this host is the same OS user, and client, agent and session are self-reported. These limits are rules of work. An example grants no authority.
- The tools of the MCP server `ont` follow the same table.

## 2. Where the contract is

| I need | Read |
| --- | --- |
| The form of a command | `ont help` |
| The installed object types, and each view with its arguments | `ont describe` (empty standard input reads as `{}`) |
| An action's parameters, byte limits, allowed values and criteria | `ont action list` (empty standard input reads as `{}`) |
| The source of the work schema | `agent-platform` `ontology/work.schema.json` and `ontology/README.md` |
| The engine's contract, its bounds, and how to write a schema | `ontology-platform` `crates/ont/README.md` |

## 3. Object Ids

An Id is JSON with `object_type`, `namespace` and `native_key`. Use the Id that `search` returned, with its namespace; a title is not an Id. A reference filter of `search` and a reference parameter of an action take the full Id; a view argument takes only the native key.

| Object type | Native key |
| --- | --- |
| `task` | The task directory's name below `/home/palantirkc/work/`, for example `ws-example` |
| `packet`, `report` | The file path relative to that work root, for example `ws-example/TASK-example.md` |
| `installed_artifact`, `instruction_file` | The absolute file path |
| `hold`, `decision`, `kpi`, `measurement`, `worker_run`, `thread`, `plan`, `review` | Generated at creation: in the apply answer it is the `id` of the `result.edits[]` entry whose `change` is `initial`; later take `result.objects[].id` from `search`. Do not invent a key |

## 4. Reads

The examples use the made-up task `ws-example`. In shell, keep JSON variables quoted.

```sh
set -o pipefail
TASK_ID='{"object_type":"task","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"ws-example"}'
# Active holds: effect and until.
printf '{"object_type":"hold","where":{"all":[{"eq":["task",%s]},{"eq":["status","active"]}]}}\n' "$TASK_ID" | ont search
# Current decisions: topic, text, decided_by, reason and undo.
printf '{"object_type":"decision","where":{"all":[{"eq":["task",%s]},{"eq":["status","current"]}]}}\n' "$TASK_ID" | ont search
# KPIs with their latest measurement: name, target, value, observed_at and evidence.
printf '{"object_type":"kpi","where":{"all":[{"eq":["task",%s]}]}}\n' "$TASK_ID" | ont search
# Running workers: name, tier, runtime, session, packet, started_at and last_note.
printf '{"object_type":"worker_run","where":{"all":[{"eq":["task",%s]},{"eq":["status","running"]}]}}\n' "$TASK_ID" | ont search
# One object by Id, with its observation history.
printf '{"id":%s}\n' "$TASK_ID" | ont get
# The text of a view. Views of the work schema, each with the argument task:
# active_holds, current_decisions, kpi_board, live_workers, closed_workers.
printf '%s\n' '{"view":"active_holds","args":{"task":"ws-example"}}' | ont render
# A count without fetching fields.
printf '%s\n' '{"object_type":"worker_run","where":{"eq":["status","running"]},"metrics":["count"]}' | ont aggregate
```

| Answer of | Where the values are |
| --- | --- |
| `search` | `result.objects[].head.fields.<property>.value`; identity `result.objects[].id` |
| `get` | `result.objects[].head` is a revision string: take the `observations[]` entry with that `revision`, then `fields.<property>.value` |
| `render` | Text in top-level `markdown`, with `rows` and `truncated` |
| `aggregate` | `result.groups[].count`; a complete count needs `qualified:true` |

| If | Then |
| --- | --- |
| `result.basis.qualified` is false | The answer is incomplete; `stale`, `partial` or `unresolved_excluded` say why |
| `result.continuation` is not null | Send it back as `continuation` with the same query until it is null, even after an empty page |
| The answer is `result_too_large` | Add a necessary equality on task or status reachable through `all`, or split the request. Each reader admits at most 10,000 candidates; a smaller display limit does not help `aggregate`, `around` or `render` |
| A property shows `(missing)` | The optional property is unset |
| A packet or report reference may not exist | Check with `get`: a reference can name a file that was never indexed |

## 5. Lead writes

Send JSON on standard input to `ont action apply`: `action_type`, `parameters`, a stable `request_id`, and optionally `agent` and `session`; the client is the option `--client NAME`. `ont action validate` takes the same body. Take every parameter name, limit and criterion from `ont action list`, not from memory.

| Action | Records | Rule that is easy to miss |
| --- | --- | --- |
| `open_thread`, `amend_thread` | A work thread of the task with its first consumer and first use | One thread per task and key |
| `open_plan`, `amend_plan`, `record_scope_review` | A thread's plan: first estimate, limit and current size in lines; a review that covers a stated size | The opening estimate is immutable; an amendment clears the scope review; families are not compared; optional `review` must be closed |
| `start_review`, `close_review` | A model's review of an exact head, with prompt/report paths and SHA-256 digests | One running review per task/head/kind; closing needs report and digest unless verdict is `abandoned`; `reviews(task)` lists newest first |
| `set_hold`, `lift_hold` | An effect that is blocked, and the condition that lifts it | The effect is unique among active holds |
| `record_decision`, `supersede_decision` | The current choice on one topic | One current decision per topic: a second one is a `supersede_decision`. A decision by the user carries the user's words and is refused when they were a question |
| `define_kpi`, `set_kpi_target`, `record_measurement` | A KPI of the task, its target, a measured value with time and evidence | `observed_at` is a real UTC time from `date -u +%Y-%m-%dT%H:%M:%SZ`, not older than the history and not later than the application time |
| `start_worker_run`, `note_worker_run`, `close_worker_run` | A dispatched worker, its progress, its final result | The thread and its plan must exist and belong to the task; a plan over its limit without a scope review refuses the start; optional `write_set` stores ordered repository-relative patterns from the packet |

| If | Then |
| --- | --- |
| An apply ended uncertain | Send the same client, agent, `request_id` and body again: it answers `already_applied` and writes nothing new |
| The body changed but the `request_id` did not | It refuses `request_id_reused`: a new action needs a new `request_id` |
| A text is longer than its byte limit | It refuses `invalid_parameter`: shorten it; the limits are in `ont action list` |
| An optional parameter is omitted or null | The stored value stays; omission does not clear it |
| You render a board | `ont render --write <board> --section <view> --arg task=<native key> --preimage-dir <dir>`, then `ont render --check <board> --expect-blocks <n>`. The first block of a section is pasted once from the `markdown` of `ont render` |

`review.running_in` is set by `start_review`, cleared by `close_review`, followed by its duplicate criterion, and never set by a caller.
`worker_run.running_in` is set by `start_worker_run`, cleared by `close_worker_run`, followed by its duplicate criterion, and never set by a caller.
`status` remains the running/closed statement for views and other criteria.
`write_pattern` checks each write-set member's ASCII path characters and a path
part of at most 297 bytes, excluding optional trailing `/**` (at most 300 bytes total);
only the caller bounds the count (64 in `ap work start`); the engine has no schema count bound.
For a new type, a uniqueness criterion links through a reference set only while
the object is open and cleared by the closing action; see `ontology/README.md` in `agent-platform`.
Review prompt/report parameters use `file_path`: an absolute printable ASCII path
without spaces, 2 to 400 bytes.

## 6. Refusals

Read `code`, `field`, `rule` and `hint` when present and repair that field or precondition. Do not repeat a refused call unchanged.

| Refusal | Cause and repair |
| --- | --- |
| `projection_stale` | The stored projection does not match the installed schema; reads refuse instead of answering from it. Only the Lead that STATE authorizes repairs it, by installing the exact installed schema bytes again |
| `wrong_value_type`, field `/where` | A reference filter received a native-key string: pass the full Id |
| `submission_criteria` / `decision_already_current` | The topic has a current decision: use `supersede_decision` with its Id |
| `submission_criteria` / `older_measurement` | The history holds a later observation: record the actual latest one (an equal time is a correction and is allowed) |

## 7. Recovery

- A session whose MCP server `ont` started before an engine installation reconnects that server; the old process refuses the new store format.
- Installation and rollback of the binary: `BUILD-AND-DELIVERY.md` section 5.
- Store backups are in the store's `backup/` directory. Only the Lead that the task's STATE authorizes restores one. Indexing files again does not bring back action history.
