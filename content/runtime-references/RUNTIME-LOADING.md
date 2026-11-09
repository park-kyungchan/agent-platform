# Native runtime loading — host-specific boundaries

Shared instruction source: `/root/.agents/AGENTS.md`. Native per-runner pointers resolve/import it; local details are selected through [llms.txt](llms.txt). Do not preload this directory or construct another loader. [OFFICIAL-DOCS.md](OFFICIAL-DOCS.md) defines bounded official fallback; publisher discovery/skills/settings documentation owns changing native behavior.

## Discovery differences to verify

| Runtime | Host entry / relevant boundary |
|---|---|
| Codex | `~/.codex/AGENTS.md` points to canonical. Native project chain and overrides are bounded; outside Git, cwd-only discovery can omit ancestor task instructions. Explicitly read assigned STATE/TASK after start/resume/compaction. Use `--no-daemon` in Herdr terminals. |
| Claude Code | `~/.claude/CLAUDE.md` imports canonical. Project AGENTS/CLAUDE selection and ancestor files are native and conditional; children can omit project guidance. A global import does not prove every project file or child loaded. |
| Muse | Trusted project discovery is native. Observed foreign `@/root/.agents/AGENTS.md` can remain literal rather than expanded; explicitly supply/read canonical plus task STATE and required contracts. Do not infer permissions or file absence from import failure. |
| Antigravity (`agy`) | `~/.gemini/AGENTS.md` and GEMINI.md point to canonical; discovery is repository-bound. Two aliases do not prove two expansions. Native rule includes use `@[label](path)`; `@filename` is a reference, not an include. Deeper module rules require explicit task selection. |
| Grok Build | `~/.grok/AGENTS.md` directly points to canonical. Claude-compatible discovery may find another pointer; inspection is not proof of duplicate model injection. Foreign MCP scanners are disabled; other compatibility/native feature ownership is preserved. Actual login/model execution requires current task authority. |

These are host routing constraints and observed limitations, not a version-independent description of every upstream loader. For an affected behavior, use exact installed native evidence; do not upgrade inspection/config proof into full session qualification.

## Skills, tools and per-turn context

Separate catalog metadata (name/description), body expansion, actual tool read and useful task application. Distinguish user turns from multiple model/tool rounds in one turn. Track file rereads, retained history, cache creation/read, fresh input and output/reasoning separately. Resume/compaction may reconstruct prior content; neither a path nor a success exit establishes correct reconstruction.

Claude ToolSearch exclusion and Codex local-versus-remote plugin activation traps are in OFFICIAL-DOCS. Do not run a restrictive probe and label its tool inventory the default baseline. Native `debug prompt-input`, an installed catalog or a settings file may omit late-added context; inspect the exact owned session when that distinction matters. Do not inspect unrelated transcripts or expose credentials.

Antigravity official discovery documents per-turn rule/skill rereading; that does not establish this host's exact deduplication, token cost or resume reconstruction. Muse literal imports, unqualified model behavior and incomplete native payload visibility remain evidence gaps until a bounded relevant case resolves them. No universal cross-CLI frontmatter switch or identical native discovery is assumed.

## Continuity and memory

CLI sessions and local task STATE/TASK own short-term execution; Hindsight owns authorized reusable mid/long-term context. Source owns implementation/contracts, domain systems own operations/recovery. Read [CONTEXT-GOVERNANCE.md](CONTEXT-GOVERNANCE.md) for promotion, correction, provenance and isolation; [HINDSIGHT-RUNTIME.md](HINDSIGHT-RUNTIME.md) for server semantics.

A registered Hindsight MCP is not a lifecycle hook. Native hook events/configs differ; inspect the selected owner via [runtime-wiring.md](runtime-wiring.md). Injection, retained sessions, extraction, consolidation, periodic guidance and derived-page refresh are separate mechanisms. Do not infer transcript retention from Stop or page refresh settings. Do not export hook-disable variables to make a worker appear isolated; authorized evaluation isolation uses Hindsight's native path overrides with readback.

A resumed session does not renew authority. Read current STATE/holds and changed source before effects. For concurrent writers and compact handoffs use [global-runtime-routing.md](global-runtime-routing.md); for authorized panes use [HERDR-ORCHESTRATION.md](HERDR-ORCHESTRATION.md). Preserve unrelated sessions and the shared daemon. Model identity, context limits, trust and allowed effects come from the actual assignment/native session, never a CLI name or process label.
