# ap

The contract of each kept verb is its specification under `specs/ap/`; its section below is one line that routes there, and the parts of "One stage" and "Delivery" name the requirement ids that hold them. The verbs `gate` and `review` ("The older verbs" says what they are kept for) and the case call of `ap unit` are documented in their sections here and have no specification.

| Verb | Form | Contract |
| --- | --- | --- |
| `version` | `ap version`, `ap --version`; `ap --help`, `ap <verb> --help` | `specs/ap/verbs.md` |
| `run` | `ap run < request.json` | `specs/ap/run.md`, `specs/ap/run-limits.md`, `specs/ap/run-object.md`, `specs/ap/policy.md` |
| `unit` | `ap unit add`, `amend`, `ready`, `drop`, `page`, `review`, `run`, `approve`, `reject` | `specs/ap/unit.md`, `specs/ap/unit-review.md`, `specs/ap/unit-run.md` |
| `deliver` | `ap deliver [check] < request.json` | `specs/ap/deliver.md` |
| `request` | `ap request [--store <path>] [--text] < request.json` | `specs/ap/request.md` |
| `intent` | `ap intent [--store <path>] [--text] < request.json` | `specs/ap/intent.md` |
| `status` | `ap status [<key> [--requests] \| --term <word>]`, `--brief [<intent>] [--directory <path>]`, `--wait`, `--indicators` | `specs/ap/status.md` |
| `quota` | `ap quota` | `specs/ap/quota.md` |
| `work` | `ap work [--store <path>]` | `specs/ap/work.md` |
| `policy` | `ap policy check [<file>]` | `specs/ap/policy.md` R12 to R16 |
| `watch` | `ap watch [--record-login --totp-file <path> [--replace]]` | `specs/ap/watch.md` |
| `check` | `ap check <dir> [--timeout <seconds>] [-- <command> [<arg>...]]` | `specs/ap/check.md` |
| `route` | `ap route index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]... [--instructions <dir>]`; `ap` with no argument as the function `route` | `specs/ap/route.md` |
| `gate` | `ap gate < request.json` | section "The gate" |
| `review` | `ap review <start\|close\|revoke> < request.json` | section "One review" |
| `unit` (a request on standard input) | `ap unit < request.json` | sections "One unit", "One case" |

`ap --version` (and `ap version`) prints one line `ap <Cargo package version> <40-hex commit>[-dirty]`, or `unknown` for the commit when Git was unavailable at build time (`specs/ap/verbs.md` R3).
The install of `ap` is `ap deliver` (`specs/ap/deliver.md`; the timer `ap-deliver.timer`). `scripts/install-ap.sh` is the step it calls, run by hand only for the repairs `deploy/README.md` names: `check` compares the installed binary (`AP_INSTALL_DIR`, default `/usr/local/bin`) with `origin/main`, `install <expected commit>` builds from a detached worktree of that exact commit, a new directory of mode 0700 under the build base `AP_INSTALL_BUILD_DIR` (default `/var/lib/agent-platform/build`, never under `/tmp` or `$TMPDIR`, so no other local account can give Cargo a `.cargo/config.toml`), and installs it after verifying the staged and installed `--version` report exactly that commit, and `rollback <before-file>` restores a backup.
For `install`, `origin/main` means the local ref, so run `git fetch` first; the previous binary is backed up as `ap.before-<first 7 hex of its sha256>` in `AP_BACKUP_DIR` (default `/var/lib/agent-platform/install`), and `rollback` accepts only that name in that directory and prints the restored version (or `no_version_output`). The script lock is `AP_INSTALL_LOCK` (default `/run/lock/ap-install.lock`). `install` creates an absent build base and its missing parents with mode 0700; it refuses `build_dir_unsafe@install-ap.sh:<line>: <component>` when the base is not absolute or a component from `/` to the base (read as it stands, never through a link) is a symbolic link, not a directory, owned by neither root nor the running user, or writable by group or other (a sticky directory such as `/tmp` counts), naming the deepest such component, and `build_dir_unusable` when the base cannot be created or cleared. Under `AP_INSTALL_LOCK` it removes every entry of the base (the worktree of an install killed before its cleanup), so a run by hand with the default base uses the default lock or stops the timer first (`deploy/README.md`).

## The older verbs

`ap --help` and `-h`, `ap <verb> --help`, `ap version` and `--version`, and the refusal of an argument list that reaches no verb (the earlier installer's `install`, `doctor` and `render` among them): `specs/ap/verbs.md` R1 to R5.

`ap gate` and `ap review` are kept for one use: a pull request merged by hand in an incident, outside `ap unit run`, whose head still needs a recorded review before `ap deliver` installs it. `ap gate` waits on that pull request's required checks ("The gate"), and `ap review start` and `close` record the review of its head ("One review"). `ap unit run` uses their code for its own gate, reviews and status. They have no specification.

## One stage

`ap run < request.json`: `specs/ap/run.md` (the request R1 to R6, the refusals before any effect R7 to R10, the run and its record R11 to R29), with `specs/ap/run-limits.md`, `specs/ap/run-object.md` and `specs/ap/policy.md`. The record, the stream and the error file are mode 0600 (R11). The role `writer`, a unit's page writer, runs on Claude only, under `limits` with its scratch directory alone, and ends at its first result as a reviewer does (`specs/ap/run.md` R53). `runtime: "agy"` (the Antigravity CLI) runs a reviewer, an implementer or a fixer (R47): `HOME` is the stage's own home, so `ap run` adds `RUSTUP_HOME` and `GIT_CONFIG_GLOBAL` from the passwd home, and `GH_CONFIG_DIR` only to a stage whose `limits.show` names `~/.config/gh`, each unless the request's `env` names it (R49); it takes no `next_accounts`.

### A stage with limits

`specs/ap/run-limits.md` R1 to R15: `limits` holds a Claude stage (a reviewer, an implementer or a fixer) or an `agy` stage (the same three roles) under `bwrap` to its home of the launch, one scratch directory and the paths its request names (`write`, `over`, `read_only`, R1, R5, R11), and hides the host's credential and socket paths from it but the one `show` names (R10; the record's `limits.hidden`). A scratch path below a link is refused (R2). `ap unit review` gives every look on Claude or `agy` its limits (`specs/ap/unit-review.md` R13), `ap unit run` every stage (`specs/ap/unit-run.md` R38), and an acceptance check of a unit runs in the view of R12. Every such stage has the door (`specs/ap/run-limits.md` R13). The account is hidden from the stage but for that home, `<account>/homes/<session>`, which holds a sign-in without the refresh token, renewed by `ap run`, and the account's `CLAUDE.md` (`specs/ap/run-limits.md` R14, R15); an `agy` stage sees of its account its own home (`<account>/homes/<uuid>`) and the account's `.gemini` read-only, no sibling home (R5, R14). Its names are `daily-cloudcode-pa.googleapis.com`, `www.googleapis.com`, `oauth2.googleapis.com` and `aicode.googleapis.com`, with the build hosts for an implementer, a fixer and a verifier (R18). The stage's network is an allow-list by name through the proxy of `ap run` (`specs/ap/run-limits.md` R17 to R20), each request a line of `AP_EGRESS_LOG` (default `/var/lib/agent-platform/egress.jsonl`).

### A stage on the API credit

`specs/ap/run.md` R54 to R56 and `specs/ap/policy.md` R17, R18: the host's policy may carry the member `api_key_accounts`, 1 to 5 entries from an account directory to a key file directly under `/etc/agent-platform/credentials/` (never `api.key_file`, which `ap policy check` refuses, reader `models`). A Claude launch whose account is such a directory, with or without `limits`, is a key launch: before it, and before its home is made, `ap run` reads the key file through the port by the rule of `specs/ap/api.md` R4, refuses a key equal to `api.key_file`'s bytes (`auth_fault` `batch_key`) and an account that holds a `.credentials.json` (`sign_in_present`), and otherwise hands the key to the CLI only as `ANTHROPIC_API_KEY` in the launch's process environment (no argument, `--settings` text, `systemd-run` word, file, record or stream of `ap` holds it), derives and binds no sign-in and calls no `claude auth status`. A refused key launch ends `failed`, `unmet` `launch`, nothing launched. The record of a Claude stage and each entry of its `attempts` hold `auth` (`api_key` or `account`) and `auth_fault`. A policy without the member launches as before. The Lead's switch to the credit for a window, and back: point the policy's `accounts` and `reviews.account` at a key account (an account directory of its own, whose `CLAUDE.md` is linked to the Lead's) and name it in `api_key_accounts`; to end it, put them back and remove the member (`ap policy check` after each edit).

### A stage of a unit or an intent

`specs/ap/run-object.md` R1 to R13.

### Closing an orphan run object

`specs/ap/run-object.md` R14.

### A typed answer

`specs/ap/run.md` R30 to R33.

### A stage on the Antigravity CLI

`specs/ap/run.md` R47 to R52.

## The gate

`ap gate < request.json` waits on ONE pull request's head until its required checks are done, and the automatic review too when the request names its reviewer, and prints one JSON answer on stdout. It reads GitHub through `gh api graphql` (the caller's `gh` login) and decides nothing a person decides: it resolves no thread, posts no status, merges nothing and calls no store. Its only writes are `gh pr ready` on a draft (once) and the one toggle below.

The request: `repository` (`owner/name`), `pull_request` (number), optional `head` (40 hex: the head the caller expects), `autoreview` (the login of an automatic reviewer whose comment carries the marker, for example `chatgpt-codex-connector[bot]`; absent or `false`, the default since 2026-11-17: no automatic reviewer is waited for, because the review of a unit is its `verifier` stage), `check_minutes` (default 90) and `autoreview_minutes` (default 40), each 1 to 1440. Another member, or a malformed one, refuses `invalid_input` with exit 2 before any read: `{"error":{"code","field","detail"}}` on stderr.

It reads every 30 seconds. The head it waits on is the one GitHub reports at the first reading. In order:
1. Merged or closed (at any reading) ends `merged` or `closed`. A caller's `head` that differs from GitHub's ends `head_differs` (`expected` names the caller's). A head that moves later ends `head_moved` (`head_now`). A draft is marked ready once; a failed mark ends `ready_failed`.
2. The required checks, judged at every reading, also while the gate waits for the review: every entry on the head's commit whose `isRequired` is true, except the status `lead-review` (a `StatusContext` of that context: the output of the Lead's accept, not a check that precedes the review, so a revoked or pending acceptance never blocks the gate; a check run of that name is judged like any other), over every page (a reading that could not be completed is no reading). One that completed with any conclusion but `SUCCESS`, or a status that is not `SUCCESS` or pending, ends `check_failed` with `failed_check: {name, state, run}`. None yet, or one pending, waits; at the bound of the phase (`check_minutes`, or `autoreview_minutes` once the review wait began) it ends `check_timeout`. A repository without a required check ends `check_timeout`.
3. The automatic review (only when `autoreview` names a login; skipped when it is absent or `false`): the marker `codex-review-state:v1 {…}` counts only in a comment of that login (a bot's GraphQL login gets `[bot]`, so a user of the same name does not count); the last such comment's `code_review` is read, from the last occurrence in that comment that parses. The marker is looked for in the last 100 comments. When the checks have passed and it names another commit (or none), the gate marks the pull request draft and ready ONCE, because a push to a ready pull request is not reviewed; not when the gate itself just marked a draft ready. It ends `passed` when `commit_sha` is the head and `status` is `completed`, and also at `autoreview_minutes`: the time limit passing is not a failure, `autoreview.state` says `timeout`.

The answer: `end`, `repository`, `pull_request`, `head`, `marked_ready`, `checks` (`[{name, state, run}]`: the judged entries, then the status `lead-review` with its state and `judged: false` when the head has one, required or not), `autoreview` (`login`, `state`: `completed`, `timeout`, `waiting` when the gate ended before it, or `off`; `commit`, `has_unresolved_findings` as the marker says, `toggled`, `threads_on_head`: the number of review threads, resolved or not, whose first comment is the automatic reviewer's and was made on the head commit (the comment's `originalCommit`); `null` when the threads could not be read or no reviewer is named), `threads` (every unresolved review thread: `id`, `path`, `line`, `author`, `answered`: its last comment is not the automatic reviewer's, `comments`: its number of comments; `null` when it could not be read), and `failed_check`, `expected`, `head_now`, `left_draft` (a toggle whose ready mark failed three times) or `unreadable` with their ends. `answered` is true for every thread when no reviewer is named. `passed` with exit 0 also covers a review that reached its time limit and a review with unresolved findings: the caller reads `autoreview.state`, `has_unresolved_findings`, `threads_on_head` and `threads`: the marker's flag does not change when a person answers and resolves a thread, the threads do. `ap gate` decides nothing of this and writes nothing more. Exit by end: 0 `passed`; 10 `merged`, `closed`, `head_differs`, `head_moved`; 11 `check_failed`, `check_timeout`; 12 `ready_failed`, `github_unreadable` (`unreadable` names the cause: `not_found`, `no_login` or `gh_not_started` end at the first reading; `no_answer` when a reading still failed at the bound of the phase).

## One review

`ap review start|close|revoke` reads one JSON object on stdin and writes
one JSON answer; a refusal is `{"error":{"code","field","detail"}}` on stderr with exit 3.
Every path is absolute. Optional `store` forwards `--store` to every `ont` call.
The review object owns the verdict; `lead-review` is a derived commit status.
The rulesets of the three platform repositories require it beside `rust`.

`start` reads and hashes a regular prompt file below `/home/palantirkc/work/`.
With a pull request, it checks that the request is open at exactly `head`.
Each optional run must exist, belong to the task and occur once. It applies
`start_review` with a stable request id based on task key, kind, head and prompt
hash, returning `status: started`, `outcome: applied`, the complete `review` id
and `prompt_sha256`. Optional integer `attempt` (2 to 99) appends a newline and
that number to the digest input; omit it for the first attempt. A repeated request
reads the review and returns `outcome: already_applied` only while it is running.
A closed review is refused with `review_already_closed`; retry with a new `attempt`.
It does not launch a reviewer. Example stdin for `ap review start`:

```json
{"task_dir":"/home/palantirkc/work/example","kind":"packet","head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","base":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","repository":"owner/repo","pull_request":12,"runs":[{"object_type":"worker_run","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"run-key"}],"model":"reviewer-model","effort":"high","prompt":"/home/palantirkc/work/example/prompt.md"}
```

`close` reads a regular report file below the work root, at most 1 MiB. Its
first line must be exactly `Verdict: accept` or `Verdict: repair`. It hashes the
bytes and applies `close_review`; optional `account` and `session` are recorded.
Alternatively, supply `verdict: abandoned` with no report. The answer contains
`status: closed`, `verdict` and `report_sha256` (null for abandoned). The stable
close request id hashes the review key and report hash (`abandoned` without a
report). Example stdin for `ap review close`:

```json
{"review":{"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"review-key"},"report":"/home/palantirkc/work/example/report.md","account":"b","session":"reviewer-session"}
```

`revoke` reads a review with a repository and head. When its `status` is
`closed` it applies `revoke_review` with the reason, under the request id
`rv-revoke-` and the first 40 hex of the SHA-256 of the review's key, the same
for every revoke of that review; a refusal or a failure of the store ends the
command with `store_refused` or `store_answer_unreadable` and nothing is posted.
When the `status` already is `revoked` it applies nothing. Only then does it post
`lead-review=failure` with `revoked: <reason>`. The object changes before the
status, because the store is what a later reader asks: a come-back of `ap unit`
and `start_install` read `status: closed`, so a revoked review is not found
again, while a status alone would be overwritten by the next `success`. A crash
between the two leaves a revoked object and no posted failure; the repeated
command finds `revoked` and posts it. A running review is not changed. The
reason must contain text and fit within 100 UTF-8 bytes. It returns
`status: revoked` and `head`. Example stdin for `ap review revoke`:

```json
{"review":{"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"review-key"},"reason":"Report missed a changed path"}
```

Refusals (local checks precede writes and status posts):

- `invalid_input`: malformed object, id, path, run list, reason or explicit verdict; field identifies the member.
- `review_prompt_unreadable`: prompt is not a readable regular file below the work root.
- `review_head_differs`: pull request is closed or has another head.
- `review_run_unknown`: a start run cannot be read as the exact current object.
- `review_run_of_other_task`: a run supplied to `start` belongs to another task.
- `review_already_closed`: `start` repeated after its review closed; repeat with a new `attempt`.
- `store_refused`: the store refused an attempted action; detail carries its codes; no retry occurs.
- `review_report_unreadable`: close report is unreadable, outside the work root or larger than 1 MiB.
- `review_report_lacks_verdict`: the report's first line is not one of the two exact verdict lines.
- `review_not_accepting`: required review state or evidence is absent; detail names the first failing member.
- `store_review_unreadable`: the exact review's current observation cannot be read.
- `store_answer_unreadable`: an action reply is malformed or does not confirm the expected create.
- `process_failed`: an external command failed or returned unreadable Git output; no automatic retry occurs.
- `io_error`: stdin or the caller's working directory cannot be read.
- `input_too_large`: stdin exceeds 1 MiB.
- `usage`: exactly one supported verb is required.

## One unit

`ap unit < request.json` has two forms (`specs/ap/unit.md` R1). A request with the member `op` is refused `usage` with the field `op` and the `detail` `ap unit <op> < request.json: the op is the first argument`, before any other check of it, a case call's too: the op of `ap unit add|amend|ready|drop` is the first argument. A request with the member `case` is a case call ("One case"). A request with neither member, one that is not JSON, and an argument list of `ap unit` whose first word is none of the words of "The unit object" and "The page writer" are refused as an argument list that reaches no verb (`specs/ap/verbs.md` R4). A unit of work runs as `ap unit run` ("The unit object").

## The unit object

`ap unit add|amend|ready|drop`, `ap unit defer|undefer|used` and `ap unit page`: `specs/ap/unit.md` (the unit's `kind`, `use` and `depends_delivered`, which `add` and `amend` take and the page prints outside its digest, and the words `defer` and `undefer`, which record the user's deferral of a unit and its end, and `used`, which records the first real use of a merged unit's result: R47 to R50; a refusal `invalid_input` of a writing op names every member that breaks a rule in `fields`, each with its rule: R37; the wave, the stored digest of the page, the clauses its `specs` name, the refusal `page_digest_differs` of `ready` and the section `## Clauses` of the page: R41 to R45); `ap unit review`: `specs/ap/unit-review.md`; `ap unit run` and `ap unit approve|reject`: `specs/ap/unit-run.md`. A unit's tiers may name `claude` or `agy` (any other word: `tier_not_supported`); an `agy` stage runs on the first path of its accounts, `bypassPermissions`, with no `next_accounts` (R2, R4). `ap unit review` settles a look's runtime before it reads the policy's `reviews` defaults, gives an `agy` look the account of `accounts` `agy/reviewer`, `agy/<model>` or `agy`, and a fresh Claude look on a mapped list of accounts the rest as `next_accounts`; `reviews.next_accounts` (1 to 4 paths) is its own list (`specs/ap/unit-review.md` R4, R8).

`ap unit run` carries an accept across an update (`specs/ap/unit-run.md` R37): when `main` moved under an accepted head and the run's own update merged it without a conflict, the other side changed no path of the unit's write set and the unit's patch is unchanged, the new head is checked (the write set, the acceptance checks, the required check) and the accept is carried to it with the store's `carry_review`, with no verifier session; the call writes `<directory>/<key>/<n>.update.md` (the heads, the bases, the other side's paths, the checks' ends) and its answer names `carried: {review, from, file}`. An author commit, a path of the write set, a conflict or a `repair` verdict still goes to a verifier round. A carried review is a verdict but not a round: neither the unit's `rounds` nor `ap status --indicators` counts it.

An author stage of `ap unit run` writes no object and no ref of the clone's Git directory `<G>` (`specs/ap/unit-run.md` R38, R51): in its view `<directory>/<key>/objects` stands at `<G>/objects`, the clone's objects are read-only at `<directory>/<key>/base` and named by the run's read-only `info/alternates`, and the branch's ref and reflog are the files of `<directory>/<key>/heads` and `logs`. After each author stage, and at the start of a call that finds it due, the run imports the stage's objects into `<G>` (a fresh repository of its own, `pack-objects --local`, `index-pack --strict` of the clone) and moves the branch from what it wrote (`<directory>/<key>/written`) to the stage's ref; a failed step stops the unit `objects_import_failed` with the detail `{"step": ...}`, and a link or an `info/alternates` the run did not write stops it `git_dir_link`. A branch of one component stops a unit before its author stage (`invalid_input`, field `branch`), and `ap unit add` and `amend` refuse it (`specs/ap/unit.md` R10). The door's hook gets `GIT_ALTERNATE_OBJECT_DIRECTORIES` naming the unit's objects (`specs/ap/door.md` R9).

A plan review is bound to what it read (schema version 27). `add` and `amend` keep each unit's `plan_digest`, the digest of its block of the plan (its heading, objective, dependencies, write set and specifications; not its tiers or estimate: `specs/ap/unit.md` R52); an accepted plan review marks every unit it read and the intent's design record (`specs/ap/unit-review.md` R25, R26); `ready` passes on the store's `plan_not_reviewed` for a unit whose block changed since its mark (R54). The order of an objective: the pages are written first (`ap unit write` amends their plan fields), the Lead names the intent's design record (`ap intent`, op `design`), and then the one plan review runs; a page amended after it in a field of its block needs the plan review again. The refusals of `ap` about the design: `design_unreadable` (field `intent`) when the plan review cannot read the named record; and, for `ready` of a `draft` unit of the lane `objective` (after the write-set and digest checks, before the page review, routing and the base), `specs_missing` (field `specs`) when the unit's `specs` is absent or holds no string entry, read before the intent, `design_missing` (field `unit`, detail `{"intent": <key>}`) when the intent exists in the store without a design, and `design_not_reviewed` (field `unit`) when the record's digest is not the one the last accepting plan review marked (`specs/ap/unit.md` R53). The lane `small`, a unit that is not `draft`, `add`, `amend`, a plan look and the look of `ap unit write` are not subject to them.

A look that the Lead starts (`ap unit review` with `unit`, the claim `lead`) of a `draft` unit is a program first (`specs/ap/unit-review.md` R28 to R31, `specs/ap/unit.md` R56 to R59): after the judgement of the write set and before the citations on the base, it runs by `ready`'s own code the checks above for the lane `objective`, the tiers of the routing map, and ONE `ont action validate` of `ready_unit` with `ready`'s parameters; a failed criterion other than `page_not_reviewed` refuses the look (field `unit`, `detail` `{"failed": [...]}`), so no reviewer session is spent on a page `ready` would refuse. A refused look starts no review and writes no prompt. A page look before the plan look is so refused `design_not_reviewed` or `plan_not_reviewed`: the plan look comes first. On every look and on `ready`, a check of kind `fails_on_base` that ends on the base with the status 129 is `acceptance_not_runnable`, as 125 to 127 are; 2 and 128 stay failures on the base.

## The page writer

`ap unit write <key> [--store <path>]`: `specs/ap/unit-write.md`.

## One case

`ap unit < request.json` with the member `case` (and no `op`) is a case call: it runs a past unit again, in a copy that holds only that unit's base, and judges it by the tests the unit was accepted with. One record says whether a stage of the named model made those tests pass; with no stage the call shows whether the case is valid. It is one pass: at most one stage, no fixer, no step repeated, and it never continues an earlier call's record; it prints the record on stdout. It starts no `gh` and no `ont`, makes no network call and writes nothing outside `worktree` and the files of `out`; what the stage and the commands of the case do is theirs.

The case file (`evals/engineering/cases/<id>.json`, see `evals/README.md`), JSON with exactly these members: `id` (`^[a-z0-9][a-z0-9-]{0,63}$`; the file is `<id>.json`), `repository` (`owner/name`), `pull_request` (the merged pull request the case was taken from), `base` and `head` (40 hex digits: the first parent of the merge commit, and the accepted head), `tests` (a nonempty array of relative repository paths without `..`: every file that judges; the candidate gets them with their bytes at `head` and the evaluator restores them), `commands` (a nonempty array of argument arrays: what the evaluator runs in the copy, in order), `page_sha256` (64 hex digits: the page given to the candidate; its text is a host file and never enters this repository).

The request, ONE JSON object of at most 1 MiB on standard input, exactly: `case` (the absolute path of the case file), `clone` (a local clone that holds `base` and `head`), `worktree` (the path of the case's copy; it must not exist), `page`, `out`, `acceptance` (the evaluator, `evals/engineering/evaluate`), optional `implementer` (one stage block, given to `ap run` as it is: `account`, `model`, `effort` and `prompt`, and optionally `runtime`, `permission_mode` (default `auto` for a stage on Claude Code only; a block on another runtime states its own), `timeout_seconds`, `next_accounts`, `env` and `compact_window`, as "One stage" says; `resume` is refused), optional `at` (`"base"`, the default, or `"head"`), optional `acceptance_seconds` (1 to 86400, default 2700). `at: "head"` together with `implementer` is refused (`invalid_input`, `at`): no stage ever works in a copy that holds the head. Every path is absolute; `page` and `out` lie below the work root, `acceptance` outside `worktree`. Any other member (`repository`, `branch`, `task_key`, `verifier`, `verdict`, `gate`, `store`, `merge`, `pull_request` among them) is refused with `invalid_input` and its name.

Refusals (exit 2, before any effect: no file, copy, process or network call), each one line `{"error":{"code","field","detail"}}` on stderr and no record, the form of `ap run`: `io_error` and `input_too_large` (field `stdin`); `invalid_input` (an unknown or malformed member, a relative path; `field` names the member, `implementer.<member>` for one of the block); `outside_work_root` for `page` and `out`; `inside_worktree` for `acceptance`; `page_unreadable`; `prompt_unreadable` (also a prompt that leaves no room for the block of facts within `ap run`'s 1 MiB); `acceptance_unreadable` (not a regular executable file); `out_exists` (`<out>.unit.json` or another file of the call exists); `out_unwritable` (the record cannot be created); `worktree_exists`; the stage's own, every member `ap run` can judge before the copy exists, judged by its code with the prefix `implementer.`; and `case_unreadable` (`case` is not a regular file, not JSON, or a member is missing, unknown or malformed); `case_page_differs` (the SHA-256 of `page` is not `page_sha256`); `case_commit_missing` (`base` or `head` is not a commit of `clone`, or a path of `tests` is not a regular file at `head`: Git mode `100644` or `100755`, so a symbolic link, whose target would be the candidate's to change, is no judging file; `field` is `case.base`, `case.head` or `case.tests`). A case call's page needs no line `Write set:`.

The steps, each recorded before it begins and after it ends; no other step is in a case record:

| Step | What it does | How it ends |
| --- | --- | --- |
| `worktree` | creates `worktree` as a new Git repository with ONE commit and no remote: at `base` the tree of `base` with every path of `tests` holding its bytes (and mode) at `head`; at `head` the tree of `head`. Only the objects of that tree are copied out of `clone` (`pack-objects` into `unpack-objects`): the copy shares no object store with `clone` and names it nowhere, so no commit of `clone` can be reached from inside the copy. The commit is made with an identity given on the command line (`-c user.name=… -c user.email=…`), so it needs none from the host. Every `git` of this step runs with `GIT_NO_REPLACE_OBJECTS=1`: a `refs/replace/` entry of `clone` changes neither the tree nor a judging file of the copy | stops with `copy_failed` |
| `implementer` | only with `implementer`: ONE `ap run` stage, role `implementer`, cwd the copy; its prompt is `<out>.implementer.prompt.md`: the request's prompt file unchanged, followed by a block of facts (the page's path, the copy's path, the paths of `tests`, each array of `commands` as JSON on a line of its own, and that the copy has no remote); answer `<out>.implementer.md` | the stage's state `ok`: the step passed. `timeout`: the call ends `failed` (`detail` `{"stage": "timeout"}`), `acceptance` does not run, `cleanup` does. Any other state (`cut`, `no_result`, `failed`), `stage_refused` (what only the stage's start shows), or `stage_record_unwritten` (the state `ok` with a final record that could not be written): the call stops with it as the cause; before the stage starts, a prompt file whose SHA-256 is no longer the recorded one stops it with `digest_differs` |
| `acceptance` | first `worktree` must still be a directory and not a link (a process of the candidate may have put a link there, which would send the evaluator to another place); else the call stops with `copy_replaced`, starts no evaluator, removes nothing and leaves the path for a reader. Then the SHA-256 of `acceptance`, recorded before any step, is compared with the file's, and so is the digest of the case file with `case.sha256` (it names the judging files and the commands, so the stage cannot change it); then `acceptance <case> <clone>` runs in the copy, stdin `/dev/null`, under `timeout -k 10 <acceptance_seconds>`, with the environment of the `ap unit` call (so the caller's `CARGO_TARGET_DIR` and `HOST_CARGO_LOCK` reach it); output to `<out>.accept.log`. No condition on the copy's state: the candidate may have committed or not | exit 0: the call ends `passed`. Exit 1, or the bound reached (exit 124): the call ends `failed` (`detail` `{"exit": 1}` or `{"exit": 124}`). Any other exit: the call stops with `acceptance_failed` (the evaluator could not judge). The suite's evaluator ends on the first signal, so the bound reached is exit 124; an evaluator that had to be killed (exit 137: it ignored the signal, or the host killed it) is another exit and a no-result row, because a kill is no statement about the candidate. `digest_differs` (of `acceptance` or of the case file) stops the call before the run |
| `cleanup` | after the result is `passed` or `failed`: first the same check of `worktree` (a link ends the call `stopped` at this step with `copy_replaced`, with no diff written and nothing removed); then writes `<out>.candidate.diff`, the difference between the copy's one commit and its final files (new files included, ignored files not, binary as Git's binary patch), then removes the copy. After a stop the copy stays for the Lead to read | a failure is recorded in the step (`result: failed`, `detail` `{diff, copy: removed\|failed}`) and changes no result |

The ends: exit 0 with `state: passed`; exit 4 with `state: failed`; exit 3 with `state: stopped` (`stopped_at`, `cause`, `detail`); 2 a refusal before any effect; 5 a record that could not be written after the claim (stdout holds the last record).

The record `<out>.unit.json` is created exclusively before any effect (the claim) and replaced after every step through a temporary name. Its members: `mode` (`"case"`), `repository` and `base` (the case's), `clone`, `worktree`, `page{path,sha256}`, `acceptance{path,sha256}`, `case{path,sha256,id}`, `at`, `prompt` (`{path, sha256}` of the request's prompt file, null without a stage: the digest of the stage's own prompt is of a file that holds the copy's path and differs in every call), `stage_seconds` (the stage's bound in effect, its `timeout_seconds` or `ap run`'s default; null without a stage: it defines what `failed` by timeout means), `stages{implementer, verifier}` (the path of the implementer's `ap run` record, on disk before the stage starts, or null; `verifier` is null), `files{gate, acceptance_output, report, candidate_diff}` (`gate` is null; `report` is the stage's answer, null without a stage), `state` (`running`, `passed`, `failed`, `stopped`), `step` (the step begun, null at the end), `stopped_at`, `cause`, `detail` (the failed step's detail at `failed` too), `steps` (each step done, in order: `name`, `result` (`passed`, `failed`, `stopped`), `start`, `end`, `detail`), `start`, `end`, and the members of the older unit records, each null: `branch`, `task_key`, `write_set`, `verifier_prompt`, `verdict_file`, `merge`, `pull_request`, `head`, `review`, `merge_commit`. No prompt or answer text and no credential. A killed case call leaves the record naming the step begun.

What a case record means, for the read-out (`evals/readout.py`) and any other reader. One record is one row of the suite: the case (`case.id`, its digest `case.sha256`), the target (the model and effort in the record of `stages.implementer`, whose usage is the row's cost), the prompt's digest (`prompt.sha256`), the result. The reader reads `mode` first and then `state` alone, never a step's result: `passed` is a pass, `failed` is a fail, `stopped` is no result. A no-result row is run again once; a second stop stays in the reading as no result; the usage of a no-result row counts in the reading's cost. A record without a stage is a validity row: a case is valid when its row at `base` ends `failed` and its row at `head` ends `passed`; a case with another pair counts in no reading. A suite value measures implementing to given tests, the easier half of the same work (the other half is writing the tests from the page): the read-out prints that beside it. A reading runs alone on the host: a build that waits on the build lock spends the stage's bound.

## One request

`ap request [--store <path>] [--text] < request.json`: `specs/ap/request.md`.

## One intent

`ap intent [--store <path>] [--text] < request.json`: `specs/ap/intent.md`. The op `design` names an intent's design record by its absolute path (`name_design`) and reads no file; a plan review reads it (R25).

## The status

`ap status [<key> [--requests] | --term <word>] [--text] [--store <path>]`, `ap status --brief [<intent>] [--directory <absolute path>] [--store <path>]` (one block of at most 40 lines for the intent, or for each open intent of the directory, the working directory without `--directory`), `ap status --wait <intent> [--timeout <seconds>] [--store <path>]` and `ap status --indicators [<intent>] [--text] [--store <path>]` and `ap status --page [--store <path>]` (the status page the service `ap-status-page.service` writes: the lines of `ap status --text`, each open intent's brief, flow and week's cost by role, the flow, cost and month's API spend of all, and the capacity of the hosts, machines and accounts; plain lines, a section that cannot be read as one line with its code): `specs/ap/status.md`.

## The CPU quota

`ap quota`: `specs/ap/quota.md`; the timer and its install are in `deploy/README.md`, "The CPU quota".

## The dispatch pass

`ap work [--store <path>]`: `specs/ap/work.md`; the timer, its install, the switch and the undo are in `deploy/README.md`, "The dispatch pass". With the policy's member `writer` the pass also starts `ap unit write <key>` for the draft units of each intent's horizon that have no page, `ap` with no argument and `ONT_FN_NAME=horizon` being that horizon as the store's function (`specs/ap/work.md` R23 to R25).

## The watch

`ap watch [--record-login --totp-file <path> [--replace]]`: `specs/ap/watch.md`; the timer, its install, the record of the login reading and the undo are in `deploy/README.md`, "The watch".

## Delivery

`ap deliver [check] < request.json`: `specs/ap/deliver.md`. "What a Lead does after each end" is the part of that title in `specs/ap/deliver.md`, and steps 1 to 12 of a call are its ids R20 to R31 (step 1 R20, step 2 R21, step 3 R22, step 4 R23, step 5 R24, step 6 R25 with the timer's limits R32, step 7 R26, step 8 R27, step 9 R28, step 10 R29, step 11 R30, step 12 R31); a Lead's rollback is R33.

### The engine and the schema

`specs/ap/deliver.md` R37 to R46; the contract of `ontology/migrate` is in its Interface, and what a Lead does with `engine` is in its part "What a Lead does after each end".

## Routes

`ap route index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]... [--instructions <dir>]`, and `ap` with no argument and `ONT_FN_NAME=route`, the program of the store's function `route` (`ont fn route`): `specs/ap/route.md`. The objects `section` and `component` and the function are those of schema version 14 (`ontology/README.md`, "Version 14: routes"); the timer, its install and the undo are in `deploy/README.md`, "Routes"; the evaluation of the answers is `evals/route/run` (`evals/README.md`, "The route suite").

## Resources

`ap resource [--directory <dir>] [--store <path>]`, and `ap` with no argument and `ONT_FN_NAME=capacity` or `ONT_FN_NAME=gate`, the programs of the store's functions `capacity` and `gate` (`ont fn capacity`, `ont fn gate`): `specs/ap/resource.md`. The objects `host`, `account`, `host_sample` and `check`, their actions and sources and the two functions are those of schema version 17 (`ontology/README.md`, "Version 17: hosts, accounts, samples and checks"). The unit files `deploy/ap-resource.service` and `deploy/ap-resource.timer` (every five minutes, at two past) are installed and the timer enabled by `ap deliver`. A pass is read with `systemctl status ap-resource.service` and `journalctl -u ap-resource.service -n 20` (its last line is the answer of `specs/ap/resource.md` R9, its `failed` the steps that did not hold) and `ls -l /var/lib/agent-platform/resource`; the undo is `systemctl disable --now ap-resource.timer`, after which the hosts' `observed_at` ages and `capacity` answers every host `stale` within ten minutes. The Lead reads `ont fn capacity` before it makes units ready or changes `work.slots`, and `ont fn gate` with `since` for the slots procedure; a `gate` that ends `function_timeout` (the read of the indicators grows with the merged units) is asked again with `intent`, which reads the units of that intent alone. An allowance no run reports is recorded by the Lead with the action `state_allowance`, naming the user's request that states it.

## Clauses

`ap clause index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]...`, and `ap` with no argument and `ONT_FN_NAME=clause`, the program of the store's function `clause` (`ont fn clause`, served through a stage's door as `ap fn clause`): `specs/ap/clause.md`. The object `clause`, its source, the function and the unit's `clauses`, `wave` and `page_digest` are those of schema version 16 (`ontology/README.md`, "Version 16: clauses, the wave and the stored page digest"). The unit files `deploy/ap-clause.service` and `deploy/ap-clause.timer` (every ten minutes, five after the route index) are installed and the timer enabled by `ap deliver`. A run is read with `systemctl status ap-clause.service`, `journalctl -u ap-clause.service -n 20` (its last line is the answer of `specs/ap/clause.md` R6) and `ls -l /var/lib/agent-platform/clause`; the undo is `systemctl disable --now ap-clause.timer`, after which the records turn stale within an hour and the function answers `stale`.

## The policy check

`ap policy check [<file>]`: `specs/ap/policy.md` R12 to R16. Its reader `writer` reads the member `writer` of the page writer (`specs/ap/unit-write.md` R2); `api` reads the member `api` of the API credit (`specs/ap/api.md` R3); its last reader, `client`, reads the member `store_url` of the remote store client (`specs/ap/policy.md` R19, `specs/ap/client.md`).

## The API credit

`ap api batch <job> --requests <file> --out <dir> [--queue standard|soak] [--store <path>]`, and `ap` with no argument and `ONT_FN_NAME=api_spend`, the program of the store's function `api_spend` (`ont fn api_spend`): `specs/ap/api.md`. A batch is a `run` of role `batch` with its `job`, `reserved_usd_micros` and `settled_usd_micros`, and the function is that of schema version 18 (`ontology/README.md`, "Version 18: the credit's ledger"). The policy's member `api` names the key file, the stops of the two queues, the day's cap and the jobs with their caps, models and bounds; the prices are the rows of `evals/prices.tsv` of the commit `ap` was built from, at half (the batch price). The key file holds the bare key on one line, owned by root with mode 0600; `ap` reads it only after it has examined it, and the key reaches `curl` only through a header file in a directory of its own, mode 0700, under the run directory (`/run/agent-platform`, or `AP_API_RUN`), removed when the call ends. The run directory also holds `api.lock`, which one call holds from its read of the spend to its reservation. A run left `running` (an uncertain submit, a poll that timed out, a failed close) counts at its reservation until it is closed by hand.

## One check

`ap check <dir> [--timeout <seconds>] [-- <command> [<arg>...]]`: `specs/ap/check.md`. It runs `./check`, or the given command without a shell, for the worktree `<dir>`: on the local PC when the policy file names it in `hosts.pc` and the PC answers, is not busy and has at least `min_available_mb` (default 2,048) MB available, the smaller of its Linux's `MemAvailable` and Windows' free memory, else in `<dir>` here (a command other than `./check` under the host's build lock). The output streams back, the exit status is the command's (124 at the timeout), and one record line goes to `AP_CHECKS_LOG` (default `/var/lib/agent-platform/checks.jsonl`), with what the probe read of the PC and the lowest `MemAvailable` of its Linux while the command ran there. The command's environment, on either host, holds the cargo settings `./check` exports (`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`), so that every cargo command shares `./check`'s artifacts; on the PC it also holds the marker `AP_CHECK_HOST=pc`, which `ap check` removes from a command it runs here (`specs/ap/check.md` R17). Inside a stage with `limits` the PC is reached through the door (`specs/ap/check.md` R20). On the PC the tests of `crates/ap/tests/door.rs` that start the door return at their start with `door: skipped (pc)` on standard error, since no systemd is reachable there (`specs/ap/door.md`); this host and CI run them.

## The door

`ap fn <name> < request.json`, `ap hook pre-commit` (the script that stands for the host's Git hook in the view of an implementer's or a fixer's stage, which asks the door to run the host's hook outside the stage) and the door of a stage with `limits`: `specs/ap/door.md` R1 to R10. The door's record is the file `AP_DOOR_LOG` names (default `/var/lib/agent-platform/door.jsonl`).

## Measures

`ap measure sessions [--since <YYYY-MM-DD>] [--accounts <dir>]... [--work-root <dir>] [--platforms-root <dir>] [--stage-dir <dir>]... [--prices <file>] [--store <path>]`, and `ap` with no argument and `ONT_FN_NAME=cost`, `flow` or `share`, the programs of the store's functions `cost`, `flow` and `share` (`ont fn cost`): `specs/ap/measure.md` (its passes, its functions, and the timer's undo, none until unit m-kpi). A pass makes every Claude Code session of this platform's agents and stages on this host (route `engineering` of `evals/readout.py`) a `run` with its list-price spend, refusals, reads and writes by UTC day: it measures the run `ap run` made for a stage (`measure_run`) and records a session no `ap run` started as a closed run of `state` `session` (`record_session`, `extend_session`); the Academy's directories are never opened. The objects, the actions and the three functions are those of schema version 19 (`ontology/README.md`, "Version 19: sessions as runs, KPIs of an intent, measures"). `flow` and `share` are refused `function_not_built` until unit m-kpi. Until the timer of unit m-kpi, the pass runs by hand; after the delivery of this version the Lead backfills once with `ap measure sessions --since <day>` (a Monday, as far back as the session records reach), then reads a week with `echo '{"parameters":{"since":"<Monday>","until":"<Sunday>"}}' | ont fn cost`. A second pass over the same records applies nothing new.

## Roles

Every apply of `ap` names its role, and the work schema's grants give each role what it applies and calls: `specs/ap/roles.md` (R1 to R6; the claims of each verb are R3, the services' `ONT_ROLE` R4, the roles file R5; its install is `specs/ap/deliver.md` R58 and R59). The steps of the first delivery are in `deploy/README.md`.

## Remote client

`ap status`, `ap request` and `ap intent` talk directly over HTTP/1.1 to `ont serve` when configured with a tailnet `store_url` (`http://<tailnet-ip>:<port>`, IPv4 `100.64.0.0/10` or bracketed IPv6 `[fd7a:115c:a1e0::/48]`, port `1..=65535`) via `AP_STORE_URL` or the policy file's `store_url` member (`specs/ap/client.md`, `specs/ap/policy.md` R19). Explicit `--store <path>` always selects local `ont` subprocess mode; `AP_STORE_URL` takes precedence over policy `store_url`. In remote mode, `ap status` skips reading local VPS delivery and systemd unit files, and VPS-only driver verbs (`ap deliver`, `ap work`, `ap unit ready|run|review|write` without `--store`) refuse `remote_mode_unsupported` (exit 2).
