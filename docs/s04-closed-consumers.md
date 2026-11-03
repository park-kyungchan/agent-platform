# S04 bounded source CI consumer closure

Candidate base: `5ff309f579c521c489b20f19d95b806138967cea` on
`rust/s04-closed-consumers-20261102`. This is an author handoff, not independent
review, Lead acceptance, hosted CI success, full S04/AP-08/P6 completion or a
runtime cutover. Current `/home/palantirkc/work/ws-replan/STATE.md`, ORCHESTRATION,
CURRENT-PLAN and the immutable final ZIP's S04, AP-08 and PROPOSAL 4.2 govern.
S01's [dispositions](rust-migration-dispositions.md) remain dated navigation.

## Normal reviewer: problem and resulting behavior

Run `41476530953` at S01 `af303d170311a2f11eac9a37125e8a89c1c776b2`
failed in all four source/review/quality/integration jobs at source-author
verification, before their real tests. The stale immediate-authoring-parent
condition is at `ci/reuse/source-author.ts:348`. Changing that metadata would
perpetuate a retired protocol rather than verify retained Rust behavior.

The automatic [source workflow](../.github/workflows/source.yml) now checks the
exact authored head and, separately on PRs, GitHub's synthetic merge. It uses
the existing checkout action pin
`3d3c42e5aac5ba805825da76410c181273ba90b1`, read-only contents permission and
`persist-credentials: false`. Ubuntu 24.04 disposable runners install Rust
1.95.0 using native rustup; no new toolchain action or guessed action SHA is
introduced. Each job has a 25-minute bound and one Cargo build job.

[Ordinary commands](../ci/check-rust-source.sh) run fmt, locked tests, strict
Clippy, locked build and the unchanged S02/S03 judges: renderer/admission CLI,
41 filesystem scenarios, additive CLI and their negative controls. S03's eight
internal fault obligations remain covered by the retained Rust tests. Synthetic
filesystem cases use a fresh private scratch parent and preserve fixture cleanup.
The shell never publishes reviews, dispatches a controller, regenerates expected
outputs, installs native configuration or contacts Academy/Hermes/Hindsight.
Dependency retrieval is Cargo's ordinary locked CI operation.

The former Bun source/quality/projection tests and native-review receipt gate
are no longer automatic prerequisites. This deliberately does not assert that
those retired suites and the finite Rust checks have identical coverage. Shared
TS implementation and tests stay available pending their own consumer closure.
Independent review of this exact candidate and Lead expected-head integration
remain required through ordinary native tools.

## Closed boundaries and deletion proof

| Boundary | Concrete source/execute closure | Disposition |
| --- | --- | --- |
| Automatic source-author checks in `source.yml` | All four direct `source-author-cli.ts verify` calls are removed; both remaining jobs execute only native rustup and `ci/check-rust-source.sh` after checkout. | Retire these invocations only. The library/CLI is retained for shared callers. |
| Automatic quality/review/receipt gate | Removed `quality`, `native-review` and `source-gate` jobs, their artifacts, token environment and `needs` edges; the remaining jobs have no dependency on them or on `ci/source-check.ts`. | Retire these automatic consumers, preserve independent review as a Lead obligation. |
| **Deleted file: `ci/source-ci-setup.sh`** | `git grep` against the exact base finds only three workflow calls (source, quality, merge-integration). All are removed. The script is not a package export or shared action: its own admission accepts only this repository's `Source verification` workflow and these three hosted job IDs. No native source import exists. Exact selected argv-path inspection found no current local bootstrap process; the latest ten hosted run records contained no non-completed run. | Delete the exclusive obsolete `/job`/Bun bootstrap. No `/job`, release, archive, service, registration or native settings are changed. |
| Assertions enforcing the removed workflow | Remove only the source workflow portions of `ci/reuse/source-author.test.ts`, `ci/reuse/bun-version.test.ts`, `ci/quality/operational-timeouts.test.ts`, and the one old workflow-structure test in `ci/quality/boundaries.test.ts`. | Keep shared action, immutable evidence workflow, process/timeout, generated-output and behavioral tests. No retained oracle/comparison is removed. |

This closes source consumers in the proposed revision. Hosted `main` and manual
workflows still execute their own retained revisions until separately integrated;
this local change does not claim to deactivate them remotely.

### Recovery, verified before deletion

- `refs/heads/rust/s03-scoped-install-20261102` resolves to exact base
  `5ff309f579c521c489b20f19d95b806138967cea`; it remains unchanged.
- `git ls-tree` at that commit resolves `ci/source-ci-setup.sh` to regular blob
  `f965aaef0c9d093835b0eaaa850b6113d8d49c02`; `git cat-file -t` returned `blob`.
  Recover bytes using `git show 5ff309f579c521c489b20f19d95b806138967cea:ci/source-ci-setup.sh`.
- `refs/heads/main` remains `1e6b771729366955cc48aa4fa48d35e6de6fe00c`.
  No ref is deleted or rewritten; no GC, push, merge or retained-original deletion.
  Recovery evidence is separate from the caller closure above.

## Surviving callers and exact holds

| Selected boundary | Current concrete evidence read for S04 | Required before further deletion |
| --- | --- | --- |
| `packages/work-continuity` | AP root package dependency remains; the root facade edge is now closed as recorded below. Hermes current-work code imports `/git-work` and `/runtime-index`. Existing agent-harness manifest has the file dependency; `src/native/context-owner.ts:39` names its Git Work API. | Close these source/manifest paths and reconcile their actual installed/loaded consumers, preserving domain effect state. No package deletion here. |
| `packages/native-codex-adapter` | `/home/palantirkc/projects/agent-harness-engineering/src/native/hooks.ts:2,4` imports/reexports `/hooks`; its package manifest still declares the adapter. Public context/native-provider registrations still contain the former AP root; `provider-closure.ts:74` reads the native registration. | Disabled old selectors and S03 fresh input do not close this source graph. Native owner must reconcile the specific wrapper/registration/loaded paths; preserve guards until then. |
| `packages/native-hermes-adapter` | Existing Hermes engineering manifest has the file dependency; `src/ontology-driven/work-context.ts:1` reexports the adapter; `provider-source.ts:401,420` reads/checks its registration. AP `platform-current-work/provider.ts` pins Work/dependency source. | Native/container loaded graph, relay and other forwarders remain unverified. No remote call, native compilation, regeneration or registration edit here. |
| `ci/quality/trusted` | `ci/quality/run.ts:1-2`, `native-tools.ts:1`, `source-check.ts:3,375` and `ci/reuse/review-attempt.ts:11` import its process/driver/QA code. `trusted-source.yml` still dispatches `controller.ts`; hosted workflow metadata says active. | Close these shared imports and manual dispatch/native publishers under their owners before deleting the directory or workflow. No execution of that protocol is authorized by its survival. |
| `ci/reuse/source-author*` and receipt code | `.github/actions/bun-source/action.yml` calls `ci/reuse/run.ts`, which imports/verifies source-author; `trusted/source.ts:5` and `trusted-review-source.ts:6` import it. `publish-review-artifact.ts:89,118` addresses the native-review workflow. | Foreign/shared action and manual publisher consumers remain unknown; do not break them or forge fresh metadata. This slice closes the automatic AP workflow only. |
| `architecture/capability-source-owners.json` | `ci/source-check.ts:214,866` reads it; `ci/verify-runtime-layout.ts:336` checks its identity. It also carries historical paths/pins. | Preserve source compatibility until these remaining callers close; its historical inventory is neither live-use proof nor a deletion gate. No registry renewal. |

The two external owner roots above were read only at the named public source,
manifest and registration metadata paths. Old AP paths in their JSON are reported
as stale metadata, not silently repaired or treated as a live loaded source.
No private custody, settings values, credentials, remote container or live
business source was read.

## Hosted observations and integration hold

Read-only native `gh` queries returned:

- `gh run view 41476530953 --repo park-kyungchan/agent-platform --json headSha,status,conclusion,jobs`: exit 0, completed/failure at exact S01; real source tests skipped after the metadata refusal.
- `gh api repos/park-kyungchan/agent-platform/actions/workflows`: exit 0; source,
  trusted-source and native-review-evidence workflows all active at observation.
- Classic `branches/main/protection/required_status_checks`: exit 1, HTTP 404
  `Branch not protected`. This did **not** establish absence of rulesets.
- `gh api repos/park-kyungchan/agent-platform/rules/branches/main`: exit 0;
  strict required contexts are `Canonical source checks`,
  `Exact head native independent review`, `Source and native review gate`,
  `native-independent-review`. Pull-request, deletion and non-fast-forward
  rules also exist.
- `gh api 'repos/park-kyungchan/agent-platform/actions/runs?per_page=10'` filtered
  to non-completed runs: exit 0, `[]`. This is bounded observation, not proof
  about all hosted history or external dispatchers.

**Exact hold:** the three retired review/gate context names remain required by
the hosted ruleset. This candidate intentionally does not emit counterfeit
success under them. Lead must handle any policy transition separately under
its actual authority, preserve independent review/expected-head integration,
and observe real candidate and merge CI. No remote policy/write was attempted.
The two manual legacy workflows and their publisher/controller sources remain
held, not recreated or invoked.

## Exact changed paths

- Modified: `.github/workflows/source.yml`, `AGENTS.md`, `README.md`.
- Modified obsolete workflow assertions only: `ci/quality/boundaries.test.ts`,
  `ci/quality/operational-timeouts.test.ts`, `ci/reuse/bun-version.test.ts`,
  `ci/reuse/source-author.test.ts`.
- Added: `ci/check-rust-source.sh`, `docs/s04-closed-consumers.md`,
  `docs/s04/static-evidence.json`.
- Deleted: `ci/source-ci-setup.sh` only.

## Validation and next Lead action

[Static evidence](s04/static-evidence.json) records command argv, raw exits and
output. Shell syntax, edited TypeScript syntax, workflow YAML/ref/pin/permission
checks, frozen Python AST/JSON/TOML parsing, Git recovery and whitespace checks
passed. The no-bootstrap-reference search correctly returned 1 with no matches.
No compiler, candidate binary, conformance judge, Bun test, install or child agent
was run by the author. Cargo files and every `crates/ap` byte match the base.

After independent exact-candidate review and the Lead's one expensive-slot grant,
from this checkout with a Lead-selected task-owned `CARGO_TARGET_DIR`:

```sh
bash ci/check-rust-source.sh
```

This executes exactly the compiler and finite comparison commands listed in that
script, all Cargo operations at `-j1`, Rust 1.95.0, locked dependencies. Use the
existing task-local toolchain/environment; no host/global install is requested.
After separately authorized push, inspect (do not infer) hosted results:

```sh
gh run list --repo park-kyungchan/agent-platform --workflow source.yml --commit <exact-candidate> --json databaseId,headSha,status,conclusion
gh run view <actual-run-id> --repo park-kyungchan/agent-platform --json headSha,status,conclusion,jobs
```

AP-08 remains open beyond this finite slice: retain Muse's trusted-project
explicit canonical read/literal foreign import, agy aliases/configuration and
unverified fresh-input/dedup obligation, vendor Herdr/Hindsight hooks, native
sessions and Hermes relay/mount/updater helper. No new ap hook is justified by
S03. Full S04, P6, P8 user judgment and P9 activation/original-deletion conditions
are not satisfied by this diff.

Native author identity: Codex session `01a19d77-2ca9-78c2-aa85-20f51af99a3c`,
process `2844436`; log
`/root/.codex/sessions/2026/11/02/rollout-2026-11-02T18-13-15-01a19d77-2ca9-78c2-aa85-20f51af99a3c.jsonl`.
No model agents, builds or background jobs were spawned by this assignment.
Observed native CLI direct children: node 2844610 (`mcp-server.js`), node
2844611/2844652/2844654 (`server.mjs`), bun 2844618 (`launcher.mjs`), and
code-mode host 2845290. Only script basenames were inspected; no arbitrary argv
or environment was printed. These are retained native MCP/tool processes, not
author-created workers. Transient shell checks completed and must not be
confused with running builds. The ordinary Stop hook must retain the completed session; the author
cannot claim a future Stop succeeded. Lead must verify retention and reconcile
children before closing this pane. Next task uses a fresh session unless STATE
records a concrete continuity exception. Task STATE/ORCHESTRATION/preparation
were read-only throughout.

## Later manual-controller source closure

After `00ff3eefb3948ba669031ca08789f118eea30d87`, the manual `trusted-source.yml` workflow, exclusive `controller.ts` entry point, `buildPolicy` export and exclusive bootstrap test are retired from current source. The operational timeout test no longer opens that removed YAML; its other process, provider, research and explicit-budget checks remain. The unused native dispatch payload and policy examples are also removed. The complete original bytes remain in that predecessor Git commit. The S01 historical workflow link resolves to its retained GitHub blob.

A fresh bounded read found GitHub workflow ID392713044 active with zero runs and no exact remote `refs/tags/trusted-ci-v1` at that observation. Those are dated observations, not a universal statement about remote or native clients. No dispatch, native profile, token, registration, service or remote activation setting changes here.

The surviving-caller table above records the first increment's observations. Its manual-dispatch source edge is now closed. `release.ts` keeps its separately consumed inventory, minimum-reader and installed-input verifier; policy/native/semantic helpers, source-author libraries, bounded process helpers, runtime ownership and domain journals/locks remain subject to their own closures. This change does not renew the retired policy or claim a full legacy aggregate pass, full S04/P6 completion or production activation.

## Later bounded root-facade source closure (2026-11-02)

On predecessor `01fe80a5a40bdca30aa8ab7c954bbac33d58e2f2`, Root selected three deletions and thirteen edits. The generic root CLI/export facade and its exclusive improvement/quality transport tests are removed. The package loses only `scripts.harness`; `architecture/common-git-harness.json` loses only its descriptive `entrypoint`. Existing `sharedDefinitionInputs`, quality-policy pins and source-owner metadata are unchanged. The [exact predecessor facade](https://github.com/park-kyungchan/agent-platform/blob/01fe80a5a40bdca30aa8ab7c954bbac33d58e2f2/scripts/harness.ts) and its two sibling test blobs remain recoverable through Git; no retained originals or data are deleted.

The verifier imports the same `gitReadEnvironment` and `resolveGitWorkSelection` directly from the existing Git Work index. Memory's public test uses the same synthetic request/reader through `withMemoryQuery`, including missing-native-reader refusal. Runtime docs retain the four alias/discovery/version/cache/current-task matrices through `reader.select`, with explicit expected retrieval, evidence gaps, limits and no authority. The unchanged portable reader tests retain request/URL/registry validation and the 262,144-byte registry file limit. Only the removed CLI's envelope, malformed-wire, stdout/stderr and 1 MiB request-file checks retire. V3 still checks direct preview, native-owner refusal and unchanged owner ref; both strict typecheck root lists lose only the removed facade. Direct improvement/quality APIs and their semantic tests remain unchanged, including malformed-observation STOP behavior. Filename strings in comparison fixtures are inert data.

`contracts/v3-implementation.json` still mentions `runHarness` as dated source-candidate metadata, pinned by `architecture/quality-policy.json`. Those bytes remain unchanged: historical prose is not a current execution edge or authority to refresh the retired contract. `common-git-harness.json` itself remains consumed by the verifier and quality scope readers; this increment closes its entrypoint description only. `ci/source-check.ts` still calls the verifier, but its retired aggregate pipeline is not revived or run.

Whole Work/Memory/context packages, native Codex/Hermes wrappers, foreign installed/loaded bundles, relay/updater and first-import guards remain held for their concrete owners. Native CLI sessions and official Hindsight keep their loops, permissions, service and data. Business object/account identity, reviewed-input bindings, dispatch fences, unresolved physical-record/field effects, journals and participating kernel locks are unchanged. No remote absence, full S04/P6/AP-08 completion, native activation or P8/P9 acceptance follows from this bounded source closure. Independent exact-candidate review and Lead expected-head integration remain required.

## Native review evidence endpoint

The current YAML endpoint and its sole selected current-YAML test are retired; the earlier publisher was removed by PR83. The version resolver and direct runtime-docs reader are preserved. `review-artifact.ts` compares historical `run.path` strings and is not a current-YAML reader. Artifact and receipt validators, source-author, ordinary process helpers and native owners remain attached and unchanged. Source recovery is available from commit `6b5e3d5e3dde4b2366447220e1b6174aa269b529` and workflow blob `0b6292cb9fa9b5f28bcdd8de1e83f93986ee03ad`. This bounded source deletion does not prove universal external or immutable-ref dispatch deactivation, native rollback, or full S04, P6, P8 or P9 completion.

## Source-author CLI leaf retirement (2026-11-03)

At source baseline `578b427fb21fb8a2632e35c8ded7745314e7e4ee`, the current AP
entrypoints, workflows, package scripts and selected Codex/Hermes public owner
source have no executable caller of `ci/reuse/source-author-cli.ts`. The direct
current-file consumer is the located-error test named "the actual author CLI
boundary reports its refusal location". Retire the 45-line CLI, that one test,
and only its diagnostic path allowlist entry together. The three other diagnostic
tests and the `source-author.ts` library remain intact.

This is a bounded source closure. It adds no author capture, replacement gate,
receipt, policy release or native authority. `trusted/source.ts` and
`trusted-review-source.ts` still import `verifyCommittedSourceAuthor`; the native
installed-source guard explicitly requires the library. Codex/Hermes context
loaders, first-import/source guards and the direct runtime-docs API retain their
separate callers and owners. An old policy union can still demand a removed
source path; this closure neither renews that policy nor weakens its reader.

Recovery: `git show 578b427fb21fb8a2632e35c8ded7745314e7e4ee:ci/reuse/source-author-cli.ts`,
Git blob `edfda488c691b844b13f696db3d750f17012403a`, SHA256
`8b3c1d2607ff5d7dff3a2d105838d70e96b4220113970f4f6820a4797bdab1ac`.
Earlier dated retention statements describe earlier boundaries. No claim is made
about every external caller, immutable ref, installed release, remote dispatch,
fresh native loading, full S04/P6 completion, P8 verdict or P9 activation/deletion.

## 2026-11-03: uncalled review-attempt/context producer island

The source-only successor retires `ci/reuse/review-attempt.ts`,
`trusted-review-source.ts` and their exclusive `review-runtimes.ts` adapter.
At pre-change source `40ac73319d75b3caeba2cbd9f918dc6c2f64a8d8`, bounded
current-source and selected public Codex/Hermes owner searches found no selected
production entrypoint importing or executing that island. Test imports and
current-byte readers, diagnostic filename strings and self-qualification did
not establish native or business liveness.

The successor removes the island's exclusive tests and the two attempt-only
producer wrappers `deriveMergeCorrespondenceAssignment` and
`validateMergeCorrespondenceAssignment` from `ci/quality/review-artifact.ts`.
It preserves that module's actual artifact/legacy receipt interpretation,
including `consumeMergeCorrespondenceEvidence`, `checkConsumedMergeEvidence`,
assignment parsing and their retained tests. The independent `sourceAuthorGit`
filesystem-monitor refusal test remains verbatim in `review-pr.test.ts`;
`review-gate.test.ts` loses only its retired controller-qualification case.
Generic process/error/source binding and `trusted-review-policy.json`'s fixed
snapshot input remain. No source-author, policy, coverage, receipt or registry
is renewed to make this retirement pass.

Source recovery is native Git at the pre-change revision/path or the recorded
Git blob. Existing immutable releases, original evidence and native owners are
unchanged. Selected Codex context-owner loading, Hermes native-hook/context,
and the installed trusted-release first-import guard require their own exact
source replacement and owner evidence. This bounded source closure establishes
neither installed/external absence nor full S04/P6, S11/S13 activation or human
Acceptance. Earlier dated dispositions above remain historical.

## 2026-11-03: uncalled namespace-delivery source island

Source baseline: `22a66303866ff30eb1c207702f35658a8ef86ab3`. Retire only
`packages/native-hermes-adapter/src/platform-current-work/delivery.ts`,
`native-delivery.ts` and their exclusive `delivery.test.ts`. The selected
production importer of delivery is native-delivery, whose only selected
invoker is that exclusive test. No selected package export, builder entry,
script or default native registration selects native-delivery. Historical
coverage and registry descriptions do not establish an executable consumer.

The source-only assessment, exact preimage hashes and retained recovery copies
are in the Rust migration task's `preparation/slices/S03/native-context-02/`;
the prescribed four-path change and its verification are in
`preparation/slices/S04/dead-delivery-15/`. Native Git retains the baseline
objects. No installed bundle, delivery claim, namespace, journal or receipt is
deleted by this source change.

The current-work native hook, provider/diagnostic tests, real reader and builder
entrypoints, source guards, manual recovery and public Codex/Hermes facades
retain their behavior. Their exact source and native-owner closure obligations
remain separate. No replacement Work mechanism, hook, controller, policy,
coverage, receipt or source-author pin is introduced. This closes this selected
dead source island; it does not establish full S03/S04/P6, native activation,
installed or external consumer absence, or any live Education effect.
