# VPS operations

Private Rust infrastructure module under agent-platform. The `vps-ops` binary
owns domain validation, observations, update sequencing, durable request fences,
SSH transport, host diagnostics, the fixed canary controller and update reports.
It preserves separate observation, verification, authority and acceptance records.

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -p vps-ops -- doctor
```

Rust 1.95.0, edition 2024, `unsafe_code = forbid`. CI keeps the existing workflow
and `source` job names. Tests use synthetic fixtures and injected command runners;
they make no network, SSH or host service calls. Source verification is separate
from live serving verification and user acceptance.

`VPS_OPS_ROOT` selects the repository/configuration root for installed binaries;
the source build defaults to its workspace root. Operational state and run receipts
remain untracked under that root. Never publish environment files, native homes,
conversations, credentials or databases.

Commands retain the previous CLI vocabulary:

- `doctor`, `inventory`, `verify <observation.json>`.
- `plan <start|stop|restart>` and `apply <request.json> --confirm-exact-target <id>`.
- `update backend-only|coordinated [--plan|--apply] [--sha FULL_SHA|--release vX.Y.Z] [--resume DIRECTORY] [--desktop-cdp URL] [--authorization TEXT]`.
- `update verify --sha FULL_SHA` and `update stage-overlay --sha FULL_SHA --patch FILE --apply --authorization TEXT`.

Apply requires the current explicit instruction. The original run and pinned SHA
own resumption. Recover unresolved native jobs and Desktop receipts before another
invocation. Backend-only does not load Desktop configuration. Coordinated mode uses
the completed native Desktop artifact SHA, or a freshly verified existing release.
Installed and running alignment remain separate results.

The binary also replaces these host-only command entrypoints through basename
aliases or explicit subcommands:

| Host alias | Explicit subcommand |
| --- | --- |
| `hermes-hostctl` | `hostctl inventory|resources|docker.status|k3s.status|ssh.status` |
| `hermes-canary-control` | `canary capabilities|status|start HASH|stop HASH` |
| `hermes-agent-auto-update`, `hermes-agent-update-latest` | `release-update` |
| `hermes-main-report` | `main-report REPO OLD_SHA NEW_SHA OUTPUT` |

The installed hostctl vocabulary is preserved, including its diagnostic failure
when the retired k3s executable is absent. The legacy release helper preserves its
first-image-line replacement and compose rollback behavior; main promotion remains
exclusively through `scripts/hermes-main-update` and the installed helper's native
one-shot path. Installing a binary or a symlink is a separate Lead-owned action.

`deployments/` holds reviewed source configurations with README host-path mappings.
The obsolete Herdr container/entrypoint deployment is removed; Herdr runs as the
native `herdr server` process. `architecture/runtime-path-index.json` names retained
runtime paths. Environment values remain in untracked host-owned files.

The native promotion helper remains unchanged at deployed SHA256
`3f36e01b72bbdbcc1bb9b7f78965f769497819a96e0ba51123194395f96bedd8`.
Embedded `protocol/` Python and PowerShell sources bridge existing installed native
APIs; they are compiled into the Rust binary, not separate installed scripts or a
second agent runtime. They retain UID/HOME, process incarnation, inode and native
serving checks. Rust CDP uses existing targets without creating or closing pages.

`architecture/rust-port-provenance.json` identifies original source hashes and the
archived domain revision. Golden fixtures were captured from the original pure
functions and isolated host-script adapters before removing the Bun/TS toolchain.
No model evaluation, host install, deployment, service restart or user acceptance
is implied by source tests.
