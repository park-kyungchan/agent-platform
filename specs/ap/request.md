# One message of the user as a `request` of the store: `ap request`

Purpose: every agent the user speaks to records the user's message as a `request` of the store in the turn it arrives, in the user's own words read from the session's record where the verb can read it, routes it to an intent or sets it aside, and lists the current requests of a topic before it asks the user anything.

## Requirements

- R1: The arguments are `--store <absolute path>`, forwarded to every `ont` call, and `--text`, admitted only with `op` `list`; any other argument, and `--text` with another `op`, is refused `usage` (field `args`) before any call to the store.
- R2: The request is one JSON object of at most 1 MiB with `op` `record`, `supersede`, `route` or `list`; an unknown, missing or malformed member is refused `invalid_input` with its field before any call to the store. A `route` that has none or more than one of its four forms (the member `route` of `record` and `supersede`, and the `op` `route`) is refused with the field `route`.
- R3: `request_id` is 1 to 50 bytes of ASCII letters, digits, `.`, `_` and `-`; `agent` and `session` are 1 to 64 bytes of ASCII letters, digits, `.`, `_`, `:` and `-`; `topic` is 1 to 120 bytes of ASCII letters, digits, `.`, `_`, `:`, `/` and `-`; `kind` is one of the schema's `request_kind`; `reading` is 1 to 1,000 bytes; `runtime` is 1 to 40 bytes of ASCII letters, digits, `.`, `_` and `-`.
- R4: A refusal is one line `{"error":{"code","field","detail","after","request"}}` on standard error with exit 2: `after` is null when this call applied nothing, `record` when the call recorded the request (also `already_applied`), or `supersede` for `route_unknown` (R24), and with `record` or `supersede` `request` is that request's native key, else null. `after` null with `process_failed` or `store_answer_unreadable` does not show that nothing was applied: the call may have been cut after an apply the verb did not see answered. The caller repeats the same call with the same `request_id`, never a new one: what was applied answers `already_applied`, and the rest is applied.
- R5: A refusal of the store keeps the store's code (the first failed criterion's code for `submission_criteria`) and has the store's refusal object, as the store answered it, as `detail`.
- R6: The actions use the request ids `<request_id>-record`, `<request_id>-supersede` and `<request_id>-route`, the client `ap`, and the request's `agent` and `session` as the actor, so a repeated call is the same request only with the same `agent` and `session`.
- R7: `words_from` takes the string of the lines of the session record at its second by the rule of `ap row`, except that a line whose `isMeta`, `isCompactSummary` or `isSidechain` is true is no line of the user and that byte-equal strings count as one; no line is `words_not_found` and more than one distinct string `words_ambiguous`, each with the count as `detail`.
- R8: Words read from a record are stored with `words_verified` true, `record` the record's path and `said_at` the line's own time to the second with `Z`.
- R9: Typed words are `words`, or `words_file` (an absolute path of a regular file whose whole content is the words, at most 1 MiB of the bytes the host's `read` returns, else `input_too_large` with field `words_file`; not valid UTF-8 is `words_not_utf8`); the record of `words_from` is read whole, whatever its size, with the required `said_at` (`YYYY-MM-DDTHH:MM:SSZ`), and are stored with `words_verified` false.
- R10: The words are never trimmed, normalised or changed, and empty words are refused `words_empty` before any call to the store.
- R11: Words over 24,000 bytes, or whose JSON string form without its two quotes is over 48,000 bytes, are stored as their first 4,000 bytes cut back to a character boundary with `truncated` true; words within the bound are stored whole with `truncated` false.
- R12: A `route` is exactly one of `{"intent": key}`, `{"aside": reason}` (1 to 600 bytes), `{"unit": key}` and `{"finding": key}`; an intent and a reason are checked before anything is applied, the intent with one `ont get`: an intent that does not exist is refused `unknown_intent`, one that is `met` or `retired` `intent_closed`, both with `after` null.
- R13: `record` applies `record_request` without a `validate` before it, then reads the created request back and compares its `words` byte for byte with the words sent (`readback_differs`, `after` `record`), and only then applies the route. The read-back reads the request's head, not its observations (`ont get` answers every observation of an object, up to 100, which passes 1 MiB for a request routed often): it takes, from `ont search` on `request` filtered by the call's `session`, `topic` and `said_at` and followed to the end, the row whose id is the request's key; no such row is `store_answer_unreadable` (field `request`).
- R14: A refused route leaves the request recorded and unrouted with `after` `record`; the same call with a corrected `route` answers `already_applied` for the record and applies the route; a call repeated whole answers `already_applied` for each action and writes nothing.
- R15: The answer of `record` is one JSON object on standard output with `request`, `record` (`applied` or `already_applied`), `route` (`applied`, `already_applied` or null), `words_bytes` (of the stored words), `words_verified` and `truncated`.
- R16: `supersede` applies `supersede_request` on the current request `request`, reads the new request back as `record` does, then applies the `route` given or, without one, the route the replaced request had (read after the supersede, from its head: its intent, aside with its reason, its unit or its finding, with the reference it holds; an unrouted one stays unrouted; the row whose id is the replaced key among those `ont search` on `request` filtered by `status` `superseded` gives, followed to the end, and no such row is `store_answer_unreadable`, field `supersedes`), and answers as `record` with `supersedes`.
- R17: The route inherited by `supersede` is not checked before the supersede: a store refusal of it (such as `intent_closed`) has `after` `record`, and the same call with a `route` given finishes it.
- R18: `route` and the member `route` apply `route_request` (`intent`), `set_request_aside` (`aside`), `route_request_to_unit` (`unit`) or `route_request_to_finding` (`finding`) on the request with `<request_id>-route`, after the check of R12; the store's refusals of the last two (such as `unknown_unit`, `unknown_finding`) are passed on; `route` answers `request` and `route`.
- R19: `list` reads with `ont search` on the type `request` (`status` `current` and the `topic`, or without a topic `status` `current` and `route` `unrouted`), follows `continuation` until there is none, requires `basis.qualified` (else `store_answer_unqualified`), reads no view and writes nothing.
- R20: `list` answers `{"requests":[...]}` with the topic's current requests newest first, or the unrouted ones oldest first, sorted by `said_at` and then by key, each with the words and the reading as the store holds them and with the key of the intent, unit or finding it is routed to.
- R21: `list --text` prints one block per request: `<said_at> <kind> <topic>`, ` [not verified]` and ` [truncated]` when they apply and the route (` -> unrouted`, ` -> intent <key>`, ` -> unit <key>`, ` -> finding <key>`, ` -> aside (<reason>)`) on the first line, then the reading, then the words as they are.
- R22: Every call to `ont` goes through the module `store.rs`, which retries an answer `store_busy` up to five times after a short wait and retries nothing else.
- R23: The verb writes nothing but through the actions `record_request`, `supersede_request`, `route_request`, `set_request_aside`, `route_request_to_unit` and `route_request_to_finding`, and writes no file.
- R24: A `route` of the replaced request that the verb does not know is never read as unrouted: `supersede` without `route` is refused `route_unknown` (field `supersedes`, `detail` the value) with `after` `supersede` and the new request's key, and the same call with a `route` given finishes it.

## Interface

`ap request [--store <path>] [--text] < request.json`. The request is one JSON object with `op`:

- `record`: `request_id`, `agent`, `session` (also stored as the request's `session`), optional `runtime`, `topic`, `kind`, `reading`, exactly one of `words_from` (`{"record": <absolute path of a Claude Code session record, at most 400 bytes>, "at": "YYYY-MM-DDTHH:MM:SS"}`), `words` and `words_file` (both with `said_at`), and optional `route`.
- `supersede`: `request` (the native key of the current request it replaces) and the members of `record`.
- `route`: `request_id`, `agent`, `session`, `request`, and exactly one of `intent` (an intent's key), `aside` (a reason), `unit` (a unit's key) and `finding` (a finding's native key); the member `route` of `record` and `supersede` takes the same four forms. `unit` and `finding` need version 11 of the schema.
- `list`: optional `topic`.

The answer of `record` and `supersede`: `{"request","record","route","words_bytes","words_verified","truncated"}`, with `supersedes` (the replaced request's key) for `supersede`. The answer of `route`: `{"request","route"}`.

The answer of `list`: `{"requests":[{"request","topic","kind","said_at","words_verified","truncated","route","intent","unit","finding","route_reason","reading","words"}]}`, where `intent`, `unit` and `finding` are the key the request is routed to and `route_reason` is the reason while the route is `aside`, each null otherwise.

Codes of `ap` itself: `usage`, `invalid_input`, `input_too_large`, `words_not_found`, `words_ambiguous`, `words_not_utf8`, `words_empty`, `unknown_intent`, `intent_closed`, `route_unknown`, `readback_differs`, `store_answer_unreadable`, `store_answer_unqualified`, `process_failed`.

A refusal of the store passes on the store's code, such as `said_in_the_future`, `request_not_current`, `request_confirms_an_intent`, `intent_closed`, `unknown_unit`, `unknown_finding`, `invalid_parameter`, `request_id_reused` or `store_busy`.

## Not included

- No reader of a Muse Code or Antigravity session record: their messages are recorded as typed words, not verified.
- No verb for intents or criteria, no `ap status`, and no change of `ap row`.
- No view is read: a view's cells are escaped and bounded, and `list` shows the words as they are.
