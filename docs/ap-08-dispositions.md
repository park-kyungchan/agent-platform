# AP-08: four active CLI dispositions

This source increment extends S02/S03's owned instruction boundary to Antigravity
CLI (`agy`). Authority: current ws-replan STATE, closure/ap08-v1/TASK.md and the
adopted review ZIP AP-08 / S03. The immutable S03 contract and dated S01 note
remain historical evidence; this document is their bounded successor.

| Active CLI | Concrete disposition | Source and dated evidence | Remaining user-gated work |
| --- | --- | --- | --- |
| Codex | Supported canonical instruction link; existing behavior retained | Renderer/native installer; S03 `native-readonly-01/RESULT.json`, `CODEX-NATIVE-LOADING-READBACK.json` | Live installation/activation requires separate authorization and fresh affected-session proof for the selected candidate. Earlier already-correct diagnosis is not a new apply. |
| Claude Code | Supported exact canonical import; existing behavior retained | Renderer/native installer; S03 `native-readonly-01/RESULT.json`, `claude-native-01/NATIVE-INPUT-READBACK.json` | Same live install/activation and fresh-session gate; native import semantics remain unchanged. |
| Antigravity CLI (`agy`) | Supported source adapter: two direct canonical links, `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md` | `crates/ap/tests/ap08.rs` and native fault tests; runtime loading/wiring references; S03 `agy-native-proof-03/READBACK-02.json` selected GEMINI.md in the native context panel | Live install plus fresh affected-session proof. The old panel concerns 1.4.12; inventory reports 1.5.0. Current-version raw input and alias deduplication remain unverified. Keep both aliases. |
| Muse | `retained_native_mechanism`: trusted project discovery and explicit canonical/task STATE handoff; no global file adapter | S03 `MUSE-NATIVE-LOADING-QUALIFICATION.json`, session `01a19d68-6066-7eb1-af62-89a26ddcdd62`: foreign Claude `@import` is literal; canonical full text was not auto-injected | No invented Muse installation. Fresh affected-session proof must verify explicit handoff/project reads under the chosen trust/foreign-personal/child flags. Preserve native support. |

S03 paths are relative to `/home/palantirkc/work/ws-replan/preparation/slices/S03/`.
Runtime references are `/root/.agents/runtime-references/RUNTIME-LOADING.md` and
`runtime-wiring.md`. No current native session or real-home inspection is performed
by this source assignment. Settings, hooks, MCP, vendor trust, profiles and account
state are outside the adapter's ownership.

## Additive request boundary

`ap render` accepts an optional strict profile, in addition to the existing
required Codex/Claude profiles. Omission preserves old output bytes. Explicit
null, unknown profile fields or missing profile members are refused.

```json
"agy": {
  "config_dir": "/explicit/home/.gemini",
  "instructions": {"kind":"missing"},
  "gemini_alias": {"kind":"missing"}
}
```

This adds `agy_instruction_link` and `agy_gemini_alias` artifacts in existing
bytewise path order. Missing entries become direct absolute canonical symlinks.
Already-matching direct absolute/relative links remain exact. Ordinary files,
foreign links, dangling links, link chains, unsafe metadata and ancestor symlinks
are refused. `GEMINI.md -> AGENTS.md -> canonical` is an unsupported chain.
Foreign content is never adopted or overwritten.

`ap install plan` / `ap doctor inspect` accept new `scope:"agy"` and explicit
`agy_dir`. This field is required exactly for agy scope; old scopes omit it.
The existing `codex_dir` and `claude_dir` stay required disjoint placeholders;
they are not opened in agy scope. All selected disposable paths must be under
the private root. No ambient home discovery or combined new scope is added.

Apply/recover keep S03's command-local durable state, staged-inode fingerprints,
per-entry atomic replacement, expected-before checks, writer coordination,
partial reports and reverse recovery. The two agy entries share one profile
lock. Recovery removes only a new link still matching this command's owned value;
preexisting links remain and later foreign edits conflict. S03's reviewed
final-window and independent size limits continue to apply.

## Doctor and compatibility

An agy-scope `ap doctor inspect` reports both link statuses (`needs_install` or
`ready`) and explicitly includes:

```json
"muse": {
  "disposition": "retained_native_mechanism",
  "canonical_path": "/root/.agents/AGENTS.md",
  "handoff_requires_task_state": true,
  "native_loading": "unverified"
}
```

The canonical path is the explicit request value. Supply the exact task STATE
and selected contract in every Muse handoff; doctor cannot infer them. This
field appears only in new agy scope to preserve frozen Codex/Claude/both report
bytes. Use the explicit agy scope for the remaining-consumer diagnosis. Muse
has no install selector or global file. A path check does not prove trusted
project discovery or actual handoff loading.

Every scope keeps `native_loading:"unverified"`, `hooks_executed:0` and
`p6_complete:false`. Doctor runs no hooks or native processes. A conflict returns
a fixed refusal rather than a misleading ready report.

## Verification and limits

New CLI tests exercise literal render artifacts/refusals and projection; fake
home install twice, direct/relative aliases, drift, recovery, doctor states and
opaque hook/settings preservation. Native fault tests cover partial effects,
locks, topology changes and process death after install/recovery rename. Frozen
S02/S03 corpora and pins are unchanged. Actual commands and candidate hashes are
in ws-replan `preparation/closure/ap08-v1/{ACTUAL-EVIDENCE.json,HANDOFF.md}`.

This establishes source-level consumer dispositions pending independent review.
Live install, fresh-session loading, binary PATH placement, support deletion,
publication/merge and whole-P6 acceptance remain outside this assignment.

## Final source retirement disposition — 2026-11-04

The user-approved repo-reorg-v2 retires legacy adapter/recovery libraries and
external agent-harness-engineering tooling. The retained Rust instruction
adapters above do not import those libraries. Native vendor hooks, sessions,
configuration and services remain separately owned. The user accepted agy's
fresh-session loading gap as a disclosed limit; this source task performs no
new native loading or installation. Historical retention holds in the original
S03/S04 notes are superseded by this decision.
