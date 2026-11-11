# Task packet

One file per bounded piece of work, with exact values instead of references to a conversation, written in the terms of `/root/.agents/runtime-references/GLOSSARY.md` and never in the user's wording. What a packet must contain and how a worker treats it: `/root/.agents/runtime-references/concurrent-work.md` sections 2 and 3. The worker, its model and the build-slot owner are recorded in STATE at dispatch, not in the packet.

## Common part

```markdown
# TASK: <bounded goal in one line>
Owner: <Lead>; State: <absolute STATE.md>
Tier: <mechanical | bounded judgment | open judgment>
Read first: <absolute TASK.md>, <absolute STATE.md>, <absolute path of each needed SKILL.md or reference>
First output after reading: one line listing the exact files read
Authority: <the objective the Lead wrote for this task, and the user's words it rests on; the task scope>
Exact targets: <repo, worktree, branch, base revision, files that may be written>
Strategy link: <STATE or STRATEGY.md line>; KPI this task moves: <KPI or guardrail, expected effect, how it is measured>
Allowed effects: <reads, writes, local commits, push, pull request, build slot>
Forbidden effects: <holds, other paths, host, domain and remote boundaries>
Expected output: <absolute report path and its fixed shape; source revision; evidence; limitations>
Verification: <commands and the output or effect to read back>
Quiet limit: <minutes without output after which the Lead treats the run as stalled; from the expected length of the run>
DONE marker: <unique literal line printed last, ending with the SHA-256 of the report>
```

The shared instructions load through each CLI's own pointer, so a packet does not list them. If a session record shows they were not loaded, repair the pointer (`/root/.agents/runtime-references/CLI-RUNTIMES.md` section 3); do not paste the rules into packets.

## Mechanical tier: add

```markdown
Inputs: <every absolute path or the exact command that lists them>
Steps: <numbered commands or operations, each with its expected kind of output>
Output schema: <file name; for a table the exact column names and separator; one row per <unit>>
Do not: interpret, rank, recommend, repair, or read files that are not listed.
If an instruction does not fit what you find: write the mismatch in the report and stop the steps that depend on it.
```

A mechanical packet is ready when a script could follow it. If a script can do all of it, write the script.

## Bounded judgment tier: add

```markdown
Contract or criteria: <absolute path and section, or the criteria written out>
Options: <the allowed outcomes per item>
Evidence rule: every judgment cites path and line, or command and output
Out of scope: <neighbouring questions the worker must not settle>
When the criteria do not decide: report the item as undecided with the evidence for each option.
If an instruction does not fit what you find: write the mismatch in the report and stop the steps that depend on it.
```

## Open judgment tier: add

```markdown
Goal and acceptance: <what a good result must make true>
Constraints: <interfaces, budgets and holds that bound the route>
Assumptions: the report lists every assumption the worker made and what would change the result.
```

## Review packet: replace the tier part with

```markdown
Request: <what the change was asked to do>
Contracts: <absolute paths>
Candidate: <repository, frozen revision, diff command>
Raw evidence: <check output paths>
Report: findings by severity (blocker, major, minor), each with path and line and the smallest repair; then a verdict.
Not given on purpose: the implementer's reasoning and any suggested verdict.
```
