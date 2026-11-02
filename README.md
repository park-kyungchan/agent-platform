# Agent platform

The current Rust migration replaces obsolete harness machinery with a small `ap` tool, builds an operational Ontology and preserves retained Education behavior. It follows the user-adopted [final review ZIP](https://drive.google.com/file/d/12fcl2fkBZGopdrGkCbsrrHGnNOYFyv_W/view), dated 2026-11-02. Rust is the destination for retained owned implementation; vendor runtimes and content keep their existing roles.

On this host, start with [current task state](/home/palantirkc/work/ws-replan/STATE.md), [orchestration](/home/palantirkc/work/ws-replan/ORCHESTRATION.md) and [the adopted plan entry](/home/palantirkc/work/ws-replan/CURRENT-PLAN.md). The user approved execution at 2026-11-02T15:28Z, superseding former WAIT snapshots and earlier WP ordering. The current assignment permits **S01 only**. Independent Muse review and Lead acceptance release S02/S05; this documentation candidate does not release them.

| Repository | Destination and boundary |
| --- | --- |
| `agent-platform` | One small `ap` crate initially: deterministic render, scoped owned-entry install/recovery, on-demand doctor and only a necessary pane hook. No dependency on a running Ontology. |
| `ontology-platform` | Operational source-qualified objects/links, logic, permissions, lineage, local transactions and action recovery; separate from Hindsight's database and Academy's journal. |
| `education-platform` | Behavior-preserving Rust ports of retained Academy parsers, scoped reads, reviews, saves/reconciliation and relevant mathematics functions, with compatible native/MCP tools and durable state. |

Muse, Claude Code, Codex, Antigravity CLI (`agy`) and Hermes retain native runtime ownership. Hindsight remains an official external integration. A short state file and handoff replace generic Work/Task/Run administration; business identity, evidence, reviewed-input bindings and uncertain-effect state remain with their domain owner. `ap` is not another model loop, scheduler, memory store or business write ledger.

[Selected migration dispositions](docs/rust-migration-dispositions.md) records source anchors, observed callers, all four active CLI dispositions and pending closure. Codex/Claude are the first renderer adapters, subject to their actual supported surfaces. Muse/agy retain their native mechanisms pending qualification; two adapters alone do not complete P6 or permit deleting another consumer's support.

S01 changes guidance only. No Rust implementation, installation, deployment or runtime retirement is claimed. Retained ports require an independently frozen reference, a qualified comparator and independent exact-candidate review. Recorded response bytes are the default; GET is not a read-only guarantee. Preserve one participating Academy account lock domain, compatible old journals and physical-field uncertainty through restart and rollback; never replay ambiguous effects. P8 remains actual user judgment and P9 retains its separate live-switch/deletion conditions.

The existing Bun workspace, packages, immutable releases and CI remain present while consumers are reconciled. [project.json](project.json) and [the ownership registry](architecture/capability-source-owners.json) remain reference descriptions of that source, including historical paths and pins. They do not authorize new Rust work, revive the retired trusted review/controller protocol or prove current native consumption. Source integrations and native activation are separate decisions.

The pre-migration package/build instructions remain in Git at `1e6b771729366955cc48aa4fa48d35e6de6fe00c:README.md`. Existing [Git Work](docs/git-work.md), [constrained improvement](docs/constrained-improvement.md) and [Runtime Index](architecture/runtime-documentation-routing/registry.json) documents describe legacy mechanisms; they are preserved as source history, not the current Rust execution procedure. Use the assigned slice's checks. For this documentation-only S01 candidate, validation is `git diff --check` plus local documentation/link checks; no Bun/Cargo build or legacy controller run is needed.
