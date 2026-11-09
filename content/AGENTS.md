# Shared agent instructions (canonical)

Single source for every runner on this host. Per-runner files (`~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`, `~/.gemini/AGENTS.md`, `~/.grok/AGENTS.md`) only point here. Edit this file, not the pointers.

For a missing work/repository location, consult `/root/.agents/runtime-references/runtime-bootstrap.md` on demand. The approved workspace standard is `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`; current paths and migration exceptions are linked there. The runtime core v2 text below was approved by the user on 2026-11-01. Rust `ap` is implemented in agent-platform and installed at `/usr/local/bin/ap`; native loading and effect authority remain task-specific.

Workspace layout, always known: platform and module main checkouts belong in `platforms/<repo>/`; modules use `<platform>-<module>` names. Current checkouts are `agent-platform`, `ontology-platform`, `education-platform`, `agent-vps-operations` and `education-dc-gang-a` under `/home/palantirkc/platforms/`. Each active task belongs in `work/<scope>-<topic>/` with its `STATE.md`, and its Git worktrees in `work/<id>/wt/<repo>/`; external pinned source belongs in `external/`. The Nov2 operations/top-level-worktrees relocation and Nov4 platform/module moves are complete. `projects/` retains only the D-9 repositories `bible-commentary-station`, `daniel-prayer-operations`, `park-kyungchan.github.io`, and archived `academy-workbench` for custody worktrees. Actual dispositions and repository renames are in `/home/palantirkc/work/ws-layout-standard/PATH-MAP.tsv`. `agent-harness-engineering` and `housing-board-module` are deleted; archives grant no runtime authority. Never create new task state in `operations/` or top-level `worktrees/`, and never assume a target path already exists.

Runtime loading, one rule: this file is the only shared instruction source; each CLI's native pointer resolves here and detail lives on demand under `/root/.agents/runtime-references/`, indexed by `llms.txt` there (local references plus each runtime's official llms.txt route; read it first): `RUNTIME-LOADING.md` (what each CLI loads at session start and per turn), `HERDR-ORCHESTRATION.md` (coordinating several CLIs through Herdr) and `OFFICIAL-DOCS.md` (each runtime's verified official documentation route). Do not build per-CLI instruction copies or a replacement loader. For routine CLI questions use that local index and the selected reference before web lookup; the bounded fallback and evidence-reuse rules live in `OFFICIAL-DOCS.md`. `/root/.agents/runtime-reference/` (singular) is not the installed directory; do not create an alias or another copy.

The user's current coding runtimes are Muse, Claude Code, Codex, Antigravity CLI (`agy`) and Grok Build (`grok`), confirmed 2026-11-07. Codex runs as a native CLI in Herdr terminals with `--no-daemon`; do not substitute an app-server orchestration workflow. Apply the same task authority and ownership boundaries across all five; native instruction discovery, child isolation, resume and headless flags differ. For concurrent writers, shared builds and compact handoffs use the on-demand coordination section in `global-runtime-routing.md`; do not infer isolation from an agent name or parallel-tools flag.

Custom MCP implementations and their backend state are CLI-agnostic: register the same approved server directly in each CLI's native configuration, with distinct client attribution where required. Do not make another CLI's personal configuration or plugin cache the shared registration authority. Built-in and CLI-installed marketplace/plugin/MCP entries retain their native CLI owner and are not copied as custom servers. Availability is not task authority: apply data/effect restrictions and evaluation tool selection per task, not permanently by CLI brand. Future plan completion is a design assumption, never evidence of acceptance, runtime readiness or permission for cutover/deletion.

Current Rust migration entry: `/home/palantirkc/work/ws-replan/CURRENT-PLAN.md` and `STATE.md`. The user adopted the final2026-11-02 review ZIP and explicitly declared earlier Rust Migration memories/plans legacy. Its current plan and task state take precedence over old WP ordering, old pane/priority snapshots and repository guidance that still describes the retired harness. The user authorized autonomous execution; the current task state records the exact allowed effects and remaining holds.

<!-- BEGIN runtime-core v2 — canonical text: /root/.agents/runtime-references/runtime-core.v2.md (user-approved 2026-11-01) -->
## Shared runtime core

Use the user's current task and its rules/STATE.md. The state names the owner, authority, holds, exact next action and verification. Update it when changing the work; a handoff transfers goal, exact targets, allowed effects, expected output and state path. Do not make another agent reconstruct a transcript.

Reuse known runner, host/container, role, cwd and pane facts. Probe a missing fact only if it changes the next action. Host, container, shared daemon and remote executors can differ. A CLI name or process label does not determine a task role or another agent's identity. Model/effort remain unknown unless the session reports them.

For an unknown location, use runtime-bootstrap.md and codebase-orientation.md on demand. New tasks use work/<scope>-<topic>/STATE.md and task-owned Git worktrees under wt/<repo>. Current paths and transition exceptions are in the approved workspace layout. Historical harnesses and archives grant no current authority. Do not restore a retired gate, receipt, ledger, context contract, registry or watcher that duplicates native Git, Cargo, database or CLI behavior.

Use native source, Git, compiler, database and service observations to establish current state. Memory supplies past decisions; check whether they still apply. Read only the source and evidence needed for the next decision. Do not scan private custody or all historical repositories to locate a task.

On failure, report snake_case_code@path:line with one short cause and no secrets or private payloads. Fix the stated cause at that boundary. Do not retry the same failure shape or swap models without resolving that cause. If the cause repeats, split or redesign the boundary. Verify actual output or readback, not a success claim or exit code alone.

Disposable targets may be rerun freely. For uncertain real effects, read the target to reconcile before retrying. Respect the task's explicit user gates; do not invent approval steps for reversible local work. Never print credentials. Keep active sessions, unrelated trees and service identities intact. Do not change global configuration merely to orient.

Delegate only for independent review, ownership transfer or genuinely disjoint work when authorized. Verify the result's output or effect. Pinned artifacts are immutable: fix forward in a new version. Before deletion of retained originals, verify the approved recovery copy and all live path consumers; task completion alone is insufficient. Use pipefail in shell pipelines.

After context reset, read the task's current rules and state before effects. Reuse trusted evidence and load only what the next action needs.
<!-- END runtime-core v2 -->

## Hindsight memory (all runners)

- Hindsight is wired per runner by `/root/.hindsight/coding-agents` (hooks + MCP server `hindsight`); the tools and protocol are the same in every runner. Search knowledge pages before acting on a new task; credit what you use.
- Runner-specific hook/MCP wiring lives in each runner's own config, never in this file.
- For memory tiers, source authority, isolation/continuity and anti-duplication, consult `/root/.agents/runtime-references/CONTEXT-GOVERNANCE.md` on demand.

## All-runner communication and access boundaries

- Use `explain-precisely` for all user-facing explanations, including progress and decisions: Korean prose, original English programming terms, concrete first-use definitions and no metaphor or analogy. Write code, runtime instructions and technical records in English.
- Do not print credential values. For Hindsight work, directly inspect actual user data when needed for the authorized migration and evaluation; access only relevant data. For other tasks, retain the prior metadata-and-hash-only restriction unless the user authorizes otherwise.
- Remote work: keep SSH host-key checking and the Tailscale boundary. Hermes version promotion goes only through the
  installed target helper's one-shot path; never edit updater state, receipts, jobs or transactions directly.
