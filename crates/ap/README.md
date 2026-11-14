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

`ap run < request.json` runs ONE headless Claude Code session (a stage: implementer, reviewer or fixer) in a transient systemd scope `ap-run-<uuid>.scope` under `agents.slice` and returns when nothing of it is alive. It prints the run record on stdout; exit 0 `ok`, 75 `cut`, 3 any other end. A refusal prints `{"error":{"code","field","detail"}}` on stderr with exit 2 and starts nothing. It reads no role table, calls no store, Herdr or GitHub.

The request: `role` (`implementer`, `reviewer`, `fixer`), `account` (the Claude configuration directory, required), optional `next_accounts`, `model`, `effort`, `permission_mode` (one of Claude Code's: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`), `cwd`, `prompt` (a regular file, at most 1 MiB, given to the CLI on stdin), `out` (the answer's path), and optional `timeout_seconds` (1 to 86400; default implementer 5400, fixer 3600, reviewer 3300), `expect` (`file`: a path that must exist nonempty; `first_line`: a pattern the answer's first line must match, where `*` matches any run of characters), `resume`, `continue_dirty`, `integration`, `mcp_config` (files; the only MCP servers loaded), `env` (extra variables; a name beginning `HERDR_` or `CLAUDE_` refuses `env_forbidden`), `runtime` (`claude`; `codex` refuses `runtime_not_built`). Every path is absolute; another member refuses `invalid_input`.

Refusals before any effect: `invalid_input`; `runtime_not_built`; `account_forbidden` (the directory that is exactly `/root/.claude` by path components, also after resolving, for an implementer or a fixer, and for a reviewer without `integration: true`); `account_missing`; `cwd_missing`; `prompt_unreadable`; `out_unwritable`; `out_named_in_prompt` (the prompt contains the out file's name); `worktree_dirty` (a fresh implementer run in a Git worktree with changes, unless `continue_dirty`); `resume_record_missing`, `resume_account_differs`, `run_still_active` (the record says `running` and its unit is active); `env_forbidden`; `systemd_absent` (systemd is not this host's init). A refusal writes a record with state `refused` and `refusal: {code, field}` only where `<out>.run.json` does not exist, so a resumable record is never replaced.

The run: an earlier `<out>` is moved to `<out>.prev`. A session id is chosen (or taken from the record on `resume`), and `<out>.run.json` is written with state `running` BEFORE the launch. The launch is `systemd-run --scope --quiet --collect --unit=… --slice=agents.slice -p RuntimeMaxSec=<timeout + 10>s -p TimeoutStopSec=10s -- claude --print --output-format stream-json --verbose --model … --effort … --permission-mode … --permission-prompts none --settings {"disableAllHooks":true} --strict-mcp-config [--mcp-config …] (--session-id | --resume) <session>`. Its environment is only `PATH HOME USER LOGNAME SHELL LANG LC_ALL TERM TMPDIR TZ` of the caller (when set), the request's `env`, and `CLAUDE_CONFIG_DIR=<account>`: no hook (of the account, the host, a project or a plugin) and no MCP server the request does not name. stdout goes to `<out>.stream.jsonl`, stderr to `<out>.err`. `RuntimeMaxSec` is the unit's own bound: it ends the stage when `ap run` is gone.

The end: the first `result` event in the stream, the CLI's exit, or `timeout_seconds`, whichever comes first; then `systemctl stop` ends every process in the unit. A print session that left a background shell emits a first `result`, waits for the shell, and may emit a second one; the stage ends at the first, and `others_at_stop` counts the processes other than the CLI that were alive in the unit at the stop.

States: `ok` (a non-error result whose text is nonempty and meets `expect`); `no_result` (a non-error result that is empty or does not meet `expect`; `unmet` names which: `answer`, `file`, `first_line`; a nonempty answer is still saved); `cut` (an error result with a rejected rate-limit event, API status 429 or the usage-limit text; no answer is written); `timeout`; `failed` (the CLI ended without a result event, an error result that is not a cut, or the launch failed; `unmet` `result_event` or `launch`); `refused`. The answer is the result text, byte for byte. A fresh run cut before it did any work (the stream holds no `tool_use` block, so no tool ran) is run again on the next of `next_accounts` with a new session; a resume never changes account.

The record `<out>.run.json`: `role, runtime, model, effort, account, session, cwd, unit, prompt{path,sha256}, answer{path,sha256}|null, start, end|null` (UTC, RFC 3339), `state, turns, tokens{input,output,cache_read,cache_creation}` (null where the CLI did not report), `permission_denials` (`tool_name` and `tool_use_id` as reported), `others_at_stop, unmet, refusal`. No prompt or answer text. `ci/host-run-scope.sh` exercises the real unit on a host with systemd.

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
