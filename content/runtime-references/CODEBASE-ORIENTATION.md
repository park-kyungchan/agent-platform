# The host as one codebase: areas, repositories, crates, state

Read this when you do not know which part of the host owns a thing, or where a repository, a crate, a task, a plan or a service is. It is a map, not live state: confirm a branch, a release or an owner from Git, the service or the task's STATE before acting. How a thing is named: `NAMING.md`. How a change is built and delivered: `BUILD-AND-DELIVERY.md`.

## 1. Areas

A change belongs to the area that owns the fact it changes.

| Area | Owns | Repository | Installed on the host |
| --- | --- | --- | --- |
| Engine | Typed objects and their observations, actions with criteria, authorization, views (the Ontology) | `lifecycle-platform` `engine/` | `/usr/local/bin/ont`; store and install state under `/var/lib/ontology-platform/` |
| Domain | The private-academy teaching domain: reads of the Academy server, the lesson workflow, the teacher's records, the Academy MCP server | `academy-platform` | The Academy rows of section 4 |
| Harness | How agents work here: the command `ap`, the shared instructions and skills, the work schema of the store, the evaluation suites, the delivery of its own install | `lifecycle-platform` | `/usr/local/bin/ap`; `/root/.agents/`; state under `/var/lib/agent-platform/`; the timer `ap-deliver.timer` |
| Host configuration | The policy file, the recorded copies of host files, the system Git configuration | `lifecycle-platform` `host/` | `/etc/agent-platform/policy.json`, installed by `ap deliver`; `/etc/gitconfig`, installed by the Lead |

The host has two repositories: `lifecycle-platform`, which holds the engine, the harness and the host's configuration, and `academy-platform`, the Academy's domain. The repositories they absorbed are archived on GitHub and have no checkout here; an installed command built from one of them, such as `/usr/local/bin/vps-ops`, is read at its archived repository before it is changed. The installed paths `/etc/agent-platform`, `/var/lib/agent-platform` and `/var/lib/ontology-platform` kept their names.

The engine knows no domain. A domain or the harness uses the engine through a schema file it owns and the engine's library or command; it does not copy engine state.

## 2. Repositories and crates

Main checkouts are under `/home/palantirkc/platforms/<repository>/` unless a path is given. A main checkout can be behind `origin/main` and can hold someone's uncommitted edits, so its files are not the current source: read source with `git -C /home/palantirkc/platforms/<repository> show origin/main:<path>` (after `git -C /home/palantirkc/platforms/<repository> fetch -q origin`) or in a task worktree (`work/<scope>-<topic>/wt/<repo>/`), and edit only in a task worktree. Each repository's own `AGENTS.md` (read at `origin/main`) says what is in it and its check.

| Repository | Crate or directory | What it is | Start reading at |
| --- | --- | --- | --- |
| `lifecycle-platform` | `crates/ap` | The command `ap`: `unit` (the unit object, its run `ap unit run` and the case call of the engineering suite), `run` (one stage), `gate`, `review`, `deliver` (the install of an accepted merge of `main`), `version`; `ap --help` names the README section of each | `README.md` (by section; it is long), then the file named after the verb under `src/` |
| | `content/` | Source of `/root/.agents/`: `AGENTS.md`, `runtime-references/`, `skills/`, with `SHA256SUMS` | `content/README.md` |
| | `ontology/` | `work.schema.json`: the store's object and action types for tasks (holds, decisions, KPIs and measurements, reviews, install attempts, worker runs); the file is the list | `ontology/README.md` |
| | `evals/` | The suites and the cost read-out: `engineering/` (cases cut from merged units, judged by an executable), `lead/` (frozen situations of a Lead, scored by a model against a rubric), `lesson/` (the lesson replay: recorded daily-record reads against the read store), `readout.py` | `evals/README.md` |
| | `deploy/` | The systemd units and the request file of the delivery timer | `deploy/README.md` |
| | `engine/` | The Ontology engine, a Cargo workspace of its own: `crates/ont-core` (types and rules without storage), `crates/ont-store` (the SQLite store: schema installation, edits, the action log), `crates/ont` (the library and command `ont`: `get`, `search`, `around`, `aggregate`, `action`, `render`, `index`, `schema`, `backup`, `mcp`), `crates/ont-gen` (a typed module from a schema file), its specifications and installer | `engine/README.md` |
| | `host/` | The host's configuration: the source of `/etc/agent-platform/policy.json` and its installer, the recorded copies of host files, the system Git configuration | `host/README.md` |
| | `github/` | The rulesets and settings of the two repositories, compared and applied as files | `github/README.md` |
| | `specs/`, `ci/`, `scripts/` | The specifications of the `ap` contracts; the checks `./check` runs; the install, backup and PC pull steps | `map.json` |
| `academy-platform` | `crates/edu-academy-core` | Domain rules and the lesson workflow: intake, review, decision, approval, save, reconciliation and their records | `src/lib.rs`; `src/workflow/` |
| | `crates/edu-academy-sources` | Reads and writes against the Academy server's pages and scripts; the qualification tool `academy-live-driver` | `src/lib.rs` |
| | `crates/edu-academy-teacher` | The teacher's draft records and their checks | `src/lib.rs` |
| | `crates/edu-academy-mcp` | The Academy MCP server in Rust and its launcher | `src/bin/` |
| | `content/skills/` | Skills `academy-lesson-day`, `academy-dailyrecord-authoring`, `academy-prestudy-review` | each `SKILL.md` |
| | `modules/academy-workbench/` | Source of the Academy MCP server the teacher uses today (Bun) | its `README.md` |
| Custody repositories | `/home/palantirkc/projects/` | `bible-commentary-station`, `daniel-prayer-operations`, `park-kyungchan.github.io`, archived `academy-workbench` | not part of the platforms; open one only when the task names it |

Each of the two repositories has one workflow, `.github/workflows/rust.yml`, with the job `rust` (`BUILD-AND-DELIVERY.md` section 3).

## 3. Tasks and plans

| Looking for | Entry |
| --- | --- |
| Any task | `/home/palantirkc/work/<scope>-<topic>/STATE.md` |
| Directory roles and names | `NAMING.md`; an old path that is gone: `/home/palantirkc/archive/INDEX.md` |
| Whole-host improvement program (the whole view, the order of work) | `/home/palantirkc/work/ws-program/STATE.md`, with `LENS.md` beside it |
| The Academy lesson route on the Rust programs and the Ontology store (it replaced the Rust migration plan and tool-by-tool parity) | `/home/palantirkc/work/ws-program/DESIGN-LESSON-ROUTE.md` |
| Academy lessons | `/home/palantirkc/work/edu-lessons/STATE.md` |
| Recovery archives | `/home/palantirkc/archive/INDEX.md`. An archive is for recovery; it carries no current rule |

The state of the work (requests, intents, units, runs, findings, holds, KPIs) is in the Ontology store: `ap status --brief` in a task's directory, and `ONTOLOGY-USE.md`.

## 4. Services and state on the host

| Thing | Where | Owner record |
| --- | --- | --- |
| Ontology store | `/var/lib/ontology-platform/ont/ontology.sqlite`, backups in `backup/` beside it; read and written only through `ont` | The task whose STATE names the object types in use |
| Academy MCP server in use (`academy_operations`) | install base `/root/.local/share/academy-mcp-host-20261106/`; its `release.json` selects the running generation | `/home/palantirkc/work/edu-mcp-host-restore/STATE.md` |
| Installed shared instructions | `/root/.agents/AGENTS.md`, `runtime-references/`, `skills/` | `lifecycle-platform` `content/` |
| Delivery of `ap` and the shared instructions | the timer `ap-deliver.timer`; its request `/etc/agent-platform/deliver.json`; its state under `/var/lib/agent-platform/deliver/` | `lifecycle-platform` `deploy/README.md` |
| Configuration of each CLI | `CLI-RUNTIMES.md` section 1 | that CLI |
| Cargo build lock of the host | `/var/lib/agent-platform/build.lock` | `BUILD-AND-DELIVERY.md` section 2 |

## 5. External tools

Third-party tools are under `/home/palantirkc/tools/`; pinned third-party source is under `/home/palantirkc/external/`. They are not first-party source: do not edit them, and read the version and build state from the directory.

Local copies of the Anthropic, Palantir and Rust design sources are under `/home/palantirkc/external/reference-docs/`; the entry is `OFFICIAL-DOCS.md` section 4.

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
- A finished-looking task directory is not safe to delete: see the deletion rule in `/root/.agents/AGENTS.md` section 5.
