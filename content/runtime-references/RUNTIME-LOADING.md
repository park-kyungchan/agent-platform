# Runtime loading and execution map

Current scope confirmed by the user2026-11-02: **Muse, Claude Code, Codex and Antigravity CLI (agy)**. Checked against their installed help, exact native configuration metadata and official documentation. This is on-demand detail for canonical /root/.agents/AGENTS.md, not an automatic loader or another policy source.

## Current native surfaces

| Runtime/version | Shared guidance and project discovery | Settings/hooks/MCP | Current evidence |
|---|---|---|---|
| Codex0.165.0 | ~/.codex/AGENTS.md resolves to canonical; no global AGENTS.override.md observed. Native root-to-cwd project chain selects one instruction file per level, with override priority and a combined size bound | ~/.codex/config.toml; ~/.codex/hooks.json; mcp_servers | Pointer, parsable config and command entrypoints checked; this running session received canonical guidance. No new model session launched in this preparation |
| Claude Code2.1.316 | ~/.claude/CLAUDE.md imports canonical. Current Claude also supports project AGENTS.md, subject to its native instruction-file selection and ancestor CLAUDE files | ~/.claude/settings.json for hooks; ~/.claude.json for MCP | Global import and configured hooks read back. Project AGENTS discovery is conditional, not universally CLAUDE-only |
| Antigravity CLI1.4.12 | ~/.gemini/AGENTS.md and GEMINI.md both resolve to canonical; AGENTS/GEMINI/.agents discovery stops at the repository boundary. Read the selected module's deeper instructions explicitly when starting at root | ~/.gemini/antigravity-cli/settings.json; shared ~/.gemini/config/{hooks.json,mcp_config.json} | Both native alias pointers exist; do not infer that the loader injects once or twice without actual context inspection. Gemini CLI itself is absent |
| Muse Code1.4.14-R7309.1 | Native trusted project lookup selects the first available AGENTS.md, CLAUDE.md, .agents/AGENTS.md or .claude/CLAUDE.md at each applicable level. A prior live session observed canonical via Claude foreign personal context | ~/.config/muse/settings.json, schema_version1; mcpServers; no configured Hindsight lifecycle hooks | Config/flags read back; project rules require native trust. Do not assume foreign personal context exists with --no-foreign-personal-context or in every child |

No new model sessions, provider quota checks, migrations or runtime installs were performed to produce this table. A pointer, registration or help entry proves its own surface; it does not prove fresh loading or successful behavior. Preserve native trust; do not edit vendor-internal trust/session stores.

## Native instruction differences that matter

- Codex builds its instruction chain at session launch; global/project overrides can replace the expected file. Its documented default project limit is32KiB, not a reason to inject all runtime references. Continue a saved session with the exact ID, and explicitly reload changed task state after resume/compaction. [Official guidance](https://learn.chatgpt.com/docs/agent-configuration/agents-md.md).
- Claude's global import does not suppress project AGENTS. An ancestor project CLAUDE.md or CLAUDE.local.md can change which project files load under its default selection. Inspect /memory or native loading evidence when that distinction affects a change; children can be configured to omit project guidance. Keep the existing global canonical import. [Official memory](https://code.claude.com/docs/en/memory).
- Antigravity's CLI settings are distinct from its shared customization directory. Workspace discovery does not automatically follow every nested module as the agent opens files; read the specific nested rule when assigning a subtree. The two global canonical aliases are preserved, with deduplication explicitly unverified. [Discovery](https://antigravity.google/docs/customization-discovery/), [rules](https://antigravity.google/docs/rules/).
- Muse project rules are native trust-gated; launch flags can exclude foreign personal rules. Supply canonical path, task STATE and exact selected contract explicitly in a handoff rather than relying on inherited Claude context. Do not add an invented global rule filename or context-window setting. [Configuration](https://dev.meta.ai/docs/muse-code/configuration).

## Launch, resume and isolation

Commands below identify native surfaces, not permission grants or a custom launcher. Run within the assigned existing native Git worktree. Native model/effort and authority come from the actual assignment, not a CLI brand or historical quota snapshot.

| Runtime | Working directory / bounded output | Resume exact session | Parallel-write pitfall |
|---|---|---|---|
| Codex | codex -C <checkout>; headless codex exec -C <checkout> --json ...; see installed help | codex resume <session-id>; headless codex exec resume <session-id> ... | A child thread is not proof of a separate checkout; record and inspect actual Git identity before writing |
| Claude | Start claude in the assigned checkout; -p and --output-format stream-json are headless surfaces | claude --resume <session-id>; preserve its assigned cwd and existing permission posture | Subagents ordinarily share cwd unless isolated. Named/team behavior can differ; confirm actual child checkout rather than assume frontmatter isolation applied |
| Antigravity | Start agy in the assigned checkout; --print --output-format stream-json; set an assignment-specific --print-timeout because default0 is unbounded | agy --conversation <id>; avoid --continue with concurrent sessions | Subagents may inherit/share/branch; a fork of context does not itself isolate files |
| Muse | muse --workspace <checkout>; for an existing worktree, supported --worktree existing --worktree-existing <path>; headless muse exec --prompt-file <file> --json | Interactive muse resume <id>; headless muse exec --session-id <uuid> ... | --parallel-tool-calls concerns tools; --subagent-worktree-isolation alone does not isolate every child. Request isolation for the particular writable child or use an assigned external worktree |

Use task-owned work/<id>/wt/<repo> paths for this migration. Runtime-managed automatic worktrees can use different locations/lifecycle cleanup; do not combine automatic creation with an already assigned external checkout. Inspect actual cwd/common Git directory/HEAD before effects. Keep durable work state outside disposable checkout caches.

Do not translate a flag from one CLI to another, change the user's model, enable agent teams, or weaken approvals globally for convenience. Local shell subprocesses need their own finite deadline/output bound; native turn completion or exit0 is not a test verdict. Muse --no-session-log disables retained recovery; keep logs for resumable large work and use concise event summaries. Muse compaction threshold flags are headless-only and are not settings.json keys. New flags are accepted only when the installed surface supports them.

User preference (2026-11-04): for headless `muse exec` runs on this host, launch with `--context-compaction-soft-threshold 0.85 --context-compaction-hard-threshold 0.9` (soft must stay below hard; the pair was validated with the echo provider). The user's target of a 400k context window cannot be set: the context window is a model-catalog reported value with no settings.json, `-c`, or launch key, so the 0.9 threshold applies to the actually reported window, not to 400k. Interactive TUI sessions cannot take these flags and cannot be changed mid-session.

Official sources: [Claude subagents](https://code.claude.com/docs/en/sub-agents), [Antigravity subagents](https://antigravity.google/docs/subagents), [CLI settings](https://antigravity.google/docs/cli/using), [Muse extending](https://dev.meta.ai/docs/muse-code/extending). Installed help snapshots are in work/ws-replan/preparation/final-review/runtime/ on this host.

## Hindsight: separate loading from retention and page refresh

All four confirmed runtimes register the same official coding-agents MCP implementation, with their own harness ID. Codex/Claude have official SessionStart, UserPromptSubmit and Stop hook registrations; Antigravity uses PreInvocation and Stop in its native namespace; Muse has MCP tools with no configured lifecycle injection hook. Hook entrypoint files all exist. Hooks and MCP are different clients with different config lifetimes; registration alone does not prove an event actually ran or a non-managed hook was trusted.

The observed official config is autoInject:pages, injectTimeoutMs5000, reflectToolTimeoutMs660000, reflectBudget:mid, gitIngest:none and retainSessions:false. Initial page injection, per-turn roster/tool guidance, extraction, consolidation and derived-page refresh are distinct operations. In particular, **sessions are not automatically retained with this config**. Do not assume a Stop hook writes the full transcript or that Muse runs another CLI's hook. Use explicit selected-document retention when authorized.

Short-lived hooks read current config on invocation; the persistent MCP client generally keeps startup settings, with token reread after rejection. Edits do not hot-replace existing process code or current model context. A native page read/search verifies that selected result, not universal per-call injection or a scored quality suite. For consequential Hindsight work load /docker/hindsight/HINDSIGHT_OFFICIAL_PER_CALL.md and its matching installed-revision references.

## Verification boundary and excluded runtimes

Gemini CLI and Grok are absent from PATH; they are not members of the user's confirmed four-CLI execution matrix. Retained Gemini-named home paths can still be active Antigravity inputs; absence of the Gemini executable does not authorize deleting them. Hermes remains a separate existing container/native service consumer; its SOUL/profiles/relay and upgrade helper are not replaced by this preparation.

Before an affected live instruction/config installation in S03, observe its supported native loading/trust surface and then a fresh affected session. Preserve already-running sessions. Do not restart every CLI to verify an unrelated adapter, ask a model to prove loading by self-report alone, or reuse old answers about absent platforms/unfinished relocation. Current filesystem/source facts come from task STATE and native readback.

For ownership, shared build serialization, handoff and failure propagation read global-runtime-routing.md's concurrency section only when coordinating work. Current Rust entry is work/ws-replan/CURRENT-PLAN.md and STATE.md; prior WP plans and Rust memory are legacy.
