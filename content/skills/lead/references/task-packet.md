# Task packet

One page per unit of work, with exact values instead of references to a conversation, written in the terms of `/root/.agents/runtime-references/GLOSSARY.md` and never in the user's wording. What a handoff must contain and how a worker treats it: `/root/.agents/runtime-references/CONCURRENT-WORK.md` sections 2 and 3. The worker run, its model and the build-slot owner are recorded at dispatch (the store's worker run, or STATE), not in the packet.

The Lead writes on this page nothing about types, functions or line numbers. A fact about the code that the worker needs is given as a command to run. What must stay true is given as an executable.

## A unit that changes product code

One pull request that can merge alone. The line `Write set:` starts at the first column, once: `ap unit` reads it (comma-separated paths, a directory as `dir/**`).

```markdown
# TASK: <the unit in one line>
Owner: <Lead>; State: <absolute STATE.md>
Tier: <mechanical | bounded judgment | open judgment>
Read first: <this file>, <absolute STATE.md>, <absolute path of each needed SKILL.md, reference or contract>
First output after reading: one line listing the exact files read
Objective: <the state to reach, as a sentence that is true or false; who uses it first and the real run that shows it>
Invariants: <what must hold before and after, each a sentence a test can break>
Acceptance checks: <executables only: the test that fails on the base and passes on the head, a differential script, a measurement with its bound; then the repository's check command>
Not built: <neighbouring things left out on purpose>
Exact targets: <repository, worktree, branch, base revision>
Write set: <the area: crates and directories>
Allowed effects: <reads, writes, local commits, push, pull request, build slot>
Forbidden effects: <holds, other paths, host, domain and remote boundaries>
Plan: before the first edit, write into the pull request description the files you change, the order, the risks and how each acceptance check is met.
Report: <your final message; at most 40 lines: the head, the check command's output, each acceptance check with its result, where you left this page and why, what is not verified>
```

The worker decides what this page does not fix. It leaves the write set only with the reason in its plan. It stops and reports when an invariant or an acceptance check cannot hold; it never changes one.

The shared instructions load through each CLI's own pointer, so a packet does not list them. If a session record shows they were not loaded, repair the pointer (`/root/.agents/runtime-references/CLI-RUNTIMES.md` section 3); do not paste the rules into packets.

## A read-only unit (inventory, measurement, observation of a service)

The common lines above without `Invariants`, `Write set` and `Plan`, plus:

```markdown
Inputs: <every absolute path or the exact command that lists them>
Questions: <numbered; each answerable by a path and line, or a command and its output>
Output schema: <file name; for a table the exact column names and separator; one row per <unit>>
Evidence rule: every answer cites path and line, or command and output; an unanswered question is reported as open with what was tried
Do not: recommend a structure, repair, or read files that are not listed.
```

A mechanical unit is ready when a script could follow it. If a script can do all of it, write the script.

## The review of a pull request (the verifier)

One reviewer session in a fresh context, on the head whose required check passed.

```markdown
Request: <absolute path of the packet>
Candidate: <repository, pull request, head revision, the diff command>
Policy: <the repository's REVIEW.md; where there is none, the two lines below are the policy>
Raw evidence: <the check output on that head>
Questions of the Lead: <what the Lead found in its own reading of the diff, each as a sequence to execute, never as a verdict>
Blocking, each with the command or test that shows it: wrong behaviour; loss or exposure of data; a broken invariant of the packet or a broken contract; an unguarded irreversible effect on a real target.
Not blocking: at most five notes (a test gap, a sentence, a name). Write no mutants by hand: the check command runs mutation testing where the repository has it.
Output: a report whose first line is exactly `Verdict: accept` or `Verdict: repair` (the form `ap unit` reads: first line the verdict, second line the head) and whose second line is the head revision; then the blocking findings, the notes, and one line per question. The findings also go onto the pull request as comments, by the reviewer or by the Lead.
Not given on purpose: the implementer's reasoning and any suggested verdict.
```

## An attack on a design note

Only for a store format, the schema of an authoritative store, or the order of record and effect on a real target. The reviewer gets the design note and the code it rests on, and is asked for a sequence of real commands that breaks an invariant, executed where it can be; opinions do not count.
