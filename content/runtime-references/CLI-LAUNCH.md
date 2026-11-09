# Native CLI launch and continuity

Read current task STATE/TASK, then select only the relevant native surface. Official indexes: [llms.txt](llms.txt); lookup: [OFFICIAL-DOCS.md](OFFICIAL-DOCS.md). Commands below describe mechanisms, never authority to launch, bypass permissions, update, authenticate or restart a session. Use the task's actual model/effort/permission assignment; verify the native session reports it. A prompt saying "Opus Lead" does not select Opus.

## Selected native entrypoints

| Runtime | Launch / exact-session continuation | Host notes |
|---|---|---|
| Codex | `codex --no-daemon -C <checkout>`; headless `codex --no-daemon exec -C <checkout> --json <prompt>`; `codex --no-daemon resume <session-id>` | Native in Herdr terminals, not app-server orchestration. Select `-m <model>` and `-c model_reasoning_effort=<effort>` per assignment. Keep existing daemon/sessions intact. Avoid `--last` when an exact ID is assigned. |
| Claude Code | `claude --model <model> --effort <effort>`; headless `claude --print <prompt>`; `claude --resume <session-id>` | Run from assigned checkout. User-selected Opus uses native model controls. Skill/ToolSearch/builtin restrictions can change loaded MCP context; see OFFICIAL-DOCS. |
| Muse | `muse exec --model <model> --reasoning-effort <effort> --session-id <uuid> <prompt>`; interactive `muse --model <model> --reasoning-effort <effort>` | Headless uses long model/effort flags; do not transfer TUI shorthand or flags blindly. For read-only version/help inspection set `MUSE_NO_AUTO_UPDATE=1` to avoid updater effects. |
| Antigravity (`agy`) | `agy --print <prompt> --output-format stream-json` | Select model and exact continuation through installed help for the assigned mode. A print timeout may end output while a turn is still running; reconcile before retry. |
| Grok Build | `grok --cwd <checkout>`; headless `grok --cwd <checkout> -p <prompt> --output-format streaming-json`; `grok --cwd <checkout> --resume <session-id>` | Native inspection/help is distinct from model execution. Follow current task login/model-execution holds; inspection never releases them. |

Run relevant installed help for an absent/unrecognized flag, changed version or contradictory output. Do not run all version/help commands for every task. Permission/trust controls follow the assignment and native owner; no universal dangerous-launch preset belongs here.

## Context and compaction

The selected orchestration checkpoint targets approximately400k context; confirm the current assignment before applying it. Supported controls differ:

- Codex: `model_context_window` and `model_auto_compact_token_limit` are native configuration fields; verify effective values and actual compaction scope rather than equating the configured window with a measured ceiling.
- Claude: native model/context selection, `CLAUDE_CODE_AUTO_COMPACT_WINDOW` or its documented settings equivalent must match the selected model and installed support. Verify effective `/context` readback; do not infer a window from a model alias alone.
- Muse headless: `--context-compaction-soft-threshold` and `--context-compaction-hard-threshold` are fractions of the selected model window; soft must be below hard. Inspect the actual catalog window before calculating fractions. These exec flags do not imply TUI support.
- Antigravity interactive checkpoints/manual `/compact` do not enforce an automatic token ceiling. Use native supported controls if the assignment requires an enforced limit.
- Grok context behavior requires separately authorized native qualification; metadata inspection alone is not behavior evidence.

A continuation must reread changed STATE/holds and source; a saved native context is not renewed permission. Existing session/tool definitions may survive a client-setting change. Prefer a fresh owned verification session instead of restarting unrelated work.

## Herdr and native registration

For explicitly authorized pane/agent control, use the installed herdr skill and [HERDR-ORCHESTRATION.md](HERDR-ORCHESTRATION.md). Tab environment belongs to native tab creation, not a guessed agent flag. Submit a bounded handoff and verify actual task reads/output; ready/idle alone is not completion.

Custom MCP configuration and hook owners are in [runtime-wiring.md](runtime-wiring.md). Registration does not enable every effect or create lifecycle hooks. Do not disable Hindsight hooks to simulate worker isolation; use only explicitly authorized native path-isolation controls in CONTEXT-GOVERNANCE. Updates use the selected runtime's native updater and current task authority, never a copied installation recipe or a historical version snapshot.
