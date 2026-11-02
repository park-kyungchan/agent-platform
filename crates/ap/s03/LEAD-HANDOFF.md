# S03 candidate: Lead verification and independent review

Authority: current user assignment, `/home/palantirkc/work/ws-replan/STATE.md`,
ORCHESTRATION/CURRENT-PLAN and adopted ZIP S03/AP-01..08. Worker source base is
accepted S02 2d4269e87533f3ef7c26fbe2347448cbff08163d, branch
rust/s03-scoped-install-20261102 in the assigned AP worktree. Common Git is
/home/palantirkc/platforms/agent-platform/.git. No child agents/model sessions,
compilation, native apply, source retirement, hook execution, push/merge/deploy,
STATE/ORCHESTRATION edits or retained-original deletion by this worker.

Preimplementation contract/corpus commit: fad7758. Exact current candidate is the
commit returned with this handoff; inspect `git rev-parse HEAD` and require an
expected clean revision before building/review. No source/S03/P6 acceptance claim.
The Lead owns the expensive slot (currently S06); these commands are FOR LATER,
only after explicit slot grant, with no parallel builds.

## Permitted checks actually run by worker

Native Cargo dependency resolution/fetch; cargo fmt; locked offline metadata with
--no-deps; S02 frozen judge self-check (87 renderer/13 admission,10 controls);
S03 literal judge self-check (41 filesystem cases/8 internal fault obligations,
13 controls), both immutable v1 and additive v2; additive CLI self-check (17
literal transcripts,4 controls); diff --check; accepted-S02 byte preservation.
`STATIC-EVIDENCE.json` records actual results and hashes.
A finite disposable Python OS-primitive probe verified directory flock behavior,
NOFOLLOW refusal, symlink mode and displaced-ancestor detection on this host;
OS-PRIMITIVE-OBSERVATIONS.json labels it explicitly NOT compiled Rust proof. No Rust check/test/build/
clippy or candidate Rust execution has run. Source review is not compiler proof.

v2 corrects only an internal reverse-recovery-order expectation before candidate
execution. V1 is preserved. See CORRECTION-v2.md. Independent Wonder must review
this correction, preimplementation contract and exact candidate before acceptance.

## Serialized build and staged proof (Lead only)

Run in `/home/palantirkc/work/ws-replan/wt/agent-platform`. Use fresh task-owned
artifact paths; do not overwrite the pinned successful S02 binary/evidence.

```bash
source /home/palantirkc/work/ws-replan/preparation/final-review/execution-env.sh
export CARGO_TARGET_DIR=/home/palantirkc/work/ws-replan/preparation/slices/S03/target-reviewed
export AP_S03_TEST_ROOT=/home/palantirkc/work/ws-replan/preparation/slices/S03/disposable-rust
install -d -m 700 "$AP_S03_TEST_ROOT"
cargo test --workspace --locked --offline -j1
cargo clippy --workspace --all-targets --locked --offline -j1 -- -D warnings
cargo build -p ap --locked --offline -j1
python3 crates/ap/conformance/verify.py --binary "$CARGO_TARGET_DIR/debug/ap"
python3 crates/ap/s03/conformance/verify-v2.py --binary "$CARGO_TARGET_DIR/debug/ap" --scratch-parent "$AP_S03_TEST_ROOT"
python3 crates/ap/s03/conformance/verify-commands-v1.py --binary "$CARGO_TARGET_DIR/debug/ap"
```

Preserve actual exits/stdout/stderr and binary hash separately. Native Rust unit
tests require explicit private AP_S03_TEST_ROOT; they include exact test-process
exit after install/recovery rename, race/partial progress, unrelated edits/modes,
ambiguous stage ownership, no-write authority and command lock collision. This is
ordinary disposable Rust process testing, not a native agent/model session.
The CLI judge checks opaque settings/hooks and a fake hook sentinel unchanged.
Its foreign-owner fixture assumes this host's root executor; an unprivileged
executor must disclose that unsupported fixture rather than claim it passed.

Top-level S02 help stays byte-identical. New usage is separately pinned in
conformance/commands-v1.json. Do not alter old S02 expectations to admit commands.

## Actual native scope and proof routes (Lead only, after reviewed staged proof)

Observed public metadata was read-only: Codex direct symlink to canonical, override
absent; Claude ordinary exact canonical import document. Muse settings hash is
recorded without payload. Both agy aliases remain direct canonical symlinks.
The current agy settings hash differs from the historical preparation snapshot;
author/content are UNKNOWN. No reset, content inference, native registration
change, source retirement or log deletion is authorized by that difference.

Generate a Spec with canonical_path=/root/.agents/AGENTS.md, codex_dir=/root/.codex,
claude_dir=/root/.claude, explicit selected scope and uid, environment
`{"kind":"coordinated_native","evidence":"<Lead-owned current coordination evidence>"}`.
The worker did not apply it. Lead first runs reviewed binary `install plan` and
`doctor inspect`; inspect public plan/digests and compare untouched settings hashes.
For reviewed apply/no-op, provide a new mode0700 command_dir in task preparation
and writers_quiesced=true only after coordinating affected writers. Use
`ap install apply < apply-request.json` and retain its exact private state. The
current pointers should yield unchanged entries, but that is a prediction until
native execution/readback. Locks can be created even on a no-op; account for them.
`ap install recover < recover-request.json` is explicit; never use a stale whole-file
backup. No command here edits native trust or official Herdr/Hindsight hooks.

The prepared installed help and native-surface report are under
`preparation/final-review/runtime/` (codex[-exec]-help.txt, claude-help.txt,
muse[-exec]-help.txt, agy-help.txt); current canonical reference is
`/root/.agents/runtime-references/RUNTIME-LOADING.md`. Consult current supported
surfaces, preserve approval posture, model/effort, exact new IDs and logs. The
commands below are invocation templates for Lead, not executions by this worker:

* Codex: `codex -m gpt-6.1-sol -c 'model_reasoning_effort="high"'
  --dangerously-bypass-approvals-and-sandbox --no-alt-screen -C <reviewed-checkout>`.
  Capture a NEW native session ID and its startup/user-instruction input in the
  corresponding ~/.codex/sessions/.../rollout-...-<exact-ID>.jsonl. Compare the
  canonical public block actually supplied by the native loader and project chain
  against the current canonical hash/expected public content; reject override
  substitution or truncation. A response saying it read AGENTS is insufficient.
  This worker's fresh task session 01a19d38-b422-7b42-9532-206db3286be8
  has verified native startup input at rollout line8 and native agents_md.text
  at line9 containing the FULL current canonical bytes, plus Sol/high at line10.
  The exact log path and public canonical digest are in STATIC-EVIDENCE. This
  is actual native-input evidence for unchanged Codex loading, not a model's
  file-read claim. Lead may correlate it with reviewed native no-op/readback if
  canonical/pointer remain unchanged; it does not prove an S03 apply happened.
* Claude: in the reviewed checkout, `claude --model <assigned-model> --effort high
  --permission-mode <existing-mode> --debug-file <task-owned-debug-file>`.
  In that NEW session use native `/memory`, preserve the loaded-file view and the
  native recorded system-prompt snapshot/loading events with exact session ID.
  Confirm canonical import AND applicable project chain/size behavior. Keep
  native snapshot/session persistence enabled. Do not inject the expected
  canonical content through a custom --system-prompt as loading proof.
* Muse: retained native trusted project/foreign personal mechanisms. Lead may
  use `muse --workspace <reviewed-checkout> --model muse-wonder-1.0
  --reasoning-effort high --yolo` for an authorized NEW independent review/proof
  session; preserve native session export/trace for its exact ID. Verify that
  --no-foreign-personal-context is absent if relying on the canonical Claude
  personal import, and native trust-gated project discovery is evidenced.
  Installed help exposes `muse trace`/`muse export`; inspect those subcommand helps
  before use rather than guessing a log schema. If native input evidence does not
  expose canonical loading, report UNVERIFIED. No ap Muse adapter is claimed.
* agy: retained native ~/.gemini/AGENTS.md and GEMINI.md aliases, repository-bound
  project discovery and existing hooks/MCP. In the reviewed checkout a Lead-owned
  NEW `agy --model <assigned-model> --effort high --log-file <task-owned-log>`
  preserves native behavior/permissions; inspect its native context view/loading
  log with the exact returned conversation ID. Headless proof, if supported for
  the chosen inspection, uses --print --output-format stream-json and finite
  --print-timeout. Do not resume an old conversation or use a model's file-read
  answer as fresh loading proof. Same-source alias deduplication and native input
  evidence remain UNVERIFIED until actually inspected. Unknown settings stay
  untouched. No ap agy adapter is claimed.

Do not launch every runtime to validate an unrelated adapter; the initial changed
scope is Codex/Claude. Retained Muse/agy dispositions must be qualified with actual
native input evidence or remain explicitly pending. Grok/Gemini executables were
absent in preparation, not deletion authorization; Hermes relay/mount stays under
its owner. No new pane hook caller was established. Necessary existing pane/native
hook behavior remains Lead proof, never doctor indiscriminate hook execution.
If supported native inspection cannot expose the loaded input, record that precise
gap; do not replace it with a manual file read or a model assertion.

P6 is blocked until all in-use runtimes have verified support/retained mechanisms
or explicit user scope decision. S03 is not complete until Lead verifies actual
staged/native behavior and Wonder challenges the exact recovery contract/candidate.
S04 is not started. No active children/builds/business effects from this worker.
