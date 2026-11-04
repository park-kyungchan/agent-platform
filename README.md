# Agent platform

A small Rust workspace for deterministic public instruction rendering, scoped
installation/recovery and on-demand diagnosis. Native Muse, Claude Code, Codex,
Antigravity CLI (`agy`) and Hermes own their sessions, tools and lifecycle.

`crates/ap` implements `ap render`, `ap install plan|apply|recover` and
`ap doctor inspect`. Requests name explicit source/profile paths. Rendering is
pure; installation owns only the selected instruction links/import, preserves
unrelated native files, and requires explicit writer coordination. Doctor does
not execute hooks or establish native loading.

Run `bash ci/check-rust-source.sh` in a disposable environment or the task's
assigned build slot. It runs pinned Cargo formatting, tests, Clippy and the
frozen finite S02/S03 conformance judges. Python is retained only for those
independent verification tools; the product implementation is Rust.

See [migration dispositions](docs/rust-migration-dispositions.md),
[active CLI coverage](docs/ap-08-dispositions.md) and the
[scoped installation contract](crates/ap/s03/CONTRACT-v1.md).
Source changes require independent review and Lead integration. Real-host
installation and runtime activation require their separately authorized evidence.

Legacy TypeScript packages and administrative tooling are retired by the
user-approved 2026-11-04 repository reorganization. Their source remains in Git
at `57813009fdde43f99bb1798d031a1c88dabe5358`. The pushed recovery tag `archive/agent-platform-pre-strip-20261104` preserves
all retired source and the 14 historical custody files. No separate Drive copy
is required for these Git-tracked files by the user's explicit direction.
