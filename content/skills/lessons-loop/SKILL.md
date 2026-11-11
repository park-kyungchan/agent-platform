---
name: lessons-loop
description: At task closure, on a repeated user correction, or as assigned curator, capture L0–L3 lessons, turn repeats into rule changes and route structural proposals to the Lead; no mandatory lesson.
---

# Lessons loop

## When to use, and the short form

Use at the closure of a task or a worker run, when the user corrects the Lead, and when STATE names you curator.

1. At closure, record in the task's files: workers, review findings by severity, rework rounds, CI minutes. An unknown value stays unknown.
2. Capture a lesson only when something durable was learned. Levels: L0 incident, L1 lesson, L2 pattern, L3 structural issue with a direction proposal. Promotion needs independent occurrences of a verified mechanism, never a document count.
3. A correction from the user is an occurrence. The first one changes the instance (skill `strategy-dialogue` section 6). A second correction of the same kind is a missing or unclear rule: the Lead changes the rule in the file that holds it, through a pull request, and reports the change.
4. An expression of the user that appears in a second task is added to `/root/.agents/runtime-references/GLOSSARY.md` section 7 the same way.
5. Write to long-term memory only when the task authorizes a write, through the official Hindsight HTTP API or client, without student, lesson or operational payloads. In a no-write task leave a short handoff file in the task directory and name it in STATE.
6. An L3 direction proposal goes to the Lead and into the current STATE under `DIRECTION PROPOSALS`. The Lead decides it by the measure it would move, carries it out as a reversible trial where one exists, and reports the decision and the result to the user.

The sections below give the details. They build no database, ledger, watcher, grader or transcript pipeline.

Use [worker-closure.md](references/worker-closure.md) for task-local closure evidence, metrics, holds and optional lesson candidates; the reference grants no memory write or new approval step.

## Capture and official ingestion

Read [record-schema.md](references/record-schema.md) when drafting or curating a
record; it defines the exact frontmatter, tags, promotion and status rules.
This is a human-authored convention inside ordinary Hindsight documents, not a
new database schema, validator, runtime gate or executable ledger.

At closure the assigned task owner records actual workers, review findings by
severity, rework rounds and CI minutes in task STATE. Distinguish CI wall duration
from billable minutes. Count a completed subtask/incident once even if pane and
parent task both close. Capture a concise L0 incident or L1 candidate only when
something durable was learned; retain allowed source locators/hashes, no raw
student/exam/lesson/operational payload. The Academy lesson archive is a separate
record kind owned by education-platform
`content/skills/academy-student-context/references/lesson-archive.md`, not a lesson.
Unknown cause or metrics remain unknown.
In a no-write task, leave an allowed task-local handoff for the curator instead.

Structured L0–L3, decision, correction, glossary/profile, strategy and
KPI-definition records use the official HTTP API/client: explicit stable
`document_id`, then complete-tag-set PATCH and GET readback. Read
[hindsight-api.md](references/hindsight-api.md) for endpoint/token-file locations,
credential handling, exact ordering, provenance and safe replacement. Never print
secrets. The review correction takes precedence over title-only MCP capture.
MCP `ingest_document` / `capture_initiative` are only for simple unstructured notes
and initiatives. A no-write task leaves a local curator handoff instead.

A genuinely approved new initiative uses official `capture_initiative` once;
a material scope/rationale change reuses its existing page. It is dated intent,
not a second task tracker or authority grant.

## Promotion, curation and structural proposals

An explicitly assigned curator (CLI and cadence named in the task STATE) reviews
closure candidates weekly and after repeated/material incidents. No CLI is the
curator by default. Verify primary evidence and privacy, deduplicate by
occurrence identity, link scoped Corrections and promote only verified chains:

- L0 → L1: cause/mitigation evidence and a bounded prevention rule.
- L1 → L2: at least two independent incident chains sharing a verified mechanism.
- L2 → L3: recurrence across layers/platforms or failure after codification;
  an exceptional critical architectural contradiction may justify a proposal
  without waiting for another harmful event. Document the exception.

Create a new linked higher-level record, preserving each incident's identity.
Review recurring failure classes, contradicted assumptions and failed
codifications. An L3 proposal reaches Lead → current STATE `DIRECTION PROPOSALS`
with its ID/pin, evidence, alternatives (including keeping the design), expected
quality/cost effect, reversible trial and success/stop criteria. The Lead decides
it and reports the decision and its result to the user; an irreversible outward
action inside a proposal waits as skill `strategy-dialogue` section 1 says.

## Readback and derived views

Search/read selected pages first; use official recall for exact evidence and
reflect only for unresolved reasoning. The installed MCP has page search/read
and reflect, but no raw recall tool; use the official HTTP/client recall route.
Credit memory used and verify against current native source. Follow
[CONTEXT-GOVERNANCE.md](../../runtime-references/CONTEXT-GOVERNANCE.md).

Only an authorized curator changes/refreshes affected Lessons ledger, Structural
risk review, User glossary or Conventions and patterns pages. These proposed
views are not assumed installed. Check original-document extraction, observation
source scope, refreshed page content and recall separately; a queued operation
or `is_stale:false` does not establish semantic freshness. Strict tagged scopes
can miss shared untagged observations; non-strict topic routing is not privacy.
Never re-ingest a generated page as new source evidence.

Use the task's official recall regression suite after each authorized migration
sub-batch/model batch, weekly curation and routing/tag/model changes. Preserve
exact specialist contracts, reject stale-current answers and record errors as
errors. No custom grader, transcript pipeline or new scheduler. Record curator
candidate/verified/promoted/superseded counts, independent recurrence, recall
outcomes/stale-current claims, unresolved corrections, source coverage, model
failures and observed consolidation/refresh time/cost; missing values stay unknown.
