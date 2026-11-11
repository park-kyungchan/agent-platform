# Shared agent instructions

Every coding CLI on this host loads this one file through its own pointer file. The source is `content/` in the `agent-platform` repository. Edit the source; never edit a pointer and never make a per-CLI copy.

## 1. Start of a task

1. Open the task directory `/home/palantirkc/work/<scope>-<topic>/` and read its `STATE.md` before any effect. Read `TASK.md` too if you were handed one. They name the owner, what you may change, what is on hold, the next action and how to verify it.
2. If you were given a list of files to read first, read exactly those with your file tool and name them in your first output.
3. Do the next action. When the work changes, update `STATE.md` by replacing the old value.

No task directory yet: create `work/<scope>-<topic>/STATE.md`. Put the task's Git worktrees in `work/<scope>-<topic>/wt/<repo>/`.

## 2. Where things are

| What | Path |
| --- | --- |
| Platform and module repositories (main checkouts) | `/home/palantirkc/platforms/<repo>/`. Modules are named `<platform>-<module>`. The list is in `codebase-orientation.md` |
| Tasks | `/home/palantirkc/work/<scope>-<topic>/` with `STATE.md`; task worktrees in `wt/<repo>/` |
| Pinned external source | `/home/palantirkc/external/` |
| Other repositories, kept for custody | `/home/palantirkc/projects/` |
| Installed shared instructions | `/root/.agents/AGENTS.md`, `/root/.agents/runtime-references/` (plural), `/root/.agents/skills/` |

Never create task state in `operations/` or in a top-level `worktrees/`. Never assume a path exists: check it.

## 3. Which file to read

Read one of these only when its situation applies. References are in `/root/.agents/runtime-references/`; a skill is `/root/.agents/skills/<name>/SKILL.md`. If your CLI does not list a skill, read its `SKILL.md` by path.

| Situation | Read |
| --- | --- |
| I do not know where a repository, task, plan or service lives | `codebase-orientation.md` |
| I start, resume or configure a CLI, register an MCP server, or must check what a session loaded | `CLI-RUNTIMES.md` |
| I hand work to another agent, receive work, or share a repository or build with one | `concurrent-work.md` |
| I must decide where a piece of information belongs, or whether it may go to memory | `CONTEXT-GOVERNANCE.md` |
| Memory returns stale or irrelevant results, or I change memory content or pages | `HINDSIGHT-RUNTIME.md` |
| I need the official documentation of a CLI, Herdr, Docker, Tailscale, Hostinger or Hindsight | `OFFICIAL-DOCS.md` |
| I change a shared instruction, a skill or a reference | Section 7 below, then `CLI-RUNTIMES.md` section 7 |
| I use the Academy tools or prepare a lesson | skill `academy-runtime-routing` for the tools; skill `academy-student-context` for lesson preparation and student status |
| I answer or report to the user | skill `explain-precisely` |
| STATE names me Lead | skills `lead-orchestration` and `strategy-dialogue` |
| I control Herdr panes or agents | skill `herdr`; for Hermes sessions `herdr-hermes` |
| A task closes, or I curate lessons | skill `lessons-loop` |

## 4. Rules

Authority
- The user's current instruction and the task's STATE decide what you may do. An available tool, a CLI name, a process label or a remembered decision is not permission.
- A handoff states the goal, exact targets, allowed effects, expected output and the STATE path. Never make another agent reconstruct a transcript.
- After a context reset or a resumed session, read the task's rules and STATE again before any effect.
- The same authority and ownership boundaries apply in every CLI. What differs per CLI is instruction discovery, child isolation, resume and headless flags.

Evidence
- Establish the current state from source, Git, compiler, database and service output. Memory tells you what was decided earlier; check that it still holds.
- Read only what the next decision needs. Do not scan every repository, history or private custody to find a task.
- Reuse facts you already have: runner, host or container, role, cwd, pane. Probe a missing fact only if it changes the next action. The model and effort are unknown unless the session reports them. Host, container, shared daemon and remote executor can have different paths and identities, and a tool shell's environment is not the user's pane.
- Verify by actual output or readback, not by a success message or an exit code alone.

Failure
- Report `snake_case_code@path:line` with one short cause and no secret or private payload. Fix that cause at that boundary.
- Do not retry the same failing step unchanged and do not switch models to get past it. If the cause repeats, split or redesign the step.

Effects
- A disposable target may be rerun. When an effect on a real target has an uncertain outcome, read the target before retrying.
- Respect the gates the task states. Do not invent approval steps for reversible local work.
- Never print credentials. Keep active sessions, unrelated working trees and service identities intact. Do not change global configuration merely to orient.
- Before deleting a retained original, verify the recovery copy and every live consumer of the path; task completion alone is not enough. A pinned artifact is immutable: fix forward in a new version.
- Use `set -o pipefail` in shell pipelines.
- Remote work keeps SSH host-key checking and the Tailscale boundary. Hermes version promotion goes only through the installed target helper's one-shot path; never edit updater state, receipts, jobs or transactions directly.

Delegation
- Delegate only when the task authorizes it, for independent review, ownership transfer or work with a disjoint write set. Verify the returned output or effect yourself.

Custom MCP servers
- A custom MCP server and its backend state are shared by all CLIs. Register the same approved server directly in each CLI's own configuration. Built-in, marketplace and plugin entries stay with the CLI that installed them.
- A registered or visible tool is not permission to use it: the task's data and effect limits apply.

Do not build
- A second instruction loader, a per-CLI instruction copy, or a gate, receipt, ledger, registry or watcher that repeats what Git, Cargo, the database or the CLI already does. Do not restore a retired harness; an archive grants no authority.
- A plan that assumes future completion is a design assumption. It is not evidence of acceptance, readiness or permission to cut over or delete.

## 5. Communication and data

- The user's prompts are short and narrow by the user's own statement. Read each one against the whole goal of the task, and propose the direction and any better alternative without being asked. Do not invent goals or widen the allowed effects.
- Explanations to the user follow the skill `explain-precisely`: Korean prose, programming terms in English, a concrete definition at a term's first use, no metaphor. Code, instructions and technical records are in English.
- Private user data: for Hindsight migration and evaluation work, inspect the actual data the task needs and nothing more. For every other task, use metadata and hashes only unless the user authorizes more.

## 6. Where to write what you learn

| What it is | Write it here | Never here |
| --- | --- | --- |
| Short-term: this task's owner, authority, holds, progress, next action, measurements, evidence paths | The task's `STATE.md` under `/home/palantirkc/work/<scope>-<topic>/`, replacing old values | Hindsight; a CLI's private memory |
| A rule or contract: how agents work, what code does, a schema, a KPI definition | The owning repository's source, through review | Hindsight as the only copy; a prompt |
| Mid- and long-term: a decision and its reason, a lesson, a correction, a user preference, an explained term, that a later task in any CLI should know | Hindsight, when the task authorizes a write. In a no-write task, or when the write fails, leave a short handoff file in the task directory and name it in STATE | STATE only; a CLI's private memory |
| An operational fact: what is deployed, a KPI value, a domain record, an action and its result | The domain owner's store; an `ontology-platform` object once STATE says that object type is in use | Hindsight; prose in an instruction file |

- A CLI's private memory (Claude Code auto-memory, Muse memory, Antigravity brain, any other) is visible to one CLI in one directory. Do not put there anything another agent or a later session in another CLI needs. If such an item is already there, move it to its row above and delete the private copy.
- Reading: each CLI reaches Hindsight through its own hooks or the MCP server `hindsight`. Search the knowledge pages, read the relevant ones and credit what you use: at a new task, a bug report, a test or convention question, integration work, a "why" or "what is left" question, and before a commit. Memory is a past record: verify it against current source and STATE. If the tool is unavailable, say so and continue from the task's files.
- Details and the one approved exception (the Academy lesson archive): `CONTEXT-GOVERNANCE.md`.

## 7. When you write instructions, STATE or a handoff

- One home per fact: write a rule, decision or fact in one file and name that file elsewhere.
- Keep changing values out of this file and out of skill descriptions: model names, effort, versions, counts, token windows, quotas, SHAs, pane and session IDs, dates of approvals. They belong in the task's STATE with the time they were observed.
- A STATE file is a checkpoint that is replaced, with the previous bytes kept, not a log that grows.
- "Loaded" means the session's own record shows the text. A path in a prompt, a catalog entry or a configuration key does not show that.
