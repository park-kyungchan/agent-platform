# Terra/high leaf handoff (requires Lead release)

1. Goal: implement the pure renderer exactly as DESIGN.md steps 1–7 after the
   Lead and independent Muse reviewer freeze this foundation. This handoff does
   not launch a worker, grant the build slot, or claim S02 complete.
2. Target: task-owned agent-platform checkout assigned by Lead, descendant of
   this foundation's exact committed revision (Lead supplies the hash). Verify
   cwd/common Git dir/branch/HEAD/dirty state. Canonical state is
   `/home/palantirkc/work/ws-replan/STATE.md`; read canonical AGENTS, current
   STATE/ORCHESTRATION and DESIGN.md first, again after compaction. Exclusive
   writable production leaf: `crates/ap/src/renderer.rs`. Optional candidate
   tests: new `crates/ap/tests/renderer_leaf.rs` only, for helper edge cases;
   tests cannot redefine public contracts. All other files, including public
   types, main/lib, Cargo manifests/lock/toolchain, DESIGN, frozen fixtures,
   conformance tests and comparator, are read-only. No competing AP writer.
3. Effects: source-only leaf implementation, bounded offline checks and local
   commit. No environment/filesystem discovery in the library, new dependency,
   config/STATE/ORCHESTRATION/CI edit, install/doctor/hook execution, deletion,
   live source/service/network call, push/merge/deploy or native lifecycle work.
   Do not build until Lead explicitly assigns the single expensive -j1 slot.
   No child-agent/model substitution. Ambiguity returns to Lead/Sol; do not
   make a design/policy/oracle decision or change an expectation to pass.
4. Complete algorithm: implement only DESIGN.md ordered steps, including first
   refusal precedence, static error fields, lexical link normalization, narrow
   import recognition/refusals, exact text retention and byte-sorted outputs.
   Fence detection unconditionally refuses any fence-bearing existing Claude file,
   including when canonical_path is absent, as DESIGN step6 specifies.
   Private helpers belong in renderer.rs. No new modules or APIs are necessary.
   Remove RendererUnimplemented from the leaf's behavior without removing the
   public error enum value (CLI still distinguishes that intentional stub).
   Frozen expectations: `conformance/cases.json`, `conformance/cli-cases.json`.
   Run before a build grant:

   ```sh
   cargo +1.95.0 fmt --all -- --check
   python3 crates/ap/conformance/verify.py --self-check
   git diff --check
   ```

   Only in the Lead-granted build slot (source task execution-env.sh first):

   ```sh
   . /home/palantirkc/work/ws-replan/preparation/final-review/execution-env.sh
   cargo +1.95.0 test --offline --locked -p ap -j1
   cargo +1.95.0 build --offline --locked -p ap -j1
   python3 crates/ap/conformance/verify.py --binary target/debug/ap
   ```

   The CLI check must use the newly built binary at the returned candidate,
   preserve child exit/status, and fail if the stub or any mismatch remains.
   No release build/repeat broad suite without a newly justified concern and
   Lead scheduling. Compiler/API incompatibility returns to Sol; Terra cannot
   repair shared interfaces. Report exact revision, changed paths, actual
   command results/failures, unresolved findings and running children/builds.
5. Stop after the bounded committed leaf result; Lead performs independent
   exact-candidate review/comparison and acceptance. S03 alone owns real path,
   owner/mode/recovery, native trust and fresh affected-session proof. Muse/agy
   remain retained native mechanisms pending qualification, not silently removed.
   User/STATE/adopted ZIP outrank dated memory. Hindsight work follows
   `/docker/hindsight/HINDSIGHT_OFFICIAL_PER_CALL.md`, coding-agents 0.8.2 and
   Hindsight 1.1.0 revision a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5 official
   features only. Preserve session logs; Lead closes the completed bounded pane
   after reconciling processes and evidence, then uses a fresh session next time.
