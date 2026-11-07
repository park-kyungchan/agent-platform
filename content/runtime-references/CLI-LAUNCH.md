# CLI launch, ~400k compaction and MCP registration — verified facts (on demand)

Dated host observations from 2026-11-06 and 2026-11-07. Re-check `--help` and the installed version
before relying on a flag. This page records what was verified across all five installed CLIs (`Muse Code`,
`Claude Code`, `Codex`, `Antigravity CLI (agy)`, `Grok Build (grok)`), so the next agent does not have to rediscover it.

## 1. Claude Code (2.1.319 after an in-session update on 2026-11-06)

- **Model:** `--model claude-sonnet-5-5` (or `claude-opus-5-5`, `opus`, `sonnet`, `fable` aliases). Append `[1m]` to
  the model name for the 1M-token context window, for example `--model 'claude-sonnet-5-5[1m]'`.
- **~400k auto-compaction window:** set `CLAUDE_CODE_AUTO_COMPACT_WINDOW=400000` (persisted in
  `/root/.claude/settings.json` `env`, or `400k` in the launch environment together with the `[1m]` model).
  Auto-compaction then holds the session at that size. Verified with `/context`: `65.1k/400k tokens`.
  A setting `autoCompactWindow` exists as the settings-file equivalent. `CLAUDE_CODE_MAX_CONTEXT_TOKENS` is
  for models that Claude Code does not recognize.
- **Effort:** `--effort high`. **Permissions:** `--permission-mode bypassPermissions` (or
  `--dangerously-skip-permissions`). **Name:** `-n <display-name>`.
- **Session log path:** `/root/.claude/projects/<project-slug>/<session-uuid>.jsonl` (`type: "tool_use"` entries).
- **Usage:** sessions on one Claude subscription share one weekly limit; a fresh session displays
  the remaining share at start.
- Docs: https://code.claude.com/docs/llms.txt (model and context pages: `model-config`,
  `context-window`, `settings`).

## 2. Muse Code (`1.4.14-R7309.1`)

- **Headless launch (`muse exec`):**
  `cd <dir>` followed by `muse exec --yolo --model <model> --reasoning-effort <effort> --context-compaction-soft-threshold <soft_fraction> --context-compaction-hard-threshold <hard_fraction> --session-id <uuid> "<prompt>"`
  - **Important flag note:** `muse exec` requires long flags `--model` and `--reasoning-effort` (or `--effort`);
    short flags `-m` and `-t` are rejected (`unexpected argument '-m' found`).
- **Compaction:** The 2026-11-07 approved checkpoint targets an approximately 400k-token ceiling. Read the selected model's actual catalog window and choose supported headless fractions with soft below hard for that window. For a reported window of 1,048,576 tokens, `0.35` and `0.38` are an example corresponding to approximately 367k and 398.5k tokens; do not assume every model has that window. Installed `muse exec --help` lists these flags; behavioral enforcement remains a separate observation. The earlier 2026-11-04 `0.85/0.9` preference is historical for this later task target; see RUNTIME-LOADING.md. Interactive TUI does not accept these threshold flags. Run `cd <dir>` followed by `muse --yolo --model <model> --reasoning-effort <effort>` and monitor context; manual `/compact` is not an enforced automatic ceiling.
- **Persistent defaults:** `/root/.config/muse/settings.json` stores `model_reasoning_efforts`
  (`{"muse-wonder-1.0": "high", "muse-spark-1.3": "medium"}`).
- **Session log path:** `/root/.local/share/muse/sessions/YYYY/MM/DD/<session-uuid>/session.jsonl`
  (`payload_type: "tool_batch.effect.started"` -> `payload.record.tool_name` and `parallel_profile.subject`).

## 3. Antigravity CLI (`agy 1.5.2`)

- **Interactive / Herdr agent launch:**
  `herdr agent start <name> --kind agy --pane <pane-id> -- --model <gemini-3.8-flash-medium|...>`
- **~400k compaction & isolation discipline:** use one bounded task per worker pane, inspect `/context`
  at checkpoints, and trigger `/compact` before `400k` tokens.
- **Session log path:** `/root/.gemini/antigravity-cli/brain/<conversation-id>/.system_generated/logs/transcript.jsonl`
  (and `transcript_full.jsonl` when `truncated_fields` is present; tool calls are in `tool_calls[].name` and `args`).

## 4. Codex (`0.167.0`, official standalone update 2026-11-07)

- **Launch flags:**
  `codex --no-daemon --dangerously-bypass-approvals-and-sandbox --no-alt-screen -m <gpt-6.1-sol|gpt-6.1-terra> -c model_reasoning_effort="high" -c model_context_window=400000 -c model_auto_compact_token_limit=320000 -C <dir>`
  (or `codex --no-daemon exec` for non-interactive runs). The user uses native Codex in Herdr terminals, not an app-server orchestration workflow. `--no-daemon` runs without the shared background server even if one already exists; existing sessions were not restarted.
- **~400k auto-compaction window:** `model_context_window = 400000` and `model_auto_compact_token_limit = 320000`
  are configured in `/root/.codex/config.toml`.
- **Session log path:** `/root/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (`payload.type` in
  `function_call`, `custom_tool_call`, `local_shell_call`).

## 5. Grok Build (`1.0.77`)

- Native binary: `/root/.grok/downloads/grok-linux-x86_64`; command `/root/.local/bin/grok` through `/root/.grok/bin/grok`.
- Interactive: `grok --cwd <checkout>`. Headless: `grok --cwd <checkout> -p "<prompt>" --output-format streaming-json`. Resume: `grok --cwd <checkout> --resume <session-id>`. These flags were checked in installed help; no model session was launched.
- Inspect discovered rules, skills, hooks and MCP with `grok inspect --json`. The three shared custom MCPs register natively, and foreign MCP scanners are off; shared and Claude-compatible skills remain separately discoverable. `~/.grok/AGENTS.md` links canonical guidance; no Hindsight lifecycle hooks are configured. See runtime-wiring.md before assuming independent memory identity or Academy authority.
- Official index: https://docs.x.ai/llms.txt (`Grok Build` section).

## Native installation and update ownership — verified 2026-11-07

| CLI | Native entry / updater | Latest selected-channel check |
|---|---|---|
| Muse | Official `/root/.local/bin/muse` launcher executes `muse-bin-1.4.14-R7309.1`; for read-only checks use `MUSE_NO_AUTO_UPDATE=1 muse --version` | Official launcher update completed; installed binary remains `1.4.14-R7309.1` |
| Claude | Native symlink into `~/.local/share/claude/versions/`; `claude update` | `2.1.319`, up to date |
| Codex | `~/.local/bin/codex` -> `~/.codex/packages/standalone/current/bin/codex`; `codex update` or official https://chatgpt.com/codex/install.sh | `0.165.0` -> `0.167.0`; former manual layout could not identify its updater |
| agy | Native `~/.local/bin/agy`; `agy update` | `1.5.2`, up to date |
| Grok | Native vendor download via symlinks; `grok update` | `1.0.77`, up to date |

These are native update checks, not model-inference acceptance. Preserve active sessions and vendor configuration. Herdr supplies terminals; native updater ownership stays with each CLI.

## Herdr launch (Herdr installed CLI)

1. `herdr tab create --workspace <ws> --cwd <dir> --label <label> --env KEY=VALUE --no-focus`
   returns `.result.tab.tab_id` and `.result.root_pane.pane_id`. Set the environment here, because
   `herdr agent start` has no `--env`.
   To rename an existing tab or pane, use positional arguments:
   `herdr tab rename <tab_id> <label>` and `herdr pane rename <pane_id> <label>`.
2. `herdr agent start <name> --kind <claude|muse|agy|codex> --pane <pane> --timeout 120000 -- <cli args>`
   returns after the agent is ready for input. For headless commands (`muse exec`, `codex exec`, bounded
   scripts), use `herdr pane run <pane> "<command>"`.
3. `herdr agent prompt <name> "<handoff pointer>" --wait --timeout 60000`, then read the pane and
   verify the agent's own report (`First output after reading: ...; Context-Injected: ...`, `/context`,
   visible tools). An idle state is not completion.

Do not export `HINDSIGHT_DISABLE_HOOKS` (native-first; see RUNTIME-LOADING.md). When a benchmark or
isolated evaluation requires zero Hindsight injection, place the working directory under a declarative
`paths.<prefix>` entry in `/root/.hindsight/coding-agent.json` (for example
`"/home/palantirkc/work/eval-isolated": {"disabled": true}`).

## Shared custom MCP registration — user requirement 2026-11-07

All five CLIs directly register `hindsight`, `academy_operations`, and `daniel_prayer_operations` in their native user settings. Use the same approved server command/arguments and credential-file references; vary only supported client metadata and schema. The exact paths/matrix are in runtime-wiring.md. Do not use Claude personal config as the shared authority, import another CLI's plugin MCPs, or create a universal launcher.

Academy uses the current host-restore launcher and existing account lock in every native registration. The earlier Muse/Claude-only activation restriction is superseded for client registration; backend cutover, save availability and domain-effect permissions are unchanged. Run Academy operations from one lesson session at a time, whichever CLI the user chooses. Shared availability does not permit domain reads/writes in an evaluation or unrelated task.

Grok explicitly disables `[compat.claude] mcps` and `[compat.cursor] mcps`; `grok inspect --json` must show the shared servers from `~/.grok/config.toml`. Its Hindsight harness is `grok-build`, not `claude-code`. Keep native vendor plugin installations owned by the CLI where installed.

## Hindsight MCP tool names (coding-agents 0.8.5)

All five native configurations directly register the same installed Node.js executable and `/root/.hindsight/coding-agents/dist/mcp-server.js`. Hindsight attribution differs per client; native lifecycle hook support remains distinct. MCP registration does not create hooks.

- Tool names are `hindsight_*`; each CLI adds its own prefix (for example
  `mcp__hindsight__hindsight_ingest_document`).
- `hindsight_ingest_document(title, content)` derives the document id from the title: lowercase,
  with runs of other characters turned into `-`. A non-ASCII title appends `--<sha256>`.
- A same-id call replaces the document (server default `update_mode: replace`).
- Tags are stamped from the directory where the CLI session started (or
  `HINDSIGHT_MCP_PROJECT_CWD`), not from a later `cd`.
