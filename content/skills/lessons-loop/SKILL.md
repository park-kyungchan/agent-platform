---
name: lessons-loop
description: At task closure, on a repeated user correction, or as assigned curator, capture L0–L3 lessons, turn repeats into rule changes and route structural proposals to the Lead; no mandatory lesson.
---

# Lessons loop

## When to use, and the short form

Use at the closure of a task or a worker run, when the user corrects the Lead, and when STATE names you curator.

1. At closure, record in the task's files: workers, review findings by severity, rework rounds, CI minutes. An unknown value stays unknown.
2. Capture a lesson only when something durable was learned. Levels: L0 incident, L1 lesson, L2 pattern, L3 structural issue with a direction proposal. Promotion needs independent occurrences of a verified mechanism, never a document count.
3. A correction from the user is an occurrence. The first one changes the instance (skill `lead` section 3). A second correction of the same kind is a missing or unclear rule: the Lead changes the rule in the file that holds it, through a pull request, and reports the change.
4. An expression of the user that appears in a second task is added to `/root/.agents/runtime-references/GLOSSARY.md` section 7 the same way.
5. Engineering capture follows the Git-original/native-publisher contract in `agent-platform` `memory/README.md`, only within task authority and without student, lesson or operational payloads. In a no-write task leave a short handoff file in the task directory and name it in STATE.
6. An L3 direction proposal goes to the Lead and into the current STATE under `DIRECTION PROPOSALS`. The Lead decides it by the measure it would move, carries it out as a reversible trial where one exists, and reports the decision and the result to the user.

The sections below give the details. They build no database, ledger, watcher, grader or transcript pipeline.

Use [worker-closure.md](references/worker-closure.md) for task-local closure evidence, metrics, holds and optional lesson candidates; the reference grants no memory write or new approval step.

## Capture through reviewed originals

Read [record-schema.md](references/record-schema.md) for the schema's source home,
and [hindsight-api.md](references/hindsight-api.md) for the publisher's operational
home. These references grant no direct engineering API/MCP write.

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

Draft admitted engineering originals under `agent-platform/memory/records/` and
commit them in the task's closure PR; the reviewed-main native publisher alone
writes their retrieval copy. The runbook owns schema, subjects/tags, maintenance,
retraction, evidence and activation. No MCP ingestion/initiative, direct retain,
manual tag PATCH, page refresh or session transcript is an engineering capture route.
An initiative or direction proposal remains intent until the current owner adopts
it; no record becomes an operational task tracker or an authority grant.

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
it and reports the decision and its result to the user.

## Readback and derived views

Use only the task-authorized read route and verify operational claims at their
native owner. The prepared engineering route is scoped native recall and exact
document reads; no reflect or engineering pages. Credit memory used. Follow
[CONTEXT-GOVERNANCE.md](../../runtime-references/CONTEXT-GOVERNANCE.md).

Publication/retraction, due review and canary/full-evaluation timing have one home:
`agent-platform` `memory/README.md`. Keep unknown coverage/costs unknown, and keep
failed or unqualified routes unavailable. No custom judge, scheduler or second
history store. The separate Academy archive retains its private owner/contract;
its lesson pages are not engineering views and are not changed by this skill.
