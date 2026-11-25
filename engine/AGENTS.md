# Working in ontology-platform

What this repository is: `README.md`. Start reading from `map.json`, which names each component, where a reader starts in it, its specification and its tests.

## What it is and who uses it

One generic engine, no domain knowledge. A domain describes its world in a schema file kept in the domain's own repository (object types with typed properties, link types, action types with submission criteria, views); the engine stores observations of those objects, answers reads over them and applies actions.

- Agents of every CLI on this host, through the installed command `ont` and its MCP server.
- `agent-platform`: its schema and the command `ap`, which calls the installed `ont`.
- Domain crates in other repositories, which pin this repository by Git revision and link `ont`, `ont-core` and `ont-store` as libraries.

Work in a Git worktree of your task on a branch from `origin/main`, never in the main checkout. The engine is generic: a change that names a domain (a lesson, a student, a packet) belongs in that domain's schema or repository, not here. The engine changes only for a named use: say in the pull request which consumer needs it and which real run will show it.

## The check

`./check` at the repository root, until it prints `check: ok`. It is the same file the CI job `rust` runs. It takes the host's build lock by itself (do not wrap it in `flock`) and builds into `target/` inside your worktree. Do not push before it passes.

## Contracts you can break

| Contract | Where it is stated | What relies on it |
| --- | --- | --- |
| The schema language | `specs/ont/schema-language.md` | Every installed schema |
| The JSON answers of each command and of the MCP tools | `specs/ont/` | `ap` in `agent-platform`, every agent that reads an answer |
| The store format | `specs/ont/store.md` | Every store file on the host; a format change needs an upgrade path and a design note before the build |
| The library boundary (`ont`, `ont-core`, `ont-store` as dependencies) | The crates' public items | Domain crates that pin a revision of this repository |

A change of one of these says so in the pull request and comes with the test that fails on the old behaviour.

## Pull request

- One pull request per unit of work, able to merge alone. Its description is your plan: files, order, risks, proof.
- Never force-push. A repair is a new commit.
- The required check is `rust`. Review policy: `REVIEW.md`.
- Installation on the host is a separate step of the Lead (`scripts/install-ont.sh`); a merged change is not an installed one.

## Why Rust

Code with a real effect, the authoritative state and the commands every CLI calls are typed Rust. Errors become compiler errors, and compiler errors are a better feedback loop for agents than a style guide (Bun's account of its rewrite in Rust). The limit is stated by GitHub's port of the Copilot runtime to Rust: most diagnostics were ordinary wiring that any static typing catches, and every regression compiled. So the weight is on tests that hold the specifications and on a protected check, not on the compiler. Shell stays for installers and probes whose shape is still moving; nothing is ported for its own sake.

## Not here

No daemon, no scheduler, no second copy of the action log, no domain parser. `ont serve` is the single tailnet listener exception to "No daemon". No student data, credentials or host paths of private data in source, tests or fixtures: fixtures are synthetic.
