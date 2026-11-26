# Context governance: which context to load, from which one home

Read this when you must decide which context to load for a task, above all a kind of task you have not met; which source answers a question; who writes a piece of information; or what may go to a long-term memory. Where each kind of information is written is the table of section 6 below. How to hand context to another agent: `CONCURRENT-WORK.md`.

## 1. The order of loading

Stop at the first step that answers; each step names the next one.

1. `ap status --brief` in the task's directory: its intents, what is next, the units, the holds (`/root/.agents/AGENTS.md` section 3).
2. The packet's `Read first:` line (full paths, a passage as `path:a-b`), when you were given one: read exactly that before anything it does not name.
3. The one reference or skill whose first lines name your situation, in the directories of `/root/.agents/AGENTS.md` section 8.
4. The area that owns the thing: `CODEBASE-ORIENTATION.md` sections 1 and 2; then that repository's `AGENTS.md` at `origin/main` (`git -C /home/palantirkc/platforms/<repo> show origin/main:AGENTS.md`, after `git -C /home/palantirkc/platforms/<repo> fetch -q origin`); then the start file the table names.
5. A search in that repository only, at `origin/main`: `git -C /home/palantirkc/platforms/<repo> grep -n '<words>' origin/main -- <directory>`; then read the lines it found.
6. A question of design (the life cycle, an Ontology store, Rust, Spec-Driven Development, context governance itself): `OFFICIAL-DOCS.md` section 4; then the corpus catalog `/home/palantirkc/external/reference-docs/llms.txt`; then `grep -rn '<words>' /home/palantirkc/external/reference-docs/docs/`.

## 2. How to read without waste

- Read by line range: `sed -n '<a>,<b>p' <file>`. A file over about 300 lines, or with very long lines, is searched first (`grep -n`) and read where the search lands. A directory is listed, never read whole.
- Ask a command for a fact instead of reading a file that copies it: `ap status --brief`, `ap status --term <word>`, the reads of `ONTOLOGY-USE.md` section 2, `ont describe`, `ont action list`, a command's `--help`.
- Read a user's row or decision with its words, never from its key alone.
- A map or an index names where to read; it is not the answer. Cite the file it points to, not the map.
- A tool result too large to show means the read was too wide: narrow it with a line range, a `grep` or a command's own filter instead of paging through it.

## 3. Copies that are not current

| Copy | Why it misleads | Read instead |
| --- | --- | --- |
| The files of a main checkout, `/home/palantirkc/platforms/<repo>/<path>` | It can lag `origin/main` and hold someone's uncommitted edit | `git -C /home/palantirkc/platforms/<repo> show origin/main:<path>`, or a task worktree |
| The store's objects of the older generation (`ONTOLOGY-USE.md`) | Index copies of files whose refresh stopped | The file itself, in Git or in the task directory |
| `text/` and `sources/` of `/home/palantirkc/external/reference-docs/` | The same text with other line numbers, and raw downloads | `docs/`, through the rows of `OFFICIAL-DOCS.md` section 4 |
| A path an older file names that no longer exists | It was retired or archived (`/home/palantirkc/archive/INDEX.md`); an archive carries no current rule | `ap status --brief` in the task's directory |
| Instructions in another CLI's home, or a skill copied instead of linked | A second copy drifts from its source | `/root/.agents/` |
| A file that says it was generated | A snapshot of its source at one revision | The source it names |

When two copies disagree, current source wins. Name the stale copy in your report so that it can be taken off the path.

## 4. Routing that every model follows

A weak model routes as well as a strong one only where a route asks for no judgment.

- The strong model routes once: the Lead writes a packet's `Read first:` line as full paths and `path:a-b`, each with what it is for, so that a stage reads instead of searching. Most of what a stage reads before its first edit is code and records, not instructions.
- At most three steps lead from `AGENTS.md` to a line: a reference in a directory its section 8 names, a table of the reference, a `path:a-b`.
- Paths are written in full, never shortened or elided; a table is keyed by the situation, in the words an assignment uses.
- A stale copy is taken off the path, not warned about.
- A change of a route is accepted on the routing suite (`lifecycle-platform` `evals/README.md`, section "The routing suite"); its cases in the repository, `evals/routing/cases.json`, cover how each CLI runs without a terminal UI, not the routes of this file or of `OFFICIAL-DOCS.md` section 4, so such a change also names the reads it was tried on.

## 5. A task kind no row covers

Follow section 1 to its end. When you needed many reads (more than about fifteen) before your first effect, your report names the route you found, as `path:a-b`, with the situation in your assignment's words. The Lead adds the row (to a reference or the unit page form of the skill `lead`) and a case to the routing suite: a mistake made twice becomes a row or a check, not one more sentence.

## 6. Who writes each kind, and how it is read

The kinds of information, with a session's own record:

| Kind | Who writes it | How to read it |
| --- | --- | --- |
| A session's conversation and a CLI's private memory | That CLI | Resume only the exact session you need. Never import another agent's transcript |
| The state of the work: requests, intents, units, runs, findings (lessons among them), holds, principles, KPIs | The Lead, through `ap request`, `ap intent`, `ap unit` and `ont action apply` (`ONTOLOGY-USE.md`) | `ap status --brief`; on entry, after a reset and before an effect |
| A rule or contract: source, schemas, tests, KPI definitions | The owner of the implementation, through review | Read the current revision. Do not rely on a remembered implementation |
| An operational fact: deployed state, a domain record, an action and its result | The owner of the domain | A fresh authorized read |
| Narrative knowledge found by meaning | Nobody yet: section 8 | — |

- A decision another agent needs must not stay only in one CLI's private memory.
- Session transcripts are short-term: a transcript carries dated status that later competes with corrections.
- A KPI and its target are set by the Lead from the user's words and recorded as a `kpi` of the store, never in a second place; the user corrects them.

## 7. Which source answers a question

| Question | Authority |
| --- | --- |
| What may I do now, and what is next? | The user's current instruction, `/root/.agents/AGENTS.md`, `ap status --brief`. Memory is history |
| What did the user decide, and why? | The dated source decision, newer corrections and the current task authority. A generated page is navigation |
| What does the program implement? | Current source at a Git revision, compiler and runtime output, tests |
| What does a KPI mean? | The store's `kpi`: its definition, target and latest value (`ONTOLOGY-USE.md`); the value as that definition computes it. How a KPI, a metric or a guardrail is written as an intent's criterion: `/root/.agents/skills/lead/references/interview.md` section 3 |
| What does a word in an instruction or in the user's message mean? | `ap status --term <word>` (`/root/.agents/AGENTS.md` section 3); a word the store has no entry for, the one file that needs it |
| What does an operational object say now? | A fresh authorized domain read |
| May a caller read, derive or act? | The domain's authenticated policy and the task grant. Neither memory nor a tag grants access |
| Did an effect happen, and is a retry safe? | Reconciliation against the authoritative target. Neither prose nor a rollback settles it |
| What was explained to the user? | The reading of the request that holds it (`ap request`). An explanation does not show that the user mastered the term |

When memory and current source disagree, the source wins for current truth. Keep the dated history and write a scoped Correction; do not rewrite one store to match the other. Unknown or stale evidence stays labelled unknown or stale, even when a page reports `is_stale:false`.

## 8. Long-term memory

- What has a key, a state or an owner is an object of the store. What a query or a check can answer is stored nowhere else.
- A later long-term memory holds only narrative knowledge found by meaning: how a kind of problem was solved, the tradecraft of a domain. Each record names the store objects it is about. A record is written only by a deliberate promotion from a closed finding or a closed task, never as a side effect. It holds no credential and nothing that goes stale.
- Until the user deploys it, no memory tool is called (`/root/.agents/AGENTS.md` section 6).
- The exception of 2026-11-06 for the Academy lesson archive is superseded by the user's word of 2026-11-24 (request `act-rq-6616f9477a559517d0349c4e22cb437d`): the teacher's dated statements and the source observations are objects of the store's Academy domain, and what that exception kept out (credentials, answer-key literals, parent contacts) stays out of them.

## 9. Lessons and Ontology

- A lesson is a finding of kind `lesson` (`ONTOLOGY-USE.md` section 4), promoted on independent occurrences of a verified mechanism, not on document count. A structural proposal goes to the Lead and to the user; no memory write enacts architecture or policy.
- The engine (`lifecycle-platform` `engine/`) owns source-qualified operational state, admission, named actions and recovery; Git owns engineering originals. There is no automatic promotion of prose into objects. Check what the engine actually provides at its current revision before relying on it.
