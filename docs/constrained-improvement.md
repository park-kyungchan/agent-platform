# Constrained improvement

Use this method for substantial optimization of an actual consumer workflow: a context reader,
prompt, implementation, model choice or delivery step. Routine fixes do not need another process.
This is a local design transfer from dated AIP Evolve/Evals knowledge, not a Foundry integration,
product API contract, automatic adoption rule or permission to execute native work.

Before trying candidates, declare the question in the owning Work:

| Element         | Declare before observing candidate results                                                               |
| --------------- | -------------------------------------------------------------------------------------------------------- |
| Target          | Actual consumer behavior and exact baseline/candidate versions or source pins.                           |
| Goal            | Useful improvement for that consumer and the behavior that must survive.                                 |
| Validation      | Representative cases, independent expected behavior, evaluator identity/version and comparison criteria. |
| Allowed changes | The code, prompt, configuration or other treatment that may change; preserve owner boundaries.           |
| Limits          | Bounded attempts, time and other relevant resource limits; conditions for stopping.                      |
| Decision        | Evidence needed to propose accept, repair, defer or stop.                                                |

Define useful completion before efficiency. Include costly omissions, caller changes, recovery and
intervention when they matter. Fewer lines, shorter replies and lower token counts alone do not
establish improvement. Do not combine failed quality gates with efficiency into a weighted score.

## Quality-first improvement

For common-harness quality work, predeclare the affected consumers and mandatory criteria.
A baseline target failure is an opportunity to repair. A failure of experiment provenance,
permission, original-history custody or admitted limits prevents attribution or stops work.
Do not confuse those two kinds of failure.

| Observed transition                                                                  | Supported claim                                          |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| Target criterion violated → satisfied                                                | Behavior improvement within the observed evidence class. |
| Target criterion satisfied → satisfied                                               | Preserved behavior; no demonstrated behavior gain.       |
| Target criterion satisfied → violated                                                | Regression; repair before proposing adoption.            |
| Evidence insufficient → better evidence                                              | Resolved uncertainty; no automatic behavior gain.        |
| Prior oracle misses a fixed defect → candidate detects it, valid controls still pass | Verification detection improvement, reported separately. |

A proposal is ready for independent review only when pinned, comparable evidence supports a
predeclared quality gain and all required candidate criteria and controls pass. Required unknowns
defer the conclusion. Faster execution, smaller prompts, more checks and historical fixes alone
are insufficient. Synthetic injected failures, source/adapter tests and actual native observations
remain distinct. In particular, this method does not establish a causal Ponytail instruction
benefit without separately admitted native task observations.

Use `quality-preflight` and `quality-compare` through the existing `bun run harness` entrypoint for
this quality path. The existing `improvement-*` commands preserve the v1 scalar efficiency contract.
Quality observations remain caller-supplied evidence; a valid envelope does not authenticate its
producer, authorize dispatch or establish Acceptance. Both paths preserve unknowns and limits.

## Apply at the existing owner boundaries

| Boundary                   | Apply the method                                                                                                                          |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Interview → Work/Spec/Plan | Preserve purpose, original answers and independent criteria before candidate results.                                                     |
| Lead → worker              | Pin selected contract, relevant original decisions, dependencies, allowed changes and durable output address.                             |
| Producer → consumer        | Test public contracts and all affected paths: common changes include Codex and Hermes adapters.                                           |
| Interruption → recovery    | Preserve completed/uncertain effects and next safe action; never replay to improve a result.                                              |
| Branch → integration       | Use separate Task worktrees, exact commits/trees, semantic transitions and expected refs.                                                 |
| Review → PR/merge          | Freeze source; publish independent exact-head/tree evidence and authenticated status before PR; Lead explicitly merges the expected head. |
| Merged source → runtime    | Each native owner separately verifies and activates; source checks do not prove native adoption.                                          |

Git-owned publication, concurrency and original/history preservation use the existing Git Work
reader, branches, commits and semantic checks. A clean textual merge is not sufficient proof of
logical compatibility. Preserve prior Inputs, historical Task contracts and third-state changes
through successor records. Live untracked native state stays with its existing owner and bounded
apply/observation contract; Git does not control open file descriptors. Do not add universal
filesystem CAS, permanent byte immutability, another scheduler or a generic lock service.

### CI Workflow Agent as one application

A CI improvement Task consumes the exact repository/head/tree, attempt and job identities,
immutable review evidence, authenticated latest status and available source/dependency evidence.
Its output is a diagnosis and scoped proposed fix, with uncertainty and affected consumers.
Native runtimes retain dispatch, retries, tools and credentials; this design adds no agent loop.

Count one incident across its jobs/retries, retain repaired attempts, and distinguish source failure,
missing dependency access and missing review publication. A final green alone loses that history.
The observed 18 review-timing incidents motivate publication ordering: source SHA exists remotely,
then immutable independent evidence/status, then PR. A fixed wait is not proof of publication.
On merge, the Lead must qualify the resulting commit explicitly; identical-tree reuse requires a
successor binding and current zero findings. If main CI runs before that proof arrives, inspect the
same run and explicitly rerun the affected job after publication; no review-arrival observer or
automatic retry/merge follows. Preserve repository rules, including merge-only rules where present.

## Compare the same work

Choose cases from normal work and consequential failures. Preserve the case input and expected
behavior independently of the candidate output; the candidate's own success statement is not an
evaluator. Select deterministic checks where possible and explicit bounded judgment where needed.
Keep the evaluator, its criterion and its evidence separate from the target being evaluated.

Bind each baseline/candidate observation to its case, iteration, target version and pinned output.
Record what context, treatment and environment actually reached each target. Those exposures must
be equivalent except for the declared treatment. A fresh session name or equal prompt filenames
do not prove equivalent exposure. Missing or uncertain observations remain unknown.

Check useful behavior and non-regression gates before comparing time, cost or footprint. Preserve
target execution errors separately from evaluator errors and unsuccessful criteria. Inspect variance
before attributing a change to the treatment; repeat within the declared limits when the question
needs it. Repetition cannot fix wrong expected behavior or absent treatment exposure.

## Use existing records and evaluation contracts

| Existing owner/record          | Role in this method                                                                             |
| ------------------------------ | ----------------------------------------------------------------------------------------------- |
| Work, Spec and Plan            | Intent, success criteria, permitted scope and the authored work to perform.                     |
| Work Input                     | Exact user/source-owner input containing the declared improvement question and source bindings. |
| Work Evidence                  | Source references to observations, verification and reviews; keep evidence kinds distinct.      |
| EvalDataset / VerificationUnit | Versioned case selection, inputs and expected criteria.                                         |
| Evaluator / EvalSuite          | Versioned evaluation method, selected target and bounded evaluation configuration.              |
| EvalRun / CaseResult           | Actual evaluation occurrence and case-specific results, metrics, diagnostics and evidence.      |

These are mappings to existing contracts, not new persisted entity types or parallel stores.
Use `@palantirkc/work-continuity/git-work` for committed Work inputs and evidence. The pure helpers
`preflightImprovement` and `compareImprovement` belong to the canonical evidence-foundry evaluation
module. They inspect supplied contracts and captured results; they open no store and launch no run.

The read-only harness commands accept prepared JSON requests:

```bash
bun run harness improvement-preflight <request.json>
bun run harness improvement-compare <request.json>
```

Use preflight before spending on a candidate, then compare captured evidence. The commands do not
execute the candidate, authenticate native authorship, persist a decision or activate a change.
Keep request files with the owning Work's inputs and reference exact observed outputs through
Evidence; do not replace canonical Work records with an optimization-specific ledger.

Preflight accepts an `ImprovementProtocol`; compare accepts
`{ protocol, baseline, candidate, changedPaths, usage }`. The exported types in
`packages/evidence-foundry/src/modules/evals/improvement.ts` define these transport inputs.
When the protocol is an existing Work Input's `content`, extract that selected content into the
request; a whole Work Input record is not a protocol. Pins remain caller declarations, not
authenticated source reads or an evaluation grant.

Normalize observations to the declared metric unit and the same cost currency before comparison.
`goal.minimumDelta` is an absolute amount in that unit, and the helper compares paired arithmetic
means. Preserve case-level results for review: a Work may additionally require medians, quality
judgments or per-case regression criteria. The helper does not silently enforce those extra rules.
Missing metrics and required usage remain unknown and defer comparison; optional unobserved cost
stays `null`. Known scope/usage stops survive a malformed result envelope.

Native Codex/Hermes retain task execution, sessions, tools, retries and permissions. In the existing
`runSuite`, a native target may run again for each evaluator. When several graders should judge the
same occurrence, collect the native task output once through its native owner, pin that output and
grade it offline with the existing evaluation contracts. Do not introduce a second native runner
or replay completed effects to obtain a more favorable result.

## Consume the evidence

The result is an evidence-backed proposal:

- **Accept:** comparable observations support the declared improvement and required quality gates.
- **Repair:** a specific defect or unmet criterion justifies another permitted change within limits.
- **Defer:** missing, uncertain or incomparable evidence prevents a supported conclusion.
- **Stop:** the limit or stopping condition has been reached, or the proposed change exceeds scope.

Retain the reason, observed limitations and exact evidence references. A proposed accept is not an
AcceptanceDecision, merge approval, release or native activation. A failed experiment does not
authorize weakening a guard, changing the evaluator after seeing results or silently raising limits.
The accountable owners separately decide review, delivery and acceptance under existing rules.

## Dated knowledge basis

The following SharedWiki source bytes were captured read-only on 2026-10-18. Their original source
clocks remain in force; the capture did not revalidate current product availability or native APIs.
They provide a design analogy, not local execution authority or a claim of measured savings.

| Captured source filename                   | As of      | SHA-256                                                            |
| ------------------------------------------ | ---------- | ------------------------------------------------------------------ |
| `palantir-aip-evolve.md`                   | 2026-09-14 | `901c814e0e238f278296b6805af7b8a0548100336d128e4de1a4ace5f617c5b0` |
| `palantir-aip-evals-official-mechanics.md` | 2026-09-06 | `8484d04075f474abfe3756813dee441bd4d614350e2b49ba9b13bb9f28ea0024` |
| `ontology-aip-evals-sdlc-agent-start.md`   | 2026-10-04 | `71216108a09fee1abdba1d2107df465d429d243bf8b7957ea535e1a5bcd3c9d3` |

The captured pages cite the official [AIP Evolve overview](https://www.palantir.com/docs/foundry/aip-evolve/overview),
[Evals suite guide](https://www.palantir.com/docs/foundry/aip-evals/create-suite),
[run guide](https://www.palantir.com/docs/foundry/aip-evals/run-suite) and
[result analysis](https://www.palantir.com/docs/foundry/aip-evals/analyze-run-results).
Check current official contracts only when an actual integration depends on them.
