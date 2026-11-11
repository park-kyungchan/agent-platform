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
| Installed first-party binaries | `/usr/local/bin/ap`, `/usr/local/bin/vps-ops`, `/usr/local/bin/ont` (Ontology store and install state under `/var/lib/ontology-platform/`) |
| Recovery archives | `/home/palantirkc/archive/INDEX.md`. An archive is for recovery; it carries no current rule |

## Names

Every platform has one long name and one short name, and no third form. The long name is for the GitHub repository, its main checkout and installed directories; the short name is for crates, binaries, task scopes, branch prefixes, MCP servers and tool prefixes.

| Thing | Long name | Short name |
| --- | --- | --- |
| The agent platform | `agent-platform` | `ap` |
| The Ontology platform | `ontology-platform` | `ont` |
| The education platform | `education-platform` | `edu` |
| Remote host and Hermes operations | `agent-vps-operations` | `vps-ops` |
| Work on the whole host | none | `ws` |
| Long-term memory work | none | `mem` |

Not used as a task scope or branch prefix: `agent-`, `ontology-`, `workspace-`, `academy/`.

`academy` is the private-academy teaching domain inside `edu` (crates `edu-academy-*`, MCP server `academy`, tools `academy_*`); it is not another word for `edu`.

| Kind | Pattern |
| --- | --- |
| Repository and main checkout | `<word>-platform`, or `<platform word>-<module>` for a module; the checkout has the repository's name under `/home/palantirkc/platforms/` (one older checkout still differs: `education-problem-edit` is at `/home/palantirkc/work/problem-edit` and is moved only on the user's word) |
| Crate and its directory | `<short>-<role>` or `<short>-<domain>-<role>` |
| Binary | the crate's name, or the short name alone for the platform's one command |
| MCP server; MCP tool | the short name or the domain, no suffix; `<server>_<object>_<verb>`, or `<server>_<verb>` when there is no object, lower case with underscores |
| Task directory | `<scope>-<topic>`, scope one of `ap`, `ont`, `edu`, `ws`, `mem`; no date in the name |
| Branch | `<short>/<topic>-<YYYYMMDD>` |
| File of a task or an instruction | upper-case words joined by hyphens, with the extension `.md`; a packet is `TASK-<id>.md`, its report `REPORT-<id>.md`, a review `REVIEW-<what>.md` |
| Shared skill | lower case, hyphen, no date |
| Top-level directory of `/home/palantirkc` | one lower-case word from `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`, plus `tools` |
| Other files in a repository | that repository's own convention |
| Installed first-party | `/usr/local/bin/<binary>`; state and previous binaries under `/var/lib/<long name>/` |

A closed task directory, an archived repository and a third-party tool keep their names. Older names that break a pattern, and the order in which they are renamed: `/home/palantirkc/work/workspace-strategy-reassessment-20261109/naming-20261111/NAMING-STANDARD.md`.

## External tools

Third-party tools kept under `/home/palantirkc/tools/`. They are not first-party source: do not edit them, and read the version and build state from the directory, not from memory.

| Tool | Path | What it is and how it is used here |
| --- | --- | --- |
| REA | `/home/palantirkc/tools/rea` (clone of `https://github.com/morluto/rea`, package `rea-agents`) | "Reverse Engineer Anything": a local command and MCP server with which an agent inspects binaries, JavaScript applications and web bundles, saved captures and running processes, and gets findings with evidence. Started with `node scripts/rea.mjs mcp` in the clone; it runs only when the clone is built (`dist/` exists) |
| Ghidra | `/home/palantirkc/tools/ghidra_12.1.4_PUBLIC` | The analysis backend REA uses for native binaries; REA finds it through the environment variable `GHIDRA_INSTALL_DIR`. It is not used on its own here |

- REA is not ReVa or any other tool with a similar name.
- Static analysis reads files the task names and does not run the target or attach to a process: allowed when the task names the target. Building REA, installing its dependencies or registering it in a CLI is an installation: it needs the task's authority.
- Dynamic use needs the user's word each time: attaching to a process, instrumenting an application or a device, capturing or intercepting traffic, sending a request to a live service.
- The Academy server the teacher uses (the LMS) is a web application without a native client: its contracts were derived from a pinned client script as text, without Ghidra. Earlier results: `education-platform` `modules/academy-workbench/tools/academy-mcp/docs/rea-name-search-contract-20261026.md`; `/home/palantirkc/work/edu-prestudy-section-mcp/evidence/`.

## Searching

- Search the named repository or task first with `rg` or `git grep`. Do not scan the whole host.
- Private custody (`edu-cached-records`, `edu-private-records`, student and exam records) stays out of searches unless the task names it.
- `/home/palantirkc/projects/palantirkc` and other retired harness records are history. Open one only to reconcile a named live dependency or to recover a specific original the user asked for.
- A finished-looking task directory is not safe to delete: see the deletion rule in `/root/.agents/AGENTS.md`.
