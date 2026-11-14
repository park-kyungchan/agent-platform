# Handoff to the next Lead (this page holds only the handoff)

A Lead is not required to keep a board file. Where the store is in use (`/root/.agents/runtime-references/ONTOLOGY-USE.md`), decisions, holds and runs live there; where it is not, they live in STATE's pointers and in Git. A value a command can produce is printed by the task's status command when it is needed and is not committed (`/root/.agents/skills/lead/SKILL.md` section 7).

The handoff file is at most two pages. It CARRIES: the mandate and the user's latest words verbatim with their decision keys; holds by pointer; the baseline as the output of the task's status command plus one command per row it cannot produce; every deadline with what is owed to the user before it; each live session with its pane, session id and kept start answer; the waits to START AGAIN with their commands (a background command dies with the old session); owed reviews; open questions to the user; capacity with the time it was read; your errors in five lines, each with the mechanism that now stands against it or the word "none"; pointers to the lens rows and reviews that hold the reasons. It does NOT carry: superseded designs, a KPI value as a target, the list of what you delivered (Git has it), estimates, decisions of an earlier program. Choose the moment: never between the creation of a credential and the readback of the effect it serves, never with the verdict of a real-effect unit half read. After the start prompt is sent you make no further effect.

1. Start prompt: the role, the task directory, the ordered list of files to read (STATE, then the lens file, then the files STATE points to), and the first output expected. No rule, value or history is written into it.
2. STATE checkpoint: the objective as the Lead wrote it and the user decisions it rests on; the source baseline with exact revisions; every effect made, with its rollback route; what is unverified; holds; one next action.
3. Nothing of the task is still running: no worker, waiter, build or pane. If something must keep running, STATE names it with its owner and how to stop it.
4. Cold-read test. Give a reader only the start prompt and ask:
   - What is the next action and how is it verified?
   - What may you not do?
   - Which statements in these files are unverified?
   - Which file would you open to choose a worker?
   When the task allows a worker, the reader is a fresh low-cost worker that writes nothing, started in a directory covered by the memory path override of `/root/.agents/runtime-references/CONCURRENT-WORK.md` section 2, so that it answers from the files and not from memory; read its session record to confirm that no memory text was injected. Otherwise read the files yourself in that order and mark the test as self-checked. Each wrong or missing answer is a defect in the documents: repair the document, not the answer, and rerun after a large repair.
5. Record the test and its result in STATE, then end the session or keep it only as the named owner of something still open.
