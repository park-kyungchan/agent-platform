---
name: herdr
description: Use for explicitly requested Herdr pane or agent control from a genuine Herdr caller; delegation requires task authority.
---

# Herdr

Before control, verify `HERDR_ENV=1` in the actual caller. Tool-shell context can differ from the user's pane; reuse a trusted explicit pane identity and do not infer it from a process name. Outside a genuine Herdr context, report the missing boundary and continue unrelated work.

Use installed `herdr --help` and the relevant command group. Bare `herdr` opens the TUI; a nested mutating command with omitted arguments may still execute. Use native help for syntax, then read the relevant section of [native-control.md](references/native-control.md): caller/remote IDs, foreground ownership, agent launch, raw commands or terminal recovery.

A pane, a recognized agent and a stored session are different identities. Inspect live native inventory and foreground ownership before sending input. Read access is not permission to prompt another agent. A title, kind label or manual reporting does not prove native recognition. Idle/done does not settle background effects.

Control only task-authorized resources, preserve user focus and active sessions, and use returned opaque IDs after topology changes. Local and remote server IDs can collide; keep the selected machine consistent. Native SSH owns authentication and host keys.

Launching agents or delegating requires the task's actual authorization. Assigned tab, worktree and cwd take precedence over examples. An assigned Lead follows [lead-orchestration](../lead-orchestration/SKILL.md), including one tab per worker; this link does not require delegation. Use [Context Governance](../../runtime-references/CONTEXT-GOVERNANCE.md) for isolation/continuity and [strategy-dialogue](../strategy-dialogue/SKILL.md) for the existing Strategy/KPI contract.

Pass goal, exact targets, allowed effects, expected output and state path, including Strategy link, KPI/guardrail moved, source revision, write set and shared-build owner when applicable. Do not invent role machinery, receipt ledgers or a scheduler. Verify returned artifacts/effects using [worker closure](../lessons-loop/references/worker-closure.md); a DONE marker or idle state alone proves no outcome. After uncertain input, read existing state before retrying.
