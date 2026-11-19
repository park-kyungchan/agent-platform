# Common harness mechanism boundary

**Scope approved by the user — 2026-11-04.** Initially prepared against
`aaefe063dbbc83f23a4f00c397c80ce60bca2003`; rechecked against agent-platform main
`42055b7b70d52e6aeb02a84136367fdc09748f45`. This is autonomy goal 8's permanent mechanism
boundary for a small Rust, CLI-agnostic harness on Herdr, used by Claude Code,
Codex, Muse and Antigravity CLI (`agy`). It assigns no role, priority, model or
work category to any CLI. Scope approval does not authorize implementation,
deployment or host configuration changes. PR #98 merged on 2026-11-05 at `2dd51af`; the current task STATE records acceptance and deployment. This milestone grants no new host effects.

## Purpose and scope

The shared layer supplies a few explicit mechanisms so an agent can find common
guidance and continue a task from clear state. Each CLI
continues to execute the task with its native tools, trust, sessions, hooks,
permissions and context handling. Herdr supplies terminals and coordination;
Hindsight supplies mid/long-term context memory; Ontology and domain modules own
operational state. `ap` does not require a running Ontology. [Z1][G1]

“Harness” here names that boundary, not a new runtime loop. Current executable
ownership is `crates/ap`; shared task/handoff/layout practices are documented
conventions. There is no Rust task allocator, handoff parser, memory engine or
session supervisor hidden behind those conventions. [A1]

## Shared mechanism inventory

| Mechanism | Current implementation or convention | Boundary proposed for the shared layer |
| --- | --- | --- |
| Canonical guidance | `/root/.agents/AGENTS.md` is the deployed shared instruction source; `content/` is its reviewed source, installed by `scripts/install-content.sh`. | Maintain reviewed source/distribution. Keep runtime details on demand. No second instruction loader, per-CLI instruction copies or automatic content rewrite. [G1] |
| Task state and handoff | Canonical guidance requires a short task `STATE.md` and a five-part handoff. `ap` does not parse, allocate or store tasks. | Share a human-readable handoff convention. The task owner updates it; a receiving agent explicitly reads current authority. No registry, receipt gate, automatic context contract or replacement Git ledger. [G1][G2] |
| Memory routing | Official Hindsight coding-agents integration supplies hooks/MCP and bank routing independently of `ap`. | Document the official shared-bank route and platform provenance. Agent-initiated reads remain visible and purposeful. No Rust Hindsight client, database, proxy, ingestion engine or transcript pipeline is implemented or proposed here. [H1][G3] |
| Workspace layout | `work/<scope>-<topic>/STATE.md`, task-owned `wt/<repo>/`, main platform checkouts under `platforms/` and pinned sources under `external/` are workspace conventions. | Use the approved layout and actual path map. Record real worktree/cwd identity in handoffs. No automatic relocation, Git cleanup or task-directory allocator. [G1][G2] |
| Herdr coordination | Herdr owns workspace/tab/pane topology and agent control. A Lead can use native Herdr tools within the task's authority. | Use caller-bound IDs and actual output/readback; never equate idle/done with verification. [G2] |

The rows describe coordination/documentation boundaries, not new executable modules.
Any later proposal to add code needs an actual consumer and a separate scope.
The mechanism-only boundary is permanent: do not re-implement native CLI
features or official Hindsight capabilities in Rust.

## What remains native to every CLI

| Native concern | Ownership rule |
| --- | --- |
| Instruction discovery | The CLI selects its global/project/override files, applies size/trust rules and decides when to read them. No link or import can override native discovery or prove what entered context. |
| Hooks and MCP wiring | The native CLI and the relevant official plugin own registration, event names, trust, process startup and tools. `ap` does not install hooks or rewrite MCP configuration. |
| Sessions, resume and compaction | Use the CLI's exact supported session identifier and native resume surface. Resume/compaction is not a task-authority refresh; explicitly reread current STATE. No cross-CLI transcript conversion or universal resume token. |
| Child execution and isolation | Native children may share cwd/config/context. Verify actual Git worktree and allowed write set before concurrent writes. A child name or parallel-tool flag is not filesystem isolation. |
| Models and effort | The current task assignment and actual native runtime determine these values. Do not infer them from CLI brand, pane name or old quota snapshots; unknown remains unknown. |
| Permissions and cancellation | Native permission/trust modes remain in force. No shared wrapper broadens approval settings, bypasses trust or assumes that canceling a CLI proves a remote effect did not occur. |
| Tools and process lifecycle | CLIs keep their tool semantics, startup, background children and exit behavior. Herdr controls terminals; domain services reconcile effects. A successful prompt submission or CLI exit is not a domain verdict. |

These rules are uniform across all four CLIs. Their concrete native interfaces
differ, so the common layer preserves those differences rather than translating
flags or editing vendor stores. [G2][G3][G4]

## A pane is not an agent identity

Herdr owns its server and topology. Workspace/tab/pane IDs are scoped to the
selected server/machine. A pane is a terminal, not an agent identity; a recognized
agent is its present occupant. The common layer must not infer the task owner,
privilege or program model from either. Outside Herdr, no guessed pane is needed
for ordinary native CLI work. [G2]

## Task continuity and memory

Use the current task's `STATE.md`, not a historical memory or another agent's
transcript, to communicate owner, authority, holds, exact next action and
verification. The five handoff parts are **goal, exact targets, allowed effects,
expected output and state path**. Exact targets should include the real repo,
worktree/branch and selected contract; record source revision and verified output
when handing over completed work. This is ordinary task documentation, not a
new machine-enforced schema. [G1][G2]

Concurrent writers need disjoint targets or explicit ownership transfer, and
shared Cargo/lock/interface changes and expensive builds need their current
assigned owner. Parallel tool calls do not establish independent writers or
isolated build resources. Keep unrelated sessions, trees and service identities
intact. A handoff transfers only its stated authority. [G1][G3]

The user's memory direction is one shared Hindsight bank for all agents, with
`platform:agent`, `platform:education` and `platform:ontology` topic tags and
platform-focused mental models. Official `bankId`/`mapPathToBank`, `retainTags`
and native integration settings are the supported surfaces; do not build those
features again in Rust. Actual bank ID and configuration are owned outside this
draft, and no routing change is made here. [H1]

Memory records past decisions; current code/native sources establish current
state. Agents search relevant pages before new work and read current task/source
evidence before effects. Correct stale memory through an authorized Correction
document and official refresh/readback, preserving source provenance. A shared
bank or topic tag is not permission to access a domain or to copy its state into
memory. Ontology objects remain source-qualified operational facts in their own
store. [G1][H1][Z1]

Memory retrieval, transcript retention, extraction/consolidation, page refresh
and an already-loaded model context are separate events. A hook registration
does not prove execution. Short-lived hooks reread config on invocation; the
persistent MCP process generally reads it at startup. Muse's dated configuration
has MCP with no configured Hindsight lifecycle injection hook; do not claim all
four runtimes inject or retain identically. Tagged models can miss shared
untagged observations, so platform scope requires actual readback, not just a
tag name. No retention or refresh operation is performed here. [G3][H1][H2]

The [herdr](../content/skills/herdr/SKILL.md) skill owns isolation/continuity of a pane
handoff, and [the always-loaded file's specification](../specs/content/agents-file.md)
owns source authority. Task handoffs follow [lead](../content/skills/lead/SKILL.md).
Task topology/worktrees win over generic pane examples; assigned Leads use one tab
per worker. Use the worker closure of [lead](../content/skills/lead/SKILL.md) (section "Your own limits")
for task-local output/effect reconciliation.
These are references to existing conventions, not new executable mechanisms.

## Rules preserving agent autonomy

1. Apply the same task authority to all CLIs; choose tools and execution within
   that authority through the current native runtime. No CLI receives a permanent
   role, priority or mandatory delegation target from this boundary.
2. Run shared mechanisms explicitly and on demand. Do not add a resident session
   loop, scheduler, watcher, mandatory pane hook or context-injection controller.
3. Keep native trust and configuration owners intact. Preserve unrelated edits
   and vendor hooks.
4. Report evidence at its boundary: static support, installation, native loading,
   runtime output and accepted task outcome are different claims.
5. Keep task continuity in short STATE/handoff files, native sessions, native Git
   and official Hindsight. Do not revive retired Work identities, review gates,
   receipts or registries under a Rust name.
6. Treat domain reads, writes and uncertain effects under their domain authority
   and recovery rules. Neither pane identity nor memory can authorize an action.

These are mechanism constraints grounded in the adopted proposal and canonical
runtime core. They leave task execution and native tool choice with the agent
and user. [Z1][G1]

## Source basis

Current code was read, not built or installed, for this documentation change.
Source links are relative to this repository; host references below are dated
operating guidance. The official Hindsight checkout was read at the pinned
revision, including system-eval tests 01, 06 and 10. No eval was executed.

[A1]: ../crates/ap/src/main.rs "Current command dispatch"
[G1]: ../content/AGENTS.md "Canonical runtime core and approved task/layout conventions"
[G2]: ../content/skills/herdr/SKILL.md "Pane handoffs and Herdr lifecycle meanings"
[G3]: ../specs/content/agents-file.md "What a session loads, by its CLI's pointer file"
[G4]: ../content/OFFICIAL-INDEXES.md "The tools' official indexes"
[H1]: https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-integrations/coding-agents/README.md "Official routing/config lifetime, provenance, retention and observation scopes; installed package 0.8.2"
[H2]: https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-docs/docs/developer/api/knowledge-pages.mdx "Official model/page filtering and refresh semantics; server documentation 1.1.0"
[Z1]: /home/palantirkc/work/ws-replan/inputs/final-review-20261102/extracted/rust_migration_reassessment_2026-11-02/PROPOSAL.md "Adopted ZIP proposal sections 4.2, 6 and 9; original ZIP SHA256 964845e260a25350823fb42d38a60e7cba973f8af607e10b505727d3bf78915c"

The host counterparts read for this draft are `/root/.agents/AGENTS.md` and the
reference files then installed beside it, since removed: their rules in force are in
`specs/content/agents-file.md`, `specs/ap/run.md`, the skill `herdr` and `content/OFFICIAL-INDEXES.md`.
Hindsight policy is `/docker/hindsight/HINDSIGHT_OFFICIAL_PER_CALL.md`, with
`/docker/hindsight/{AGENTS,README}.md` and the installed coding-agents skill.
Repository-owned copies are reviewed distribution content, not a replacement
loader or evidence that any currently running session consumed them.

## Approved decisions — 2026-11-04

1. **Q1 — Permanent mechanism-only scope.** Keep task/handoff/layout and memory
   routing as conventions and native integrations. Do not add Rust replacements
   for native CLI features or Hindsight.
2. **Q2 and Q3** concerned the instruction installer and the content proposal
   and diagnosis; those verbs of `ap` are removed, and so are these two answers.
3. **Q4 — Pane heuristics only on real demand.** Preserve current lookup.
   Require a concrete consumer and its actual native session surface before
   proposing another CLI-specific heuristic.

These answers assign no role to a CLI. The Lead may apply the proposed document
edits through worker, review and CI, then perform the authorized expected-head
merge. This document grants no host changes or operational cutover.
