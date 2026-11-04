# VPS operations Rust conversion

Status: SOURCE_COMPLETE; PR #6 CI path-normalization repair complete; awaiting Lead review. Worker remains available only for repairs.
Owner: Codex worker for Lead Claude Opus (Herdr w1J:p43).
Authority: `/home/palantirkc/work/ws-replan/preparation/repo-reorg-v2/vps-ops-v1/TASK.md`, user-approved 2026-11-04.
Branch: `rust/vps-ops-20261104`, based on `470809b`.

Completed source effects:
- Retired Herdr container and entrypoint deployment sources; removed stale runtime index/network contract/unit ordering.
- Captured nine host configuration files byte for byte with existing environment placeholders; host-path README mappings and hashes recorded.
- Added the pinned Rust 1.95.0 edition 2024 workspace, typed infrastructure domain and `vps-ops` binary, native update adapters, resume/receipt fences and host helper aliases.
- Kept `scripts/hermes-main-update` unchanged; repo and host remain byte-identical at SHA256 `3f36e01b72bbdbcc1bb9b7f78965f769497819a96e0ba51123194395f96bedd8`.
- Retired Bun/TS dependencies, sources, tests and release tarballs; Rust CI retains workflow `VPS source verification` / job `source`.
- Golden fixtures identify their original TS/host sources; 44 Rust tests pass (3 unit, 21 command/integration, 20 golden/contract) without network, SSH, Docker or systemd effects.
- Resolved all seven requested fidelity boundaries from the independent review at `preparation/reviews/wonder/reorg-vps/REVIEW.md`, plus the canary Python timestamp format. Added nine regression tests; enabled cached serde_json float_roundtrip to retain the negative safe-integer endpoint. No dependency versions or lockfile changed.

- Reproduced PR #6 failure with CARGO_TARGET_DIR unset; normalized fixture roots and both comparison operands lexically, with explicit dot-segment and symlink assertions. All 44 tests pass under both default and external Cargo targets; production logic is unchanged.

Verification: `cargo fmt --all --check`; `cargo clippy --locked --offline -j1 --all-targets -- -D warnings`; `cargo test --locked --offline -j1`, with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. All checks pass twice: CARGO_TARGET_DIR unset (worktree target), then the task-owned external target. Full CI repair logs and the expected pre-fix failure are retained in `.local/checks/ci-repair-v1/` and referenced in the appended handoff. Both disposable targets were deleted after verification.

Allowed next action: Lead reviews commits and HANDOFF.md, then owns any install decision. No worker push, PR, install, service action, business call, model evaluation or host state mutation was performed. Installed originals, native sessions and unrelated services remain intact.
Handoff: `/home/palantirkc/work/ws-replan/preparation/repo-reorg-v2/vps-ops-v1/HANDOFF.md`.
