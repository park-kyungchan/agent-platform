---
name: lessons-loop
description: At task closure or assigned curation, capture authorized L0–L3 engineering lessons and route structural proposals through Lead; no mandatory lesson.
---

# Lessons loop

## Skill contract

At task/pane closure, read current TASK/STATE and the relevant primary evidence. Record workers, review findings by severity, rework rounds and CI minutes; leave unknown values unknown. Capture only durable, student-free incidents/lessons through authorized official Hindsight HTTP API/client. Respect explicit no-write tasks: draft a local handoff and do not ingest, retag, refresh or delete. Preserve source IDs/content pins and distinguish user decisions, agent reports and verified output. Use the L0 Incident -> L1 Lesson -> L2 Pattern -> L3 Structural issue/direction-proposal schema and exact tags in the approved memory design; never promote by document count. Preserve independent occurrence identity, cause confidence, scope, counterevidence, status and codified_in. A curator may promote only with verified evidence; preserve pinned originals and use scoped Corrections. Send L3 proposals through Lead to current STATE `DIRECTION PROPOSALS`; do not enact them. Use official recall/readback and refresh only within actual authority; no database, custom ledger, watcher, grader or transcript pipeline.

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

An explicitly assigned periodic curator (CLI and cadence assigned in task STATE;
the 2026-11-05 default assignment is Codex) reviews closure candidates weekly
and after repeated/material incidents. That is a dated task assignment, not a
permanent CLI privilege. Verify primary evidence and privacy, deduplicate by
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
quality/cost effect, reversible trial, success/stop criteria and decision owner.
Lead presents the concrete decision to the user; no proposal enacts itself.

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
