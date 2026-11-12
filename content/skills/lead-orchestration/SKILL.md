---
name: lead-orchestration
description: When STATE names you Lead, decide what to delegate, shape each task packet to the worker's ability, run and accept workers, review by risk, deliver, close and hand over.
---

# Lead orchestration

Use this when the task's STATE names you Lead. The assignment comes from STATE, never from a CLI name. It covers the Lead and its workers, and one Lead handing a task to the next. The Lead and the user: skill `strategy-dialogue`. Terms: `/root/.agents/runtime-references/GLOSSARY.md`.

The rules every agent follows for ownership, handoff, receiving work, accepting it and using Herdr are in `/root/.agents/runtime-references/CONCURRENT-WORK.md`. This skill adds only what the Lead decides.

## 0. The whole view

A Lead sees packets; an objective fails in what no packet shows. Answer these at the named moment, in the plan or the acceptance record. A plan lists a work thread's packets with an estimate in changed lines; a slice is what one pull request delivers. Where the task allows no reviewer of another model family, answer yourself and mark it self-checked.

Opening or amending a work thread or its plan, and whenever question 4's limit is passed:
1. Use: what does it first make usable, for whom (a production route, an agent), by which packet? That packet comes first. With no named user, build no further.
2. First choice: which one design choice causes most of the planned size, and which clause of the requirement needs it? Write the smallest design that meets the requirement without it; take it, or say why not.
3. One implementation: when a contract, a repository, the engine or the service already holds the logic, consume or share it. A cited contract is pinned, and its vectors run in the consumer's tests.
4. Growth: record the first estimate. When the current estimate or the delivered size passes one and a half times it, or a review finds a second blocker in the same part, add no further special case: a reviewer of another model family, given the requirement and no earlier review, writes the smallest design that meets it, and that part is redesigned; a check that covers every case proves it, where one is possible.

Accepting a packet:
5. A review covers its packet's range. Name what the packet assumed about the rest and the test that runs that assumption; repair a contradiction of a cited contract before the next packet that builds on it.

Before a slice whose packets section 5 sent to review is made ready:
6. One reviewer of another model family reads the whole branch against the main branch and its contracts, without the packet reviews; one real run shows the use of question 1.

After every ten accepted packets, after the second compaction of your context, and each working day:
7. An independent reader answers from the task files alone (and the store, where STATE names it in use): do the threads add up to the objective, who uses what, what is out of proportion, what is stale? Report it to the user. If it shows your picture of the task wrong in a way that changes the plan, hand over to a fresh Lead.

When you broke a rule that exists, add no sentence: make the step a command or a refusal, or record why it cannot be one.

## 1. What the Lead does itself


- Delegation needs the task's authority. STATE says whether the Lead may start workers and which runtimes it may use; without that line, do the work yourself.
- Within that authority, delegate work that means reading many files, producing a large diff, reading long command output, or waiting.
- Do it yourself when writing the packet costs more than the work, or when the work is the whole-task judgment itself: the interpretation of the user's words, a strategy, a shared rule, an acceptance.
- Start with the report, in a fixed shape; then read the cited source and the changed boundaries the acceptance needs.
- When a check can be a command, write the command instead of asking a model.

## 2. Match the packet to the worker

Which runtime and model serves which tier, the order in which to use them and what is unavailable are written in the task's assignment file, which STATE names. If STATE names none, write the assignment into STATE from what the user said before the first dispatch.

Name the model on every spawn, and the effort where the CLI has a flag for it; then read back what the session reports. Where a CLI has no model flag, record that in STATE and use what the session reports. A subagent of your own CLI is started with an explicit model too: a default that inherits the Lead's model is not used, and the Lead's model is not a worker unless the user says so.

| Tier | Work | Packet shape |
| --- | --- | --- |
| Mechanical | Collect, count, hash, diff, run fixed checks, fill a table | Closed. Every path and command is listed, the output has a fixed schema, no decision is left open |
| Bounded judgment | Implement to a written contract, classify by given criteria, diagnose inside named files | Criteria and options written out, evidence required as path and line, an explicit out-of-scope list |
| Open judgment | Design, diagnosis across repositories, semantic review | Goal, constraints and acceptance; the worker chooses the route and states its assumptions |

Rules that prevent the routing failures of weaker workers:

- The packet routes; the worker does not. List exact absolute paths. Never write "find the relevant file", and never count on a worker opening a reference on its own.
- The packet speaks the glossary's terms; a term outside `/root/.agents/AGENTS.md` section 8 is defined in the packet, or `GLOSSARY.md` is listed under "Read first".
- One phase per run and one output per phase. A later phase starts after you accepted the earlier one.
- Split a mixed task into collect, judge, review, so the stronger worker reads a prepared result instead of raw material.
- A mechanical or bounded worker never chooses on its own: the packet tells it to write a mismatch in the report and stop the steps that depend on it.
- Do not restate a shared rule in other words inside a packet. If the packet must differ from a shared rule, say so in one explicit line.
- Try a new packet shape once on a disposable target before dispatching several.

Template and the three shapes: `references/task-packet.md`.

## 3. Running a worker

Mechanics: `CONCURRENT-WORK.md` section 5 and skill `herdr`; the commands of each CLI: `/root/.agents/runtime-references/CLI-RUNTIMES.md` section 2. The Lead adds:

- At dispatch, record worker, tier, runtime, session, pane, packet, start time and the build-slot owner, from a command's output, never from your hand or an estimate. Before a build, check free disk space against the floor STATE states.
- A fresh session or the same one: `/root/.agents/runtime-references/CONCURRENT-WORK.md` section 2; read the context a session has left before you reuse it.
- When a task runs many packets, script the steps you repeat for each one, so that every record and timestamp comes from the script's output. Never edit a script in place while a command runs it: install the changed file by rename.
- Before each dispatch, count the task's working worker sessions against the limit STATE states; a queued packet counts from the moment its worker starts.
- Wait in the background with a timeout. Completion needs both the DONE line and a nonempty report; a DONE line that carries the report's SHA-256 shows the file is the finished one.
- Liveness: set the packet's quiet limit from the expected length of the run, never below the time a comparable run took. A worker whose session record and pane did not change for that long has stalled. Read the pane; end the worker by closing the tab you created or by process ID (a name pattern can match your own shell); reconcile what it already changed (`CONCURRENT-WORK.md` section 4); record the cause, or "unknown". Then `/root/.agents/AGENTS.md` section 4 "Failure" applies.
- Give a worker the memory conclusions it needs, already verified, with their source.

## 4. Accepting

`CONCURRENT-WORK.md` section 4, and:

- Accept, send concrete repairs, or reject, and record which.
- A condition carried from a review into a later packet is a plan change: check it against the contract and that packet's write set first; if it fails either, make it a decision or a prerequisite packet.
- A dispatched packet is not edited. A changed decision, including a correction from the user, gets a new packet version, and STATE names the current one.

## 5. Review by risk

An independent review costs a second worker. Use it where a wrong change spreads or cannot be undone. When unsure, review.

- Review: shared instructions and skills; a contract, schema or public interface; anything that writes persistent data, host configuration or a deployment; a change across repositories; source the Lead wrote itself.
- No separate review: a read-only inventory; a mechanical change that the required check covers completely; work on a disposable target. The Lead's own readback is the acceptance there.
- Freeze the candidate revision before sending it. The reviewer works in a fresh context, on a different model from the implementer when one is available, with the request, the contracts, the frozen revision and raw evidence. A repaired candidate returns to the same reviewer session; a new candidate gets a new one.

## 6. Delivery

1. Source goes through a pull request, the required checks and, where delivery relies on it, the automatic review on the exact reviewed head, resolved review threads, then a merge guarded by the expected head. Never force-push. A task that permits only a pull request stops before the merge.
2. When changing required checks, inspect both classic branch protection and repository rulesets; save and read back each surface you change.
3. Merged, installed and loaded are three facts (`CLI-RUNTIMES.md` section 7). When the task covers installation: install from the merged revision, verify by checksum, and keep the previous bytes with a rollback command. The same holds for any host configuration change: previous bytes, rollback command, change, readback.

## 7. Closure

For each finished worker: record worker, role, runtime, session, packet, revision and output; reconcile its child processes, builds and uncertain effects; close the tab you created; confirm that no pane, process or waiter of the task remains. Record findings by severity, rework rounds, CI minutes and reported token use.

After the pull request is merged and `git status` in the worktree is clean, remove the worktree you created, without force, then its branch, then your disposable build roots after keeping the logs you need. Never remove a worktree or build root of another task.

Report template and the optional lesson candidate: `/root/.agents/skills/lessons-loop/references/worker-closure.md`.

## 8. Keeping the task readable, and handoff to the next Lead

- A task with several work threads keeps a board; what it holds and what STATE holds, with no value in both: `references/program-board.md`.
- Write the checkpoint before your context is compacted or reset.
- Handoff to the next Lead (start prompt, checkpoint, cold-read test): `references/program-board.md`. The Lead that hands over also lists its decisions the user has not confirmed and the errors it made. The next Lead checks those places and the checkpoint's claims against source, Git and service output before it continues; it may replace a decision of the previous Lead, with a reason; a decision or a hold of the user stays.
