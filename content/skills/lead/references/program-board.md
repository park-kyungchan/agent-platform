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

The line formats of "Decisions" and "Kept for the user" are in `/root/.agents/skills/lead/references/strategy-brief.md`. A superseded decision is replaced by the one that supersedes it; the task's Git history or the kept previous bytes hold the older line. The assignment file is named in STATE; the board does not repeat it.

## Measured lines and the commit hook (a task whose STATE names a Lead)

A KPI, guardrail or baseline value that a command can produce is never typed. The task directory has one executable `measure` (no network, no write, under 10 seconds, coarse values so that a line changes only when a fact changes; `./measure --write` replaces the block). STATE holds its output between the lines `<!-- measure -->` and `<!-- /measure -->`. The hook `/root/.agents/skills/lead/hooks/pre-commit` runs it at every commit and refuses a commit whose block differs from the output of now, a `measure` that fails, and a STATE over 12,000 bytes; it prints the size of a staged page over 6,000 bytes. Install it when the task directory becomes a repository:

```bash
printf '#!/bin/sh\nexec bash /root/.agents/skills/lead/hooks/pre-commit\n' > .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
```

A value no command can produce (capacity read from a pane, a cause not established) stays prose beside the block, with the time it was read.

## Handoff to the next Lead

The handoff file is at most two pages. It CARRIES: the mandate and the user's latest words verbatim with their decision keys; holds by pointer; the baseline as the output of `./measure` plus one command per row it cannot produce; every deadline with what is owed to the user before it; each live session with its pane, session id and kept start answer; the waits to START AGAIN with their commands (a background command dies with the old session); owed reviews; open questions to the user; capacity with the time it was read; your errors in five lines, each with the mechanism that now stands against it or the word "none"; pointers to the lens rows and reviews that hold the reasons. It does NOT carry: superseded designs, a KPI value as a target, the list of what you delivered (Git has it), estimates, decisions of an earlier program. Choose the moment: never between the creation of a credential and the readback of the effect it serves, never with the verdict of a real-effect unit half read. After the start prompt is sent you make no further effect.

1. Start prompt: the role, the task directory, the ordered list of files to read (STATE, then the lens file, then the board), and the first output expected. No rule, value or history is written into it.
2. STATE checkpoint: the objective as the Lead wrote it and the user decisions it rests on; the source baseline with exact revisions; every effect made, with its rollback route; what is unverified; holds; one next action.
3. Nothing of the task is still running: no worker, waiter, build or pane. If something must keep running, STATE names it with its owner and how to stop it.
4. Cold-read test. Give a reader only the start prompt and ask:
   - What is the next action and how is it verified?
   - What may you not do?
   - Which statements in these files are unverified?
   - Which file would you open to choose a worker?
   When the task allows a worker, the reader is a fresh low-cost worker that writes nothing, started in a directory covered by the memory path override of `/root/.agents/runtime-references/CONCURRENT-WORK.md` section 2, so that it answers from the files and not from memory; read its session record to confirm that no memory text was injected. Otherwise read the files yourself in that order and mark the test as self-checked. Each wrong or missing answer is a defect in the documents: repair the document, not the answer, and rerun after a large repair.
5. Record the test and its result in STATE, then end the session or keep it only as the named owner of something still open.
