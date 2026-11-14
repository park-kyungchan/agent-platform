# The host as one codebase: areas, repositories, crates, state

Read this when you do not know which part of the host owns a thing, or where a repository, a crate, a task, a plan or a service is. It is a map, not live state: confirm a branch, a release or an owner from Git, the service or the task's STATE before acting. How a thing is named: `NAMING.md`. How a change is built and delivered: `BUILD-AND-DELIVERY.md`.

## 1. Areas

A change belongs to the area that owns the fact it changes.

| Area | Owns | Repository | Installed on the host |
| --- | --- | --- | --- |
| Engine | Typed objects and their observations, actions with criteria, authorization, views (the Ontology) | `ontology-platform` | `/usr/local/bin/ont`; store and install state under `/var/lib/ontology-platform/` |
| Domain | The private-academy teaching domain: reads of the Academy server, the lesson workflow, the teacher's records, the Academy MCP server | `education-platform`; modules `education-dc-gang-a`, `education-problem-edit` | The Academy rows of section 4 |
| Harness | How agents work here: the command `ap`, the shared instructions and skills, the work schema of the store, the engineering memory records | `agent-platform` | `/usr/local/bin/ap`; `/root/.agents/`; state under `/var/lib/agent-platform/` |
| Host operations | Remote host and Hermes operations | `agent-vps-operations` | `/usr/local/bin/vps-ops` |
| Memory service | Long-term memory (Hindsight), a container on this host; no first-party source | none | `/docker/hindsight/`; agent integration under `/root/.hindsight/` |

The engine knows no domain. A domain or the harness uses the engine through a schema file it owns and the engine's library or command; it does not copy engine state.

## 2. Repositories and crates

Main checkouts are under `/home/palantirkc/platforms/<repository>/` unless a path is given. A main checkout can be behind `origin/main` and can hold someone's uncommitted edits: read source at a named commit in a task worktree (`work/<scope>-<topic>/wt/<repo>/`) or with `git show <commit>:<path>`, and edit only in a task worktree.

| Repository | Crate or directory | What it is | Start reading at |
| --- | --- | --- | --- |
| `ontology-platform` | `crates/ont-core` | Types and rules of the engine without storage | `src/lib.rs` |
| | `crates/ont-store` | The SQLite store: schema installation, edits, the action log, candidates for queries | `src/lib.rs` |
| | `crates/ont` | The library and command `ont`: `get`, `search`, `around`, `aggregate`, `action`, `render`, `index`, `schema`, `backup`, `mcp` | `README.md`, then `src/lib.rs` and the file named after the command |
| | `crates/ont-daemon` | An older service form of the engine; not installed on the host | its `Cargo.toml` |
| `agent-platform` | `crates/ap` | The command `ap`: `work` (start, wait for and close a worker), `pane`, `install`, `doctor`, `render` | `src/main.rs`; `src/work.rs`; `src/native/` |
| | `crates/kpi-producer` | The binary `ap-kpi` | `README.md` |
| | `content/` | Source of `/root/.agents/`: `AGENTS.md`, `runtime-references/`, `skills/`, with `SHA256SUMS` | `content/README.md` |
| | `ontology/` | `work.schema.json`: the store's object types for tasks (`hold`, `decision`, `kpi`, `measurement`, `worker_run`, `thread`, `plan`) | `ontology/README.md` |
| | `memory/` | Engineering memory: checked originals and their publisher | `memory/README.md` |
| `education-platform` | `crates/edu-academy-core` | Domain rules and the lesson workflow: intake, review, decision, approval, save, reconciliation and their records | `src/lib.rs`; `src/workflow/` |
| | `crates/edu-academy-sources` | Reads and writes against the Academy server's pages and scripts; the qualification tool `academy-live-driver` | `src/lib.rs` |
| | `crates/edu-academy-teacher` | The teacher's draft records and their checks | `src/lib.rs` |
| | `crates/edu-academy-mcp` | The Academy MCP server in Rust and its launcher | `src/bin/` |
| | `content/skills/` | Skills `academy-runtime-routing`, `academy-student-context` | each `SKILL.md` |
| | `modules/academy-workbench/` | Source of the Academy MCP server the teacher uses today (Bun) | its `README.md` |
| `education-dc-gang-a` | `crates/edu-dc-gang-a` | Module of one academy | `README.md` |
| `agent-vps-operations` | `crates/vps-ops` | The command `vps-ops` | `README.md` |
| `education-problem-edit` | checkout `/home/palantirkc/work/problem-edit` | Problem editing module | `README.md` |
| Custody repositories | `/home/palantirkc/projects/` | `bible-commentary-station`, `daniel-prayer-operations`, `park-kyungchan.github.io`, archived `academy-workbench` | not part of the platforms; open one only when the task names it |

Every platform repository has one workflow, `.github/workflows/rust.yml`, with the job `rust` (`BUILD-AND-DELIVERY.md` section 3).

## 3. Tasks and plans

| Looking for | Entry |
| --- | --- |
| Any task | `/home/palantirkc/work/<scope>-<topic>/STATE.md` |
| Directory standard and where an old path went | `/home/palantirkc/work/ws-layout-standard/LAYOUT.md` and `PATH-MAP.tsv` beside it |
| Whole-host improvement program (threads, order) | `/home/palantirkc/work/workspace-strategy-reassessment-20261109/STATE.md` |
| Rust migration plan and Academy parity | `/home/palantirkc/work/ws-replan/STATE.md`, with the plan and strategy files beside it |
| Academy lessons | `/home/palantirkc/work/edu-lessons/STATE.md` |
| Memory selection and evaluation | `/home/palantirkc/work/mem-host-memory/STATE.md` |
| Recovery archives | `/home/palantirkc/archive/INDEX.md`. An archive is for recovery; it carries no current rule |

A task's current holds, decisions, KPI values and worker runs are in the Ontology store when its STATE says so (`ONTOLOGY-USE.md`).

## 4. Services and state on the host

| Thing | Where | Owner record |
| --- | --- | --- |
| Ontology store | `/var/lib/ontology-platform/ont/ontology.sqlite`, backups in `backup/` beside it; read and written only through `ont` | The task whose STATE names the object types in use |
| Academy MCP server in use (`academy_operations`) | install base `/root/.local/share/academy-mcp-host-20261106/`; its `release.json` selects the running generation | `/home/palantirkc/work/edu-mcp-host-restore/STATE.md` |
| Hindsight | container defined in `/docker/hindsight/`; API on `127.0.0.1:8888` | `/docker/hindsight/STATE.md` and `HINDSIGHT_OFFICIAL_PER_CALL.md` there |
| Hindsight integration of the CLIs | `/root/.hindsight/coding-agents`; settings `/root/.hindsight/coding-agent.json` (contains a credential: never print it) | skill `hindsight-control` |
| Installed shared instructions | `/root/.agents/AGENTS.md`, `runtime-references/`, `skills/` | `agent-platform` `content/` |
| Configuration of each CLI | `CLI-RUNTIMES.md` section 1 | that CLI |
| Cargo build lock of the host | `/home/palantirkc/work/ws-replan/wt/.host-cargo.lock` | `BUILD-AND-DELIVERY.md` section 2 |

## 5. External tools

Third-party tools are under `/home/palantirkc/tools/`; pinned third-party source is under `/home/palantirkc/external/`. They are not first-party source: do not edit them, and read the version and build state from the directory.

| Tool | Path | Use here |
| --- | --- | --- |
| REA (package `rea-agents`, clone of `https://github.com/morluto/rea`) | `/home/palantirkc/tools/rea` | A command and MCP server with which an agent inspects binaries, JavaScript applications, web bundles, saved captures and running processes. Started with `node scripts/rea.mjs mcp` in the clone, and only when the clone is built (`dist/` exists) |
| Ghidra | `/home/palantirkc/tools/ghidra_12.1.4_PUBLIC` | The backend REA uses for native binaries, found through `GHIDRA_INSTALL_DIR`. Not used on its own |

- Static analysis reads files the task names and neither runs the target nor attaches to a process: allowed when the task names the target.
- Building REA, installing its dependencies or registering it in a CLI is an installation: it needs the task's authority.
- Dynamic use (attaching to a process, instrumenting an application or a device, capturing traffic, sending a request to a live service) needs the user's word each time.

## 6. Searching

- Search the named repository or task first with `rg` or `git grep`. Do not scan the whole host.
- Private custody (`edu-cached-records`, `edu-private-records`, student and exam records) stays out of searches unless the task names it.
- A finished-looking task directory is not safe to delete: see the deletion rule in `/root/.agents/AGENTS.md` section 4.
