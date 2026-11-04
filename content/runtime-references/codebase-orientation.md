# Codebase and workspace orientation

Current-location guide, corrected 2026-11-02 against native Git and the user-adopted final Rust ZIP. An on-demand index, not an automatic loader or a source of live branch/provider/owner state.

Directory roles and exceptions: `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`.
Exact migration dispositions: that work item's `PATH-MAP.tsv`.
Human entry: `/home/palantirkc/WORKSPACE.md`.

| Task | Current entry |
|---|---|
| Education source | `/home/palantirkc/projects/education-platform`; clean main cc6e81f observed2026-11-02T14:28Z, matching the final review. Recheck the selected branch/worktree before editing |
| Agent-platform source | `/home/palantirkc/platforms/agent-platform` (moved from `projects/agent-platform` by WP-11 on 2026-11-02); existing implementation is legacy TypeScript, Rust `ap` is the approved target |
| Ontology-platform | Planned repository, absent at observation. Never run commands at an invented projects/ontology-platform path |
| Current Rust architecture/execution plan | `/home/palantirkc/work/ws-replan/CURRENT-PLAN.md` routes to the final review ZIP; current authority/owner/next action is STATE.md. Old PLAN.md, plan/01–08, WP ordering and Rust memories are legacy references |
| Memory selection/evaluation | `/home/palantirkc/work/mem-host-memory/STATE.md` (moved from `operations/hindsight-memory-research-20261029` on 2026-11-02 by explicit user instruction for this item) |
| Academy lessons | `/home/palantirkc/work/edu-lessons/STATE.md` is current; the retained October30 source handoff is `/home/palantirkc/work/edu-lesson-handoff/STATE.md` |
| Hindsight service | `/docker/hindsight/STATE.md`; before Hindsight actions apply HINDSIGHT_OFFICIAL_PER_CALL.md there. Actual provider/config comes from authorized service inspection |
| Current MCP wiring | `runtime-wiring.md`; Academy release.json is the active generation pointer |
| Original recovery | `/home/palantirkc/archive/INDEX.md`; archives have no current policy authority |

New task: `work/<scope>-<topic>/STATE.md`, with task-owned Git worktrees under `work/<id>/wt/<repo>/`. Final main repository root is `platforms/`; remaining projects/ rows are explicit transition exceptions. On2026-11-02, all contents of operations/ and top-level worktrees/ selected by the user were moved into work/ and both source roots were verified empty; use PATH-MAP.tsv and each destination STATE.md, not old task paths. Never assume a different planned target has already been created or moved. Operating several CLIs through Herdr: HERDR-ORCHESTRATION.md. Official documentation routes: OFFICIAL-DOCS.md. What each CLI loads and when: RUNTIME-LOADING.md.

Search the exact task/repository first with `rg` or `git grep`. Avoid unbounded host scans. Old `projects/palantirkc` is explicitly retired by the user; do not search its historical contents or select its harness instructions into current memory. Other old harness records are historical evidence only. Inspect a specific retired path only to reconcile a named live dependency or recover a specifically requested original.

Scope and user authority come from current instructions and task state. Hindsight gives past decisions; current source/configuration establishes current behavior. Keep private custody outside layout scans. Never infer safe deletion from a completed task label.
