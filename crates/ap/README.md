# ap

`ap --version` (and `ap version`) prints one line `ap <Cargo package version> <40-hex commit>[-dirty]`, or `unknown` for the commit when Git was unavailable at build time.
`scripts/install-ap.sh` is the install script for `ap`: `check` compares the installed binary (`AP_INSTALL_DIR`, default `/usr/local/bin`) with `origin/main`, `install <expected commit>` builds from a temporary detached worktree of that exact commit and installs it after verifying the staged and installed `--version` report exactly that commit, and `rollback <before-file>` restores a backup.
For `install`, `origin/main` means the local ref, so run `git fetch` first; the previous binary is backed up as `ap.before-<first 7 hex of its sha256>` in `AP_BACKUP_DIR` (default `/var/lib/agent-platform/install`), and `rollback` accepts only that name in that directory and prints the restored version (or `no_version_output`). The script lock is `AP_INSTALL_LOCK` (default `/run/lock/ap-install.lock`).
Limitation: a source tarball nested inside another Git repository reports the outer repository's revision, because the build derives the revision from the enclosing repository.

## One worker run

`ap work --help` lists the commands. `ap work start`, `ap work wait` and `ap work close` each read one JSON object from stdin (at most 1 MiB) and emit one JSON object. Refusals emit `{"error":{"code","field","detail"}}` on stderr with a nonzero exit. These commands execute the installed `ont` and `herdr`; they do not install either program.

`start` takes `task_dir`, `name`, `packet`, `report`, `thread`, `layer`, `tier`, `cwd`, `model`, `effort`, `workspace`, and optional `worktree`, `store`, `continue_dirty`. Paths are absolute; packet/report references are relative to `/home/palantirkc/work/`. Names match `^[a-z][a-z0-9-]{0,31}$`. A worktree requires a clean Git status unless `continue_dirty` is true, plus exactly one packet line beginning `Write set:` (`ontology/README.md`). The packet's other lines are not checked: its form is `content/skills/lead/references/task-packet.md`.

The store validates admission before any Herdr call, with `session` omitted. After admission, `start` creates one Codex agent and polls for its exact name, created pane and tab, kind `codex`, and `interactive_ready: true` for at most 60 seconds (`agent_not_ready` on timeout). A session id is optional at this point. It records the full run Id using a stable packet-hash request id; only an ok apply with exactly one initial `worker_run` edit permits the one pane-targeted prompt. Its output includes `run`, `binding`, nullable `session`, `pane`, `tab` and `prompt` (`sent` or `uncertain`); `prompt_target` identifies the created pane, and `worktree_head` records the Git head when supplied. Keep the full `run` for closing. There is no polling after the prompt.

The existing store field `worker_run.session` holds the pane/tab binding, not the runtime session id: `ap-work/1:` followed by the compact JSON array `["<pane>","<tab>"]`, for example `ap-work/1:["w1:p2","w1:t2"]`. Both ids must be nonempty and the entire value at most 200 UTF-8 bytes. The reader requires exactly the writer's canonical encoding, including two parts and no extra whitespace. Old session ids, `unknown`, absent fields and other encodings refuse `run_binding_missing`; the Lead reconciles those rows manually.

An uncertain create/start/apply leaves the tab open for the Lead to reconcile; a confirmed store refusal closes the unprompted tab by its just-created id without label or state checks, because this process created it and has sent no prompt. A failed refusal cleanup reports `tab_closed: false`.

`wait` takes `name`, `report`, `timeout_seconds` (unsigned integer), and optional `pane` (used instead of the name when supplied). It waits once and checks the report even if waiting fails. A present report returns its `sha256` and `bytes` with exit 0; an absent or empty report returns `report_missing` with exit 3. `agent_status` is null when unavailable. It sends no prompt and reads no pane.

`close` takes the full `run`, `result`, `name`, `tab`, and optional `session`, `report`, `store`, `agent_gone`. It reads that exact run's current observation, requires `running` and a valid binding, and checks the supplied name/tab against it. It checks the stored tab's label and single-pane count, then reads the stored pane to confirm its tab membership. The live agent with the run's name must occupy both bound ids, be `idle`, `done` or `blocked`, and report a nonempty session id. A caller-known non-null `session` must match (`session_mismatch`); `start`'s null session may be omitted. The stored result appends `; session=<live id>`; the entire result must fit within 600 UTF-8 bytes before any write.

A missing agent requires `agent_gone: true`, supplied after the Lead observed it ended, the same bound sole-pane membership, and no listed agent with that name, pane or tab. Its result appends `; session=unavailable (agent_gone)`, explicitly recording the absence of a live session. Only an ok apply with the two updates of that exact `worker_run` (the modification and the cleared `running_in`), followed by readback of its closed status, result and unchanged binding/name, permits tab closure. A failed tab close returns `tab_closed: false` with exit 3. An uncertain store close or readback leaves the tab open.

`store` forwards `--store <absolute path>` to every Ontology call, permitting disposable-store use. Uncertainty requires readback by the Lead; no command automatically retries a prompt or sends keys.

## One stage

`ap run < request.json` runs ONE headless Claude Code session (a stage: implementer, reviewer or fixer) in a transient systemd scope `ap-run-<uuid>.scope` under `agents.slice` and returns when nothing of it is alive. It prints the run record on stdout. Exit 0 `ok`, 75 `cut`, 3 any other end, 5 a file of the run could not be written after the record was claimed, which includes the case in which nothing ran (stdout holds the final record; the file holds it when a write in place succeeded). Exit 2 is a refusal before any effect, and nothing else: `{"error":{"code","field","detail"}}` on stderr. It reads no role table, calls no store, Herdr or GitHub.

The request: `role` (`implementer`, `reviewer`, `fixer`), `account` (the Claude configuration directory, required), optional `next_accounts`, `model`, `effort`, `permission_mode` (one of Claude Code's: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`), `cwd`, `prompt` (a regular file, at most 1 MiB, given to the CLI on stdin), `out` (the answer's path), and optional `timeout_seconds` (1 to 86400; default implementer 5400, fixer 3600, reviewer 3300), `expect` (`file`: a path that this run must write nonempty; `first_line`: a pattern the answer's first line must match, where `*` matches any run of characters), `resume` (`{"record": "<absolute path of the earlier run's record>"}`), `continue_dirty`, `integration`, `mcp_config` (files; the only MCP servers loaded), `env` (extra variables; a name beginning `HERDR_`, `CLAUDE_` or `ANTHROPIC_`, and `HOME`, refuse `env_forbidden`: the account named in the record is the account that answers), `runtime` (`claude`; `codex` refuses `runtime_not_built`), `bind` (the name of a systemd unit that is active now: when it ends for any reason, stopped, killed or failed, the stage's unit stops), `skills` and `memory` (booleans: the account's skills and slash commands, and its auto-memory, load only when true). `bind` is for a transient unit (the caller's own, as a chain's): one that ended between the request and the launch makes the launch fail (the record says `failed`); a unit that systemd can start again by name is started again by the launch, because `BindsTo` requires it. Every path is absolute; another member refuses `invalid_input`, and so does an `out` whose file name ends with a suffix `ap run` derives (`.run.json`, `.stream.jsonl`, `.err`, `.ap-tmp`): no run's answer path is another run's file. A resume is a run of its own with its own `out`; it continues the session of the earlier run on that run's account. `ap run` guards a run's files against another run; it does not guard a session against its caller: two resumes of one record at the same time are not refused, because the caller is the one owner of a session and runs its stages in sequence. On a resume the prompt file is the next message to the session: give a continuation or the findings, not the first prompt again.

Refusals (exit 2, before any effect, no file written): `invalid_input`; `runtime_not_built`; `account_forbidden` (the directory that is exactly `/root/.claude` by path components, also after resolving, for an implementer or a fixer, and for a reviewer without `integration: true`); `account_missing`; `cwd_missing`; `prompt_unreadable`; `out_unwritable`; `out_named_in_prompt` (the prompt contains the out file's name); `out_exists` (`out`, `<out>.run.json`, `<out>.stream.jsonl` or `<out>.err` exists: nothing of an earlier run is replaced, removed or truncated by a later one); `worktree_dirty` (a fresh implementer run in a Git worktree with changes, unless `continue_dirty`); `worktree_unreadable` (Git cannot read `cwd` for a reason other than "not a git repository"; `git` runs with the fixed short list of variables only, not the request's `env`); `resume_record_missing`, `resume_account_differs`, `resume_cwd_differs`, `resume_role_differs` (a resume keeps the earlier record's `cwd` and its role, or is a `fixer` over an implementer's or a fixer's record); `bind_inactive` (`systemctl is-active` for `bind` did not answer `active`); `run_still_active` (whatever the earlier record's state, `systemctl is-active` for its unit did not answer a state in which nothing of its unit lives: `active`, `activating`, `deactivating`, `reloading`, `refreshing`, a failed call and no answer all refuse); `env_forbidden`; `systemd_absent`.

The run: every run, fresh or resumed, creates its own record. `<out>.run.json` is created exclusively, and that creation is the claim: of two requests on one `out` started at the same instant, one passes and the other is refused `out_exists`. Then `<out>.stream.jsonl` and `<out>.err` are created exclusively; every write of the answer or the record goes through a temporary name unique to that write and created exclusively, so it is never another run's file. A session id is chosen (or taken from the earlier record on a resume), and the record is written with state `running` BEFORE the launch; a resume's record names the earlier record in `resumes`. The launch is `systemd-run --scope --quiet --collect --unit=… --slice=agents.slice -p RuntimeMaxSec=<timeout + 10>s -p TimeoutStopSec=10s [-p BindsTo=<bind> -p After=<bind>] -- claude --print --output-format stream-json --verbose --model … --effort … --permission-mode … --permission-prompts none --setting-sources user --settings <settings> --disallowedTools Monitor --strict-mcp-config [--mcp-config …] [--disable-slash-commands] (--session-id | --resume) <session>`. The settings, for every role and with nothing in the request: `disableAllHooks: true`; `env` with `BASH_DEFAULT_TIMEOUT_MS` and `BASH_MAX_TIMEOUT_MS` at 2700000 (a 40-minute check runs in the foreground of one Bash call) and `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1` (a command that reaches its limit stops instead of moving to the background); `autoMemoryEnabled: false` unless `memory`. `--disable-slash-commands` is left out only with `skills`. So the stage has no `Monitor` and no background shell, and its first event lists no skill and no memory directory, while the account's `CLAUDE.md` still loads (shown on Claude Code 2.1.325). Its environment is only `PATH HOME USER LOGNAME SHELL LANG LC_ALL TERM TMPDIR TZ` of the caller (when set), the request's `env`, and `CLAUDE_CONFIG_DIR=<account>`: no hook (of the account, the host, a project or a plugin), no MCP server the request does not name, and only the account's settings (a worktree's `.claude/settings.json` and `settings.local.json` do not load; with them the project's `CLAUDE.md` and `.claude/agents` do not load either, while the account's `CLAUDE.md` does). stdout goes to `<out>.stream.jsonl`, which `ap run` reads only through the handle it opened before the launch; stderr to `<out>.err`, removed at the end when empty. The stream holds the session's transcript (answers and tool output): keep `out` where it is private. `RuntimeMaxSec` is the unit's own bound: it ends the stage when `ap run` is gone.

The end, by role. A print session that left a background shell emits a `result`, waits for the shell, then may run a second turn and emit another `result` (each `result` carries the `usage` and `num_turns` of its own part; the sum is the session's). So a `result` ends the stage only for a `reviewer`: at its first `result`. An `implementer` or a `fixer` runs to the CLI's exit; for every role `timeout_seconds` ends the stage. Then `systemctl stop` ends every process in the unit, the stream is read to its end (a last line without a newline included), and `systemctl is-active` must answer a state that is not `active`, `activating`, `deactivating`, `reloading` or `refreshing`; anything else, a failed call included, makes the state `failed` with `unmet: stop`.

States, from the result the stage is judged by: the LAST `result` whose answer is nonempty, not an error or a cut, and meets `expect.first_line` when one met it (a later result does not unmake it), else the last `result` seen (a reviewer has only its first): `ok` (a non-error result whose text is nonempty and meets `expect`); `no_result` (a non-error result that is empty or does not meet `expect`; `unmet` names which: `answer`, `file`, `first_line`; a nonempty answer is still saved); `cut` (an error result with a rejected rate-limit event, API status 429 or the usage-limit text, or a non-error result with the usage-limit text and a rejected rate-limit event; no answer is written); `timeout` (the bound ended the stage, for every role, even when a result is read during the shutdown; the last answer, if any, is saved and `expect_met` says whether it met `expect`); `failed` (the CLI ended without a result event, an error result that is not a cut, the launch failed, the stop was not confirmed, or a write failed: `unmet` `result_event`, `launch`, `stop` or `write`). A refusal has no record. The answer is the result text, byte for byte. `expect.file` is met only by a nonempty file whose digest or modification time differs from `expect_file_before`: the file as it was before THIS run's launch. Only a resume that continues the same unfinished stage (the earlier record has the same role and a state other than `ok`) takes the earlier record's `expect_file_before` instead; a fixer over an implementer always takes its own. A fresh run cut before it did any work (the stream holds no `tool_use` block, so no tool ran) is run again, within the same run and its record, on the next of `next_accounts` with a new session; a resume never changes account.

The record `<out>.run.json`: `role, runtime, model, effort, account, session, cwd, unit, prompt{path,sha256}, answer{path,sha256}|null, start, end|null` (UTC, RFC 3339), `state, turns, tokens{input,output,cache_read,cache_creation}` (sums over the `result` events; null where the CLI did not report), `permission_denials` (`tool_name` and `tool_use_id` as reported, over all results), `results` (the number of `result` events read), `expect_met` (null without a result), `expect_file_before` (`{path, sha256, modified}` or null), `unmet`, `resumes` (the earlier record's path, or null). No prompt or answer text. `ci/host-run-scope.sh` exercises the real unit on a host with systemd.

## The gate

`ap gate < request.json` waits on ONE pull request's head until its required checks and the automatic review are done, and prints one JSON answer on stdout. It reads GitHub through `gh api graphql` (the caller's `gh` login) and decides nothing a person decides: it resolves no thread, posts no status, merges nothing and calls no store. Its only writes are `gh pr ready` on a draft (once) and the one toggle below.

The request: `repository` (`owner/name`), `pull_request` (number), optional `head` (40 hex: the head the caller expects), `autoreview` (the login whose comment carries the marker, default `chatgpt-codex-connector[bot]`; `false` for a repository without an automatic reviewer), `check_minutes` (default 90) and `autoreview_minutes` (default 40), each 1 to 1440. Another member, or a malformed one, refuses `invalid_input` with exit 2 before any read: `{"error":{"code","field","detail"}}` on stderr.

It reads every 30 seconds. The head it waits on is the one GitHub reports at the first reading. In order:
1. Merged or closed (at any reading) ends `merged` or `closed`. A caller's `head` that differs from GitHub's ends `head_differs` (`expected` names the caller's). A head that moves later ends `head_moved` (`head_now`). A draft is marked ready once; a failed mark ends `ready_failed`.
2. The required checks, judged at every reading, also while the gate waits for the review: every entry on the head's commit whose `isRequired` is true, over every page (a reading that could not be completed is no reading). One that completed with any conclusion but `SUCCESS`, or a status that is not `SUCCESS` or pending, ends `check_failed` with `failed_check: {name, state, run}`. None yet, or one pending, waits; at the bound of the phase (`check_minutes`, or `autoreview_minutes` once the review wait began) it ends `check_timeout`. A repository without a required check ends `check_timeout`.
3. The automatic review (skipped with `autoreview: false`): the marker `codex-review-state:v1 {…}` counts only in a comment of that login (a bot's GraphQL login gets `[bot]`, so a user of the same name does not count); the last such comment's `code_review` is read, from the last occurrence in that comment that parses. The marker is looked for in the last 100 comments. When the checks have passed and it names another commit (or none), the gate marks the pull request draft and ready ONCE, because a push to a ready pull request is not reviewed; not when the gate itself just marked a draft ready. It ends `passed` when `commit_sha` is the head and `status` is `completed`, and also at `autoreview_minutes`: the time limit passing is not a failure, `autoreview.state` says `timeout`.

The answer: `end`, `repository`, `pull_request`, `head`, `marked_ready`, `checks` (`[{name, state, run}]`), `autoreview` (`login`, `state`: `completed`, `timeout`, `waiting` when the gate ended before it, or `off`; `commit`, `has_unresolved_findings` as the marker says, `toggled`), `threads` (every unresolved review thread: `id`, `path`, `line`, `author`, `answered`: its last comment is not the automatic reviewer's; `null` when it could not be read), and `failed_check`, `expected`, `head_now`, `left_draft` (a toggle whose ready mark failed three times) or `unreadable` with their ends. `answered` is true for every thread when `autoreview` is `false`. `passed` with exit 0 also covers a review that reached its time limit and a review with unresolved findings: the caller reads `autoreview.state`, `has_unresolved_findings` and `threads`. Exit by end: 0 `passed`; 10 `merged`, `closed`, `head_differs`, `head_moved`; 11 `check_failed`, `check_timeout`; 12 `ready_failed`, `github_unreadable` (`unreadable` names the cause: `not_found`, `no_login` or `gh_not_started` end at the first reading; `no_answer` when a reading still failed at the bound of the phase).

## One unit

`ap unit < request.json` runs one unit of work from the page to a judged head with nobody in between, and ends with ONE record `<out>/unit.json`. It checks the request, writes the record with state `running`, starts the chain in its own transient unit `ap-unit-<16 hex of sha256(cwd)>.service` under `agents.slice` (`RuntimeMaxSec` the chain's bound plus 300 s; `ExecStopPost` is `ap unit finish`) and prints `{"record","unit"}` with exit 0. The unit's name comes from the worktree, so a second chain on one worktree is refused (`chain_active`). The chain does not depend on its caller: a killed caller changes nothing. Every stage is an `ap run` with `bind` set to the chain's unit, so nothing of a chain outlives it. `ap unit wait < {"record", "timeout_seconds"}` (default 2400, 1 to 86400) blocks until the record's state is not `running` and the chain's unit is not active, and prints the record with the end's exit code; exit 8 and the record as it is when the time passed first. `ap unit finish <out>` (systemd runs it after the chain's process ended) gives a record that still says `running` the end `bound` when the unit reached its `RuntimeMaxSec`, else `stopped`.

The request: `repository` (`owner/name`), `cwd` (the worktree; its branch is the pull request's head branch), `out` (an existing directory outside the worktree), `implementer` and `reviewer` (each `account`, `model`, `effort`, `prompt`, optional `permission_mode` (default `auto`) and `timeout_seconds`), optional `bound_minutes` (the chain's bound, default 480, 10 to 1440), `gate` (`autoreview`, `check_minutes`, `autoreview_minutes` as `ap gate` takes them), `continue` (the path of an earlier chain's `unit.json`; `out` is its directory) and `findings` (with `continue` only: a JSON file, an array of at most 20 objects of the verdict's blocking form whose `id` matches `^L[0-9]{1,2}$`, the Lead's findings). Refusals, exit 2, before any effect: `invalid_input`, `cwd_missing`, `branch_missing` (the worktree has no branch), `prompt_unreadable`, `findings_unreadable`, `out_in_worktree`, `out_exists` (a fresh request whose `unit.json` exists), `record_missing`, `record_differs` (a continuation of another worktree or repository), `chain_cannot_continue` (the earlier end is `merged_or_closed`), `chain_active`, `systemd_absent`. A launch of the chain that fails puts the record back as it was (removed for a fresh request) and exits 3.

The sequence. implementer (a fresh session on the implementer's account; the Lead's prompt with the facts of `unit/implementer.md` appended) → the reading of the pull request (the open one whose head branch is the worktree's branch and whose head repository is `repository`) → the gate (`ap gate`'s code on the head read) → reviewer (a fresh session on the reviewer's account; the Lead's prompt with the facts of `unit/reviewer.md`; structured output by `unit/verdict.schema.json`) → when there is something to dispose (a blocking finding, a note, an unresolved thread the gate reports as not answered): fixer (a resume of the implementer's session; `unit/fixer.md` with the items; structured output by `unit/fix.schema.json`) → the reading → the gate → the judgment (a resume of the reviewer's session; `unit/judge.md` with the dispositions, the command that prints the fixer's diff and the list of test files that diff changed or deleted: a path with a directory `tests`, `test` or `fixtures`, a file name that begins `test_` or `test-` or contains `_test.`, `.test.` or `_tests.`, or a file from which the diff removes a line containing `#[test]` or `assert`) → repeat or end. The reading follows every stage: the pull request merged or closed, or a head that is not the one the last stage named (the verdict's head, the fixer's head), ends the chain. A gate that ends `check_failed` sends the failed check (name, run) to the fixer and comes before any reviewer. A verdict is `accept` with no blocking finding or `repair` with at least one, and names the head the gate passed; anything else is no verdict (the stage's state is `no_result`). A fixer's answer has one disposition per item id, or it is `no_result`.

Rounds. A round is fixer → gate → judgment. A round started by a blocking finding or by a failed required check uses one of two. A round started only by notes or threads happens at most once, after the first verdict, and uses none. After a judgment that says `accept` the chain ends; its notes, the refused notes and threads and the threads the last gate reports as not answered are listed in `left` and start nothing. A refused blocking finding (of the reviewer, of the Lead or a failed check) ends the chain.

A usage limit. A stage whose state is `cut` names `resets_at` in its record (the stream's `resetsAt` of the rejected rate-limit event). When it lies inside the chain's bound the chain sleeps to it plus 60 s and resumes that stage's session on its own account (`unit/continue.md`); else it ends `usage_limit` with `resets_at`.

Continuation. A request with `continue` continues from what the record shows: it rewrites the SAME record (state `running` from the continuation's first step; `continued` gets the time), keeps the rounds used and does not repeat a stage whose state is `ok`. A stage that is not `ok` is resumed (`unit/continue.md`). The first round a continuation starts is the Lead's and uses none of the two: after `blocking_left` or `check_failed` it disposes what was left; after `accepted` without findings the chain reads the pull request, gates its head and judges it. `findings` go to the fixer as blocking findings before any other step.

The ends, closed (state = end, exit = code; `end_from` names the stage (`stage`, its index), the gate (`gate`) or the reading it came from, and the detail):

| End | Exit | When |
| --- | --- | --- |
| `accepted` | 0 | on the head the record names the gate ended `passed`, the last verdict of the reviewer's session says `accept` and names that head, and no stage ran after it; `autoreview` says `completed`, `timeout` or `off` |
| `blocking_left` | 20 | a blocking finding is left after two rounds |
| `blocking_refused` | 21 | the fixer refused a blocking finding |
| `check_failed` | 22 | a required check still fails after the rounds, or the gate ended `check_timeout` |
| `no_pull_request` | 23 | no open pull request for the branch |
| `merged_or_closed` | 24 | the pull request was merged or closed during the chain (cannot continue) |
| `head_moved` | 25 | a head that is not the last stage's: someone else's push, a fixer that did not push |
| `stage_failed` | 26 | a stage ended `failed`, `no_result` or `timeout`, or `ap run` refused it (`refused`) |
| `usage_limit` | 27 | a cut whose reset lies past the chain's bound, or none was reported |
| `bound` | 28 | the chain's own bound |
| `github_unreadable` | 29 | a reading or the gate could not read GitHub (also `ready_failed` of the gate) |
| `stopped` | 30 | the chain's unit was stopped or killed before the chain wrote its end |

The record `<out>/unit.json`, written before the first stage and replaced at every step: `repository, cwd, branch, unit, state, exit, step` (the current step: `implement`, `read`, `gate`, `verdict`, `fix`, `sleep`, or null), `next` (the step a continuation takes), `end_from, pull_request, base, heads` (every head in the order read), `stages` (per stage: `role` (`implementer`, `reviewer`, `judge`, `fixer`), `record` (its `ap run` record), `state, turns, tokens, head`), `gates` (per gate: `head` and the `answer`), `verdicts` (per verdict: `stage, head, verdict, blocking, notes`, the last two counts), `dispositions` (`stage, id, kind, disposition, commit`), `rounds_used, notes_round_used, left` (`notes`, `threads`, `refused`), `autoreview, tokens` (the sum over the stages), `start, finish, continued`. No prompt text and no finding text: those stay in the stages' files in `out` (`<n>-<role>.md`, its prompt `<n>-<role>.prompt.md`, the items `<n>-items.json`). The chain calls no store, posts no status, merges nothing, resolves no thread and reads no ruleset; its only writes outside `out` are those of `ap gate` and of the stages.

## One review

`ap review start|close|accept|revoke` reads one JSON object on stdin and writes
one JSON answer; errors use the same stderr envelope and exit codes as `ap work`.
Every path is absolute. Optional `store` forwards `--store` to every `ont` call.
The review object owns the verdict; `lead-review` is a derived commit status.
The status is required by a ruleset only after the Lead applies it.

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

`accept` requires a closed `packet` or `integrated` review with verdict `accept`,
repository, pull request, head, base and an unchanged report. Every named run
must belong to the review task and have a nonempty write set. It verifies the open pull request targets `main`
at the review head, fetches `origin main`, checks the merge base and compares
changed paths to the union of those write sets, using `git --no-replace-objects`
and `diff --no-renames --ignore-submodules=none --no-relative --name-only -z`. Paths are read NUL-separated,
so non-ASCII names are compared unquoted, including names containing newlines or
quotes. A path that is not valid UTF-8 ends with `process_failed` (field `git`).
A moved file must have both its
old and new paths in that union. A pattern matches its exact
path; a trailing `/**` matches descendants, not the directory itself or another
name with the same prefix. The caller must be outside the task's `wt/`.
Only then does it post `lead-review=success` for that head and return
`status: accepted`, `head` and the number of `paths`. Repeating it posts again.
Example stdin for `ap review accept`:

```json
{"review":{"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"review-key"},"worktree":"/home/palantirkc/work/example/wt/repo"}
```

`revoke` reads a review with a repository and head, then posts
`lead-review=failure` with `revoked: <reason>`. The reason must contain text and
fit within 100 UTF-8 bytes. It returns `status: revoked` and `head`. Example
stdin for `ap review revoke`:

```json
{"review":{"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"review-key"},"reason":"Report missed a changed path"}
```

Refusals (local checks precede writes and status posts):

- `invalid_input`: malformed object, id, path, run list, reason or explicit verdict; field identifies the member.
- `review_prompt_unreadable`: prompt is not a readable regular file below the work root.
- `review_head_differs`: pull request is closed, has another head, or (at accept) targets another branch.
- `review_run_unknown`: a start run cannot be read as the exact current object.
- `review_run_of_other_task`: a run supplied to `start` or `accept` belongs to another task.
- `review_already_closed`: `start` repeated after its review closed; repeat with a new `attempt`.
- `store_refused`: the store refused an attempted action; detail carries its codes; no retry occurs.
- `review_report_unreadable`: close report is unreadable, outside the work root or larger than 1 MiB.
- `review_report_lacks_verdict`: the report's first line is not one of the two exact verdict lines.
- `review_not_accepting`: required review state or evidence is absent; detail names the first failing member.
- `review_report_changed`: report is unreadable or its current bytes have another digest.
- `review_runs_required`: an accepting review needs a nonempty array of runs.
- `review_run_lacks_write_set`: a run is unreadable or lacks a nonempty write set; detail gives its key.
- `review_base_differs`: fetched merge base differs from the review base; detail gives the merge base.
- `review_path_outside_write_set`: changed paths escape the union; detail contains at most ten paths and the total count.
- `review_accept_from_worktree`: caller's working directory is under the reviewed task's `wt/`.
- `store_review_unreadable`: the exact review's current observation cannot be read.
- `store_answer_unreadable`: an action reply is malformed or does not confirm the expected create.
- `process_failed`: an external command failed or returned unreadable Git output; no automatic retry occurs.
- `io_error`: stdin or the caller's working directory cannot be read.
- `input_too_large`: stdin exceeds 1 MiB.
- `usage`: exactly one supported verb is required.
