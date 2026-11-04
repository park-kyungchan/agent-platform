# Common harness mechanism boundary

**DRAFT — user approval pending.** Prepared 2026-11-04 against agent-platform
`aaefe063dbbc83f23a4f00c397c80ce60bca2003`. This is autonomy goal 8's boundary
proposal for a small Rust, CLI-agnostic harness on Herdr, used by Claude Code,
Codex, Muse and Antigravity CLI (`agy`). It assigns no role, priority, model or
work category to any CLI. Review does not authorize implementation or deployment;
the resulting PR must remain open for the user's approval.

## Purpose and scope

The shared layer supplies a few explicit mechanisms so an agent can find common
guidance, identify its caller pane and continue a task from clear state. Each CLI
continues to execute the task with its native tools, trust, sessions, hooks,
permissions and context handling. Herdr supplies terminals and coordination;
Hindsight supplies mid/long-term context memory; Ontology and domain modules own
operational state. `ap` does not require a running Ontology. [Z1][G1]

“Harness” here names that boundary, not a new runtime loop. Current executable
ownership is `crates/ap`; shared task/handoff/layout practices are documented
conventions. There is no Rust task allocator, handoff parser, memory engine or
session supervisor hidden behind those conventions. [A1][A2][A3]

## Shared mechanism inventory

| Mechanism | Current implementation or convention | Boundary proposed for the shared layer |
| --- | --- | --- |
| Canonical guidance | `/root/.agents/AGENTS.md` is the deployed shared instruction source. The fixed public `content/` inventory is compiled into `ap`; `ap render content` proposes content and `ap doctor content` compares explicit installed paths. | Maintain reviewed source/distribution and diagnose drift. Keep runtime details on demand. No second instruction loader, per-CLI instruction copies or automatic content rewrite. There is currently no content apply selector. [A3][G1] |
| Native instruction adapters | `ap render` is pure JSON proposal generation. `install plan`, `apply`, `recover` and `doctor inspect` handle explicit supported profiles and owned instruction entries. | Preserve unknown/unrelated settings, hooks, MCP and vendor trust. Coordinate actual writers and use expected-before checks and command-local recovery. A ready path check does not prove native instruction consumption. [A1][A2][A4] |
| Caller-pane lookup | Linux `ap pane` resolves through Herdr/process/terminal evidence; it also retains explicitly warned Codex session heuristics. | Return a bounded observation of pane identity. No agent/task authorization, session creation, role inference, pane lifecycle control or universal cross-CLI session identity claim. [A5] |
| Task state and handoff | Canonical guidance requires a short task `STATE.md` and a five-part handoff. `ap` does not parse, allocate or store tasks. | Share a human-readable handoff convention. The task owner updates it; a receiving agent explicitly reads current authority. No registry, receipt gate, automatic context contract or replacement Git ledger. [G1][G2] |
| Memory routing | Official Hindsight coding-agents integration supplies hooks/MCP and bank routing independently of `ap`. | Document the official shared-bank route and platform provenance. Agent-initiated reads remain visible and purposeful. No Rust Hindsight client, database, proxy, ingestion engine or transcript pipeline is implemented or proposed here. [H1][G3] |
| Workspace layout | `work/<scope>-<topic>/STATE.md`, task-owned `wt/<repo>/`, main platform checkouts under `platforms/` and pinned sources under `external/` are workspace conventions. | Use the approved layout and actual path map. Record real worktree/cwd identity in handoffs. No automatic relocation, Git cleanup or task-directory allocator. [G1][G2] |
| Herdr coordination | Herdr owns workspace/tab/pane topology and agent control. A Lead can use native Herdr tools within the task's authority. | Use caller-bound IDs and actual output/readback; never equate idle/done with verification. `ap pane` only resolves identity and cannot close a pane or certify child cleanup. [A5][G2] |

The first three rows describe existing Rust functionality. The remaining rows
describe coordination/documentation boundaries, not new executable modules.
Any later proposal to add code needs an actual consumer and a separate scope.

## What remains native to every CLI

| Native concern | Ownership rule |
| --- | --- |
| Instruction discovery | The CLI selects its global/project/override files, applies size/trust rules and decides when to read them. An `ap` link/import cannot override native discovery or prove what entered context. |
| Hooks and MCP wiring | The native CLI and the relevant official plugin own registration, event names, trust, process startup and tools. `ap`'s instruction adapter does not install hooks or rewrite MCP configuration. |
| Sessions, resume and compaction | Use the CLI's exact supported session identifier and native resume surface. Resume/compaction is not a task-authority refresh; explicitly reread current STATE. No cross-CLI transcript conversion or universal resume token. |
| Child execution and isolation | Native children may share cwd/config/context. Verify actual Git worktree and allowed write set before concurrent writes. A child name or parallel-tool flag is not filesystem isolation. |
| Models and effort | The current task assignment and actual native runtime determine these values. Do not infer them from CLI brand, pane name or old quota snapshots; unknown remains unknown. |
| Permissions and cancellation | Native permission/trust modes remain in force. No shared wrapper broadens approval settings, bypasses trust or assumes that canceling a CLI proves a remote effect did not occur. |
| Tools and process lifecycle | CLIs keep their tool semantics, startup, background children and exit behavior. Herdr controls terminals; domain services reconcile effects. A successful prompt submission or CLI exit is not a domain verdict. |

These rules are uniform across all four CLIs. Their concrete native interfaces
differ, so the common layer preserves those differences rather than translating
flags or editing vendor stores. [G2][G3][G4]

## Current instruction dispositions, without role assignments

The AP-08 table is a consumer-support matrix, not a capability ranking. The current
Rust `Scope` variants are `Codex`, `Claude`, `Both` and `Agy`; `Both` means the
existing Codex+Claude pair, not all four CLIs. There is no Muse install scope.
The additive agy render profile is optional; omission preserves the earlier
render contract. [A2][A4]

| CLI | Preserved mechanism and qualification limit |
| --- | --- |
| Codex | Canonical instruction link, with native override/project discovery preserved. Shared installation owns only its supported entry. Exact-session resume, hooks and MCP remain native. |
| Claude Code | Exact canonical `@` import, with native instruction selection and project/child behavior preserved. The common layer does not assume loaded bytes equal source bytes. |
| Antigravity CLI (`agy`) | Two direct canonical links, `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md`. Keep both. Raw current input and alias deduplication are unverified; the user accepted the fresh-session loading gap as a disclosed limit. Acceptance of that limit is not proof of loading. |
| Muse | AP-08 `retained_native_mechanism`: trusted project discovery and explicit canonical/task-STATE handoff, with no invented global adapter. Foreign-personal and child flags can change the input. |

This matrix follows [AP-08 dispositions](ap-08-dispositions.md), including its
2026-11-04 final disposition, and the native source. The dated runtime loading
map records installed-version observations; it is not a promise about the latest
upstream release or every currently running process. This draft launches no
fresh native sessions and changes no installed configuration. [A4][G3]

Both instruction and content doctor reports deliberately retain
`native_loading:"unverified"`, `hooks_executed:0` and `p6_complete:false`.
Zero hooks executed by doctor does not mean vendor hooks are unused or removable.
A new install needs its own authority and affected-session evidence; existing
source support and old already-correct results do not imply a new apply. [A2][A3]

## Pane identity is a bounded lookup

`ap pane` requires `HERDR_ENV=1` for lookup. It compares process ancestry,
caller-terminal association and reported session evidence. Default lookup may
fall back to a warned resume/new-session candidate; `--exports` refuses those
heuristics. `--focused` reports UI focus and must not be used as caller identity.
Non-focused results are checked again against fresh pane/process association.
Inherited pane variables alone cannot bind a caller. The residual race after
returning an ID remains. [A5]

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

## Rules preserving agent autonomy

1. Apply the same task authority to all CLIs; choose tools and execution within
   that authority through the current native runtime. No CLI receives a permanent
   role, priority or mandatory delegation target from this boundary.
2. Run shared mechanisms explicitly and on demand. Do not add a resident session
   loop, scheduler, watcher, mandatory pane hook or context-injection controller.
3. Keep native trust and configuration owners intact. Scope installation to
   supported owned entries and preserve unrelated edits and vendor hooks.
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

[A1]: ../crates/ap/src/main.rs "Current command dispatch and pure render entrypoint"
[A2]: ../crates/ap/src/native/types.rs "Explicit native profiles, scopes, apply/recovery and doctor reports"
[A3]: ../crates/ap/src/native/content.rs "Read-only content proposal/diagnosis; fixed inventory in content_inventory.rs"
[A4]: ap-08-dispositions.md "Four CLI support dispositions and final retirement/loading limits"
[A5]: ../crates/ap/pane/README.md "Pane resolver, heuristics, rechecks and compatibility limits; implementation in src/pane.rs"
[G1]: ../content/AGENTS.md "Canonical runtime core and approved task/layout conventions"
[G2]: ../content/runtime-references/HERDR-ORCHESTRATION.md "Native topology, handoffs and lifecycle meanings"
[G3]: ../content/runtime-references/RUNTIME-LOADING.md "Native discovery, hooks, MCP, resume and isolation differences"
[G4]: ../content/runtime-references/OFFICIAL-DOCS.md "Verified official per-runtime documentation routes"
[H1]: https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-integrations/coding-agents/README.md "Official routing/config lifetime, provenance, retention and observation scopes; installed package 0.8.2"
[H2]: https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-docs/docs/developer/api/knowledge-pages.mdx "Official model/page filtering and refresh semantics; server documentation 1.1.0"
[Z1]: /home/palantirkc/work/ws-replan/inputs/final-review-20261102/extracted/rust_migration_reassessment_2026-11-02/PROPOSAL.md "Adopted ZIP proposal sections 4.2, 6 and 9; original ZIP SHA256 964845e260a25350823fb42d38a60e7cba973f8af607e10b505727d3bf78915c"

The host counterparts read for this draft are `/root/.agents/AGENTS.md` and
`/root/.agents/runtime-references/{RUNTIME-LOADING,HERDR-ORCHESTRATION,OFFICIAL-DOCS}.md`.
Hindsight policy is `/docker/hindsight/HINDSIGHT_OFFICIAL_PER_CALL.md`, with
`/docker/hindsight/{AGENTS,README}.md` and the installed coding-agents skill.
Repository-owned copies are reviewed distribution content, not a replacement
loader or evidence that any currently running session consumed them.

## Open questions for the user

These are deferred design choices, not requests to interrupt the current work.

1. Is this mechanism-only boundary the desired permanent scope of the common
   harness, including keeping task/handoff/layout and memory routing as
   conventions and native integrations rather than new Rust subsystems?
2. What native affected-session evidence should close the disclosed agy loading
   and alias-deduplication limit in future adapter work, if further proof is
   wanted? Its current accepted limitation remains in force.
3. Is a future explicit content-apply operation useful after read-only content
   diagnosis, or should installation of reviewed public guidance remain a
   separate task? Such an operation does not exist today.
4. Is there a concrete non-Codex consumer that needs more than ancestry/terminal
   pane lookup? If so, identify that native session surface before adding another
   CLI-specific heuristic.

No answer assigns a role to a CLI. The Lead may arrange independent review and
publish this draft PR, but must leave it open for the user's approval.
