# CLI launch and MCP registration — verified facts (on demand)

Dated host observations from 2026-11-06. Re-check `--help` and the installed version before relying
on a flag. This page records what was verified, so the next agent does not have to rediscover it.

## Claude Code (2.1.319 after an in-session update on 2026-11-06)

- **Model:** `--model claude-sonnet-5-5` (or `opus`, `sonnet`, `fable` aliases). Append `[1m]` to
  the model name for the 1M-token context window, for example `--model 'claude-sonnet-5-5[1m]'`.
- **Context window other than the default:** set `CLAUDE_CODE_AUTO_COMPACT_WINDOW=400k` (also
  `400000`) in the launch environment together with the `[1m]` model. Auto-compaction then holds
  the session at that size. Verified with `/context`: `65.1k/400k tokens`. A setting
  `autoCompactWindow` exists as the settings-file equivalent. `CLAUDE_CODE_MAX_CONTEXT_TOKENS` is
  for models that Claude Code does not recognize.
- **Effort:** `--effort high`. **Permissions:** `--permission-mode bypassPermissions` (matches
  the Lead's mode). **Name:** `-n <display-name>`.
- **Usage:** sessions on one Claude subscription share one weekly limit; a fresh session displayed
  the remaining share at start.
- Docs: https://code.claude.com/docs/llms.txt (model and context pages: `model-config`,
  `context-window`, `settings`).

## Herdr launch (Herdr installed CLI)

1. `herdr tab create --workspace <ws> --cwd <dir> --label <label> --env KEY=VALUE --no-focus`
   returns `.result.tab.tab_id` and `.result.root_pane.pane_id`. Set the environment here, because
   `herdr agent start` has no `--env`.
2. `herdr agent start <name> --kind claude --pane <pane> --timeout 120000 -- <claude args>`
   returns after the agent is ready for input.
3. `herdr agent prompt <name> "<handoff pointer>" --wait --timeout 60000`, then read the pane and
   verify the agent's own report (`/context`, visible tools). An idle state is not completion.

Do not export `HINDSIGHT_DISABLE_HOOKS` (native-first; see RUNTIME-LOADING.md).

## academy_operations MCP registration (user decisions 2026-11-06)

- **Muse:** `~/.config/muse/settings.json` → `mcpServers.academy_operations`, `mode: optional`,
  command `/usr/bin/env ACADEMY_MCP_INSTALL_DIR=/root/.local/share/academy-mcp-host-20261106
  /usr/local/bin/bun --no-env-file /root/.local/share/academy-mcp-host-20261106/launcher.mjs`.
- **Claude Code:** registered at user scope with the same command:
  `claude mcp add --scope user academy_operations -- <same command>`. Verified with
  `claude mcp list`: `✔ Connected`. Only sessions started after registration see the tools.
- **Codex and agy:** disabled until the full-surface Rust cutover.
- Both enabled CLIs use one academy account: run Academy calls from one lesson session at a time.
  The current generation is in `/root/.local/share/academy-mcp-host-20261106/release.json`. Do not
  modify the install base or the release pointer.

## Hindsight MCP tool names (coding-agents 0.8.5)

Every CLI registers the same server, `node /root/.hindsight/coding-agents/dist/mcp-server.js`.

- Tool names are `hindsight_*`; each CLI adds its own prefix (for example
  `mcp__hindsight__hindsight_ingest_document`).
- `hindsight_ingest_document(title, content)` derives the document id from the title: lowercase,
  with runs of other characters turned into `-`. A non-ASCII title appends `--<sha256>`.
- A same-id call replaces the document (server default `update_mode: replace`).
- Tags are stamped from the directory where the CLI session started (or
  `HINDSIGHT_MCP_PROJECT_CWD`), not from a later `cd`.
