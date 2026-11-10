---
name: lead-orchestration
description: When task STATE assigns Lead, coordinate authorized workers, isolated review, PR delivery and verified closure across CLIs.
---

# Lead orchestration

Use this when the task's STATE names you Lead. The assignment comes from STATE, never from a CLI name. The Lead owns sequencing, dispatch, acceptance and delivery; the CLIs own sessions and tools, Herdr owns terminals, Hindsight owns memory. This is a playbook, not a loader, scheduler or permission system.

Strategy brief, user decisions, KPI-first reports and the work-thread list: skill `strategy-dialogue`. Wording toward the user: skill `explain-precisely`. Ownership, worktrees, the build slot and the handoff rules that every agent follows: `/root/.agents/runtime-references/concurrent-work.md`.

## 1. Choosing a worker

- The runtimes and models that are available now are written in the task's STATE (or in the one file STATE names for assignment). Never write a model name, an effort or a token window into a skill or into `/root/.agents/AGENTS.md`.
- Match the worker to the reasoning the task needs. Collection, fixed checks, hashing, counting, diffing and projection into a table need no judgment: write a script when a script can do it, otherwise use the cheapest available model. Design, diagnosis and semantic review need a strong model.
- Split a mixed task into collect, judge, review, so the expensive model reads a prepared result instead of raw material.
- The reviewer runs in a fresh context that never saw the implementer's reasoning.
- Keep your own context for orchestration: read reports, not raw worker output or whole diffs.

## 2. The TASK packet

Write one file per bounded piece of work, with exact values instead of references to a conversation:

```markdown
# TASK: <bounded goal>
Owner: <Lead>; State: <absolute STATE.md>
Read first: <absolute TASK.md>, <absolute STATE.md>, <absolute path of each needed SKILL.md or reference>
First output after reading: one line listing the exact files read
Authority: <user decision and task scope>
Exact targets: <repo, worktree, branch, base revision, files that may be written>
Strategy link: <STATE or STRATEGY.md line>; KPI this task moves: <KPI or guardrail, expected effect, how it is measured>
Allowed effects: <reads, writes, local commits, push, PR, build slot>
Forbidden effects: <holds, other paths, host, domain and remote boundaries>
Expected output: <absolute report path, source revision, evidence, limitations>
Verification: <commands and the output or effect to read back>
DONE marker: <unique literal line the worker prints last>
```

- The shared instructions load through each CLI's own pointer; a packet does not list them. If a session record shows a CLI did not load them, repair the pointer (`/root/.agents/runtime-references/CLI-RUNTIMES.md`).
- The worker, its model and the build-slot owner are recorded in STATE at dispatch, not in the packet. A dispatched packet is not edited: a changed decision gets a new packet version, and STATE names the current one.
- One packet phase is one worker run. A later phase starts only after you accepted the earlier one.
- Update STATE at each milestone by replacing values: owner, authority and holds, exact revisions, evidence, one next action.

## 3. Running a worker

Use Herdr only when pane control is authorized and `HERDR_ENV=1` holds in your pane; syntax is in skill `herdr`. One tab per worker, created without taking focus; use the IDs the commands return.

Headless worker (one prompt, then exit), the default for bounded packets:

```bash
herdr tab create --workspace "$HERDR_WORKSPACE_ID" --cwd <launch cwd> --label <label> --no-focus
herdr pane run <pane-id> "<headless command of that CLI with the TASK file as the prompt>"
herdr pane wait-output --regex "<DONE marker>" --source recent-unwrapped --timeout <ms> <pane-id>
herdr tab close <tab-id>    # after acceptance
```

Headless and interactive commands per CLI are in `CLI-RUNTIMES.md`. An interactive worker uses `herdr agent start` and `herdr agent prompt`; a subagent of your own CLI needs no pane.

- Always give a waiter a timeout, and run it in the background so it does not block your turn. Do not build a resident watcher.
- Completion needs both the DONE line in the pane and a nonempty report file; having the worker print the report's SHA-256 in the DONE line lets you check that the file is the one it finished. A TASK file that contains the marker text is not completion.
- A send acknowledgement, `idle`, `done` or exit code 0 is not completion. After an uncertain submission, read the pane before sending again.
- A trust or approval dialog in a worker pane belongs to the user: report it, do not answer it.
- Workers keep native Hindsight. Memory isolation for an evaluation or a review uses the path override described in `concurrent-work.md`.

## 4. Acceptance, review and delivery

1. Read the report, then read back the actual source, output or effect yourself. Check each finding against source.
2. Freeze the exact candidate revision. Give the independent reviewer the request, the contracts, the raw artifacts and that revision; not the implementer's reasoning or a suggested verdict.
3. Send concrete repairs. A changed candidate is reviewed again.
4. Deliver source through pull request, CI on the exact reviewed head, resolved review threads, then a merge guarded by the expected head. Never force-push. A task that permits only a pull request stops before the merge.
5. When changing required checks, inspect both classic branch protection and repository rulesets; save and read back each surface you change.
6. Report the observed KPI or guardrail effect with its evidence; label an effect that is expected but not measured.

## 5. Closure

For each finished worker: record in STATE the worker, role, runtime, session ID, packet, revision and output; reconcile its child processes, builds and uncertain effects; close the tab you created for it; confirm that no pane, process or waiter owned by the task remains. Leave every other session untouched. Record the review findings by severity, rework rounds and CI minutes; an unknown value stays unknown.

Details and the optional lesson candidate: `/root/.agents/skills/lessons-loop/references/worker-closure.md`. Successful routine work needs no lesson.

## 6. Safety

- Before changing host configuration: save the exact previous bytes, write a rollback command, change, read back.
- Before deleting: create and verify the recovery copy, then check every live consumer of the path. A recovery copy alone does not authorize deletion.
- Builds use a Cargo target directory and fixture roots owned by the task; check free disk space before a build and delete the disposable roots after keeping the logs you need.
- Which information goes to memory and which stays in task files: `/root/.agents/runtime-references/CONTEXT-GOVERNANCE.md`.
- Ask the user only for decisions that belong to the user, with a recommendation and its consequences. End a turn with one status line: what is complete, or what is held and the next action.
