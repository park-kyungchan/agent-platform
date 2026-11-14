# Where information belongs

Read this when you must decide which source answers a question, who writes a piece of information, or whether something may go to memory. Where each kind of information is written is the four-row table of `/root/.agents/AGENTS.md` section 6; this file does not repeat it. How to hand context to another agent: `CONCURRENT-WORK.md`. How the memory server ranks and refreshes: skill `hindsight-control`, `references/runtime.md`.

## 1. Who writes each kind, and how it is read

The kinds are the rows of `/root/.agents/AGENTS.md` section 6, plus a session's own record.

| Kind | Who writes it | How to read it |
| --- | --- | --- |
| A session's conversation and a CLI's private memory | That CLI | Resume only the exact session you need. Never import another agent's transcript |
| Short-term: the task's STATE, packets, the files STATE points to | The assigned owner, replacing old values | On entry, after a reset, at a handoff, and before an effect when it changed |
| A rule or contract: source, schemas, tests, KPI definitions | The owner of the implementation, through review | Read the current revision. Do not rely on a remembered implementation |
| Mid- and long-term: decisions with reasons, lessons, corrections, preferences, explained terms | The task owner, only with the task's authority, by the route of section 4 | Search and read as section 3 says; verify against current source and STATE |
| An operational fact: deployed state, a KPI value, a domain record, an action and its result | The owner of the domain or the task; the Lead for the object types its STATE names in use in the Ontology store (`ONTOLOGY-USE.md`) | A fresh authorized read. Memory is only a pointer to it |

- A decision another agent needs must not stay only in one CLI's private memory.
- Session transcripts are short-term. Automatic retention of whole sessions into Hindsight (`retainSessions` in the integration settings) stays off: retained transcripts carry dated status that later competes with corrections.
- A KPI and its target are set by the Lead from the user's words and recorded in the task's strategy, or in the store's `kpi` object when the task's STATE names that object type in use, never in both; the user corrects them. A measured KPI value is an operational fact.

## 2. Which source answers a question

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

## 3. Reading memory

- When to search is in `/root/.agents/AGENTS.md` section 6: the route in use is the knowledge pages, through a CLI's hooks or the MCP server `hindsight`. Do not search on every tool call; read only the selected pages.
- The native recall route that `agent-platform` `memory/README.md` prepares for engineering memory is not in use until its owner's task says so.
- If the memory tool is unavailable or disabled for your task, say so and work from the task's files. Do not start a client or change global configuration to get it back.

## 4. Writing memory

- A skill or a visible tool is not a write grant. Writing needs the task's authority. A no-write task does not ingest, PATCH, refresh or delete.
- Engineering capture, correction, publication and retraction follow `agent-platform` `memory/README.md`; it is the operational home. Draft a checked Git original and commit it in the owning closure PR; Hindsight is a retrieval copy, never a second original/history store.
- Do not use MCP `ingest_document` or `capture_initiative`, direct retain/PATCH, generated pages or a copied STATE as the engineering write route. The publisher alone sends reviewed-main publication bytes and derived tags under its authorized maintenance window.
- Corrections edit the same original through review; preserve Git history and pinned evidence. Retraction proves the listed reads before the owning PR removes the source; uncertain work keeps the route unavailable. No fallback to the mixed bank.
- The separate private Academy archive keeps its owner and the exception below. This source change does not activate a route or release registration/content holds.

## 5. Tags and derived pages

- Engineering tags come only from the vocabulary and the publisher of `agent-platform` `memory/README.md`.
- On retained older material keep the genuine `source:*`, `harness:*`, `project:*` and historical tags; never imitate a plugin's attribution.
- A tag routes a topic. It is not privacy and not permission. A strict tagged scope can miss shared untagged observations.
- A generated page is navigation. Never ingest one as new evidence, and do not assume a page exists: list the pages.

## 6. What never goes to memory

Write each fact once, at its owner. Do not retain in Hindsight: raw student, exam or lesson records; operational rows; action receipts or journals; KPI measurements or time series; full STATE copies; pane, model or quota assignments; CI logs; transcripts; copies of executable schemas; a second current-state table. Keep the detailed artifact at its owner and retain only a permitted locator or hash and a reusable lesson without student data. Never retain credentials. Never infer a user's ability or a broad permission.

One exception, by the user's decision of 2026-11-06: the Academy lesson archive. The academy server removes old rows, so the designated lesson owner archives dated teacher statements and source observations in Hindsight under the `academy-practical-protocol` archive-writer contract. The locator is `education-platform` `content/skills/academy-student-context/references/lesson-archive.md`. The archive excludes credentials, answer-key literals and parent contact details. An archived observation is a dated snapshot, not live state: a fresh owner read replaces it only for the same fact and stage of the same occurrence while the server still shows the row, and an original source score does not reopen a later teacher-final completion.

## 7. Lessons and Ontology

- Lessons are promoted on independent occurrences of a verified mechanism, not on document count. A structural proposal goes through the Lead to the task STATE's `DIRECTION PROPOSALS` and to the user; no memory write enacts architecture or policy. Procedure: skill `lessons-loop`.
- `ontology-platform` owns source-qualified operational state, admission, named actions and recovery; Git owns engineering originals; Hindsight owns their retrieval copy under `agent-platform` `memory/README.md`. Boundary document: `/home/palantirkc/platforms/ontology-platform/docs/hindsight-boundary.md`. There is no shared database, state replica or automatic promotion of prose into objects between them. Check what `ontology-platform` actually provides at its current revision before relying on it.
