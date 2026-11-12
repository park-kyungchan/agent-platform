# Design: the rules of this host, the lens file and the design note

The Lead reads this before it writes a lens file, a design note or a packet that changes product code. A worker reads it only when its packet lists it.

## Target structure of this host

One codebase in Rust on one Ontology. "Ontology" is Palantir's sense: object types with typed properties, links, and action types that are the only way to change an object (parameters, submission criteria, rules), read by search, traversal, aggregation and views.

| Layer | Home | Rule |
| --- | --- | --- |
| Engine | `ontology-platform` | Generic. It knows no domain. It changes only for a named use by a domain or by the harness |
| Domain | One schema file in the domain's repository and a Rust service that embeds the engine as a library on its own private store | The state a domain decides on (what was attempted, what is pending, what is settled) is objects written by actions. Raw evidence of an external system stays in the domain's private files and is referenced |
| Harness | `agent-platform`: the `ap` commands, the work schema, the shared instructions | A step of the Lead that repeats is a command over the store |
| Memory | Long-term memory | Decisions with reasons, lessons, terms. No operational fact |

## Rules a design is checked against

- R1 One implementation. When the engine, a crate, a contract or a service already holds a mechanism, the new code consumes it. A cited contract is pinned and its vectors run in the consumer's tests.
- R2 One authority per fact. No mirror: a second copy that nothing must read is not built.
- R3 One shape for an effect on a real target. An action records the attempt before the effect and refuses while an attempt on the same target is open; the owner performs the effect once, only after a newly applied attempt; an action records the outcome; an open attempt is closed only by a read of the target.
- R4 Timing. The cheapest moment to move a state under the engine is before a production store of the old form exists. A deferral names what waiting costs.
- R5 Proportion. A measuring instrument is smaller than what it measures. A fact about a real service is measured on the real service, by the Lead where one run suffices.
- R6 New code is Rust in a platform crate. A script stays only as an exception the lens file names with its bound.
- R7 First use. Every built thing names who first uses it (a production route, an agent) and by which packet. With no named user, nothing further is built.

## Lens file (`LENS.md` in the task directory)

```markdown
# LENS — <task>
Written by the Lead (<session>, <time from the clock>) from source, Git and the store. Replaced, not appended.
## 1. Target structure           <only what this task adds to the table and rules above>
## 2. Measured structure         <per repository: lines by language and crate, what consumes what, what a live route runs; revisions>
## 3. Evaluation                 | # | Finding (evidence: path and line, command, measured value) | Disposition |
## 4. Design decisions           <each: the decision, the alternative refused and why, the order, the undo>
## 5. Design owed                <notes and decisions the Lead still has to write, in order>
```

## Design note (`DESIGN-<topic>.md` in the task directory)

```markdown
# DESIGN: <topic>
Lead <session>, <time>. Lens rules this rests on: <R numbers>. Decision record: <topic in the store or STATE>.
1. Use: who uses the result first, by which route, shown by which real run.
2. The code today: the items this changes and their callers, with path and line; what state exists and where.
3. Decision: the structure chosen; the alternative refused and why; what is not built.
4. Shape: types, functions, schema and actions to add or change, by file; the one authority of each fact.
5. Invariants; for an effect, the walk through a crash between every two steps and what each leaves.
6. Tests by guarantee: each test, the guarantee it holds, the wrong implementation it fails on. Existing tests that must pass unchanged.
7. Packets in order, each with its write set, its estimate in changed lines and what it makes usable.
8. Undo, and the point after which undo needs a migration.
9. Open points only the user can decide.
```

## Design block of a packet

```markdown
Design (the Lead's; a mismatch with the code is reported, not resolved by the worker):
- Change: <item, file:line> -> <what it becomes>
- State and authority: <where each fact lives>
- Invariant: <sentence>
- Callers searched: <command>; affected: <file:line list>; each one's fate
- Tests that must fail on the wrong implementation: <test -> guarantee -> the wrong implementation>
- Not built: <neighbouring things left out on purpose>
```
