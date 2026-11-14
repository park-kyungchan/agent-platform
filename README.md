# Agent platform

A small Rust workspace of the command `ap`. Native Muse, Claude Code, Codex,
Antigravity CLI (`agy`) and Hermes own their sessions, tools and lifecycle.

`crates/ap` implements the verbs `version`, `work`, `run`, `gate`, `review`,
`unit`, `deliver` and `pane`; [crates/ap/README.md](crates/ap/README.md)
documents each. `ap pane` resolves the caller's Herdr pane;
`--focused` explicitly selects UI focus, `--exports` emits validated caller
variables and `--resume-candidate` returns an unverified Codex candidate.
See [pane compatibility](crates/ap/pane/README.md). The earlier installer's
verbs were removed; the command refuses them like any unknown verb.

`./check` is the one check of this repository and what the CI job `rust` runs:
the content check, the memory record checks, the test of the install script,
format, lints as errors, all tests, and the rehearsal of `ontology/work.schema.json`
against the engine revision in `ontology/ENGINE-PIN`. `ci/check-rust-source.sh`
builds the pane fixture and runs the frozen pane oracle `crates/ap/pane/golden.py`; the conformance judges it also ran were removed, and no job runs it.
How an agent works here: [AGENTS.md](AGENTS.md). What blocks a pull request: [REVIEW.md](REVIEW.md).

Source changes require independent review and Lead integration. Real-host
installation and runtime activation require their separately authorized evidence.

Legacy TypeScript packages and administrative tooling are retired by the
user-approved 2026-11-04 repository reorganization. Their source remains in Git
at `57813009fdde43f99bb1798d031a1c88dabe5358`. The pushed recovery tag `archive/agent-platform-pre-strip-20261104` preserves
all retired source and the 14 historical custody files. No separate Drive copy
is required for these Git-tracked files by the user's explicit direction.

Public runtime references and the generic skill bundles are tracked in
[content](content/README.md). `scripts/install-content.sh` installs them on the
host, and `ap deliver` calls it; `content/SHA256SUMS` lists their files.
