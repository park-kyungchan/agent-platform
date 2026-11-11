# Board and handoff to the next Lead

## Board file

One file in the task directory, kept to one screen where possible. It is the one home of the values below; STATE holds the owner, authority, holds, source baseline, effects with their rollback, one next action and the path of this file, and repeats none of the board's values. Values are replaced, not appended. Every measured value carries its source and the time read from the clock.

```markdown
# <task> — BOARD
Last update: <time> (<who>; <one line on what changed>)

## KPI
| KPI (what it counts) | current | change since last message | target |
| --- | --- | --- | --- |

## Work threads (limit <n> in progress; in progress: <n>)
| Thread | Priority | Owner | KPI or guardrail | Status | Next action |
| --- | --- | --- | --- | --- | --- |

## Live workers
| Worker | Tier | Runtime | Session | Pane | Packet | Started | Quiet limit |
| --- | --- | --- | --- | --- | --- | --- | --- |

## Decisions (the latest per topic)
## Kept for the user (irreversible actions taken; reminders owed)
## Counts
Corrections of the Lead's interpretation: <n> · Questions the Lead asked: <n> · Requests to explain again: <n>
```

The line formats of "Decisions" and "Kept for the user" are in `/root/.agents/skills/strategy-dialogue/references/strategy-brief.md`. A superseded decision is replaced by the one that supersedes it; the task's Git history or the kept previous bytes hold the older line. The assignment file is named in STATE; the board does not repeat it.

## Handoff to the next Lead

1. Start prompt: the role, the task directory, the ordered list of files to read, and the first output expected. No rule, value or history is written into it.
2. STATE checkpoint: the objective as the Lead wrote it and the user decisions it rests on; the source baseline with exact revisions; every effect made, with its rollback route; what is unverified; holds; one next action.
3. Nothing of the task is still running: no worker, waiter, build or pane. If something must keep running, STATE names it with its owner and how to stop it.
4. Cold-read test. Give a reader only the start prompt and ask:
   - What is the next action and how is it verified?
   - What may you not do?
   - Which statements in these files are unverified?
   - Which file would you open to choose a worker?
   When the task allows a worker, the reader is a fresh low-cost worker that writes nothing, started in a directory covered by the memory path override of `/root/.agents/runtime-references/concurrent-work.md` section 2, so that it answers from the files and not from memory; read its session record to confirm that no memory text was injected. Otherwise read the files yourself in that order and mark the test as self-checked. Each wrong or missing answer is a defect in the documents: repair the document, not the answer, and rerun after a large repair.
5. Record the test and its result in STATE, then end the session or keep it only as the named owner of something still open.
