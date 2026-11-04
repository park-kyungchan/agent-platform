# Runtime wiring (current, on demand)

Current readback: 2026-11-02 by Codex during user-authorized preparation across Muse, Claude, Codex and Antigravity CLI. The November1 wiring remains historical installation evidence. Current Rust authority: `/home/palantirkc/work/ws-replan/CURRENT-PLAN.md` and STATE.md; final-review S02/S03 describe future render/install, not delivered tools. Layout/current exceptions: `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`. No native configuration/service was changed by this documentation audit.

## Instructions: one canonical text, pointer files
- Canonical: `/root/.agents/AGENTS.md` (user-approved runtime core v2 + unchanged Hindsight section + Codex-only addenda). Native pointer files remain pointers. The exact approved core source is runtime-core.v2.md here; preserve it as an immutable version and fix forward for later wording changes.
- agy: `~/.gemini/AGENTS.md` is a symlink to it. Codex: `~/.codex/AGENTS.md` is a symlink to it.
- Claude Code: `~/.claude/CLAUDE.md` holds only `@/root/.agents/AGENTS.md` (import).
- Muse: no separately maintained global instruction copy; prior Muse model-session evidence saw canonical guidance via the Claude foreign-personal chain. Native trusted project rules and flags can differ; use explicit canonical/STATE paths in handoffs. See RUNTIME-LOADING.md.
- Gemini CLI and Grok are absent from PATH and outside the user's confirmed four-CLI matrix. Both `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md` still resolve to canonical and are documented Antigravity inputs; their presence is not proof of an installed Gemini CLI, and deduplicated injection is unverified. No pointer was removed. This preparation does not claim fresh-session loading for all four active CLIs.
- Hermes (container `hermes-agent-2q3y-hermes-serve-1`): `/root/.agents` is mounted read-only at `/opt/host-agents` (compose line added 2026-11-01). Hermes `SOUL.md`, memory and profiles are untouched; Hermes is used by the VPS host on demand, no separate owner.

## MCP registrations (same server file, harness id differs)
| Runtime | hindsight (`HINDSIGHT_MCP_HARNESS`) | academy_operations |
|---|---|---|
| Claude Code (`~/.claude.json`) | claude-code | scoped / stable launcher.mjs |
| Codex (`~/.codex/config.toml`) | codex | scoped / stable launcher.mjs |
| agy (`~/.gemini/config/mcp_config.json`) | antigravity-cli | scoped / stable launcher.mjs |
| Muse (`~/.config/muse/settings.json`) | muse (MCP tools only; no hooks, no auto-inject) | scoped / stable launcher.mjs |

- Server: `node /root/.hindsight/coding-agents/dist/mcp-server.js`; the harness id is a free string. Hooks exist only for Claude, Codex and agy.
- academy_operations: private academy operations server (lesson history, DailyRecord authoring, homework/test preparation). Install `/root/.local/share/academy-mcp-scoped-20261030`. Codex used to point at `academy-mcp-yoo-20261030`; that install was deleted on 2026-11-01 (user decision).
- All four host selectors invoke `/root/.local/share/academy-mcp-scoped-20261030/launcher.mjs`; this preparation read their command/args back without printing credentials. Codex's unrelated daniel_prayer_operations registration remains present. Hermes keeps its existing same-owner relay/socket/native boundary. At2026-11-02T14:28Z, `release.json` selected `main-20261102-cc6e81f`, schema academy.mcp-release/1, pointer SHA256b5159c1bd815b87d67a4488dd4e978c051b777686856dd542b9a5452ad9aad3b. Earlier path-relocation/b0865d1 generations and loaded-digest snapshots below are historical. Pointer/registration readback is not a fresh model-session, complete graph or semantic parity check. Existing sessions are not hot-replaced.
- The retained shared-lock/config task is now `/home/palantirkc/work/edu-class-operations`. On2026-11-02 the user authorized the full path cutover after work stopped: all three live endpoint lock arguments moved together in forward generation `path-relocation-20261102-b0865d1`, holding the same lock inode across directory/pointer replacement. The original b0865d1 bridge bytes, owner/admission, common launcher and Hermes socket were preserved; host/Hermes42tools, native endpoint inventories and one actual read were verified. Old immutable release definitions retain historical paths and require coordinated directory/pointer rollback before reuse. Host and container artifact paths are separate; never pin a recreated container's old ID.

## Removed on 2026-11-01 (legacy)
`academy-mcp-dailytest-20261030`, `academy-mcp-guidance-20261030`, Muse `academy-runtime-20261023-{backup,retired}-*`, `.agents/skills-archive`, `runtime-references/backup-20261029`, dated `.bak-*`/`.before-*` copies of instruction, settings and hook files, the `/tmp/hindsight-native-install-wSQAJVAx/pilot` entries in Codex `[projects]` and `~/.hindsight/coding-agent.json`. Kept: this session's `*.bak-20261101*` rollback copies and the installer's `*.hindsight-backup` files.

## Not verified yet
Fresh model sessions for every remaining CLI were not launched during relocation. The fresh shared host launcher and Hermes relay passed identity/42-tool/preflight checks; all four native endpoint inventories and one actual DailyRecord read passed. These checks do not prove universal instruction injection or that every existing client loaded new code. Earlier Muse/lesson-successor model-session evidence belongs to its dated task reports. Official documentation routes for every runtime: OFFICIAL-DOCS.md; Herdr coordination: HERDR-ORCHESTRATION.md.

## Custom skills (on demand)

Shared custom source and verified native discovery: [skills.md](skills.md). This does not change native hook/MCP wiring or claim fresh-session injection.
