# Rust migration dispositions — 2026-11-04 repo reorganization

Authority: ws-replan `preparation/repo-reorg-v2/ap-strip-v1/TASK.md` and the
user-approved reorganization proposal. This supersedes the dated S01/S04
retention tables at `57813009fdde43f99bb1798d031a1c88dabe5358`.

| Source family | Final source disposition |
| --- | --- |
| Rust `ap`, pinned Cargo workspace, CI and frozen judges | Retained, independently verifiable implementation/support. |
| Legacy TypeScript libraries, compatibility forwarders and administrative work records | Retired; no Rust counterpart for generic Work/Task/Run, context gates or trusted controllers. |
| agent-harness-engineering recovery consumers | User decided RETIRE; external repository bundling/deletion belongs to Lead. No external edits here. |
| Infrastructure TypeScript domain | AP copy retired. Private vps-operations owns its vendored tarball copy until its Rust port; it does not read current AP package bytes. |
| Bun reusable action | Retired from HEAD. External immutable SHA-pinned actions remain available through Git history. |
| Historical custody archives | Removed from HEAD after local/origin tag verification and byte-for-byte comparison of all 14 files against `archive/agent-platform-pre-strip-20261104` (= base). User explicitly accepted pushed Git history as the recovery copy. |

Deletion proof and exact commands/results are in ws-replan
`preparation/repo-reorg-v2/ap-strip-v1/`. Frozen Rust conformance corpora and
pins are unchanged. No host installation, credential access, container effect,
external recovery execution or Academy operation is part of this change.

Codex/Claude/agy instruction adapters and Muse's retained native mechanism are
specified in [AP-08](ap-08-dispositions.md). Real runtime loading and source
acceptance remain separate from these bounded source changes.
