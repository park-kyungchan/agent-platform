# The intent of an objective with its success criteria: `ap intent`

Purpose: the Lead of an objective drafts the user's `intent` from the request that started it, with its success criteria, amends it, names its Lead by the user's routed word, has it confirmed by the user's word or under the user's delegation, checks or waives its criteria and closes it; and only the session the intent names as its Lead acts with the role `lead`.

## Requirements

- R1: The arguments are `--store <absolute path>`, forwarded to every `ont` call, and `--text`, admitted only with `op` `show`; any other argument, and `--text` with another `op`, is refused `usage` (field `args`) before any call to the store.
- R2: The request is one JSON object of at most 1 MiB with `op` `draft`, `amend`, `note`, `lead`, `assume`, `confirm`, `criterion`, `close`, `retire`, `end_delegation` or `show`; an unknown, missing or malformed member is refused `invalid_input` with its field before any call to the store.
- R3: `request_id` is 1 to 44 bytes of ASCII letters, digits, `.`, `_` and `-`; `agent`, `session` and `lead_session` are 1 to 64 bytes of ASCII letters, digits, `.`, `_`, `:` and `-`; a native key (`intent` but in `draft`, `origin`, `by`, `delegation`, `request`, `stopped_by`, `criterion`) is 1 to 512 bytes; a text is a string of at most the schema's bytes (`outcome` 1,200, `first_user` 600, `why`, `not_included` and `constraints` 800, `directory` 300, `notes` 4,000, `reason` 600 and of `waive` 1,000, `evidence` 1,000); `state` is `pass` or `fail`. An empty text is the store's to refuse (`outcome_empty`, `reason_empty`, ...).
- R4: The key of a new intent (`intent` of `draft`) is 1 to 60 bytes of lower-case ASCII letters, digits and `-`, beginning with a letter; any other is refused `invalid_input` (field `intent`) before any call to the store.
- R5: `questions` is an array of at most five objects, each with a nonempty `question` and `default` and an optional `answer`, stored as the empty string when absent; six or more are refused `too_many_questions` (field `questions`, `detail` the count) before any call to the store.
- R6: A refusal is one line `{"error":{"code","field","detail","after","intent","criteria"}}` on standard error with exit 2: `after` is null when this call applied nothing, else the suffix of the last action this call applied (an `already_applied` counts): `draft`, `criterion-<n>` or the suffix of the `op` (R8); `intent` is the intent's key when the call names or drafted one, else null (also for an `invalid_input` whose field is `intent`: the member it refuses is carried as null, while `unknown_intent` and `intent_exists` carry the key as given); `criteria` the keys of the criteria this call added so far, in order.
- R7: A refusal of the store keeps the store's code (the first failed criterion's code for `submission_criteria`) and has the store's refusal object, as the store answered it, as `detail`; a `revision_conflict` of `draft_intent` (the key is used) is `intent_exists`.
- R8: Every action is applied without a `validate` before it, with the client `ap`, the request's `agent` and `session` as the actor and the request id `<request_id>-<suffix>`: `draft` (`draft_intent`), `criterion-<n>` (the n-th criterion of `draft`, from 1), `amend`, `note`, `lead` (`name_lead`), `assume`, `confirm`, `criterion` (`add_criterion`), `check`, `waive`, `close`, `retire` and `end` (`end_delegation`).
- R9: Every apply carries the member `role` `lead` (`specs/ap/roles.md` R3). `assume` and `confirm` with `delegation` also keep the Lead check: they are applied only when the call's `session` equals the intent's `lead_session`, read with one `ont get` before the apply; when they differ, or the intent has no Lead, the call is refused `not_the_lead` (`detail` the intent's `lead_session` or null, `after` null) before any apply; an intent that does not exist is `unknown_intent`. Every other apply carries the role and no `expected`.
- R10: The apply of `assume` and of `confirm` with `delegation` passes `expected` with the intent's id and the revision of that `ont get`; the store's `revision_conflict` (a Lead renamed, or the intent changed, between the read and the apply) is passed on with `after` null.
- R11: When the environment variable `ONT_ROLE` is `lead` and R9 does not decide the call, every writing `op` is refused `role_in_environment` (field `ONT_ROLE`, `after` null) before any call to the store, except `assume` and `confirm` with `delegation`, which R9 decides whatever `ONT_ROLE` is: a call by the intent's Lead is applied, and one by a session that is not the Lead is refused `not_the_lead` (`after` null); `show` writes nothing and is not refused; another value is no refusal, and the body's `role` stays `lead`.
- R12: `draft` applies `draft_intent` with `intent`, `outcome`, `first_user`, `origin` and the optional `why`, `not_included`, `constraints`, `directory` and `questions`, then for each of the optional `criteria` (1 to 12 objects with a `statement` of 1 to 600 bytes, a `check` of 1 to 800 bytes and an optional `baseline` of at most 300 bytes, all checked before any apply) `add_criterion` in their order.
- R13: A `draft` repeated whole answers `already_applied` and writes nothing; a `draft` cut after the draft or between two criteria and repeated adds the criteria that are missing and none twice.
- R14: After every apply the object is read back with `ont get`, and the answer is one JSON object on standard output: `intent` (`criterion` and `intent` for `criterion`, `request` for `end_delegation`), `outcome` (`applied` or `already_applied`, of the call's last action; of `draft_intent` for `draft`), `status` as read (`state` for a criterion), and for `draft` `criteria`, the keys in order. An `already_applied` can meet a state the action did not leave (a confirmation repeated after an amendment): a new confirmation needs a new `request_id`, and `status` shows what the store holds.
- R15: `amend` applies `amend_intent` with `intent` and the optional members of `draft` but `origin` and `criteria`; `note` `note_intent` with `notes`; `lead` `name_lead` with `lead_session` as its `session` and `by`, without `expected` (how an intent gets its first Lead); `close` and `retire` `close_intent` and `retire_intent` with `reason`; `end_delegation` applies `end_delegation` with `request` and `stopped_by`.
- R16: `confirm` has exactly one of `by` (applies `confirm_intent`, without `expected`) and `delegation` with `reason` (applies `confirm_intent_delegated` under R9 and R10).
- R17: `criterion` has `do`: `add` (`intent`, `statement`, `check`, optional `baseline`, as in R12: `add_criterion`), `check` (`criterion`, `state`, `evidence`: `check_criterion`) or `waive` (`criterion`, `by`, `reason`: `waive_criterion`).
- R18: `show` reads the intent with `ont get` (`unknown_intent` when it does not exist), its criteria with `ont search` of `criterion` by `intent` (the intent's full id), and the requests routed to it with `ont search` of `request` by `status` `current` and `route` `intent`, keeping the rows whose `intent` is the asked one; each search follows `continuation` until there is none and requires `basis.qualified` (else `store_answer_unqualified`); requests are sorted by `said_at` then key; criteria are put in the order the draft listed them, which `added_at` (equal to the second for the criteria of one `draft`) and their keys (hashes) do not give: the order of the creations in `ont log` of the `action_type` `add_criterion` (filtered by the outcome `applied`, with the parameters, followed to the end, oldest first, the rows of this intent's criteria only); a criterion the log does not hold keeps its place by `added_at` then key, before the logged ones; it writes nothing.
- R19: The answer of `show` is one JSON object: `intent` (the key), every property of the intent as the store holds it, `criteria` (`criterion`, `statement`, `check`, `baseline`, `state`, `evidence`, `checked_at`), `requests` (`request`, `topic`, `kind`, `said_at`, `words_verified`) and `confirmed`: `no` unless `status` is `confirmed` or `met`, then `by delegation` when `confirmed_delegated` is true, else `by the user`.
- R20: `show --text` prints plain lines: `<key> <status>`, with ` (confirmed by delegation)` when R19 says so or ` (assumed: <reason>)` while `assumed`; `outcome: `, `first user: `, `not included: ` (when the intent has it); one line per question `question: <question> (default: <default>; answer: <answer>)`; one line per criterion `[open|pass|fail|waived] <statement> — <check>`; `notes: ` (when the intent has them).
- R21: Every call to `ont` goes through the module `store.rs`, which retries an answer `store_busy` up to five times after a short wait and retries nothing else.
- R22: The verb writes nothing but through the actions `draft_intent`, `amend_intent`, `note_intent`, `name_lead`, `assume_intent`, `confirm_intent`, `confirm_intent_delegated`, `add_criterion`, `check_criterion`, `waive_criterion`, `close_intent`, `retire_intent` and `end_delegation`, and writes no file.

## Interface

`ap intent [--store <path>] [--text] < request.json`. The request is one JSON object with `op`; every writing `op` has `request_id`, `agent` and `session`:

- `draft`: `intent`, `outcome`, `first_user`, `origin`; optional `why`, `not_included`, `constraints`, `directory`, `questions`, `criteria`.
- `amend`: `intent` and the optional members of `draft` but `origin` and `criteria`.
- `note`: `intent`, `notes`.
- `lead`: `intent`, `lead_session`, `by`.
- `assume`: `intent`, `reason`.
- `confirm`: `intent` and exactly one of `by` and `delegation` (with `reason`).
- `criterion`: `do` `add` (`intent`, `statement`, `check`, optional `baseline`), `check` (`criterion`, `state`, `evidence`) or `waive` (`criterion`, `by`, `reason`).
- `close`, `retire`: `intent`, `reason`.
- `end_delegation`: `request`, `stopped_by`.
- `show`: `intent`.

The answer of a writing `op`: `{"intent","outcome","status"}` with `criteria` for `draft`; `{"criterion","intent","outcome","state"}` for `criterion`; `{"request","outcome","status"}` for `end_delegation`. The answer of `show`: the intent's properties with `intent`, `criteria`, `requests` and `confirmed` (R19).

Codes of `ap` itself: `usage`, `invalid_input`, `input_too_large`, `too_many_questions`, `not_the_lead`, `role_in_environment`, `unknown_intent`, `intent_exists`, `store_answer_unreadable`, `store_answer_unqualified`, `process_failed`.

A refusal of the store passes on the store's code, such as `origin_not_current`, `intent_closed`, `intent_not_draft`, `intent_not_open`, `by_not_the_users_word`, `by_not_routed_here`, `by_older_than_amendment`, `no_criterion`, `not_a_delegation`, `criterion_not_passed`, `revision_conflict`, `request_id_reused` or `store_busy`.

## Trust

The session is the caller's claim, as the engine's actor is: R9 guards against a mistake (a session acting as the Lead of an intent it does not lead), not against a caller that names another session. Any session can also name itself Lead with a routed order of the user (`lead`): that too is a claim, and the engine's log shows it.

## Not included

- No `ap status`, no units, no change of the schema, no report card: `--text` is plain lines.
- No view is read: `show` reads the objects and prints them as they are.
