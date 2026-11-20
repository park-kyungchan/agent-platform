# Judging: designs, units and findings

Read when a design note or an attack is due, when a unit is accepted or stopped, when a review or a measurement brings findings, when you take a task over, and when a second blocker hits the same part.

## 1. A design note and its attack

Due before the build only for three things: a store format, the schema of an authoritative store, or the order of record and effect on a real target. A plan, and a design that fixes structure across repositories, is reviewed instead as a plan (`ap unit review`, kind `plan`; the frame `crates/ap/src/unit/frames/plan-reviewer.md`).

The design note is `DESIGN-<topic>.md` in the task directory. Write section 0 first, and only from what a session read or ran TODAY, with the command or the file and line; when you have not read the code, a worker's page produces that list before the design exists. A design written from the memory of a plan or of code is the error this section stands against.

```markdown
# DESIGN: <topic>
0. Facts it rests on, each with its command or file and line and who read it today; the user's decisions and the plan's rows on this topic, and every departure from them.
1. Use: who uses the result first, by which route, shown by which real run.
2. Decision: the structure chosen; the alternative refused and why; what is not built.
3. Invariants, each with the executable that breaks on a wrong implementation; for an effect, the walk through a crash between every two steps and what each leaves.
4. Pull requests in order, each with what it makes usable and its estimate in changed lines.
5. Undo, and the point after which undo needs a migration.
6. Open points only the user can decide.
7. Least sure of: what the reviewer should break first.
```

A design is checked against these rules. One implementation: a mechanism the engine, a crate, a contract or a service holds is consumed, not built again. One authority per fact: no mirror and no registry of what Git, the hosting provider or the file system lists; a derived store is rebuilt, not migrated. One shape for an effect on a real target: an action records the attempt before the effect and refuses while an attempt on the same target is open; the owner performs the effect once, after a newly applied attempt; an action records the outcome; an open attempt is closed only by a read of the target. Proportion: an instrument is smaller than what it measures; a fact about a real service is measured on that service; nothing is built for an incident without harm.

The attack is one reviewer session in a fresh context that did not write the design; every statement of the design is a claim to it. Its prompt gives the design's path, the code it changes (worktree and revision), what it must not read or call, and the rows of `OFFICIAL-DOCS.md` section 4 its question needs as paths with line ranges (up to four). It is read-only outside its output directory, builds and runs only in a scratch copy it removes, uses no memory tool and no subagent, and writes one line per finding to a notes file as it goes. It is asked, in this order:
1. First, before the design's own reasoning: list the user's decisions and the plan's rows on this topic and test the design against each; a departure the design does not name is a finding.
2. Re-run or re-read each fact of section 0; a wrong fact, or one no one read today, is a finding.
3. Break each invariant with an input, an order of events or a crash between two steps: counterexamples executed where it can, not opinions.

Its first line is exactly `Verdict: sound`, `Verdict: sound with changes` or `Verdict: unsound`; then "Counterexamples" (each with the command and what it showed), "Departures from decisions and plan", "Wrong facts", "What the design does not answer", "Not verified". You decide each change and rewrite the note yourself.

A design of the scale of a whole intent (more than about ten units, or the store, delivery or the login path) gets, after its reviewer accepted, one more look by a session with no history before the next units are planned on it.

## 2. What blocks

A finding blocks only when it is one of: wrong behaviour shown by a command or a test the reviewer supplies; loss or exposure of data; a broken invariant of the page or a broken contract; an unguarded irreversible effect on a real target. Nothing else blocks. A note that does not block stays in the report on the pull request, or goes into the next unit's page when it matters. No commit is added to a head the verifier accepted.

## 3. Judging an accepted unit

- A machine check comes before a session's look: what a check, a test or the gate can show is not left to a reviewer's reading.
- A unit of class `plain` is accepted by its verifier and merged by `ap unit run`; you read its report.
- A unit of class `contract` or `effect`, and any unit with a real effect, a schema, a store format or a guard of a live request, stops at `accepted`: you read the production diff yourself, then `ap unit approve` with the reason, or `ap unit reject`. A page file run with `ap unit <page>` gets `merge: false` for the same units: read the diff, then call again with `pull_request` and `verdict`.
- Before you build on several merged units as one base: accepts of single units do not show that the whole works. One real run shows the first use end to end, and each cited contract is pinned and executed in its consumer.

## 4. Judging a stopped unit

`ap status <key>` names the cause. A stop gets one repair: yours when it is under about 30 lines, else an amended page (`ap unit amend`, a new page review for class `contract` or `effect`) and `ap unit run` again. A worker that reports that a rule of its page cannot hold (`page_cannot_hold`) is answered by amending the page, never by weakening the rule. A second stop in the same part ends the unit for a redesign (section 5).

## 5. Evaluation and triage

At each moment this file is read for, look at what exists as its opponent. Measure what a live route runs, who consumes what, the size by language and crate, and how long a change takes from the first check to the merge. Ask of each built thing: who uses it today; what it repeats; what is out of proportion (a plan far past its first estimate, an instrument larger than what it measures, a mechanism built for an incident without harm); what is dead. An accepting review inside a design does not show that the design's first choice was needed. When a plan has grown far past the user's words, dispatch none of its units: name the first choice that drives the size and compare it with the smallest design that meets the user's words through the native mechanism. Where a structure is the bottleneck, change direction, record the decision with the alternative refused and its undo, and tell the user in the next message. When your own context is the cause, hand the task to a fresh Lead (skill `lead` section 4): never between the creation of a credential and the readback of the effect it serves, never with the verdict of a real-effect unit half read, and with no effect of yours after the start prompt is sent. Before sending, give a fresh worker that writes nothing (or yourself, marked self-checked) only the start prompt and ask for the next action and its check, what may not be done, what is unverified, and which file it would open to choose a worker; each wrong answer is a defect of the documents, repaired there; the test and its result go into STATE.

Every finding gets one disposition: a decision now, a unit, a named later point, or kept with the reason.

A correction of the user: find the one thing it changes and how far it reaches (this task, or every task), change it in the one file that holds it, and say back in one sentence what changed from what to what. A second correction of the same kind becomes a case of the Lead suite (`evals/lead/`). When you broke a rule that exists, add no sentence: make the step a command or a check, or record why it cannot be one.
