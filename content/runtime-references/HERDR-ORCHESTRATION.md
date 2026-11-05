# Herdr orchestration — one page for a Lead running several CLIs

Purpose: an Orchestrator Lead coordinating multiple CLI agents through Herdr
should spend tokens on work, not on re-discovering topology, identities, or
lifecycle meanings. This page is the shared summary; the installed `herdr`
skill and `herdr --help` remain the operating authority, and the official
agent guide is in OFFICIAL-DOCS.md.

## Topology and identity

- Three levels only: workspace `wN` → tab `wN:tN` → pane `wN:pN`. A pane is a
  terminal; an agent is the recognized program currently occupying a pane.
  They are different identities: a pane exists without an agent, and an agent
  name follows the current occupant of its pane.
- Agent names are unique among live agents (`[a-z][a-z0-9_-]{0,31}`) and are
  cleared when the agent exits or is replaced. Target agents by unique name
  or by the pane ID hosting them — never by terminal ID, CLI kind label, or
  sidebar order.
- Caller context is injected per pane (`HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`,
  `HERDR_PANE_ID`, `HERDR_ENV=1`). Verify `HERDR_ENV=1` before any control
  command. Prefer `--current` or an explicit pane ID; never rely on another
  client's focused pane. IDs are scoped to one server/machine; local and
  remote `w1:p1` can collide, so keep one `--machine` selector consistent.

## Starting and steering (the efficient path)

1. Use the TASK/STATE-assigned tab, worktree and cwd before layout examples.
   An assigned Lead follows [lead-orchestration](../skills/lead-orchestration/SKILL.md):
   one tab per worker, with its assigned checkout. Only when topology/location
   are unspecified may ordinary authorized pane work use a same-tab/cwd sibling.
   Keep `--no-focus`; split right on wide panes, down on narrow/tall panes.
2. Start with `herdr agent start <name> --kind <kind> --pane <new-pane-id> -- <native-args...>`.
   It returns only when Herdr detects the expected agent ready for input.
   Kinds are listed by `herdr agent` (claude, codex, gemini, agy, muse, and
   others); pass only native args after `--`.
3. Hand work over as a handoff, not a transcript: goal, exact targets, allowed
   effects, expected output, and exact `TASK.md`, `STATE.md` and canonical
   `/root/.agents/AGENTS.md` paths. Launch source work at its assigned Git
   worktree, and name the needed skill bodies by absolute path too. Require the
   receiver to read these exact files first with native tools, before effects,
   and list the files read in its first output after reading. Verify native read
   calls and readback: a canonical path, catalogue entry or literal `@import`
   alone proves no read. Muse personal/project `@/root/.agents/AGENTS.md` stays
   literal; Codex outside Git does not discover ancestor AGENTS.md. Use existing
   handoff verification, without an instruction copy, loader or new gate.
   Include Strategy link, KPI/guardrail moved, source revision, write
   set and shared-build owner where applicable; [strategy-dialogue](../skills/strategy-dialogue/SKILL.md)
   owns their reporting conventions. Without a Git root, Codex project discovery checks cwd only;
   ancestor task guidance is not guaranteed. Do not create fake repositories
   or copy guidance to make discovery work; nobody reconstructs a conversation.
4. Submit with `herdr agent prompt <name> "..." --wait`. Submission alone does
   not prove a turn started; `--wait` settles on the first `idle`, `done`, or
   `blocked`. A timeout or stalled result does not prove the prompt was never
   delivered — inspect before resending anything.
5. Read results through `herdr agent get` / `herdr agent read` (prefer
   `recent-unwrapped` for logs). If a long result cannot be read from the
   pane, ask the agent to write Markdown to a temp file and reply with only
   the path — as a fallback, not the default.

## Lifecycle meanings (do not over-read them)

- `idle` / `done`: ready for input. Neither proves an approval dialog was
  understood, that background subagents finished, or that the retained agent
  is in the foreground. Inspect the pane before sending input.
- `blocked`: Herdr recognized an approval or question UI. Inspect it; asking
  the user comes before answering another agent's dialog.
- `unknown`: an agent is present but not confidently classified. It is not
  proof of completion.
- A successful `agent start` or prompt submission is admission, not an
  outcome. Verify the work through its returned output or readback.

## Coordination rules that save time

- Know yourself and the others from RUNTIME-LOADING.md (what each CLI loads
  at session start and per turn, when config edits apply) and runtime-wiring
  for registrations. Do not re-probe a fact that does not change the next
  action, and do not infer a pane's owner from a process name or CLI label.
- One exact next action per task, recorded in its `STATE.md`. Update the state
  when the work changes; a finished readiness check or report is not the next
  task.
- Keep active sessions, unrelated trees, and service identities intact. Do
  not close panes/tabs/workspaces you did not create, do not run
  `herdr server stop` from an active session, and never kill the main Herdr
  process.
- Remote machines: discover IDs on that machine with the same `--machine`
  prefix for every command; a connection failure does not prove a mutation
  was not applied — inspect remote state before retrying.

For isolation versus justified continuity use [Context Governance](CONTEXT-GOVERNANCE.md).
For acceptance readback, owned-resource reconciliation and optional student-free
lesson candidates use [worker closure](../skills/lessons-loop/references/worker-closure.md).
No success marker or native idle state substitutes for the actual result.
