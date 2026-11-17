# Context governance: which context to load, from which one home

Read this when you must decide which context to load for a task, above all a kind of task you have not met; which source answers a question; who writes a piece of information; or whether something may go to memory. Where each kind of information is written is the table of `/root/.agents/AGENTS.md` section 6; this file does not repeat it. How to hand context to another agent: `CONCURRENT-WORK.md`. How the memory server ranks and refreshes: skill `hindsight-control`, `references/runtime.md`.

## 1. The order of loading

Stop at the first step that answers; each step names the next one.

1. The task's STATE: owner, holds, the next action (`AGENTS.md` section 1).
2. The packet's `Read first:` line (full paths, a passage as `path:a-b`), when you were given one: read exactly that before anything it does not name.
3. The row of `/root/.agents/AGENTS.md` section 3 for your situation: one reference or one skill.
4. The area that owns the thing: `CODEBASE-ORIENTATION.md` sections 1 and 2; then that repository's `AGENTS.md` at `origin/main` (`git -C /home/palantirkc/platforms/<repo> show origin/main:AGENTS.md`, after `git -C /home/palantirkc/platforms/<repo> fetch -q origin`); then the start file the table names.
5. A search in that repository only, at `origin/main`: `git -C /home/palantirkc/platforms/<repo> grep -n '<words>' origin/main -- <directory>`; then read the lines it found.
6. A question of design (the life cycle, an Ontology store, Rust, Spec-Driven Development, context governance itself): `OFFICIAL-DOCS.md` section 4; then the corpus catalog `/home/palantirkc/external/reference-docs/llms.txt`; then `grep -rn '<words>' /home/palantirkc/external/reference-docs/docs/`.

## 2. How to read without waste

- Read by line range: `sed -n '<a>,<b>p' <file>`. A file over about 300 lines, or with very long lines, is searched first (`grep -n`) and read where the search lands. A directory is listed, never read whole.
- Ask a command for a fact instead of reading a file that copies it: `ap state <task directory> --text`, `ap backlog <task key> --text` (the board: items, objectives, the order; no holds), the view `active_holds` for holds (`ONTOLOGY-USE.md` section 2), `ont describe`, `ont action list`, a command's `--help`.
- Read a user's row or decision with its words, never from its key alone.
- A map or an index names where to read; it is not the answer. Cite the file it points to, not the map.
- A tool result too large to show means the read was too wide: narrow it with a line range, a `grep` or a command's own filter instead of paging through it.

## 3. Copies that are not current

| Copy | Why it misleads | Read instead |
| --- | --- | --- |
| The files of a main checkout, `/home/palantirkc/platforms/<repo>/<path>` | It can lag `origin/main` and hold someone's uncommitted edit | `git -C /home/palantirkc/platforms/<repo> show origin/main:<path>`, or a task worktree |
| The store's objects of type `instruction_file`, `packet`, `report` and `task` | Index copies of files whose refresh stopped; their `fresh_until` has passed | The file itself, in Git or in the task directory |
| `text/` and `sources/` of `/home/palantirkc/external/reference-docs/` | The same text with other line numbers, and raw downloads | `docs/`, through the rows of `OFFICIAL-DOCS.md` section 4 |
| A path an older file names that no longer exists | It was retired or archived (`/home/palantirkc/archive/INDEX.md`); an archive carries no current rule | The task's STATE |
| Instructions in another CLI's home, or a skill copied instead of linked | A second copy drifts from its source | `/root/.agents/` |
| A file that says it was generated | A snapshot of its source at one revision | The source it names |

When two copies disagree, current source wins. Name the stale copy in your report so that it can be taken off the path.

## 4. Routing that every model follows

A weak model routes as well as a strong one only where a route asks for no judgment.

- The strong model routes once: the Lead writes a packet's `Read first:` line as full paths and `path:a-b`, each with what it is for, so that a stage reads instead of searching. Most of what a stage reads before its first edit is code and records, not instructions.
- At most three steps lead from `AGENTS.md` to a line: a row of its section 3, a table of the reference, a `path:a-b`.
- Paths are written in full, never shortened or elided; a table is keyed by the situation, in the words an assignment uses.
- A stale copy is taken off the path, not warned about.
- A change of a route is accepted on the instructions suite (`agent-platform` `evals/README.md`, section "The instructions suite"); its cases today cover how each CLI runs without a terminal UI, not the routes of this file or of `OFFICIAL-DOCS.md` section 4, so such a change also names the reads it was tried on.

## 5. A task kind no row covers

Follow section 1 to its end. When you needed many reads (more than about fifteen) before your first effect, your report names the route you found, as `path:a-b`, with the situation in your assignment's words. The Lead adds the row (to `AGENTS.md` section 3, a reference or the packet form) and a case to the instructions suite: a mistake made twice becomes a row or a check, not one more sentence.

## 6. Who writes each kind, and how it is read

The kinds are the rows of `/root/.agents/AGENTS.md` section 6, plus a session's own record.

| Kind | Who writes it | How to read it |
| --- | --- | --- |
| A session's conversation and a CLI's private memory | That CLI | Resume only the exact session you need. Never import another agent's transcript |
| Short-term: the task's STATE, packets, the files STATE points to | The assigned owner, replacing old values | On entry, after a reset, at a handoff, and before an effect when it changed |
| A rule or contract: source, schemas, tests, KPI definitions | The owner of the implementation, through review | Read the current revision. Do not rely on a remembered implementation |
| Mid- and long-term: decisions with reasons, lessons, corrections, preferences, explained terms | The task owner, only with the task's authority, by the route of section 9 | Search and read as section 8 says; verify against current source and STATE |
| An operational fact: deployed state, a KPI value, a domain record, an action and its result | The owner of the domain or the task; the Lead for the object types its STATE names in use in the Ontology store (`ONTOLOGY-USE.md`) | A fresh authorized read. Memory is only a pointer to it |

- A decision another agent needs must not stay only in one CLI's private memory.
- Session transcripts are short-term. Automatic retention of whole sessions into Hindsight (`retainSessions` in the integration settings) stays off: retained transcripts carry dated status that later competes with corrections.
- A KPI and its target are set by the Lead from the user's words and recorded in the task's strategy, or in the store's `kpi` object when the task's STATE names that object type in use, never in both; the user corrects them. A measured KPI value is an operational fact.

## 7. Which source answers a question

| Question | Authority |
| --- | --- |
| What may I do now, and what is next? | The user's current instruction, `/root/.agents/AGENTS.md`, the task's STATE. Memory is history |
| What did the user decide, and why? | The dated source decision, newer corrections and the current task authority. A generated page is navigation |
| What does the program implement? | Current source at a Git revision, compiler and runtime output, tests |
| What does a KPI mean? | The definition in the owning repository or the task's strategy; the target in the task's strategy; the value as that definition computes it. When the task's STATE names the store's `kpi` object type in use, that object holds the task's definition, target and latest value (`ONTOLOGY-USE.md`). What the words KPI, metric and guardrail mean: `/root/.agents/skills/lead/references/measurement.md` |
| What does a word in an instruction or in the user's message mean? | `/root/.agents/AGENTS.md` section 8, then `GLOSSARY.md` |
| What does an operational object say now? | A fresh authorized domain read |
| May a caller read, derive or act? | The domain's authenticated policy and the task grant. Neither memory nor a tag grants access |
| Did an effect happen, and is a retry safe? | Reconciliation against the authoritative target. Neither prose nor a rollback settles it |
| What was explained to the user? | Dated glossary evidence. An explanation does not show that the user mastered the term |

When memory and current source disagree, the source wins for current truth. Keep the dated history and write a scoped Correction; do not rewrite one store to match the other. Unknown or stale evidence stays labelled unknown or stale, even when a page reports `is_stale:false`.

## 8. Reading memory

- Long-term memory is off until it is rebuilt (`/root/.agents/AGENTS.md` section 6): call no memory tool and register no memory hook. When it returns, the route is the knowledge pages through the MCP server `hindsight`; search only when a decision needs it, never on every tool call, and read only the selected pages.
- The native recall route that `agent-platform` `memory/README.md` prepares for engineering memory is not in use until its owner's task says so.
- If the memory tool is unavailable or disabled for your task, say so and work from the task's files. Do not start a client or change global configuration to get it back.

## 9. Writing memory

- A skill or a visible tool is not a write grant. Writing needs the task's authority. A no-write task does not ingest, PATCH, refresh or delete.
- Engineering capture, correction, publication and retraction follow `agent-platform` `memory/README.md`; it is the operational home. Draft a checked Git original and commit it in the owning closure PR; Hindsight is a retrieval copy, never a second original/history store.
- Do not use MCP `ingest_document` or `capture_initiative`, direct retain/PATCH, generated pages or a copied STATE as the engineering write route. The publisher alone sends reviewed-main publication bytes and derived tags under its authorized maintenance window.
- Corrections edit the same original through review; preserve Git history and pinned evidence. Retraction proves the listed reads before the owning PR removes the source; uncertain work keeps the route unavailable. No fallback to the mixed bank.
- The separate private Academy archive keeps its owner and the exception below. This source change does not activate a route or release registration/content holds.

## 10. Tags and derived pages

- Engineering tags come only from the vocabulary and the publisher of `agent-platform` `memory/README.md`.
- On retained older material keep the genuine `source:*`, `harness:*`, `project:*` and historical tags; never imitate a plugin's attribution.
- A tag routes a topic. It is not privacy and not permission. A strict tagged scope can miss shared untagged observations.
- A generated page is navigation. Never ingest one as new evidence, and do not assume a page exists: list the pages.

## 11. What never goes to memory

Write each fact once, at its owner. Do not retain in Hindsight: raw student, exam or lesson records; operational rows; action receipts or journals; KPI measurements or time series; full STATE copies; pane, model or quota assignments; CI logs; transcripts; copies of executable schemas; a second current-state table. Keep the detailed artifact at its owner and retain only a permitted locator or hash and a reusable lesson without student data. Never retain credentials. Never infer a user's ability or a broad permission.

One exception, by the user's decision of 2026-11-06: the Academy lesson archive. The academy server removes old rows, so the designated lesson owner archives dated teacher statements and source observations in Hindsight under the `academy-practical-protocol` archive-writer contract. The locator is `education-platform` `content/skills/academy-student-context/references/lesson-archive.md`. The archive excludes credentials, answer-key literals and parent contact details. An archived observation is a dated snapshot, not live state: a fresh owner read replaces it only for the same fact and stage of the same occurrence while the server still shows the row, and an original source score does not reopen a later teacher-final completion.

## 12. Lessons and Ontology

- Lessons are promoted on independent occurrences of a verified mechanism, not on document count. A structural proposal goes to the Lead, who decides it as a `decision` in the store, and to the user; no memory write enacts architecture or policy. Procedure: skill `lessons-loop`.
- `ontology-platform` owns source-qualified operational state, admission, named actions and recovery; Git owns engineering originals; Hindsight owns their retrieval copy under `agent-platform` `memory/README.md`. Boundary document: `ontology-platform` `docs/hindsight-boundary.md` at `origin/main`. There is no shared database, state replica or automatic promotion of prose into objects between them. Check what `ontology-platform` actually provides at its current revision before relying on it.
