# Worker closure evidence

Use at an assigned worker's task/pane closure and the accepting Lead's readback.
This is a task-local report template, not a new global skill, parser, ledger,
Hindsight record or approval step. Apply the current TASK/STATE's allowed writes;
a read-only task writes only its already permitted local result. If a metric,
process or session observation is unavailable, say unknown rather than launching
a client, reading transcripts or expanding authority to obtain it.

[Lead](../../lead/SKILL.md) owns KPI-first reporting;
[explain precisely](../../explain-precisely/SKILL.md) owns user-facing language.
[Context Governance](../../../runtime-references/CONTEXT-GOVERNANCE.md) owns
source authority, isolation/continuity and memory boundaries. Parent-facing
outputs retain their domain format. Link these contracts; do not copy them.

## Task-local report template

The first block is a KPI table. Keep baseline provenance and observation time,
current value, delta, target and measurement status explicit:

| KPI / guardrail | baseline (source/time) | current | delta | target | measured / expected / unknown |
| --- | --- | --- | --- | --- | --- |
| <task's adopted KPI/guardrail> | <native evidence or unknown> | <value or unknown> | <value or unknown> | <adopted target> | <status> |

Then include only applicable evidence:

- Exact TASK/STATE and Strategy link; source repo/worktree/branch, base and final
  head; allowed write set and actual changed files. State task/user-visible
  behavior and the observed or explicitly expected KPI/guardrail effect.
- Source/output/effect readback with bounded locators/hashes. List checks actually
  run and their meaningful output, material gaps and any uncertain effect that
  still needs authoritative target reconciliation. Do not call exit zero alone
  an accepted outcome.
- Independent review status and findings by severity (unknown if unavailable),
  rework rounds, PR URL/head and required checks for that exact head. Record CI
  wall time and billed minutes separately; do not infer billing from wall time.
- Current holds, remaining work and exact next owner/action. A PR-only assignment
  leaves merge/deploy with its owner and does not claim installation or loading.
- Worker/runtime/model/effort and native pane/tab/session identities only when
  observed or supplied by authoritative assignment. Reconcile owned artifacts,
  child processes, builds and waiters within authority; preserve supported native
  session/result evidence before authorized closure. Keep unrelated resources.
- Zero or more lesson candidates: independent incident identity, bounded cause
  and confidence, source evidence, mitigation, scope and counterevidence. No
  novel lesson is a valid outcome; routine closure need not become memory.

A DONE marker, file existence, idle state or a submitted prompt is a completion
signal to inspect, never acceptance evidence. The accepting owner reads back the
actual candidate, result and material effects under the task's acceptance rules.
Do not retain completed worker context merely for convenience; apply the linked
Context Governance contract for an actual unfinished same-scope continuation.

## Optional curator handoff

Ordinary closure and live metrics stay task-local. Only an authorized lessons
curator decides whether sanitized durable evidence merits official capture under
the parent lessons-loop skill and its existing schema/API references. A no-write
task leaves an allowed local candidate; it does not ingest, retag or refresh.

Exclude credentials, raw student/exam/lesson/operational facts and session content.
Do not mirror STATE or live KPI time series in Hindsight, re-ingest derived pages,
access its database directly, retain transcripts automatically, create a custom
schema grader or invent an incident to satisfy this template. Historical aggregate
engineering metrics may support a permitted lesson without becoming a live table.
