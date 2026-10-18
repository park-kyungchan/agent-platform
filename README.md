# VPS operations

This native infrastructure project owns SSH, Docker, deployment projections and
the existing Hermes update CLI. Canonical shared domain implementation remains
in `agent-platform/packages/infrastructure-operations`; this project consumes an
immutable release of its exact Git tree. The release verifier checks its source
commit, members and hashes offline, so source CI needs no sibling private checkout
or transferred credentials.

```bash
bun install --frozen-lockfile --ignore-scripts
bun run check:source
bun run check
# Requires the native Docker Compose plugin; validates source only:
bun run check:deployment
# Reads the configured host and native serving state:
bun run cli doctor
```

Follow `architecture/update-workflow.json` for explicitly requested Hermes
updates. Native backend-only and coordinated Desktop/backend modes have distinct
owners and effect boundaries. A passing source check does not activate a runtime
or authorize an update.

## Runtime layout

`architecture/runtime-path-index.json` maps deployment source, persistent native
Home/data, credentials, Git source, launchers and operation evidence to their
owners. Shared naming semantics and the integration Work live in `agent-platform`.
Host Herdr deployment source is `/docker/herdr-runtime/deployment`; `build` is a
compatibility alias. Native Hermes data/workspace paths and sealed historical
handoff paths retain their native owners.

The user's WSL2 entrypoint is `herdr --remote srv1873899`. Inside the server,
`codex-orchestrator` starts a blank native Codex TUI and `hermes-backend --profile
PROFILE` opens the installed native Hermes TUI. Launchers are source-controlled
in `deployments/herdr-entrypoint`; their Home copies are verified projections.
The original paused logical Work is resumed only on an explicit user instruction.

## Source and CI boundary

Maintain `src`, `tests`, the native updater, explicit deployment source and the
finite CI policy. Keep operation runs, state, historical effect helpers, native
Homes/auth/session stores, caches and provisioning artifacts with their existing
owners; they are excluded from Git. `scripts/ci/source-policy.ts` checks tracked
files before publication and CI. It provides bounded path/known-secret-marker
checks, rather than universal credential attestation.

GitHub CI performs frozen installation, existing deterministic tests, immutable
release verification, shell syntax and mandatory native Compose configuration
validation. It performs no deployment or model evaluation. The responsible Lead
reviews exact source and checks before any explicit merge; automatic merge is
prohibited.


### Opt-in v0.22.2 font-derived backend

`fontReadability` defaults to false. It admits only upstream
`d923a5eb487a17c0a68801bba9ef0f56c81040a8`, official `v0.22.2`, Bot Desktop,
and `deployments/hermes-font-readability.patch` SHA256
`d9d7d4bb8f446f80ffd7402d9ad685b829f454c3fd1d91c5d1589616ae8d2ee2`
at the updater's existing content-addressed overlay path. Source and patch
staging, independent review, helper/policy adoption and native promotion remain
separate authorized actions. The source default policy does not enable it.

The native `--font-readability` flag records a **derived**
`-desktop-overlay-<digest>` image and `dirty=true` install stamp; it cannot
replace the pristine release/Desktop image. General Bot/release overlays remain
rejected. The updater verifies the exact Dockerfile base and postimage, the
Desktop build feature and overlay tree. Every candidate validation, including
cache hits and rollback promotion, starts a fresh network-disabled UID10000
container and opens/hashes/copies the ten fixed public font assets without live
profile/data mounts. A repair in a running container's upper layer is not proof
that a fresh container from that image is readable. No PM sync/GC, profile,
credential or service action follows from checking this source.


Typed final verification also invokes that same probe through the native
helper's read-only `verify-font-readability` action. This action emits joined
image/tree/patch proof on stdout and skips all updater directory/lock/receipt
writes. Healthy same-SHA and already-deployed paths may avoid lifecycle work,
but cannot complete verification from metadata alone when the font option is
selected. A missing, mismatched or unreadable fresh-image proof fails the final
verification. Ordinary default-off verification invokes no font probe.
