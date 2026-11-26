# The Ontology engine (`engine/`)

The Ontology engine of this host: one generic engine, no domain knowledge. A domain describes its world in a schema file kept in the domain's own repository (object types with typed properties, link types, action types with submission criteria, views); the engine stores observations of those objects, answers reads over them and applies actions.

## Crates

| Crate | What it is |
| --- | --- |
| `ont-core` | The schema language and the data model: identity, observations with their knowledge state, coverage, freshness and lineage, scopes, grants |
| `ont-store` | The store: one SQLite file per store, atomic appends, snapshot reads |
| `ont` | The command `ont` and its library: `schema check`, `schema install`, `schema gen`, `describe`, `get`, `search`, `around`, `aggregate`, `render`, `index`, `action list`, `action validate`, `action apply`, `backup` and `mcp` (the same reads and actions as an MCP server over standard input and output) |
| `ont-gen` | The generator of a typed Rust module from one schema file: a type per object type, value type, action type and function, both directions of every link, and call functions that send each request through an engine the consumer implements |

The reads and the action commands (`describe`, `get`, `search`, `around`, `aggregate`, `render`, `action ...`) read one JSON request on standard input and print one JSON answer; `schema check`, `schema install`, `schema gen`, `index` and `backup` take arguments only; `mcp` speaks the MCP protocol. `ont --help` lists every form and the request fields. The contract of each command is in [specs/ont/](specs/ont/).

## Who uses it

- Agents of every CLI on this host, through the installed command `ont` and its MCP server.
- `agent-platform`: its schema `ontology/work.schema.json` and the command `ap`, which calls the installed `ont`.
- Domain crates in other repositories, which pin this repository by Git revision and link `ont`, `ont-core` and `ont-store` as libraries. The first is `edu-academy-ontology` of `education-platform`, on a branch that is not merged yet.
- A consumer may generate the Rust types of its schema with `ont-gen` in its build script, or by running the pinned engine's `ont schema gen FILE` there (`specs/ont/gen.md`), pinned at the revision it pins the engine at.

A change of an answer's shape or of the schema language is a change of a contract those users rely on.

## Check, install

- The repository's `./check` is the one check; `./check engine` runs the engine's steps: format, lints as errors, all tests of this workspace, the trace of `specs/ont/`, the test of the install script. `./check` runs them when it had to build the engine, and the CI job `test` when `engine/` changed.
- `scripts/install-ont.sh install <commit>` builds and installs the command from `engine/` of a commit of the repository around it, building in a new 0700 directory under `/var/lib/ontology-platform/build` (never under `/tmp`), and refuses when a component of that path is a link or writable by an account other than root; `check` compares the installed files with the record that install wrote (sha256 and commit); `rollback` restores the previous one. It installs no schema and runs no index.

## What else is in the tree

- `conformance/fixtures/`: synthetic fixtures that tests of `ont-core` and `ont-store` read.
- The repository's `AGENTS.md` ("The engine"): how an agent works on the engine. Its `REVIEW.md`: what blocks a pull request.
