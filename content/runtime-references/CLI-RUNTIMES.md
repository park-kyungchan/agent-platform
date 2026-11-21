# The coding CLIs on this host: files, commands and what a session loads

Read this when you start, run without a terminal UI, resume or configure a CLI (permissions, MCP servers, hooks, compaction, events, usage limit, account), register an MCP server, or must check what a session loaded. The CLIs are Claude Code (`claude`), Codex (`codex`), Muse (`muse`), Antigravity (`agy`) and Grok Build (`grok`). A command below shows the mechanism. It is not permission to launch, authenticate, update or change configuration: that comes from the task's STATE.

| I need | Section |
| --- | --- |
| The file a CLI reads for instructions, MCP servers or skills | 1 |
| The command that starts, runs once or resumes a session | 2 |
| To show what a session loaded | 3 |
| The compaction control | 4 |
| The rules for MCP servers | 5 |
| Hooks; skill sources; a CLI's private memory | 6; 7; 8 |
| A stage or a unit of work on this host (`ap run`, `ap unit`) | 9 |
| Claude Code, Muse Code or the Antigravity CLI without a terminal UI: the run, permission modes, what loads and how to keep it out of one run, compaction, events, resume, usage limit, account | 10, 11, 12 |

## 1. Files per CLI

| CLI | Shared-instruction pointer | Custom MCP servers: file / section | Hindsight client name | Skills it lists |
| --- | --- | --- | --- | --- |
| Claude Code | `~/.claude/CLAUDE.md` (a file that imports the shared file with an `@` line) | `~/.claude.json` / `mcpServers` | `claude-code` | `~/.claude/skills/` (symlinks to the shared skills of section 7 that the user chose) |
| Codex | `~/.codex/AGENTS.md` (symlink) | `~/.codex/config.toml` / `mcp_servers` | `codex` | `/root/.agents/skills/` directly |
| Muse | `~/.config/muse/AGENTS.md` (symlink) | `~/.config/muse/settings.json` / `mcpServers` | `muse` | `/root/.agents/skills/` directly |
| Antigravity | `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md` (symlinks) | `~/.gemini/config/mcp_config.json` / `mcpServers` | `antigravity-cli` | `~/.gemini/config/skills/` (symlinks) |
| Grok Build | `~/.grok/AGENTS.md` (symlink) | `~/.grok/config.toml` / `mcp_servers` | `grok-build` | read `SKILL.md` by path |

Every pointer resolves to `/root/.agents/AGENTS.md`. If a CLI does not show that text in a session, repair its pointer; do not paste the text into a prompt as a permanent fix and do not create a copy.

## 2. Start, run without a terminal UI, resume

| CLI | Interactive | Headless (one prompt, then exit) | Resume an exact session |
| --- | --- | --- | --- |
| Claude Code | `claude --model <model> --effort <effort>` in the assigned checkout | `claude --print <prompt>` (section 10; a stage: `ap run`, section 9) | `claude --print --resume <session-id> <prompt>` (section 10) |
| Codex | `codex --no-daemon -C <checkout> -m <model> -c model_reasoning_effort=<effort>` | `codex --no-daemon exec -C <checkout> --json <prompt>`; a stage of a unit is started by `ap run` with `runtime: codex` (`crates/ap/README.md`, "One stage") | `codex --no-daemon resume <session-id>` |
| Muse | `muse --model <model> --reasoning-effort <effort>` | `MUSE_NO_AUTO_UPDATE=1 muse exec --json --model <model> --reasoning-effort <effort> --contributor-mode off --disable-reminders --session-id <new uuid> --workspace <checkout> --prompt-file <absolute TASK path>` (section 11) | `muse exec --session-id <the same id> --workspace <the same dir> <prompt>` (section 11; `muse resume <session-ref>` opens the terminal UI) |
| Antigravity | `agy --model <model>` | `agy --print <prompt> --output-format stream-json` (section 12) | `agy --print <prompt> --conversation <id>` (section 12) |
| Grok Build | `grok --cwd <checkout>` | `grok --cwd <checkout> -p <prompt> --output-format streaming-json` | `grok --cwd <checkout> --resume <session-id>` |

- The Lead of a task runs in the CLI its STATE names (skill `lead`); every CLI of this table may run a worker or a reviewer when the task's assignment file names it.
- Codex always runs with `--no-daemon` in a Herdr terminal. Do not replace this with an app-server workflow.
- Muse: `MUSE_NO_AUTO_UPDATE=1` stops the updater from running during the command. `--disable-reminders` removes two built-in reminder agents and the extra model requests they make; use it for workers. `muse exec --provider echo` (without `--model`) runs a session with no model call, which is the cheap way to inspect what a session loads. Muse's session record does not store the effort of the main request.
- Resume by exact session ID, not `--last`, when several sessions exist. A resumed session keeps older instructions and tool definitions: read the task's STATE again before any effect.
- If a flag is rejected or missing, read that CLI's installed `--help`. Do not run every CLI's help for every task.
- Looking at a CLI's help, configuration or session list does not release a hold on logging in or running a model with it; the hold is in the task's STATE.
- Update a CLI only with its own updater and only when the task authorizes it. Do not copy an installation recipe.

## 3. What a session loads, and how to check it

"Loaded" means the session's own record shows the text.

| CLI | Session record | Limits you must know |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/<cwd with each / and . as ->/<session-id>.jsonl` (`$CLAUDE_CONFIG_DIR/projects/<cwd with each / and . as ->/<session-id>.jsonl` when that variable is set) | A subagent may not receive project instruction files. Excluding ToolSearch from `--tools` loads every MCP tool schema at start |
| Codex | `~/.codex/sessions/` (`rollout-*.jsonl`) | Outside a Git repository Codex reads project instruction files in the cwd only, not in parent directories. The user-level pointer still loads. Do not create a fake repository to change this |
| Muse | `~/.local/share/muse/sessions/<yyyy>/<mm>/<dd>/<session-id>/session.jsonl` | Without `~/.config/muse/AGENTS.md` Muse falls back to `~/.claude/CLAUDE.md` and shows its `@` line as literal text |
| Antigravity | `~/.gemini/antigravity-cli/conversations/<conversation-id>.db` | A native rule include is written `@[label](path)`; a bare `@filename` is a reference, not an include |
| Grok Build | `~/.grok/sessions/` (not yet inspected on this host) | Imports of other CLIs' MCP servers are off: `[compat.claude] mcps=false`, `[compat.cursor] mcps=false` |

- A skill has three separate states: listed in the catalog (name and description only), body read, and applied. A listing does not show that the body was read.
- A handoff therefore names the files to read first, and the receiver names them in its first output. A child agent does not inherit the parent's activated skills.
- Codex plugins: `plugins.<plugin>.enabled` applies to local-marketplace entries only; `apps.<id>.enabled` and `skills.config` are separate controls.
- A session started with restricted tools or flags shows a restricted inventory: do not report it as that CLI's default.
- A configuration change does not replace what a running session already loaded. Check with a new session of your own; do not restart someone else's.

## 4. Context compaction

The target window is a task decision and is written in the task's STATE. A configured value is not a measured ceiling: read back the effective value, and for Muse read the model's window from its catalog before computing a fraction. The native control per CLI:

| CLI | Control |
| --- | --- |
| Claude Code | `CLAUDE_CODE_AUTO_COMPACT_WINDOW` and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` in `~/.claude/settings.json` `env`; read back with `/context` in the terminal UI. For one run: section 10 |
| Codex | `model_context_window` and `model_auto_compact_token_limit` in `~/.codex/config.toml` |
| Muse headless | `--context-compaction-soft-threshold` and `--context-compaction-hard-threshold`, fractions of the model's window, soft below hard (section 11) |
| Antigravity | Automatic: older turns become checkpoint summaries; no flag or setting for the point was found (section 12) |
| Grok Build | Not qualified on this host; qualify before relying on a ceiling |

## 5. MCP servers

- Custom servers shared by all CLIs: `hindsight`, `academy_operations`. Each is registered directly in every CLI's own file from section 1, with that CLI's client name where the server asks for one. The engine's server `ont` (`ont mcp`) is registered only in the CLIs where the task that owns the store registered it: read the CLI's file, and use the command `ont` where the server is absent (`ONTOLOGY-USE.md`).
- Another CLI's personal configuration or plugin cache is never the registration source. Built-in, marketplace, account-synced and plugin servers stay with the CLI that installed them; do not copy them into the custom set and do not edit their caches.
- Registered, enabled, handshake succeeded, tool visible and effect allowed are five different facts. Check the one your next action needs.
- `academy_operations` works on one academy account with one lock: only the current lesson owner named in the Academy task STATE performs its operations. A visible write tool is not write permission. Do not reach the academy source by direct HTTP, browser or shell instead of the server.
- The Hindsight integration is installed in `/root/.hindsight/coding-agents`; its settings file `/root/.hindsight/coding-agent.json` contains a credential: read fields inside a process, never print the file.
- `academy_operations` keeps the launch command, release and credential files its own service defines. Do not put a not-yet-accepted Rust binary in its place.
- When the user excludes a feature for one CLI, use that CLI's own control, keep the previous bytes and read the result back in a new session of your own. That is not uninstalling it for the account.
- When reading a configuration file to reconcile a difference, read key names and the command and arguments only. Never print credential values.

- Installing source changes no registration and no running session.

## 6. Hooks

Hook wiring differs per CLI and changes with CLI and plugin versions, so read the actual configuration instead of assuming it: Claude Code `~/.claude/settings.json` `hooks` plus its installed plugins, Codex `~/.codex/hooks.json`, Muse `~/.config/muse/settings.json` `hooks`, Antigravity `~/.gemini/config/hooks.json`, Grok `~/.grok/hooks/`. A CLI with the `hindsight` MCP server but no Hindsight hook gets no automatic memory injection and no automatic session retention; the agent calls the tools itself. A hook name does not show that injection or retention happened.

Do not export a hook-disable variable and do not remove an MCP server to isolate a worker. Memory isolation uses the Hindsight path override described in `CONCURRENT-WORK.md`. To keep hooks and registered MCP servers out of ONE run, give that run its own flags (sections 10 to 12, `ap run` in section 9): they change no saved configuration and export nothing, so they are not what this paragraph forbids.

## 7. Skills

| Skill | Source |
| --- | --- |
| `lead`, `herdr`, `herdr-hermes`, `report` | `agent-platform` `content/skills/` |
| `academy-lesson-day`, `academy-dailyrecord-authoring`, `academy-prestudy-review` | `education-platform` `content/skills/` |
| `lead-monitor` | The user's own; installed by the user |
| `hindsight-coding-agent` | Generated by the Hindsight integration; removed from every CLI while Hindsight is off, with its hooks |
| Account-synced, marketplace and built-in skills | Their CLI or vendor; changed only through that CLI |

Installed shared path: `/root/.agents/skills/<skill>/SKILL.md`. Change a skill in its source repository through review, then install it (`BUILD-AND-DELIVERY.md` section 6); never edit only the installed copy. Frontmatter fields other than `name` and `description` do not behave the same in every CLI.

## 8. Private memory of each CLI

| CLI | Location | Loaded when |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/<cwd with slashes as dashes>/memory/` (`MEMORY.md` index plus one file per entry) | Every session started in that cwd |
| Muse | `~/.local/share/muse/memory/personal/` and `~/.local/share/muse/memory/projects/<cwd name and hash>/` | Sessions in that cwd |
| Antigravity | `~/.gemini/antigravity-cli/brain/` | Per conversation |
| Codex, Grok Build | None found on this host | — |

This memory is private to one CLI and one directory, so no other agent sees it and nothing reviews it. The rule for what may go there is in `/root/.agents/AGENTS.md` section 5. When an entry there contradicts the task's STATE or the shared instructions, STATE and the shared instructions win; delete the entry.

## 9. A stage or a unit of work on this host

- A stage (one implementer, reviewer or fixer session of Claude Code, of the Codex CLI, of Muse Code with `runtime: "muse"`, or, for a reviewer, of the Antigravity CLI with `runtime: "agy"` in the request; `crates/ap/README.md`, "One stage", the parts headed A stage on Muse Code and A stage on the Antigravity CLI) is started with `ap run < request.json`: one headless run in its own systemd scope, with a time bound (`timeout_seconds`), its record `<out>.run.json` that another command reads, and, for Claude Code, the flags of section 10 set for you (no MCP server of the account, no hook, no auto-memory, no skill unless asked). A unit (implementer, gate, verifier, acceptance) is started with `ap unit`. The contract of both: `agent-platform` `crates/ap/README.md`, sections "One stage" and "One unit".
- The raw commands of sections 10 to 12 are for what those two commands do not cover: a Muse Code run that is not a stage, an Antigravity CLI run that is not a reviewer stage, a probe of what a session loads, a reader of a test.
- A Claude stage states its compaction window in the request's `compact_window` (`crates/ap/README.md`, "One stage"): an integer from 100000 to 1000000, carried as `CLAUDE_CODE_AUTO_COMPACT_WINDOW` in the `env` of the CLI's `--settings` value; without it the account's settings decide. A stage of Codex, Muse Code or the Antigravity CLI refuses the member: `invalid_input` at the field `compact_window` (`crates/ap/src/run.rs`, `unbuilt`). A request's `env` still refuses a name beginning `CLAUDE_` (`env_forbidden`).

## 10. Claude Code without a terminal UI

Every row is for ONE run and changes no saved file. The account is `~/.claude`, or the directory in `CLAUDE_CONFIG_DIR`.

| Need | Command or setting |
| --- | --- |
| The run | `claude --print --output-format stream-json --verbose --model <model> --effort <effort> --session-id <new uuid> "<prompt>"`, started in the working directory (another directory the tools may use: `--add-dir <dir>`). The prompt is the last argument or comes on stdin. `stream-json` with `--print` requires `--verbose`. Exit 0 on success, non-zero when the run fails, 143 on SIGTERM |
| A time bound | None of wall time. `--max-turns <n>` caps turns, `--max-budget-usd <amount>` the spend. Wrap the command in `timeout <seconds>`, or use `ap run` (section 9) |
| No permission prompts | `--permission-mode bypassPermissions`: every check is skipped; the few calls that would still prompt are denied in a `--print` run |
| A model decides each approval, no person | `--permission-mode auto`: a classifier model reviews each call instead of a person; a blocked call does not run and the run goes on. It needs a model that supports it; otherwise the session starts in manual mode |
| Only pre-approved calls | `--permission-mode dontAsk` (reads inside the working directories, plus `--allowedTools "<rule>"`); anything that would prompt is denied |
| A call needs an approval | With no mode given a `--print` run starts in manual mode and denies the call. `--permission-prompts none`: nobody answers, anything that would prompt is denied. Denials are listed in `permission_denials` of the `result` line |
| What a run loads from the account | Instruction pointer `CLAUDE.md`, skills `skills/`, MCP servers (`~/.claude.json` `mcpServers`, plugins, a project's `.mcp.json`), hooks (`settings.json` `hooks`, plugins), auto-memory (`projects/<cwd>/memory/`) |
| Keep the registered MCP servers out | `--strict-mcp-config` (only servers of `--mcp-config <file>` load) |
| Keep hooks and auto-memory out | `--settings '{"disableAllHooks":true,"autoMemoryEnabled":false}'` |
| Several settings for one run | One `--settings` whose JSON holds every key of the run: a second `--settings` replaces the first, so merge the keys (hooks, auto-memory and the `env` compaction window) into one JSON instead of passing two |
| Keep skills and slash commands out | `--disable-slash-commands` |
| Keep the project's settings files out | `--setting-sources user` |
| Keep everything of the account out, `CLAUDE.md` included | `--safe-mode` (skips `CLAUDE.md`, skills, plugins, hooks, MCP servers, auto-memory; login, model and built-in tools work). `--bare` also skips `CLAUDE.md` but reads no subscription login: it needs an API key |
| The compaction window of one run | `--settings '{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"<tokens>"}}'`: the value counts tokens and beats the same variable in the account's `settings.json`, the `--autocompact <tokens>` flag and the `autoCompactWindow` setting. `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` (a percentage of that window) can only lower the point. The window is capped at the model's context window |
| Read the window back | No documented read-back in a headless run (`/context` in the terminal UI). `--debug-file <path>` writes a line `autocompact: … effectiveWindow=<n>` (seen on this host, not documented) |
| Session id in the events | `session_id` of the first line `{"type":"system","subtype":"init"}`, and of every later line |
| Final answer | `result` of the line `{"type":"result"}`, with `is_error`, `num_turns`, `permission_denials` |
| A tool call | A content block `{"type":"tool_use","name":…,"input":…}` in a line `{"type":"assistant"}` |
| Usage | `usage` (`input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`) and `modelUsage` of the `result` line |
| Resume | `claude --print --output-format stream-json --verbose --resume <session-id> "<next prompt>"`, in the same working directory: the session id stays the same and the session record grows |
| At a usage limit | The session ends with the message `You've hit your <window> limit · resets <time>`; in the session record that last line has `"error":"rate_limit"` and `"apiError":"usage_limit_reached"`. `ap run` counts such a run as `cut` (exit 75) |
| The remaining quota | Each `rate_limit_event` line: `rate_limit_info.status` (`allowed`, `allowed_warning`, `rejected`) and per window of `rate_limit_info.unifiedWindows` (`five_hour`, `seven_day`) `utilization` (the fraction used) and `resetsAt` (epoch seconds). `overageStatus` is not the limit's status |
| Another account or configuration directory | `CLAUDE_CONFIG_DIR=<dir> claude …`: settings, session records and plugins are read from and written under that directory |

## 11. Muse Code without a terminal UI

Every row is for ONE run and changes no saved file. The configuration directory is `~/.config/muse`.

| Need | Command or setting |
| --- | --- |
| The run | `MUSE_NO_AUTO_UPDATE=1 muse exec --json --model <model> --reasoning-effort <effort> --contributor-mode off --disable-reminders --session-id <new uuid> --workspace <dir> --prompt-file <absolute path>` (or the prompt as the last argument instead of `--prompt-file`; not both). Pass `--contributor-mode off` because the configuration directory's `settings.json` holds `contributor_mode_enabled: true`, and that file is the user's and is not changed; the flag holds for this run only. `--json` prints the events as JSON lines on stdout; stderr gets `muse:` lines. Exit 0 when the turn completes, 1 when it fails or is cancelled, 2 for a usage error, 130 or 143 on a signal |
| The serving model | A `task_lifecycle` line's provider facet names the model that served the call: an entry of `payload.event.details.facets` whose `detail.kind` is `provider`, with the model in `detail.model`. `ap run` stops a Muse Code stage whose serving model is not the requested one, and the stage's record names that model in `served_by` (`crates/ap/README.md`, "One stage") |
| A time bound | None of wall time. `--max-model-steps <n>` caps model steps (the run then exits 1). Wrap the command in `timeout <seconds>` |
| A model decides each approval, no person | The default: approval mode `on-request` with `--approval-judge on` (the profile Auto-review). A call the policy holds goes to the judge model, which decides it; the event of that call shows `"policy_decision":"allow:llm_judge"`. The judge's model is the CLI's choice: no flag names it, and the session record's `automated_review_completed` event names the model that judged |
| No approval prompts | `--disable-approval` (the sandbox stays), or `--approval-mode never` (nothing stops for approval; the sandbox alone contains what runs). `--disable-sandbox` removes the sandbox and keeps approval. Unlike `--yolo`, these flags do not trust the workspace; use them only for a run that must also go without approval and sandbox. They are not what keeps MCP servers out: that is the row "Keep the registered MCP servers out". Each of `--yolo` and `--trust-workspace` writes nothing for its trust: it adds no workspace to `trust.json` and changes no file of a copied configuration directory |
| No approval and no sandbox at all | `--yolo`. It also trusts the workspace, so the MCP servers of the home start (next rows) |
| A call needs an approval | With `--approval-judge off` (or when the judge is unavailable) a held call waits for a person; a headless run has none, so the action stays unresolved |
| What a run loads from the home | Instruction pointer `~/.config/muse/AGENTS.md`, skills (`~/.config/muse/skills`, `~/.agents/skills`, also `~/.claude/skills`), MCP servers `~/.config/muse/settings.json` `mcpServers`, hooks (`settings.json` `hooks`), a memory snapshot (`~/.local/share/muse/memory/`) |
| Keep the registered MCP servers out | Run in a workspace Muse Code does not trust: a new directory, without `--yolo` and without `--trust-workspace`. It starts none, and the session record says `MCP startup withheld <n> servers: workspace is untrusted.` Each of `--trust-workspace` and `--yolo` trusts the workspace and the servers start. In a trusted workspace no switch of one run keeps them out (`-c mcpServers…` is refused) |
| Keep the workspace's own rules, skills and hooks out | The same untrusted workspace: they are not loaded |
| Keep other items out | `--disable-reminders` (reminder agents), `--no-foreign-personal-context` (foreign personal rules and skills), `--disable-shell`, `--disable-web-tools`, `--no-session-log`. The instruction pointer, user hooks, single user skills and the memory snapshot have no switch of one run (`muse skills disable` is saved). On this host `--disable-write` made the run end `model … does not exist or you lack access`: leave it out |
| One setting for one run | `-c <key>=<JSON value>` (repeatable) |
| The compaction point of one run | `--context-compaction-soft-threshold <fraction> --context-compaction-hard-threshold <fraction>`: fractions of the model's context window, soft in (0, 1), hard in (0, 1], soft below hard. At soft a compaction starts in the background, at hard a blocking one runs. Compute: hard = target tokens / the window; soft a little below. The window is `context_limit` of the model's row in `~/.local/share/muse/model-catalog/*.json` (no command lists it). A hard point under the size of the startup prompt refuses the start and prints `startup prompt estimate <n> reaches hard threshold <n>`. The defaults and a read-back of a running session were not found |
| Session id in the events | `stream.id` of a line whose `stream.kind` is `session` (the id given with `--session-id`) |
| Final answer | `payload.text` of the line whose `payload.kind` is `run_terminal`; `payload.terminal` is `completed` or `failed`, with the cause in `payload.reason` |
| A tool call | Lines `payload.kind` `task_lifecycle`: `event.kind` `side_effect_intent` with `operation` (`tool:bash`) and `policy_decision`, then `started`, `output`, `completed`; then a line `tool_result` |
| Usage | The line `payload.kind` `run_usage`: `models[]` with `input_tokens`, `output_tokens`, `cached_tokens`, `reasoning_tokens`, and `allowance` |
| Resume | `MUSE_NO_AUTO_UPDATE=1 muse exec --json --model <model> --contributor-mode off --session-id <the same id> --workspace <the same dir> "<next prompt>"`: the same session continues. `muse resume` opens the terminal UI. Another workspace is refused unless `--allow-workspace-switch`. The same resume also continues a session whose earlier run was ended by a signal and printed no `run_terminal` and no `run_usage` |
| At a usage limit | Not seen on this host. A failed turn exits 1 and ends with `run_terminal`, `terminal` `failed` and the text in `reason` |
| The remaining quota | `muse usage --json` (no model turn): `allowance.window` and `allowance.weekly`, each `usedPercent` and `resetsAtMs`. A `--json` run's `run_usage.allowance` has the same members in snake_case |
| Another account or configuration directory | `XDG_CONFIG_HOME=<dir> muse …` reads settings and login from `<dir>/muse`; with no login there `muse usage` answers `not logged in`. Sessions and memory go under `XDG_DATA_HOME` (`<dir>/muse/sessions`) |

## 12. Antigravity CLI without a terminal UI

Every row is for ONE run and changes no saved file. The configuration directory is `~/.gemini`.

| Need | Command or setting |
| --- | --- |
| The run | `agy --print "<prompt>" --output-format stream-json --model <slug> --effort <effort>`, started in the working directory (no flag names it; another directory: `--add-dir <dir>`). `agy models` lists the slugs; a name that is not a slug is refused (exit 1, `status` `ERROR`). A prompt on stdin needs `--input-format stream-json`: one line `{"event":"user","message":{"content":"<prompt>"}}` per turn, without `--print` |
| The auto-update of one run | Set `AGY_CLI_DISABLE_AUTO_UPDATE=1` in the run's environment, the variable the CLI holds for disabling its auto-update. A short-lived `--bg-updater` child can still start, so the variable does not show that no updater process appears |
| A time bound | `--print-timeout <duration>` (for example `15m`); without it there is none. The bound can end the output while the turn still runs: read the result before retrying |
| No permission prompts | `--dangerously-skip-permissions`: every tool call is approved (the `init` line shows `permission_mode` `always-proceed`) |
| A model decides each approval, no person | `--mode auto`: workspace edits are approved, commands run in the terminal sandbox, and a command outside it is evaluated automatically, so safe, reversible ones run and risky ones still need an approval. It needs the terminal sandbox; where that is missing a headless run prints a warning and goes on in the default mode |
| A call needs an approval | What was measured, not what the flags promise. Without either flag a `read_file` is soft-denied: the run goes on and exits 0, stderr names the denied permission, the `result` lists it in `denied_actions`, and the response can be empty. A command is not: `agy --print` runs `run_command` with no approval, the command writes outside the working directory and reaches the network, and `--sandbox` changes none of this. Only a confinement outside the CLI held (`bwrap` with the root read-only): start a headless run inside one, and never take a soft denial for a limit. Allow named calls for one run: `--settings '{"permissions":{"allow":["command(rg)","read_file(<dir>)"]}}'` |
| What a run loads from the home | Instruction pointers `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md`, skills `~/.gemini/config/skills/`, MCP servers `~/.gemini/config/mcp_config.json`, hooks `~/.gemini/config/hooks.json`, per-conversation memory `~/.gemini/antigravity-cli/brain/` |
| Keep the registered MCP servers, hooks and skills out | No flag does it. Start the run with `HOME=<a new directory>` that holds `.gemini/antigravity-cli/antigravity-oauth-token` as a symbolic link to the real token (to keep the shared instructions, also links `.gemini/AGENTS.md` and `.gemini/GEMINI.md`) and no `.gemini/config/`: no MCP server starts, and the conversation is written under that directory. Link the token, never copy it; remove the directory afterwards. Never use `agy mcp disable`, `agy mcp remove` or an edit of `mcp_config.json` for one run: they change the registration for every session |
| Keep slash commands out; one setting for one run | `--disable-slash-commands`; `--settings '<JSON of settings.json keys>'` (never saved) |
| Compaction | Automatic: older turns become checkpoint summaries. No flag or setting for the point was found; `/context` in the terminal UI shows how often it happened |
| Session id in the events | `conversation_id` of the line `{"event":"init"}`, of every `step_update` and of the `result`. It cannot be chosen |
| Final answer | `result.response` of the line `{"event":"result"}`, with `result.status` (`SUCCESS`, `ERROR`, …) and `result.error` |
| A tool call | A line `{"event":"step_update"}` whose `step_update.step_type` is `tool`, with `tool_name` and `tool_info` (`name`, `parameters`) |
| Usage | `result.usage`: `input_tokens`, `output_tokens`, `thinking_tokens`, `cache_read_tokens`, `total_tokens`, summed over the conversation |
| Resume | `agy --print "<next prompt>" --output-format stream-json --conversation <conversation_id>`, in the same working directory and with the same `HOME`: the id stays the same. `--continue` takes the most recent conversation |
| At a usage limit | Not documented and not seen. A run that fails exits non-zero and writes the cause to stderr; the `result` has `status` `ERROR` and `error` |
| The remaining quota | `agy --print /usage` (no model turn): one tab-separated line per limit: model group, limit, the remaining percent, the reset time |
| Another account or configuration directory | No flag or variable is documented. `HOME=<dir>` moves the whole `~/.gemini` (row "Keep the registered MCP servers" above) |
