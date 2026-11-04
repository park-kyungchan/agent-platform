# Golden fixture origins

Captured on 2026-11-04 before retiring the local Bun/TS toolchain. Source checksums
and immutable Git revisions are in `architecture/rust-port-provenance.json`.
The temporary oracle generator and dependency installation were then removed.

| Fixtures | Original oracle |
| --- | --- |
| `domain.json`, `domain-evaluation.json` | `tests/domain.fixtures.ts` and archived infrastructure `evaluateInfrastructure` |
| `backend*.json`, `jobs.json` | `backend-update.ts` evaluate/candidate/job functions and `backendIdentity` |
| `desktop.json`, `desktop-command.json` | `desktop-update.ts` receipt, eligibility, native command functions; `desktopIdentity` |
| `desktop-recovery.json`, `rejected-proof.json` | Original recovery-scope and rejected-launch proof functions |
| `arguments.json`, `workflow.json` | Original update argument parser and `runUpdate` with in-memory operations |
| `sanitizer.json` | Original Desktop output sanitizer, using synthetic credential-shaped strings |
| `canary.json` | Installed Python controller's `dispatch` with injected backend and lock, fixed clock and execution ID |
| `report.json`, `report.md` | Installed Python report with mocked Git responses and fixed clock; original output bytes |

The installed immutable domain package used by the original TS oracles matched
all five archived domain implementation files byte for byte (recorded hashes).
The backend fixture clock was pinned during evaluation; timestamps and run IDs in
workflow results alone were normalized for deterministic comparison. Native
receipts retain their independent provenance, and failure cases remain failures.

No source oracle or Rust test called SSH, a network service, Docker, systemd or
Hermes native business APIs. The production process-runner tests spawn only
bounded local fixture Python processes; syntax checks parse source without
executing its native operations. Stateful tests write only beneath the selected
Cargo target and remove their own temporary directories.
