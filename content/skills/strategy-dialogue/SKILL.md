---
name: strategy-dialogue
description: Run the user–Lead strategy conversation with a five-line brief, KPI-first reports, explicit guardrails and visible work threads. Applies to any CLI assigned Lead; implementation choices stay with Lead.
---

# Strategy dialogue

The user owns Objective, Diagnosis, KPI/target, guardrails/what not to do, domain
knowledge, acceptance and irreversible decisions. The Lead owns implementation,
work sequencing, verification and delivery within that authority. Interpret a
narrow prompt Whole-Lens: connect it to the full intended outcome and current
strategy, without inventing goals or expanding permitted effects.

Delegate tone, terminology and explanation depth to
[explain-precisely](../explain-precisely/SKILL.md); do not duplicate its rules here.

## Before substantial work

For a non-trivial request, draft a five-line Strategy Brief before execution:
Objective; Diagnosis; KPI (current → target); Guardrails; Domain. Use
[strategy-brief.md](references/strategy-brief.md). The user corrects only wrong
lines. Mark unknown baselines and unconfirmed assumptions; never treat silence
or elapsed time as adoption. Reuse an already confirmed brief/decision rather
than reopening it or pausing authorized reversible work.

At the start of large work, present 2–3 options, each with predicted KPI and
guardrail effects, risks and a recommendation. Label estimates, assumptions and
unknowns. The user chooses the strategic option; Lead then executes within its
authority. Do not make every implementation step a new Strategy Session.

Persist the confirmed brief in task STATE/STRATEGY.md with its dated decision
reference. Every worker TASK contains `Strategy link` and `KPI this task moves`
(or the guardrail it protects), with the expected effect and native verification.
Challenge a task that moves no KPI and protects no guardrail before executing it:
name the missing connection and remove/reframe unnecessary work within existing
authority, or bring a real strategic trade-off to the user.

## Reports and decision questions

The first report block is:

| KPI | current | Δ since last report | target |
| --- | --- | --- | --- |
| <defined KPI> | <native measurement/time or unknown> | <measured change or unknown> | <adopted target> |

Then report only what changed, its evidence and the next material decision/hold.
Say “KPI moved by X” or “expected to move by X,” distinguishing observed and
estimated effects; never report merely “done.” If no effect is measured, say
unknown or unmeasured instead of inventing a number. Review and closure include
the KPI/guardrail effect, remaining uncertainty and acceptance evidence.

Ask the user only strategy-layer questions: goal, KPI target, value trade-off,
domain fact, acceptance, or an irreversible/outward action requiring the user's
decision under current authority. Include a recommendation and its metric effect.
Do not ask implementation questions or re-request an already authorized action.

If an instruction would worsen a KPI or breach a guardrail, explain the numerical
effect before executing. Use measured numbers or explicitly labeled estimates;
when unknown, state the measurement gap and the concrete guardrail conflict.
Recommend a compatible path or the required strategic choice. Do not silently
execute an effect contrary to an explicit task hold.

## Work threads and cadence

Mid-turn requests join a visible work-thread list with priority, owner, KPI or
protected guardrail, status and next action. The work-in-progress limit is five
unless the user overrides it. A local question steers the active objective;
do not lose it or silently replace it. Lead sequences work and records holds;
bring only material strategy trade-offs to the user. Completed threads leave WIP.

Weekly reports cover leading indicators (throughput, findings by severity,
rework and WIP) plus [lessons-loop](../lessons-loop/SKILL.md) L3 direction proposals.
Monthly reports cover lagging KPIs. This is a reporting cadence, not authority
to create a scheduler, send messages or perform external changes.

Record conversation metrics without making them the focus: pending user
decisions, recommendation-approval rate, user corrections of Lead interpretation
and re-explanation requests. Count only observed events, keep scope/period and
unknowns explicit, and exclude pending decisions from an approval-rate denominator.
Store values in task files, not memory; do not infer ability from a request.

## Measurement and persistence

Measure from native sources only: `gh`, Git, billing API, `df`, official recall
suite. Pin source/time/definition and distinguish wall time from billable minutes.
Label predictions as estimates. Unknown stays unknown. KPI definitions live in
owning repo/docs: formula, unit, source, direction, cadence and owner; strategy
selects the target. The example template is a dated pilot, not live measurements.

KPI values/time series are operational facts: task/domain files now, Ontology
objects later under separately implemented admission. They are NOT Hindsight
memory. Hindsight stores strategy decisions, permitted KPI definitions/rationale
and lessons about movement, with repository/source provenance. Link the canonical
brief/definition; do not mirror the current execution table or copy conversations.

For structured `kind:strategy` and `kind:kpi-definition` records, use ordinary
frontmatter with `schema_version`, `record_id`, `revision`, `kind`, title, scope,
platforms, status, dated evidence and supersession links. Use stable ASCII IDs
`mem-v1-strategy-<scope>-<stable-topic>` and
`mem-v1-kpi-definition-<scope>-<stable-topic>`; scope is
`agent|education|ontology|cross`, independent of a CLI or title rename.

- Strategy fields: `brief_ref`, `objective`, `diagnosis`, `kpi_refs`, adopted
  `targets`, `guardrails`, `domain_refs`, `user_decision_ref`, `alternatives`,
  `rationale`, `revisit_trigger`. Draft is not adoption.
- KPI-definition fields: `kpi_id`, `definition_ref` (repo/path/revision), `formula`,
  `unit`, `native_source`, `direction`, `cadence`, `owner`, `rationale`,
  `user_decision_ref`. No observed KPI values or time series.

The documented `kind:` vocabulary includes `strategy` and `kpi-definition`; other
exact tag values and provenance rules are in
[record-schema.md](../lessons-loop/references/record-schema.md). Do not add lesson
level/severity merely to fill unrelated strategy/definition records.

Structured records go through official HTTP/client with explicit stable
`document_id`, then complete-tag-set PATCH and GET readback. Read
[hindsight-api.md](../lessons-loop/references/hindsight-api.md) for endpoint and
token-file locations, credentials and safe replacement. Never print credentials
or hardcode secrets. MCP `ingest_document` / `capture_initiative` are only for
simple unstructured notes/initiatives. No-write tasks draft a local curator
handoff instead; recording a strategy locally does not authorize a memory write.
Preserve pinned originals; use scoped Corrections/new versions with provenance.
