# The coding CLIs on this host: files, commands and what a session loads

Read this when you start, resume or configure a CLI, register an MCP server, or must check what a session loaded. The CLIs are Claude Code (`claude`), Codex (`codex`), Muse (`muse`), Antigravity (`agy`) and Grok Build (`grok`). A command below shows the mechanism. It is not permission to launch, authenticate, update or change configuration: that comes from the task's STATE.

| I need | Section |
| --- | --- |
| The file a CLI reads for instructions, MCP servers or skills | 1 |
| The command that starts, runs once or resumes a session | 2 |
| To show what a session loaded | 3 |
| The compaction control | 4 |
| The rules for MCP servers | 5 |
| Hooks; skill sources; a CLI's private memory | 6; 7; 8 |

## 1. Files per CLI

| CLI | Shared-instruction pointer | Custom MCP servers: file / section | Hindsight client name | Skills it lists |
| --- | --- | --- | --- | --- |
| Claude Code | `~/.claude/CLAUDE.md` (a file that imports the shared file with an `@` line) | `~/.claude.json` / `mcpServers` | `claude-code` | `~/.claude/skills/` (symlinks to the shared skills) |
| Codex | `~/.codex/AGENTS.md` (symlink) | `~/.codex/config.toml` / `mcp_servers` | `codex` | `/root/.agents/skills/` directly |
| Muse | `~/.config/muse/AGENTS.md` (symlink) | `~/.config/muse/settings.json` / `mcpServers` | `muse` | `/root/.agents/skills/` directly |
| Antigravity | `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md` (symlinks) | `~/.gemini/config/mcp_config.json` / `mcpServers` | `antigravity-cli` | `~/.gemini/config/skills/` (symlinks) |
| Grok Build | `~/.grok/AGENTS.md` (symlink) | `~/.grok/config.toml` / `mcp_servers` | `grok-build` | read `SKILL.md` by path |

Every pointer resolves to `/root/.agents/AGENTS.md`. If a CLI does not show that text in a session, repair its pointer; do not paste the text into a prompt as a permanent fix and do not create a copy.

## 2. Start, run without a terminal UI, resume

| CLI | Interactive | Headless (one prompt, then exit) | Resume an exact session |
| --- | --- | --- | --- |
| Claude Code | `claude --model <model> --effort <effort>` in the assigned checkout | `claude --print <prompt>` | `claude --resume <session-id>` |
| Codex | `codex --no-daemon -C <checkout> -m <model> -c model_reasoning_effort=<effort>` | `codex --no-daemon exec -C <checkout> --json <prompt>` | `codex --no-daemon resume <session-id>` |
| Muse | `muse --model <model> --reasoning-effort <effort>` | `MUSE_NO_AUTO_UPDATE=1 muse exec --model <model> --reasoning-effort <effort> --disable-reminders --session-id <new uuid> --workspace <checkout> --prompt-file <absolute TASK path>` | `muse resume <session-ref>` |
| Antigravity | `agy --model <model>` | `agy --print <prompt> --output-format stream-json` | `agy --conversation <id>` |
| Grok Build | `grok --cwd <checkout>` | `grok --cwd <checkout> -p <prompt> --output-format streaming-json` | `grok --cwd <checkout> --resume <session-id>` |

- The Lead of a task runs in the CLI its STATE names (skill `lead`); every CLI of this table may run a worker or a reviewer when the task's assignment file names it.
- Codex always runs with `--no-daemon` in a Herdr terminal. Do not replace this with an app-server workflow.
- Muse: `MUSE_NO_AUTO_UPDATE=1` stops the updater from running during the command. `--disable-reminders` removes two built-in reminder agents and the extra model requests they make; use it for workers. `muse exec --provider echo` (without `--model`) runs a session with no model call, which is the cheap way to inspect what a session loads. Muse's session record does not store the effort of the main request.
- Antigravity: a `--print` timeout can end the output while the turn still runs. Read the result before retrying.
- Resume by exact session ID, not `--last`, when several sessions exist. A resumed session keeps older instructions and tool definitions: read the task's STATE again before any effect.
- If a flag is rejected or missing, read that CLI's installed `--help`. Do not run every CLI's help for every task.
- Looking at a CLI's help, configuration or session list does not release a hold on logging in or running a model with it; the hold is in the task's STATE.
- Update a CLI only with its own updater and only when the task authorizes it. Do not copy an installation recipe.

## 3. What a session loads, and how to check it

"Loaded" means the session's own record shows the text.

| CLI | Session record | Limits you must know |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/<cwd with slashes as dashes>/<session-id>.jsonl` | A subagent may not receive project instruction files. Excluding ToolSearch from `--tools` loads every MCP tool schema at start |
| Codex | `~/.codex/sessions/` (`rollout-*.jsonl`) | Outside a Git repository Codex reads project instruction files in the cwd only, not in parent directories. The user-level pointer still loads. Do not create a fake repository to change this |
| Muse | `~/.local/share/muse/sessions/<yyyy>/<mm>/<dd>/<session-id>/session.jsonl` | Without `~/.config/muse/AGENTS.md` Muse falls back to `~/.claude/CLAUDE.md` and shows its `@` line as literal text |
| Antigravity | `~/.gemini/antigravity-cli/conversations/` | A native rule include is written `@[label](path)`; a bare `@filename` is a reference, not an include |
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
| Claude Code | `CLAUDE_CODE_AUTO_COMPACT_WINDOW` and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` in `~/.claude/settings.json` `env`; read back with `/context` |
| Codex | `model_context_window` and `model_auto_compact_token_limit` in `~/.codex/config.toml` |
| Muse headless | `--context-compaction-soft-threshold` and `--context-compaction-hard-threshold`, fractions of the model's window, soft below hard |
| Antigravity | Manual `/compact` only; there is no automatic ceiling |
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

- Installing source changes no registration and no running session. The registration route prepared for engineering memory is in `agent-platform` `memory/README.md`; it is not in use until its owner's task says so.

## 6. Hooks

Hook wiring differs per CLI and changes with CLI and plugin versions, so read the actual configuration instead of assuming it: Claude Code `~/.claude/settings.json` `hooks` plus its installed plugins, Codex `~/.codex/hooks.json`, Muse `~/.config/muse/settings.json` `hooks`, Antigravity `~/.gemini/config/hooks.json`, Grok `~/.grok/hooks/`. A CLI with the `hindsight` MCP server but no Hindsight hook gets no automatic memory injection and no automatic session retention; the agent calls the tools itself. A hook name does not show that injection or retention happened.

Do not export a hook-disable variable and do not remove an MCP server to isolate a worker. Memory isolation uses the Hindsight path override described in `CONCURRENT-WORK.md`.

## 7. Skills

| Skill | Source |
| --- | --- |
| `explain-precisely`, `lead`, `lessons-loop`, `herdr`, `herdr-hermes`, `hindsight-control` | `agent-platform` `content/skills/` |
| `academy-runtime-routing`, `academy-student-context` | `education-platform` `content/skills/` |
| `hindsight-coding-agent` | Generated by the Hindsight integration; changed only through it |
| Account-synced, marketplace and built-in skills | Their CLI or vendor; changed only through that CLI |

Installed shared path: `/root/.agents/skills/<skill>/SKILL.md`. Change a skill in its source repository through review, then install it (`BUILD-AND-DELIVERY.md` section 6); never edit only the installed copy. Frontmatter fields other than `name` and `description` do not behave the same in every CLI.

## 8. Private memory of each CLI

| CLI | Location | Loaded when |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/<cwd with slashes as dashes>/memory/` (`MEMORY.md` index plus one file per entry) | Every session started in that cwd |
| Muse | `~/.local/share/muse/memory/personal/` and `~/.local/share/muse/memory/projects/<cwd name and hash>/` | Sessions in that cwd |
| Antigravity | `~/.gemini/antigravity-cli/brain/` | Per conversation |
| Codex, Grok Build | None found on this host | — |

This memory is private to one CLI and one directory, so no other agent sees it and nothing reviews it. The rule for what may go there is in `/root/.agents/AGENTS.md` section 6. When an entry there contradicts the task's STATE or the shared instructions, STATE and the shared instructions win; delete the entry.
