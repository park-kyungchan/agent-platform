# Where things live on this host

Read this when you do not know where a repository, task, plan or service is. It is a map, not live state: confirm a branch, a release or an owner from Git, the service or the task's STATE before acting.

## Repositories

| Repository | Main checkout | Contents |
| --- | --- | --- |
| `agent-platform` | `/home/palantirkc/platforms/agent-platform` | Rust `ap`; the shared instructions, references and generic skills under `content/` |
| `ontology-platform` | `/home/palantirkc/platforms/ontology-platform` | Rust Ontology: typed objects, evidence, authorization, actions, program report |
| `education-platform` | `/home/palantirkc/platforms/education-platform` | Academy sources, Rust Academy crates, the two Academy skills under `content/skills/` |
| `education-dc-gang-a` | `/home/palantirkc/platforms/education-dc-gang-a` | Private-academy domain module |
| `agent-vps-operations` | `/home/palantirkc/platforms/agent-vps-operations` | Rust `vps-ops`; remote host and Hermes operations |
| `education-problem-edit` | `/home/palantirkc/work/problem-edit` | Problem editing module |
| Custody repositories | `/home/palantirkc/projects/` | `bible-commentary-station`, `daniel-prayer-operations`, `park-kyungchan.github.io`, archived `academy-workbench` |

Edit source in a task worktree, `work/<scope>-<topic>/wt/<repo>/`, not in a main checkout. Before editing, read the worktree's branch, HEAD and `git status`; uncommitted edits you did not make belong to someone else.

## Tasks, plans and services

| Looking for | Entry |
| --- | --- |
| Any task | `/home/palantirkc/work/<scope>-<topic>/STATE.md` |
| Directory standard and where an old path went | `/home/palantirkc/work/ws-layout-standard/LAYOUT.md` and `PATH-MAP.tsv` in that directory |
| Whole-host improvement program (threads, order, board) | `/home/palantirkc/work/workspace-strategy-reassessment-20261109/STATE.md` |
| Rust migration plan and Academy parity | `/home/palantirkc/work/ws-replan/CURRENT-PLAN.md`, `STRATEGY.md` and `STATE.md` |
| Academy lessons | `/home/palantirkc/work/edu-lessons/STATE.md` |
| Academy MCP server in use | install base `/root/.local/share/academy-mcp-host-20261106/`; its `release.json` selects the running generation; owner record `/home/palantirkc/work/edu-mcp-host-restore/STATE.md` |
| Hindsight service | `/docker/hindsight/STATE.md` and `HINDSIGHT_OFFICIAL_PER_CALL.md` there; agent integration under `/root/.hindsight/` |
| Memory selection and evaluation task | `/home/palantirkc/work/mem-host-memory/STATE.md` |
| Installed first-party binaries | `/usr/local/bin/ap`, `/usr/local/bin/vps-ops` |
| Recovery archives | `/home/palantirkc/archive/INDEX.md`. An archive is for recovery; it carries no current rule |

## Searching

- Search the named repository or task first with `rg` or `git grep`. Do not scan the whole host.
- Private custody (`edu-cached-records`, `edu-private-records`, student and exam records) stays out of searches unless the task names it.
- `/home/palantirkc/projects/palantirkc` and other retired harness records are history. Open one only to reconcile a named live dependency or to recover a specific original the user asked for.
- A finished-looking task directory is not safe to delete: see the deletion rule in `/root/.agents/AGENTS.md`.
