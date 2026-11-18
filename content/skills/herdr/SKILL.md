---
name: herdr
description: Use for explicitly requested Herdr pane or agent control from a genuine Herdr caller: the caller check, the safety rules and the five common calls; delegation requires task authority.
---

# Herdr

## Before any call

Verify `HERDR_ENV=1` in the actual caller (`test "${HERDR_ENV:-}" = 1`). A tool shell can differ from the user's pane; reuse a trusted explicit pane identity and never infer it from a process name. Outside a genuine Herdr context, say so, control nothing, and continue unrelated work.

The installed `herdr --help` and the group without a subcommand (`herdr pane`, `herdr agent`, `herdr tab`) are the authority for syntax. Bare `herdr` opens the user interface; a nested mutating command with omitted arguments may still execute. Most calls answer JSON: read IDs from the answer, never from examples or sidebar order, and use the returned IDs after a change of layout.

## Safety and coordination rules

- Close only workspaces, tabs, panes or sessions you created, unless the user explicitly asked.
- Never run `herdr server stop` from an active session unless the user explicitly intends to stop the server and its pane processes, and never kill the main Herdr process. A missing method is not permission to stop or upgrade a server.
- Keep the user's focus: `--no-focus` for background work; target `--current`, an explicit pane ID or a unique agent name, never another client's focused pane.
- Use `--trust-repository` only after the user verified the repository; it is no routine retry.
- Control only what the task authorizes. Launching agents or delegating needs the task's own authorization; the assigned tab, worktree and cwd come before any example. A Lead gives each worker its own tab. Invent no role machinery, receipt ledger or scheduler.
- A pane, a recognized agent and a stored session are different identities. Inspect the live inventory and the foreground before sending input; read access is not permission to prompt another agent, and a title or kind label proves no native recognition. Idle or done settles no background effect, and a timeout does not prove a prompt was not delivered: read the state before sending again.

## The five calls a Lead uses most

| Call | Exact form | When |
| --- | --- | --- |
| List panes and agents | `herdr pane list [--workspace <workspace_id>]`; `herdr agent list` | Before any input or close: which panes exist and which agent is in each |
| Read a pane | `herdr pane read <pane_id> --source recent-unwrapped --lines 120`; for an agent `herdr agent read <target> --source recent-unwrapped --lines 120` | To see a worker's output or state; before answering a `blocked` agent |
| Send input to a pane | `herdr agent prompt <target> <text> --wait --timeout 120000` to an agent; `herdr pane run <pane_id> <command>` to a shell | A bounded handoff to an idle agent; a command in a pane the task assigned |
| Open a tab or pane for a worker | `herdr tab create --cwd <worktree> --label <name> --no-focus`, or `herdr pane split --current --direction right --cwd <path> --no-focus`; then `herdr agent start <name> --kind <kind> --pane <pane_id>` | A worker the user watches, in the task's worktree |
| Close a tab you created | `herdr tab close <tab_id>` | At the worker's end, after its result is read and reconciled |

A handoff states the goal, the exact targets, the allowed effects, the expected output and the STATE path. A DONE marker, a successful submission or an idle state is a signal to read the result, not the result: verify the output and the effects ([worker closure](../lessons-loop/references/worker-closure.md)).

Everything else (caller and remote IDs, lifecycle states, waits, keys, layout, machines, ordinary commands, recovery): [native-control.md](references/native-control.md), read only for it.
