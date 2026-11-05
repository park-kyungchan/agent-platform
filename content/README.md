# Public runtime guidance content

Reviewed distribution source for canonical `AGENTS.md`, runtime references and
the generic `herdr`, `herdr-hermes`, `explain-precisely`, `lead-orchestration`,
`lessons-loop` and `strategy-dialogue` skill bundles. The initial capture was
byte-for-byte from
`/root/.agents/` on 2026-11-04; subsequent reviewed edits evolve repository source.
The fixed Rust inventory is `crates/ap/src/native/content_inventory.rs`. This
README is repository documentation, not an owned host entry.

The academy-runtime-routing and academy-student-context skills belong to
education-platform and are excluded. Native Herdr hooks, per-CLI pointer files,
third-party skills and Hindsight integrations are not copied or owned. Skills
assign no permanent role to a CLI and add no instruction loader.

Content describes native integrations and existing host paths; copying it does
not execute a skill, install a runtime or authorize any described operation.
Update canonical source content deliberately through review. No generated
registry or watcher synchronizes the host.

`SHA256SUMS` records every fixed owned source file in inventory order; the
manifest itself is repository evidence outside that inventory. Changed paths
and bytes require an inventory/test/hash update. Neither source acceptance nor
a rebuilt binary proves host synchronization, native discovery or execution.
The task owner separately authorizes and verifies any host installation.
