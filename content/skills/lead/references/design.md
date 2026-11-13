# Design: the target structure of this host, the lens file and the design note

The Lead reads this before it writes a lens file, a packet or a design note. A worker reads it only when its packet lists it.

## Target structure of this host

One codebase on Rust and one Ontology. "Ontology" is Palantir's sense: object types with typed properties, links, and action types that are the only way to change an object (parameters, submission criteria, rules), read by search, traversal, aggregation and views.

| Layer | What it is on this host | Authoritative for |
| --- | --- | --- |
| 0 Sources | External services, Git and GitHub, long-term memory, the host's services | Their own facts. Code, plans and reviews: Git and the pull request |
| 1 Adapters | One thin Rust crate per external system that emits records | Nothing: they translate |
| 2 Engine | `ontology-platform`: schema language, store, reads, actions, ingest, the command and its MCP server | The meaning of a schema. It knows no domain |
| 3 Stores | One file per data class and authority. A derived store is rebuilt from its sources; an authoritative store is written by actions | Derived: nothing, delete and rebuild. Authoritative: what a domain decided |
| 4 Surface | Rust types generated from each schema, for services; the engine's generic tools, rendered views and one refresh per domain, for agents | The contract between a schema and its users |
| 5 Harness | `agent-platform`: the `ap` commands, the shared instructions; per repository one instruction page, one check command, a review policy, the CI job | Nothing operational |

## Rules a unit is checked against

- R1 One implementation. When the engine, a crate, a contract or a service already holds a mechanism, the new code consumes it. A cited contract is pinned and its vectors run in the consumer's tests.
- R2 One authority per fact. No mirror, and no registry of what Git, the hosting provider or the file system already lists. A derived store needs no migration, no backup and no erasure procedure: it is rebuilt.
- R3 One shape for an effect on a real target. An action records the attempt before the effect and refuses while an attempt on the same target is open; the owner performs the effect once, only after a newly applied attempt; an action records the outcome; an open attempt is closed only by a read of the target.
- R4 Every durable contract is checked by a machine: a type, a test, a schema check, a CI job. Where code calls a schema, it calls through types generated from that schema, never through names in strings.
- R5 Proportion. A measuring instrument is smaller than what it measures. A fact about a real service is measured on the real service. A mechanism is not built for an incident without harm.
- R6 Typed Rust for the engine, generated types, code with a real effect or authoritative state, and the commands every CLI calls. A probe, a browser driver and glue under about 200 lines that the CI job tests stay in the language that explores fastest. Nothing is ported tool by tool: a replacement ships to its users and deletes what it replaces in the same change.
- R7 First use. Every built thing names who first uses it (a route of the user's domain, an agent) and the real run that shows it. Work on the engine or the harness without such a user is not started.
- R8 One check. Each repository has one check command that an agent runs on the host and the CI job runs unchanged; the required check of the main branch is that job.

## Lens file (`LENS.md` in the task directory, at most two pages)

```markdown
# LENS — <task>
Written by the Lead (<session>, <time from the clock>) from source, Git and the store. Replaced, not appended; history is in Git.
## 1. Target structure     <only what this task adds to the table and rules above; per repository what a live route runs and who consumes what, with the command that measures it>
## 2. Open decisions       <each: the question, the options, what would decide it, by when>
```

## Design note (`DESIGN-<topic>.md` in the task directory)

Written only for a store format, the schema of an authoritative store, or the order of record and effect on a real target; attacked once before the build (prompt: `references/review-attack.md`). A design that fixes structure across repositories is evaluated by an Integration Review instead (prompt: `references/review-integration.md`).

Read before you write. Section 0 is filled first and holds only what a session read or ran TODAY, with the command or the file and line; when you have not read the code, a worker's page produces that list before the design exists. A design written between two dispatches, from the memory of a plan or of code, is the error this section stands against: its reviewer then does the reading at the price of the strongest model.

```markdown
# DESIGN: <topic>
Lead <session>, <time>. Rules this rests on: <R numbers>. Decision record: <topic in the store or STATE>.
0. Facts this design rests on: each with its command or file and line, and who read it today. The decisions of the user and the rows of the accepted plan on this topic, listed from the store, and every deviation from them.
1. Use: who uses the result first, by which route, shown by which real run.
2. Decision: the structure chosen; the alternative refused and why; what is not built.
3. Invariants, each with the executable that breaks on a wrong implementation; for an effect, the walk through a crash between every two steps and what each leaves.
4. Pull requests in order, each with what it makes usable and its estimate in changed lines.
5. Undo, and the point after which undo needs a migration.
6. Open points only the user can decide.
7. Least sure of: what the reviewer should break first.
```
