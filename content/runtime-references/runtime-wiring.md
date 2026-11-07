# Runtime wiring — native clients, shared custom servers

## Current policy and scope — 2026-11-07

The user requires custom MCPs to be CLI-agnostic. Their server implementation, credential-file owner, account and domain behavior belong to the service; each CLI directly registers that same server using its supported native schema. Another CLI's personal config is not a shared source. This is documented configuration ownership, not a new runtime registry, loader or wrapper.

Built-in tools and MCPs installed through a particular CLI's marketplace/plugin/native integration remain with that CLI. Do not copy vendor plugin definitions into the custom set, disable their native loading, or rewrite plugin caches. A native plugin may legitimately use its own runtime or bundled server; it is not a competing coding CLI installation.

Shared availability grants no task effects. Evaluation, implementation and lesson tasks select tools and data through their current rules, native controls and domain admission. Do not reintroduce permanent CLI-brand roles or exclusions.

## Native user registrations

| CLI | Native configuration | Shared custom MCP entries | Hindsight client attribution |
|---|---|---|---|
| Claude Code | `~/.claude.json` / `mcpServers` | `hindsight`, `academy_operations`, `daniel_prayer_operations` | `claude-code` |
| Muse | `~/.config/muse/settings.json` / `mcpServers` | Same three, enabled and optional | `muse` |
| Codex | `~/.codex/config.toml` / `mcp_servers` | Same three, enabled | `codex` |
| Antigravity | `~/.gemini/config/mcp_config.json` / `mcpServers` | Same three, `disabled=false` | `antigravity-cli` |
| Grok Build | `~/.grok/config.toml` / `mcp_servers` | Same three, enabled | `grok-build` |

For all five clients, each server's command and arguments match exactly; Academy/Daniel environment references also match. Hindsight's client attribution differs deliberately, while its implementation and configured shared bank are the same. Credentials stay with their existing file owner; no secret is copied into native configuration.

| Custom server | Shared implementation / launch contract | State and authority |
|---|---|---|
| Hindsight | Installed Node.js executable runs `/root/.hindsight/coding-agents/dist/mcp-server.js` | `/root/.hindsight/coding-agent.json`; current bank, routing and path overrides remain native Hindsight settings |
| Academy | `/usr/bin/env ACADEMY_MCP_INSTALL_DIR=/root/.local/share/academy-mcp-host-20261106 /usr/local/bin/bun --no-env-file /root/.local/share/academy-mcp-host-20261106/launcher.mjs` | One existing account/lock and domain-owned `release.json`; current TS backend remains until separately qualified Rust cutover |
| Daniel operations | Installed Node.js executable runs the same existing `/opt/daniel-prayer/operations/releases/release-20261102-e0d8f02/mcp/server.mjs` | The existing `DANIEL_SESSION_COOKIE_FILE` reference is reused; no cookie values read/copied by the configuration task |

Absolute Node.js resolution avoids shell PATH differences across native clients. The server process cwd was not fixed globally, preserving Hindsight's project/path selection. Do not replace a domain-owned launcher or point at a future Rust binary before the owning release actually exists and passes its gate.

## Grok compatibility correction

Grok defaults to reading Claude/Cursor MCP config. The initial install left those defaults in place, so it discovered custom entries from `~/.claude.json` and inherited Hindsight's `claude-code` attribution. This was a default compatibility path, not a shared-MCP design.

Current `~/.grok/config.toml` explicitly sets `[compat.claude] mcps=false` and `[compat.cursor] mcps=false`, while defining all three custom servers natively. `grok inspect --json` reports their source as `configToml`, not `claudeJson`. Other compatibility surfaces and native plugin settings were preserved; this change is not a blanket foreign-skill or native-feature suppression.

`~/.grok/AGENTS.md` now links directly to `/root/.agents/AGENTS.md`. Inspection lists its full canonical byte size, so guidance no longer depends on Claude `@import` expansion. The separate 60-byte Claude-compatible pointer may still be discovered; duplicate/turn-level expansion remains a distinct native loading question.

## Shared instructions, skills and lifecycle hooks

- Canonical guidance is `/root/.agents/AGENTS.md`; Codex, Antigravity and Grok use native aliases to it. Claude's global `CLAUDE.md` imports it. Muse trusted project discovery and its earlier literal foreign-import limit are documented in RUNTIME-LOADING.md.
- Shared custom skills live under `/root/.agents/skills/`, with source ownership in agent-platform or education-platform. CLI-native plugin and account-synced skills retain their owners; they are not custom MCP definitions.
- Hindsight hooks are configured for Claude/Codex (`SessionStart`, `UserPromptSubmit`, `Stop`) and agy (`PreInvocation`, `Stop`). Muse/Grok have their own directly registered MCP client but no configured Hindsight lifecycle hooks. Do not copy Claude hook code into another runtime or claim identical auto-injection/retention.
- Existing Herdr hooks, model/permission/trust settings and native plugins were preserved. Codex work uses `codex --no-daemon` in Herdr terminals. Existing sessions were not restarted; new registration does not hot-replace their loaded tool catalogs.

## Verification and current-plan boundary

Evidence: `/home/palantirkc/work/agent-custom-mcp-audit/STATE.md`. Fifteen enabled native declarations passed configuration equivalence checks. Grok native inspection proves direct source discovery and disabled foreign MCP scanners; native MCP doctor passed command/start/handshake/tools-list for all three shared servers (Academy 44 tools, Daniel 5). This is protocol connectivity, not live domain reads, write acceptance, full five-model-session qualification, or Academy Rust parity.

The earlier Nov6 Muse/Claude-only Academy registration decision is superseded at the client-registration layer by the user's Nov7 all-custom CLI-agnostic requirement. Account serialization, missing save backend, domain permissions and the full Rust cutover gate remain unchanged. The TS install base, immutable releases and service pointers were not modified.

Recent broader checkpoints remain separate authority: `/home/palantirkc/work/ws-replan/STATE.md`, `preparation/lead-checkpoint-20261107/CHECKPOINT.md`, `/home/palantirkc/work/ontology-coengineering/STATE.md`, the Hindsight task STATE files and `/home/palantirkc/work/problem-edit-rust/STATE.md`. Apply durable interfaces and task-scoped boundaries now; do not pre-declare K1/K2 acceptance, 44/44 Rust qualification, PRIVATE visibility, archive ingestion or legacy deletion from a completion assumption.

Historical install/relocation details remain in their dated task states. Current filesystem and native discovery take precedence over old version, missing-CLI or registration snapshots.
