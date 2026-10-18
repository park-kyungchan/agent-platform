# Constrained improvement

Use this method for substantial optimization of an actual consumer workflow: a context reader,
prompt, implementation, model choice or delivery step. Routine fixes do not need another process.
This is a local design transfer from dated AIP Evolve/Evals knowledge, not a Foundry integration,
product API contract, automatic adoption rule or permission to execute native work.

Before trying candidates, declare the question in the owning Work:

| Element | Declare before observing candidate results |
| --- | --- |
| Target | Actual consumer behavior and exact baseline/candidate versions or source pins. |
| Goal | Useful improvement for that consumer and the behavior that must survive. |
| Validation | Representative cases, independent expected behavior, evaluator identity/version and comparison criteria. |
| Allowed changes | The code, prompt, configuration or other treatment that may change; preserve owner boundaries. |
| Limits | Bounded attempts, time and other relevant resource limits; conditions for stopping. |
| Decision | Evidence needed to propose accept, repair, defer or stop. |

Define useful completion before efficiency. Include costly omissions, caller changes, recovery and
intervention when they matter. Fewer lines, shorter replies and lower token counts alone do not
establish improvement. Do not combine failed quality gates with efficiency into a weighted score.

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

| Existing owner/record | Role in this method |
| --- | --- |
| Work, Spec and Plan | Intent, success criteria, permitted scope and the authored work to perform. |
| Work Input | Exact user/source-owner input containing the declared improvement question and source bindings. |
| Work Evidence | Source references to observations, verification and reviews; keep evidence kinds distinct. |
| EvalDataset / VerificationUnit | Versioned case selection, inputs and expected criteria. |
| Evaluator / EvalSuite | Versioned evaluation method, selected target and bounded evaluation configuration. |
| EvalRun / CaseResult | Actual evaluation occurrence and case-specific results, metrics, diagnostics and evidence. |

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

| Captured source filename | As of | SHA-256 |
| --- | --- | --- |
| `palantir-aip-evolve.md` | 2026-09-14 | `901c814e0e238f278296b6805af7b8a0548100336d128e4de1a4ace5f617c5b0` |
| `palantir-aip-evals-official-mechanics.md` | 2026-09-06 | `8484d04075f474abfe3756813dee441bd4d614350e2b49ba9b13bb9f28ea0024` |
| `ontology-aip-evals-sdlc-agent-start.md` | 2026-10-04 | `71216108a09fee1abdba1d2107df465d429d243bf8b7957ea535e1a5bcd3c9d3` |

The captured pages cite the official [AIP Evolve overview](https://www.palantir.com/docs/foundry/aip-evolve/overview),
[Evals suite guide](https://www.palantir.com/docs/foundry/aip-evals/create-suite),
[run guide](https://www.palantir.com/docs/foundry/aip-evals/run-suite) and
[result analysis](https://www.palantir.com/docs/foundry/aip-evals/analyze-run-results).
Check current official contracts only when an actual integration depends on them.
