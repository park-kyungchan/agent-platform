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

## Hostctl retirement and spawn diagnostics — 2026-11-04

`hostctl-v2.json` follows the Lead's Part C decision after k3s was uninstalled.
Its inventory expectation removes the retired service/node probes from the
original hostctl contract; it is an explicit policy projection, not a claim that
the old script already omitted k3s. The old immutable source hashes above remain
unchanged. `k3s.status` is now unsupported (64) with no external call. The tracked
architecture/targets files contained no k3s-only entries at base `81087b2`.

Missing PATH command, missing absolute executable, and non-executable-file
diagnostics were independently captured with `/bin/bash --noprofile --norc -c
'"$@"' hermes-hostctl <PROGRAM>` under a disposable empty PATH and `LC_ALL=C`.
The fixture records the actual Bash version and binary hash. Only the Bash
source-location prefix `hermes-hostctl: line 1: ` and disposable root are
normalized; diagnostic bodies, stdout and exit codes (127/127/126) are exact.
Rust does not invent a Bash source filename or line number. Native ProcessRunner
tests compare against these bytes. Real fixture basename-alias cases check
missing `hostname`/`uptime` (127) and the missing-`sshd` pipeline (1, retaining
grep/pipefail ordering). The Bash pipeline oracle runs only absolute grep/sort
on empty input; PATH lookup cannot resolve host executables.
The permission-denied fixture is a non-executable disposable script. No global
environment variable or host executable is changed.

Fixture/source provenance and the explicit policy exception are recorded in
`architecture/rust-port-provenance.json::hostctlRetirement20261104`.
