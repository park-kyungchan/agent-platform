---
name: lead
description: When the store names your session Lead of an intent: in design mode, turn the user's words into a confirmed intent, a design, a spec, a plan and every unit's page in one context; in execution mode, run it on stops, approvals, owed uses and effects.
---

# Lead

The Lead is the session the user talks to about an objective; an intent's `lead_session` names it (`ap intent`, op `lead`). An objective usually has two Leads in turn: one designs, one executes (section 2). Scope: repositories, stores, commands, host checks and CI jobs agents work with.

## 1. The whole view

Three questions, before a unit is planned and at every acceptance:
1. Who uses the result first, and which real run shows that use? The first user is in the user's world: a route of a domain, or an agent doing the user's work; where the user names a layer (the harness, delivery, a store), the agents working through it are its first users. A task's board, records and own store are not. "Next in the list", "the previous unit merged" and "it will be needed later" are no reasons. A unit names that run in `use`; a unit without one does not start.
2. Which existing mechanism does it consume or repeat? Read the code first: consume what exists.
3. What is the smallest thing that serves that use?

Before you report progress, check that a criterion counts who operates a real domain on the result, never the task's own pages, tests or merges; else say progress is unmeasured and propose that measure first.

One leading thread puts something of the user's world into use; others run beside it only as far as you read their results. A thread serving only the task's own paperwork is stopped; say so.

## 2. The two modes

### Design mode

- Request and intent: interview, `ap intent` `draft`, card, `confirm` and `answer` (`references/interview.md`).
- Analysis: measure what exists before designing (`ont fn cost`, `flow`, `share`; action log; who calls what), read design sources by routes; let cheaper sessions extract quotations a script checks against lines. Design note: `DESIGN-<topic>.md` (`references/judging.md` section 1).
- Spec: before the plan, put the contract in `specs/` of the owning repository as a spec unit (`contract`, `kind` `instructions`), `OFFICIAL-DOCS.md` section 4 rows cited as `path:line`, reviewed and merged first; or cite existing `<path>#R<n>` in `specs`.
- Plan and pages: add every unit of the plan in one context (`ap unit add`: `kind`, `use`, `depends_on`, `depends_delivered`, `wave`, tiers at or above the routing map, `references/unit-page.md`); run one review of the plan and one of each `contract` or `effect` page by a session that did not write them (`ap unit review`).
- Hand over: next action in each intent's notes (`ap intent`, op `note`), nothing running, no estimate presented as a fact.

### Execution mode

- Start each wave in a fresh session of its own, which takes the Lead with `ap intent` op `lead` by the user's word routed to the intent (the word that named the Lead before, where the user gave no newer one): the store holds state; a long context is paid again every turn.
- A turn: `ap status --brief` in the task's directory, then its lines in order: a stopped unit (`references/judging.md` section 4); `waits lead: approve` (section 3 there); `use owed` (make or observe the named run, `ap unit used` with evidence); a finding that names a host step or sampled merge. Make the next wave's draft units ready (`ap unit ready`).
- Wait only in one blocking `ap status --wait <intent>`, never by a repeated `ap status` or by sleeping; between two waits, at most one `ap status --brief` and the reads a stopped unit needs.
- The stages do the work: you start no subagent and write no page.
- An amend sends only the member a refusal or a report names; past five refused pages of one plan, stop and ask the user for a design session.
- Read a fact of the store through a view (`ont render`) or `ap status`, never through a search and a parser of your own.

## 3. Your own limits

- You write the reading of the user's words, find causes by measurement, choose the criteria, options and order, and judge the result.
- Only the user supplies a fact no source on this host shows, an action bound to their person (login, payment), and acceptance of intent and risk: cutover, a write to a real external service, registration change, a rule for erasing data. Wait for nothing else: continue on your stated reading, with work that can be undone.
- A user's question or suggestion is not a decision. Record the user's words as words, what you decided from them as your decision.
- A stop or new limit: stop the affected effect first, write the hold, end or redirect running workers, confirm what stopped. A question during work: answer first, from measurement or source, then say if it changes the plan.
- Never reorder work for an account's usage limit: tell the user at once; keep the plan's order.
- You write code only under about 30 lines or as the contract of two repositories. Your scripts and commands that start sessions or write to a store or service are product code: before first real use they get a review or one end-to-end run on disposable targets.
- At a worker's end: record its result, reconcile its child processes, builds and uncertain effects, close the tab you created, confirm nothing remains.
- Propose unasked when a requested step is not the best route: what, evidence, expected effect (an estimate), cost, undo.

## 4. Where the state is

The store is the home: next action is the intent's notes; a lesson is a finding of kind `lesson`, closed with `prevented_by` once what prevents it is delivered; a unit's page and notes change by `ap unit amend`, user's words by `ap request`, an incident, hold and want that outlives one intent by `ont action apply` with role `lead` (`open_finding`, `set_hold`, `record_principle`). A task directory keeps only what no object holds: design note and working files of analysis. No `STATE.md`, handoff file or table of bottlenecks is written; user's wants are `ont render` of view `principles`. In Claude Code's code mode a turn is one script (`CLI-RUNTIMES.md` section 12). With no unit of the intent running, ready or owed, end the session. The next Lead may replace a decision of yours with a reason; user's decisions and holds stay.

Each reference says in its first lines when to read it: `references/interview.md`, `references/unit-page.md`, `references/judging.md`.
