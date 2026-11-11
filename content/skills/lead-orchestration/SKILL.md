---
name: lead-orchestration
description: When STATE names you Lead, decide what to delegate, shape each task packet to the worker's ability, run and accept workers, review by risk, deliver, close and hand over.
---

# Lead orchestration

Use this when the task's STATE names you Lead. The assignment comes from STATE, never from a CLI name. It covers the Lead and its workers, and one Lead handing a task to the next. The Lead and the user: skill `strategy-dialogue`. Terms: `/root/.agents/runtime-references/GLOSSARY.md`.

The rules every agent follows for ownership, handoff, receiving work, accepting it and using Herdr are in `/root/.agents/runtime-references/CONCURRENT-WORK.md`. This skill adds only what the Lead decides. It is a playbook, not a loader, a scheduler or a permission system.

## 1. What the Lead does itself

Keep your context for three things: orchestration, acceptance, and the conversation with the user.

- Delegation needs the task's authority. STATE says whether the Lead may start workers and which runtimes it may use; without that line, do the work yourself.
- Within that authority, delegate work that means reading many files, producing a large diff, reading long command output, or waiting.
- Do it yourself when writing the packet costs more than the work, or when the work is the whole-task judgment itself: the interpretation of the user's words, a strategy, a shared rule, an acceptance.
- Read reports, not raw worker output or whole diffs. Ask for the report in a fixed shape.
- When a check can be a command, write the command instead of asking a model.

## 2. Match the packet to the worker

Which runtime and model serves which tier, the order in which to use them and what is unavailable are written in the task's assignment file, which STATE names. If STATE names none, write the assignment into STATE from what the user said before the first dispatch. Never write it into a skill or into `/root/.agents/AGENTS.md`.

Name the model on every spawn, and the effort where the CLI has a flag for it; then read back what the session reports. Where a CLI has no model flag, record that in STATE and use what the session reports. A subagent of your own CLI is started with an explicit model too: a default that inherits the Lead's model is not used, and the Lead's model is not a worker unless the user says so.

| Tier | Work | Packet shape |
| --- | --- | --- |
| Mechanical | Collect, count, hash, diff, run fixed checks, fill a table | Closed. Every path and command is listed, the output has a fixed schema, no decision is left open |
| Bounded judgment | Implement to a written contract, classify by given criteria, diagnose inside named files | Criteria and options written out, evidence required as path and line, an explicit out-of-scope list |
| Open judgment | Design, diagnosis across repositories, semantic review | Goal, constraints and acceptance; the worker chooses the route and states its assumptions |

Rules that prevent the routing failures of weaker workers:

- The packet routes; the worker does not. List exact absolute paths. Never write "find the relevant file", and never count on a worker opening a reference on its own.
- The packet speaks the glossary's terms. The user's wording never enters a packet; a term outside `/root/.agents/AGENTS.md` section 8 is defined in the packet, or `GLOSSARY.md` is listed under "Read first".
- One phase per run and one output per phase. A later phase starts after you accepted the earlier one.
- Split a mixed task into collect, judge, review, so the stronger worker reads a prepared result instead of raw material.
- A mechanical or bounded worker never chooses on its own: the packet tells it to write a mismatch in the report and stop the steps that depend on it.
- Do not restate a shared rule in other words inside a packet. If the packet must differ from a shared rule, say so in one explicit line.
- Try a new packet shape once on a disposable target before dispatching several.

Template and the three shapes: `references/task-packet.md`.

## 3. Running a worker

Mechanics: `CONCURRENT-WORK.md` section 5 and skill `herdr`; the commands of each CLI: `/root/.agents/runtime-references/CLI-RUNTIMES.md` section 2. The Lead adds:

- Default to an interactive session (commands: `CONCURRENT-WORK.md` section 5): open the CLI in a new tab with as few flags as it needs (the model), then type the prompt, which names the packet file. The user can watch it and the session can take a follow-up. Use a headless run only where an interactive session cannot work.
- A workspace trust dialog in a pane you started for a worker is answered by the Lead: trust and continue. A dialog that asks to approve one effect is answered by the packet: approve an effect the packet allows, decline any other. A login or a payment stays with the user.
- At dispatch, record in the board or STATE: worker, tier, runtime, session, pane, packet, start time read from the clock, and the build-slot owner. Before a build, check free disk space against the floor STATE states.
- Wait in the background with a timeout. Completion needs both the DONE line and a nonempty report; a DONE line that carries the report's SHA-256 shows the file is the finished one. Do not build a resident watcher.
- Liveness: set the packet's quiet limit from the expected length of the run, never below the time a comparable run took. A worker whose session record and pane did not change for that long has stalled. Read the pane; end the worker by closing the tab you created or by process ID (a name pattern can match your own shell); reconcile what it already changed (`CONCURRENT-WORK.md` section 4); record the cause, or "unknown". Do not rerun it unchanged: change the packet, split the step, or change the way it is run. Moving to another model needs a reason other than getting past the failure.
- Give a worker the memory conclusions it needs, already verified, with their source.

## 4. Accepting

`CONCURRENT-WORK.md` section 4, and:

- A worker's finding is a pointer. Check it against source before acting on it.
- Accept, send concrete repairs, or reject, and record which. Repairs in the same scope may go to the same worker session.
- A dispatched packet is not edited. A changed decision, including a correction from the user, gets a new packet version, and STATE names the current one.

## 5. Review by risk

An independent review costs a second worker. Use it where a wrong change spreads or cannot be undone. When unsure, review.

- Review: shared instructions and skills; a contract, schema or public interface; anything that writes persistent data, host configuration or a deployment; a change across repositories; source the Lead wrote itself.
- No separate review: a read-only inventory; a mechanical change that the required check covers completely; work on a disposable target. The Lead's own readback is the acceptance there.
- Freeze the candidate revision before sending it. The reviewer works in a fresh context, on a different model from the implementer when one is available, with the request, the contracts, the frozen revision and raw evidence. A repaired candidate returns to the same reviewer session; a new candidate gets a new one.

## 6. Delivery

1. Source goes through a pull request, the required checks on the exact reviewed head, resolved review threads, then a merge guarded by the expected head. Never force-push. A task that permits only a pull request stops before the merge.
2. When changing required checks, inspect both classic branch protection and repository rulesets; save and read back each surface you change.
3. Merged, installed and loaded are three facts (`CLI-RUNTIMES.md` section 7). When the task covers installation: install from the merged revision, verify by checksum, and keep the previous bytes with a rollback command. The same holds for any host configuration change: previous bytes, rollback command, change, readback.
4. Judge the result by the KPI: report the observed effect with its evidence, and label an effect that is expected but not measured.

## 7. Closure

For each finished worker: record worker, role, runtime, session, packet, revision and output; reconcile its child processes, builds and uncertain effects; close the tab you created; confirm that no pane, process or waiter of the task remains. Leave every other session untouched. Record findings by severity, rework rounds, CI minutes and reported token use; an unknown value stays unknown.

After the pull request is merged and `git status` in the worktree is clean, remove the worktree you created, without force, then its branch, then your disposable build roots after keeping the logs you need. Never remove a worktree or build root of another task.

Report template and the optional lesson candidate: `/root/.agents/skills/lessons-loop/references/worker-closure.md`. Successful routine work needs no lesson.

## 8. Keeping the task readable, and handoff to the next Lead

- A task with several work threads keeps a board. The board is then the one home of KPI values, work threads, live workers, decisions and open questions; STATE holds the owner, authority, holds, source baseline, effects with their rollback, one next action and the path of the board. No value is written in both. Template: `references/program-board.md`.
- Write the checkpoint before your context is compacted or reset, and read the task's files again afterwards before any effect.
- Handoff to the next Lead: a start prompt that only points to files; a STATE that lists every effect with its rollback, what is unverified and one next action; and a cold-read test of those files. Steps and the test: `references/program-board.md`.
