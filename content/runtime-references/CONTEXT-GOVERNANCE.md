# Where information belongs

The four-row rule every agent follows is in `/root/.agents/AGENTS.md` section 6; this file is its detail. Read this when you must decide where a piece of information is written, which source answers a question, or whether something may go to memory. This is a convention shared by all CLIs. It is not a loader, a memory engine, a scheduler or a permission system. How to hand context to another agent is in `CONCURRENT-WORK.md`; how the memory server ranks and refreshes is in `HINDSIGHT-RUNTIME.md`.

## 1. Owner of each kind of information

| Information | Where it lives | Who writes it | How to read it |
| --- | --- | --- | --- |
| The active conversation, reasoning, a resumable session, and notes that matter only to that one CLI | The CLI's own session store and private memory | The CLI | Resume only the exact session you need. Never import another agent's transcript |
| Owner, authority, holds, next action, task metrics, verification, the brief, and for a task with several work threads the board | The task's `STATE.md`, `TASK.md`, `STRATEGY.md`, and the board file STATE names | The assigned owner, replacing old values | On entry, after a reset, at a handoff, and before an effect when it changed |
| Implemented behaviour, schemas, APIs, tests, build instructions, KPI definitions | The owning repository's source and docs | The implementation owner, through review | Read the current revision. Do not rely on a remembered implementation |
| Durable decisions and their reasons, lessons, corrections, scoped user preferences, explained terms | The shared Hindsight bank `coding-agent::shared` | An authorized owner or curator through the official HTTP API or client; the MCP tools only for simple notes and initiatives | Search pages, read the selected ones, recall exact evidence. Reflect only when those leave reasoning unresolved |
| Current operational objects, observations, KPI values and time series, policies, actions, recovery state | The domain or task owner today; `ontology-platform` when its admission is implemented | The trusted domain owner under the task's authority | A fresh authorized read with revision, scope, coverage and freshness. Memory is only a pointer to it |

- A decision another agent needs must not stay only in one CLI's private memory: write it in the repository contract or in an authorized, sourced Hindsight document.
- Session transcripts are short-term. Automatic retention of whole sessions into Hindsight (`retainSessions` in the integration settings) stays off (reason: `HINDSIGHT-RUNTIME.md`, "Session path on this host").
- A KPI and its target are set by the Lead from the user's words and recorded in the task's strategy; the user corrects them. A measured KPI value is an operational fact and stays with its task or domain owner. Do not turn a strategy document into a second execution table.

## 2. Which source answers a question

| Question | Authority |
| --- | --- |
| What may I do now, and what is next? | The user's current instruction, `/root/.agents/AGENTS.md`, the task's STATE. Memory is history |
| What did the user decide, and why? | The dated source decision, newer corrections and the current task authority. A generated page is navigation |
| What does the program implement? | Current source at a Git revision, compiler and runtime output, tests |
| What does a KPI mean? | The definition in the owning repository or the task's strategy; the target in the task's strategy; values from the native measurement. What the words KPI, metric and guardrail mean: `/root/.agents/skills/strategy-dialogue/references/measurement.md` |
| What does a word in an instruction or in the user's message mean? | `/root/.agents/AGENTS.md` section 8, then `GLOSSARY.md` |
| What does an operational object say now? | A fresh authorized domain read |
| May a caller read, derive or act? | The domain's authenticated policy and the task grant. Neither memory nor a tag grants access |
| Did an effect happen, and is a retry safe? | Reconciliation against the authoritative target. Neither prose nor a rollback settles it |
| What was explained to the user? | Dated glossary evidence. An explanation does not show that the user mastered the term |

When memory and current source disagree, the source wins for current truth. Keep the dated history and write a scoped Correction; do not rewrite one store to match the other. Unknown or stale evidence stays labelled unknown or stale, even when a page reports `is_stale:false`.

## 3. Reading memory

- When to search is in `/root/.agents/AGENTS.md` section 6. Do not search on every tool call; read only the selected pages.
- If the memory tool is unavailable or disabled for your task, say so and work from the task's files. Do not start a client or change global configuration to get it back.

## 4. Writing memory

- A skill or a visible tool is not a write grant. Writing needs the task's authority. A no-write task does not ingest, PATCH, refresh or delete.
- Structured records (incidents, lessons, decisions, Corrections, profile, glossary, strategy, KPI definition) go through the official HTTP API or client with an explicit stable `document_id`, then a complete-tag-set PATCH, then a GET readback. The exact sequence, endpoint and token-file location: `/root/.agents/skills/lessons-loop/references/hindsight-api.md`.
- MCP `ingest_document(title, content)` derives its ID from the title and takes no ID, tags or frontmatter: use it only for simple unstructured notes. `capture_initiative` is for an approved initiative and reuses its page when the scope changes.
- A pinned original is never rewritten: link a Correction or a new version. A mutable record is replaced under the same ID with its complete body, after saving the previous body. A complete-tag-set PATCH replaces all tags and can requeue related sources. Document history is not a text backup.
- An acknowledgement or a queued refresh shows nothing about the outcome. Read back the document, the extraction, the page content and the recall separately. Reconcile an uncertain write before retrying.

## 5. Tags and derived pages

- Use real provenance: platform `agent`, `education` or `ontology`, plus the repository. A module carries its owner's platform. Keep genuine `source:*`, `harness:*`, `project:*` and historical tags; never imitate a plugin's attribution.
- A tag routes a topic. It is not privacy and not permission. A strict tagged scope can miss shared untagged observations.
- The pages Lessons ledger, Structural risk review, User glossary, and Conventions and patterns are proposed views; do not assume they are installed. A curator changes them through the official page configuration. Never ingest a generated page as new evidence.

## 6. What never goes to memory

Write each fact once, at its owner. Do not retain in Hindsight: raw student, exam or lesson records; operational rows; action receipts or journals; KPI measurements or time series; full STATE copies; pane, model or quota assignments; CI logs; transcripts; copies of executable schemas; a second current-state table. Keep the detailed artifact at its owner and retain only a permitted locator or hash and a reusable lesson without student data. Never retain credentials. Never infer a user's ability or a broad permission.

One exception, by the user's decision of 2026-11-06: the Academy lesson archive. The academy server removes old rows, so the designated lesson owner archives dated teacher statements and source observations in Hindsight under the `academy-practical-protocol` archive-writer contract. The locator is `education-platform` `content/skills/academy-student-context/references/lesson-archive.md`. The archive excludes credentials, answer-key literals and parent contact details. An archived observation is a dated snapshot, not live state: a fresh owner read replaces it only for the same fact and stage of the same occurrence while the server still shows the row, and an original source score does not reopen a later teacher-final completion.

## 7. Lessons and Ontology

- Lessons are promoted on independent occurrences of a verified mechanism, not on document count. A structural proposal goes through the Lead to the task STATE's `DIRECTION PROPOSALS` and to the user; no memory write enacts architecture or policy. Procedure: skill `lessons-loop`.
- `ontology-platform` owns source-qualified operational state, admission, named actions and recovery; Hindsight owns contextual decisions and lessons. Boundary document: `/home/palantirkc/platforms/ontology-platform/docs/hindsight-boundary.md`. There is no shared database, state replica or automatic promotion of prose into objects between them. Check what `ontology-platform` actually provides at its current revision before relying on it.
