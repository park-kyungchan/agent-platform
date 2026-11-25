# Using the Ontology work store

Read this when you read the state of the work from the store, or write to it as Lead: requests, intents and their criteria, units with their runs and reviews, findings, holds, principles, KPIs. Objects of the store's older generation are index copies of files whose refresh stopped: read the file itself.

## 1. Who may do what

| Who | May run |
| --- | --- |
| Every agent | `ap status`, `ap request` with `op` `list`, and the reads of `ont`: `help`, `describe`, `search`, `around`, `get`, `aggregate`, `render` to standard output, `action list`, `action validate` |
| The Lead, and a session that works for it | `ap request`, `ap intent`, `ap unit`, and `ont action apply` with the role `lead` for the actions of section 4 |
| A stage | No write |

Every agent here is the same OS user; a role is a claim the engine checks against the roles file beside the store (`agent-platform` `specs/ap/roles.md`). These limits are rules of work; an example grants no authority. The contract: `ont describe < /dev/null` (types, views and their arguments) and `ont action list < /dev/null` (parameters, byte limits, criteria); without `< /dev/null` they wait on standard input.

## 2. Reads

```sh
set -o pipefail
ap status --brief        # in an intent's directory: each open intent of it
ap status --text         # everything open, and what needs the Lead
printf '%s\n' '{"view":"intents_open"}' | ont render
KEY=$(printf '%s\n' '{"object_type":"intent","where":{"in":["status",["draft","assumed","confirmed"]]},"limit":1}' | ont search | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"]["objects"][0]["id"]["native_key"])')
ap status --brief "$KEY" # one intent, from anywhere
ap status "$KEY" --text  # the same intent in full
printf '{"view":"units_of_intent","args":{"intent":"%s"}}\n' "$KEY" | ont render
ID=$(printf '{"object_type":"intent","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"%s"}' "$KEY")
printf '{"id":%s}\n' "$ID" | ont get
printf '%s\n' '{"from":{"object_type":"intent","where":{"eq":["status","confirmed"]}},"path":["subject_requests"]}' | ont around
printf '%s\n' '{"object_type":"finding","where":{"all":[{"eq":["kind","lesson"]},{"eq":["state","open"]}]}}' | ont search
```

Views without an argument: `intents_open`, `units_live`, `runs_running`, `findings_open`, `holds_on_work`, `principles`, `requests_unrouted`, `installs`. With one: `units_of_intent` and `criteria_of_intent` (`intent`), `runs_of_unit` and `reviews_of_unit` (`unit`), `requests_by_topic` (`topic`). A view argument is a native key.

| Answer of | Where the values are |
| --- | --- |
| `search`, `around` | `result.objects[].head.fields.<property>.value`; identity `result.objects[].id` |
| `get` | The `observations[]` entry whose `revision` is `result.objects[].head`, then `fields.<property>.value` |
| `render` | Top-level `markdown`, `rows`, `truncated`; `(missing)` is an unset property |

`result.basis.qualified` false is an incomplete answer (`stale`, `partial`, `unresolved_excluded` say why). A `result.continuation` goes back as `continuation` with the same query until it is null.

## 3. Ids

An Id is `{"object_type","namespace","native_key"}`, the namespace `{"instance":"vps://palantirkc","account":"root"}`. A reference filter of `search` and a reference parameter of an action take the full Id; a view argument and the verbs of `ap` take the native key. The Lead chooses the key of an intent, a unit, a finding and a principle; a request's and a hold's are generated (`ap request` answers one, an apply the `id` of the `result.edits[]` entry whose `change` is `initial`).

## 4. The Lead's writes

- The user's words: `ap request` (`specs/ap/request.md`).
- An intent, its criteria, Lead and confirmation, a delegation (`op` `confirm` with `delegation`) and its end (`end_delegation`), its close and retirement (`retire`): `ap intent` (`specs/ap/intent.md`), never a hand apply.
- A unit: `ap unit add`, `amend`, `ready`, `drop`, `run`, `approve`, `reject` (`specs/ap/unit.md`, `specs/ap/unit-run.md`).
- The rest: `ont action apply --client` with your CLI's name, the body `action_type`, `parameters`, a stable `request_id`, `"role":"lead"`, `agent`, `session`. `ont action validate` takes it and writes nothing:

```sh
printf '%s\n' '{"action_type":"open_finding","parameters":{"finding":{"object_type":"finding","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"lead-example-gap"},"kind":"lesson","detector":"lead","summary":"An example","severity":"info"},"request_id":"lead-example-gap-open","role":"lead","agent":"lead","session":"example"}' | ont action validate --client claude-code
```

| Action | When |
| --- | --- |
| `open_finding` | A fault, a drift, a bottleneck, a note or a lesson is seen first |
| `see_finding_again` | An open or planned one is seen again |
| `close_finding` | The change is made (a lesson: its preventing refusal is delivered) |
| `reopen_finding` | A closed or dismissed one holds again |
| `dismiss_finding` | It needs no change |
| `plan_finding_as_intent`, `plan_finding_as_unit` | It becomes the work of an intent or of a unit |
| `set_hold` | An effect on an intent, unit, finding or host is blocked until a condition; one active hold per effect |
| `lift_hold` | The condition holds |
| `record_principle` | A want of the user outlives one intent, with its requests |
| `amend_principle`, `retire_principle` | Its words change; the user no longer wants it |
| `set_intent_principles` | The principles an open intent obeys, the whole list |
| `define_kpi` | A KPI is named; no example, as its required parameter is an object of the older generation |
| `set_kpi_target` | The user's target changes |
| `record_measurement` | A value is measured: `observed_at` from `date -u +%Y-%m-%dT%H:%M:%SZ` |
| `set_kpi_intent` | A KPI becomes one of an intent's |
| `state_allowance` | The user states an allowance its runs do not give |

A KPI of a new intent is defined on an existing task object until the schema lets a KPI live on an intent.

## 5. Refusals and recovery

Repair what `code`, `field`, `rule` and `hint` name; never repeat a refused call unchanged.

| Refusal or answer | Cause and repair |
| --- | --- |
| An uncertain end; an `ap` refusal with `after` | The same body, `request_id`, client, agent and session again: the applied part answers `already_applied` |
| `request_id_reused` | A new body takes a new `request_id` |
| `not_granted`, `role_not_admitted` | The apply names no role, or one the roles file does not list |
| `role_in_environment` | A writing `ap intent` under `ONT_ROLE=lead`: drop the variable, which is for `ont fn` |
| `not_the_lead` | The session is not the intent's `lead_session` |
| `invalid_parameter` | A value of the wrong form or over its byte limit |
| `submission_criteria` | Repair each code of `failed`; `older_measurement`: record the latest value (an equal time corrects) |
| `revision_conflict` | The key is used: see the finding again, reopen it, or take another key |
| `wrong_value_type`, field `/where` | A reference filter received a native key: pass the full Id |
| `result_too_large` | Add an equality on `status` or a reference through `all`, or split the query |
| `projection_stale` | The projection differs from the installed schema: the Lead that installs it repairs it |
| `store_busy` | Repeat after a short wait; `ap` does so itself |

- A session whose MCP server `ont` started before an engine installation reconnects that server.
- Installing and rolling back the engine: `BUILD-AND-DELIVERY.md` section 5. Backups are in the store's `backup/` directory; only a Lead the user named for it restores one. Indexing files again does not bring back action history.
