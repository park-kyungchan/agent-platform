# Codebase and workspace orientation

Current-location guide, corrected 2026-11-04 against native Git and the user-adopted final Rust ZIP. An on-demand index, not an automatic loader or a source of live branch/provider/owner state.

Directory roles and exceptions: `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`.
Exact migration dispositions: that work item's `PATH-MAP.tsv`.
Human entry: `/home/palantirkc/WORKSPACE.md`.

| Task | Current entry |
|---|---|
| Education source | `/home/palantirkc/platforms/education-platform` (moved 2026-11-04). Recheck the selected branch/worktree before editing |
| Agent-platform source | `/home/palantirkc/platforms/agent-platform`; Rust `ap` source, public runtime guidance and generic skills; installed CLI `/usr/local/bin/ap` |
| Ontology-platform | `/home/palantirkc/platforms/ontology-platform`; Rust ontology source |
| Agent VPS operations module | `/home/palantirkc/platforms/agent-vps-operations` (formerly vps-operations); Rust `vps-ops` source |
| Education DC module | `/home/palantirkc/platforms/education-dc-gang-a` (formerly dc-gang-a) |
| Other renamed education modules | GitHub `park-kyungchan/education-problem-edit` and `park-kyungchan/education-academy-observation`; use each task STATE for its checkout |
| Current Rust architecture/execution plan | `/home/palantirkc/work/ws-replan/CURRENT-PLAN.md` routes to the final review ZIP; current authority/owner/next action is STATE.md. Old PLAN.md, plan/01–08, WP ordering and Rust memories are legacy references |
| Memory selection/evaluation | `/home/palantirkc/work/mem-host-memory/STATE.md` (moved from `operations/hindsight-memory-research-20261029` on 2026-11-02 by explicit user instruction for this item) |
| Academy lessons | `/home/palantirkc/work/edu-lessons/STATE.md` is current; the retained October30 source handoff is `/home/palantirkc/work/edu-lesson-handoff/STATE.md` |
| Hindsight service | `/docker/hindsight/STATE.md`; before Hindsight actions apply HINDSIGHT_OFFICIAL_PER_CALL.md there. Actual provider/config comes from authorized service inspection |
| Current MCP wiring | `runtime-wiring.md`; all five native CLIs directly register the shared custom servers (user requirement 2026-11-07). `/root/.local/share/academy-mcp-host-20261106/release.json` selects the current TS generation (owner `/home/palantirkc/work/edu-mcp-host-restore/STATE.md`); Rust activation still requires Stage 2 qualification. The older `/root/.local/share/academy-mcp-scoped-20261030` pilot is historical only |
| Original recovery | `/home/palantirkc/archive/INDEX.md`; archives have no current policy authority |

New task: `work/<scope>-<topic>/STATE.md`, with task-owned Git worktrees under `work/<id>/wt/<repo>/`. Platform and module main repository root is `platforms/`; modules use `<platform>-<module>`. `projects/` retains only the three D-9 repositories and archived academy-workbench custody checkout. Agent-harness-engineering and housing-board-module are deleted; math-research-module, gang-a, math-education and academy-workbench are archived. On2026-11-02, all contents of operations/ and top-level worktrees/ selected by the user were moved into work/ and both source roots were verified empty; use PATH-MAP.tsv and each destination STATE.md, not old task paths. Never assume a different planned target has already been created or moved. Operating several CLIs through Herdr: HERDR-ORCHESTRATION.md. Official documentation routes: OFFICIAL-DOCS.md. What each CLI loads and when: RUNTIME-LOADING.md.

Search the exact task/repository first with `rg` or `git grep`. Avoid unbounded host scans. Old `projects/palantirkc` is explicitly retired by the user; do not search its historical contents or select its harness instructions into current memory. Other old harness records are historical evidence only. Inspect a specific retired path only to reconcile a named live dependency or recover a specifically requested original.

Scope and user authority come from current instructions and task state. Hindsight gives past decisions; current source/configuration establishes current behavior. Keep private custody outside layout scans. Never infer safe deletion from a completed task label.
