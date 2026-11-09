# Native registration and service ownership

## One implementation, separate native clients

Custom MCP implementations and backend/domain state are CLI-agnostic. Register the same approved server directly in each CLI's supported native schema; vary supported client attribution where required. Another CLI's personal config or plugin cache is not shared registration authority. Current commands, release pointers and credential-file references belong to the actual service/native configuration, not a copied table of old launch paths.

| Client | Native configuration / custom-server section | Hindsight client attribution |
|---|---|---|
| Claude Code | `~/.claude.json` / `mcpServers` | `claude-code` |
| Muse | `~/.config/muse/settings.json` / `mcpServers` | `muse` |
| Codex | `~/.codex/config.toml` / `mcp_servers` | `codex` |
| Antigravity | `~/.gemini/config/mcp_config.json` / `mcpServers`; CLI-specific settings are separate | `antigravity-cli` |
| Grok Build | `~/.grok/config.toml` / `mcp_servers` | `grok-build` |

Shared custom names: `hindsight`, `academy_operations`, `daniel_prayer_operations`. Read only relevant config metadata and approved executable/arguments to reconcile a discrepancy; never print credentials or copy their values. Server registration, enabled config, successful protocol handshake, actual model availability, domain effects and qualified backend cutover are separate facts.

Hindsight coding-agent integration is installed under `/root/.hindsight/coding-agents`; its settings owner is `/root/.hindsight/coding-agent.json`. Hindsight bank/routing/path overrides remain native-owned. Academy and Daniel retain their service-owned launch/release/credential contracts; do not substitute a future Rust binary or infer write support from tool visibility. Academy uses one account/serialization boundary: only the current authorized lesson owner may perform its operations. Migration acceptance and account authority come from current domain STATE/native evidence, not a stale tool count or old release path.

## Vendor plugins and compatibility

Built-in and CLI-installed marketplace/account/plugin MCPs retain their native CLI owner. Do not copy them into the custom-server set, rewrite caches, globally disable features just to orient, or use one CLI as another's config authority. A user-authorized client feature exclusion must use that runtime's supported native control and actual readback; do not confuse it with uninstalling or disabling an account-wide connection. See OFFICIAL-DOCS for Codex remote-plugin and Claude ToolSearch distinctions.

Grok's custom MCPs are registered directly, with `[compat.claude] mcps=false` and `[compat.cursor] mcps=false`. Native inspection should identify the entries as its own config, with its own Hindsight attribution. Other compatibility and vendor features remain native-owned. Current STATE owns task-specific login/model-execution holds; inspection does not release them.

## Instructions, skills and hooks

Canonical instruction pointers/discovery limits are in [RUNTIME-LOADING.md](RUNTIME-LOADING.md). Shared skills and source owners are in [skills.md](skills.md). Keep one shared instruction source; do not build per-CLI policy copies, an MCP registry, custom loader or resident watcher.

Configured Hindsight lifecycle surfaces differ: Claude/Codex use SessionStart/UserPromptSubmit/Stop; Antigravity uses PreInvocation/Stop; Muse/Grok have registered MCP clients without configured Hindsight lifecycle hooks. Verify selected native config when that difference affects the task. A hook name is not proof of memory injection, session retention or successful record updates. Existing Herdr/native hooks and other settings retain their owners.

Codex runs natively in Herdr with `--no-daemon`. Registration/config changes do not hot-replace every already-loaded catalog; preserve existing sessions/daemon. Follow [CONTEXT-GOVERNANCE.md](CONTEXT-GOVERNANCE.md) for isolation, promotion and memory read/write boundaries. Tool availability never grants effects, evaluation data access or permanent CLI-brand roles.
