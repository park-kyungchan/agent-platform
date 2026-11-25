# Log

Purpose: Reading the action log and the history of the log format.

Moved from crates/ont/README.md ("Reading the action log", "History") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## Reading the action log

- R1: `ont [--store PATH] log` takes its request as one JSON object on stdin, like `search`; the MCP tool `ont_log` takes the same object. Members, all optional: `outcome` (`applied`, the default, or `refused`); `action_type` (a string); `since` and `until` (UTC strings of the form `2026-11-17T00:00:00Z`, compared with `submitted_at`; `since` inclusive, `until` exclusive); `actor` (an object of string members, each compared for equality with the logged actor's member, for example `{"role":"verifier"}`; a row whose actor lacks the member does not match); `parameters` (a boolean, default false); `limit` (1 to 1000, default 100); `continuation` (from a previous answer).
- R2: An unknown member or a member of the wrong type is refused `invalid_request` with its `field`, as the other reads refuse it; a time not of that form (or not a real date) is `invalid_request` at `/since` or `/until`; `limit` 0 is `invalid_request` and a limit over 1000 `limit_too_large`, as in `search`; a continuation that this verb did not give for the same `outcome` is `invalid_continuation`.
- R3: Sources: applied rows come from the store's action log table (`action_log`, read in one snapshot), not from `edits.jsonl`.
- R4: Refused rows come from `refusals.jsonl`: a missing file is zero rows; an incomplete last line (no newline yet) is not read; a line that is not valid JSON, or not an object with the members of a refusal line, is skipped and counted in the answer's member `skipped`, as is a line over 1,048,576 bytes, which is not held in memory.
- R5: Admission: a row is returned only when the calling principal holds the grant of the row's action type in the installed schema (the admission `action list` applies) and, for an applied row, the read grant on the type of every object its `edits` name.
- R6: A row whose action type the installed schema no longer defines (a later version removed it, or its domain is no longer installed) is admitted by the read grants on the types of its edits alone, so history stays readable; a refused row of such a type has no edits to judge by, so it is returned without its `parameters` even when they are asked for, and with `parameters_withheld: true`.
- R7: Rows that match the request's filters and are not admitted are left out and counted in the answer's member `withheld`.
- R8: Bound of work: one call examines at most 10,000 rows of the table, or 10,000 lines of the file read from its end, beyond its continuation.
- R9: A call that stops at that bound before `limit` rows matched answers the rows it has, its continuation and `scan_limited: true`.
- R10: Answer: the envelope of the other reads (`schema` holds every installed domain).
- R11: `result` is `{"rows":[...],"continuation":C,"withheld":N,"skipped":N,"scan_limited":B}` with the rows newest first. A row holds `action_type`, `domain`, `actor`, `submitted_at` as a UTC string, and `outcome`; an applied row also `action_id` and `edits` as logged; a refused row also `code` and, when recorded, `failed` and `request_id`; `parameters` only when the request asked for them.
- R12: Paging is that of `search`: `continuation` is null when no older row is left, otherwise a string to pass with the same request for the next older rows.
- R13: A row's `actor` is the logged one, never rewritten: a row logged since `specs/ont/action.md` R101 holds the engine's `principal` and `host` and names its claimed members in `claimed`; an older row or line has `principal` `agent`, no `host` and no `claimed`. The `actor` filter of R1 matches `principal` and `host` like any string member, for example `{"host":"<node>"}`, so it selects the rows one node applied; an older row matches no `host`.

## History

O17 renamed the composition crate and synthetic CLI from `ontd` to `ont-daemon`
(Rust import `ont_daemon`). It was the consumer of `ont-core`/`ont-store`
used by `ont-program-pilot`, `ont-local-fake` and the synthetic integration tests,
until the crate was removed from the workspace because nothing consumed it; its source is in Git history.
The operational command is `ont`. Pinned contracts, recorded diagnostics and
commands over archived source retain their historical names.

