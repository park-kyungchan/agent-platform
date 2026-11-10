# Working alongside other agents: ownership, handoff, return

Read this when you hand work to another agent, receive work, or share a repository, a build or a service with one. It uses the task's STATE and Git. It adds no registry, lease database, watcher or scheduler.

## 1. Ownership before anyone writes

- The task's STATE names, for each writer: the checkout, branch and base revision, the files it may write, and the shared targets it must not touch.
- Two agents may read the same checkout. Two agents that write need separate Git worktrees or file sets that do not overlap.
- A worktree separates files only. It does not separate global settings, databases, ports, service accounts or a shared build directory.
- Shared files have one integrator, the Lead unless STATE names another: `Cargo.lock` and manifests, public schemas, generated outputs, and global or per-CLI configuration. Everyone else proposes a change to the integrator.
- One expensive build runs at a time, at `-j1`, with its owner named in STATE. Each checkout uses its own `CARGO_TARGET_DIR`. Never delete a cache or a lock file to get past a running build.

## 2. Handing work over

Give a new worker a small packet, never a transcript:

1. Goal.
2. Exact targets: checkout, branch, base revision, files it may write.
3. Allowed and forbidden effects.
4. Expected output: the absolute path of the report, the checks to run, the literal completion line.
5. The absolute paths of the TASK file and the STATE file.

Also name the files to read first, and require the worker's first output to list the files it read. Name the Strategy line and the KPI or guardrail the task moves when the task has one. Start source work in its assigned worktree.

Fresh context or the same session:
- Default to a fresh worker. Continue an existing session only when the follow-up stays in the same scope and files, needs the context already loaded and has room left; record that reason in STATE.
- A review always runs in a fresh context that never saw the implementer's reasoning. Give the reviewer the request, the contracts, the frozen candidate revision and raw evidence, not a suggested verdict.
- Separate context is not separate files, builds, services or authority. Check each of those separately.

Memory for workers:
- Workers keep their native Hindsight hooks and MCP server. When a task needs memory isolation (an evaluation, an independent review), run it in a directory covered by a `paths.<prefix>` override in the Hindsight integration settings, for example `/home/palantirkc/work/eval-isolated` with `{"disabled": true}`, and verify the override took effect. Adding an override changes host configuration and needs task authority.
- If memory injected into a worker was irrelevant or misleading, record that in STATE and correct the page or the path override.

## 3. When you receive work

1. Read the files the packet names, then state them in your first output.
2. Before writing, check the actual cwd, `git rev-parse --git-common-dir`, branch, HEAD and `git status`. Edits you did not make belong to someone else: keep them and report them.
3. Stay inside the write set. If the task needs a shared file, stop at that file and report the proposed change.
4. If a dependency fails, stop the steps that depend on it. Work that does not depend on it may continue.

## 4. Returning and accepting work

The worker reports: the exact revision or diff, each check it ran with the observed result, findings it did not resolve, and any child process or build still running.

The accepting agent:
- Reads the report and the actual output or effect. A completion line, an exit code of 0, an `idle` state or a successful prompt submission shows none of these.
- When a result is in doubt, reads the worker's session record (locations in `CLI-RUNTIMES.md`). A session record shows recorded tool calls; it cannot show that an unrecorded effect did not happen.
- Integrates only the returned revision. Never fetch, reset or cherry-pick another owner's tree beyond it.
- Treats commit, push, merge and deploy as separate effects, each needing task authority.

Closure details (roster, owned panes and processes, optional lesson candidate): `/root/.agents/skills/lessons-loop/references/worker-closure.md`.

## 5. Herdr

Use Herdr only from a pane where `HERDR_ENV=1`, and only when the task authorizes pane control. Command syntax and safety rules: skill `herdr` and its `references/native-control.md`.

- IDs: workspace `wN`, tab `wN:tN`, pane `wN:pN`. A pane is a terminal; an agent is the program Herdr recognizes in it. Address a pane by the ID a command returned, never by a title, a process name or a position.
- One tab per worker, created with `--no-focus` in the worker's assigned cwd.
- A headless run (one prompt, then exit) uses `herdr pane run <pane-id> "<command>"`. An interactive agent uses `herdr agent start <name> --kind <kind> --pane <pane-id> -- <native args>` and then `herdr agent prompt <name> "<text>" --wait`.
- `idle` or `done` means ready for input. `blocked` means an approval or question is on screen: inspect it, and leave trust and approval dialogs to the user. `unknown` means not classified. None of them shows that the work finished.
- After a timeout or an uncertain submission, read the pane before sending anything again.
- Close only the tabs and panes your task created. Never run `herdr server stop` from an active session.
- IDs belong to one machine. For a remote machine pass the same `--machine` value on every command; a connection failure does not show that a change was not applied.
