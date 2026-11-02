# Bounded S02 foundation return

Foundation/design only. No renderer success, independent review, S02 acceptance,
native activation or deployment is claimed. The exact foundation commit is
returned to Lead; it must become Terra's pinned prerequisite after review.

## Git and allowed writes

Verified checkout `/home/palantirkc/work/ws-replan/wt/agent-platform`, common Git
directory `/home/palantirkc/platforms/agent-platform/.git`, branch
`rust/s02-renderer-20261102`, initially clean exact base
`af303d170311a2f11eac9a37125e8a89c1c776b2`.

All additions are within the exclusive assignment:

* `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.
* `crates/ap/Cargo.toml` and `src/{lib,types,main,renderer}.rs`.
* `crates/ap/{DESIGN,TERRA-HANDOFF,FOUNDATION-RESULT}.md`.
* `crates/ap/conformance/{cases.json,cli-cases.json,verify.py}`.
* `crates/ap/tests/conformance.rs`.

Existing source, Bun/package locks, canonical instructions/native pointers,
global/native config, CI, task STATE and ORCHESTRATION remain untouched. The
local commit disables Git hooks only for that command to honor the assignment's
no-hook-execution boundary; no persistent Git setting is changed.

## Result and explicit red obligations

The exact API/CLI/errors, ordered refusal algorithm, selectors, artifacts,
preservation boundaries and S03 exclusions are in DESIGN.md and src/types.rs.
The first two real native surfaces are the observed Codex global instruction
link and Claude canonical import. Settings/MCP/hooks remain opaque preserved
snapshots; no speculative native configuration key or lifecycle hook is added.
Muse/agy retain native mechanisms pending qualification.

`render` always returns `RendererUnimplemented` in this revision. All 81 frozen
renderer cases remain obligations for Terra, including their refusals; no test
has been marked ignored or changed to accept the stub. The library projection
test is pure after-state idempotence, not installation evidence. CLI code is
foundation code and has not been compiled or executed. Of the 13 fixed CLI
admission cases, none is reported as candidate-passing.

Terra's only production write is `crates/ap/src/renderer.rs`; optional new
candidate tests are limited to `crates/ap/tests/renderer_leaf.rs`. Everything
else, especially expected outputs/API/manifests, remains frozen/read-only after
Lead/Muse review. TERRA-HANDOFF.md contains exact commands, algorithm reference,
ambiguity routing and stop conditions. No Terra or reviewer was launched here.

## Dependency choices and actual bounded checks

Rust 1.95.0 is installed and pinned; edition 2024; one non-publishable crate;
workspace lints inherited; unsafe forbidden. Direct pins are serde 1.0.228 with
only derive/std and serde_json 1.0.150 with only std. Their cached Cargo feature
declarations were inspected. Eleven registry packages were resolved offline;
Cargo.lock pins all transitives. No async, CLI, TOML editing or filesystem
dependency is needed for these two selectors.

Actual commands, all exit 0:

```text
cargo +1.95.0 generate-lockfile --offline
cargo +1.95.0 metadata --offline --locked --no-deps --format-version 1
cargo +1.95.0 fmt --all -- --check
python3 crates/ap/conformance/verify.py --self-check
git diff --cached --check
```

Metadata readback identified exactly one package, library, binary and integration
test with the pinned features/toolchain requirement. Comparator self-check
reported 81 renderer cases, 13 CLI admission cases and 10 rejected negative
controls (changed bytes, missing field, ordering, type, exit, extra stdout,
stderr, framing, stub and refusal identity). Those checks validate the corpus,
judge and foundation structure; they do not establish compiled behavior. No
cargo check/test/build/clippy, native hook, install, doctor, live source call,
service mutation, push, merge or deploy was performed.

## Evidence and questions for Lead

Source-backed observations and native documentation routes are recorded in
DESIGN.md. SHA256 at inspection:

```text
aa3510250ce8c69127a3ca3c1d259a13e8f132e5b191facd1c6ac5352a3f0657  /root/.agents/runtime-references/RUNTIME-LOADING.md
70d960401dfe708743865b6338dfe986663345b58cca9ca67308d041e90f4053  preparation/final-review/runtime/codex-help.txt
5ca3a9dac2026e56c221fdd34380c7d7d45ebbb51c1d01b7c17c75cb076a0be8  preparation/final-review/runtime/claude-help.txt
```

No implementation-blocking ambiguity remains within the proposed leaf contract.
Lead/Muse must accept or correct the deliberately conservative path alphabet,
import-ambiguity refusals and pointer-only selector set before freezing Terra's
prerequisite. These are explicit proposed design choices, not native vendor
requirements. Any accepted correction needs a new reviewed foundation revision;
Terra cannot make it. Compilation/API compatibility is unverified until the
Lead grants the build slot. Real parent/symlink identity, uid/mode, locks, native
trust, fresh affected-session loading, partial application, rollback, doctor and
closure of retired consumers remain S03/later work. The actual user P8 verdict
and concrete P9 activation/deletion conditions remain unchanged.

This worker started no agents or persistent background processes and holds no
build slot. All tools/commands returned synchronously. Stop at the committed
foundation return; Lead owns next review/release and bounded-pane closure.
