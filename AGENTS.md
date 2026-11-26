# Working in agent-platform

What the repository is and who uses it: `README.md`. Start reading from `map.json`, which names each component, where a reader starts in it, its specification and its tests.

This repository holds the command `ap`, the shared instructions every coding CLI on this host loads, the specifications of the `ap` contracts, and the checks that hold them. Its users are the Lead, the workers and verifiers of a unit in this repository, and agents of every CLI that call `ap`.

`engine/` is the Ontology engine, a Cargo workspace of its own in this repository; its rules are in "The engine" below, and `ci/check-map.py` reads it with the rest of the tree. The engine in use is the one built from `engine/` of the same commit, named by the Git tree id of `engine/` (`deploy/README.md`, "An engine change"): the root `./check` takes that engine and runs the engine's own steps (`./check engine`) when it had to build it.

`host/` is the host's configuration: the policy `/etc/agent-platform/policy.json` that `ap deliver` installs from `host/etc/` of the commit it delivers (`specs/ap/deliver.md` R71 to R74), so that a new member of the policy and the code that reads it are one pull request; the recorded copies of host files, which the watch compares; and the system Git configuration, which the Lead installs by hand. It is a protected tree; its own rules (a record is refreshed only from the host, the login path changes only by the user's procedure) are in `host/README.md`, and `./check` runs its steps without a Rust build.

## The check

`./check` at the repository root, until it prints `check: ok`. The CI job `rust` runs the same file and is the required check of `main`. `./check content` runs only the steps without a Rust build, until it prints `check: ok (content)`; it is enough for a change of `content/` or of documents. The whole `./check` is for the rest. The script takes the build lock by itself (do not wrap it in `flock`) and builds into `target/` inside your worktree. `ap check <worktree>` runs it on the local PC when the host's policy names the PC and the PC is free, else here; `ap check <worktree> -- <command>` does the same for one cargo command (`specs/ap/check.md`). Do not push before it passes.

## The command `ap`

`ap --help` lists the verbs: `version`, `run`, `gate`, `review`, `unit`, `deliver`, `request`, `intent`, `status`, `quota`, `work`, `policy`, `watch`, `check`, `fn`, `route` and `clause`. Each verb is documented in `crates/ap/README.md`; the contracts are in `specs/ap/`. A kept verb's contract is its specification under `specs/ap/`, to which its README section routes. `ap` stays small: it keeps no registry of what Git, GitHub, Herdr or the file system already list, and it is not extended without a named user in a domain's work.

## Contracts you can break

| Contract | Where it is stated | What relies on it |
| --- | --- | --- |
| The schema of the Lead's records, `ontology/work.schema.json` | `ontology/README.md`, the schema's text by version: its value types, among them `nonempty_text`, which refuses `""` with `invalid_parameter` for the string parameters of the version 9 actions, while the actions of versions 10 and 11 refuse an empty string by a `<parameter>_empty` criterion with a rehearsal row (not every string parameter), and what the store does not check; the check `ont schema check ontology/work.schema.json`; `ontology/rehearsal.sh`, which installs the schema into a disposable store and asserts every action answer that `ap` parses | `ap` and every reader of the store |
| The engine revision, the Git tree id of `engine/` | `specs/ap/deliver.md` R66; a change of `engine/` comes with a passing rehearsal (`ontology/rehearsal.sh`) and the answer shapes named in the pull request | `ap` parsing an engine answer; `ontology/rehearsal.sh` |
| The shared instructions, `content/` with `content/SHA256SUMS` | `content/SHA256SUMS`, the one list of the content | Every coding CLI that loads the installed instructions; `ci/check-content.py` |
| The rulesets and settings under `github/` | `github/repositories.json`, `github/ruleset-main.json`, `github/ruleset-tags.json`; compared and applied by `github/apply` | The repositories the files name; `./check` |
| The map, `map.json` | `map.json`; `ci/check-map.py` checks it | `ap unit page`, which prints one line per entry that meets a unit's write set, and `./check` |

A new action or value type of the work schema follows the latest version section of `ontology/README.md` (`## Version 11`) and adds a rehearsal row for each new refusal; it also adds the action or function to the grants (`specs/ap/roles.md` R2).

A pull request that adds, removes or renames a file under the coverage roots (`crates/`, `ci/`, `content/`, `deploy/`, `engine/`, `evals/`, `github/`, `host/`, `ontology/`, `scripts/`, `specs/`, `.github/`, `check`, `rust-toolchain.toml`, `Cargo.toml`, `Cargo.lock`), or shortens a file below an entry's range, changes `map.json` in the same pull request; a removed root also changes the root list in `check`. A file added to or removed from `content/` is added to or removed from `content/SHA256SUMS`; every changed file gets its new checksum. Never edit the installed copy of the content.

## Rules of this repository

- Work in a Git worktree of your task on a branch from `origin/main`, never in the main checkout.
- No model name, date, version, session or pane id in `content/AGENTS.md` or in a skill's frontmatter.
- No credential and no student data anywhere.

## Pull request

- One pull request per unit of work, able to merge alone. Its description is your plan and your proof. Never force-push. A repair is a new commit.
- The required check is `rust`; the required status is `lead-review`. Review policy: `REVIEW.md`.
- `ap` and `content/` are installed by `ap deliver` (its timer) after the merge; the `scripts/install-ap.sh` and `scripts/install-content.sh` files are delivery's own steps. A merged change is not an installed one.

## The engine

`engine/` is one generic engine, no domain knowledge. A domain describes its world in a schema file kept in the domain's own repository (object types with typed properties, link types, action types with submission criteria, views); the engine stores observations of those objects, answers reads over them and applies actions. What it holds: `engine/README.md`; its components are the entries `engine-*` of `map.json`.

- Agents of every CLI on this host, through the installed command `ont` and its MCP server.
- This repository: its schema and the command `ap`, which calls the installed `ont`.
- Domain crates in other repositories, which pin this repository by Git revision and link `ont`, `ont-core` and `ont-store` as libraries.

The engine is generic: a change that names a domain (a lesson, a student, a packet) belongs in that domain's schema or repository, not in `engine/`. The engine changes only for a named use: say in the pull request which consumer needs it and which real run will show it. Its steps of the check (`./check engine`: rustfmt, clippy and the tests of its workspace, `engine/scripts/check-spec-trace.py`, `engine/scripts/test-install-ont.sh`) run inside `./check` when the engine had to be built, and in the CI job `test` when `engine/` changed. `engine/rust-toolchain.toml` names the root's channel: `engine/scripts/install-ont.sh` builds from `engine/` alone.

| Contract | Where it is stated | What relies on it |
| --- | --- | --- |
| The schema language | `engine/specs/ont/schema-language.md` | Every installed schema |
| The JSON answers of each command and of the MCP tools | `engine/specs/ont/` | `ap`, every agent that reads an answer |
| The store format | `engine/specs/ont/store.md` | Every store file on the host; a format change needs an upgrade path and a design note before the build |
| The library boundary (`ont`, `ont-core`, `ont-store` as dependencies) | The crates' public items | Domain crates that pin a revision of this repository |

A change of one of these says so in the pull request and comes with the test that fails on the old behaviour.

Not in the engine: no daemon, no scheduler, no second copy of the action log, no domain parser. `ont serve` is the single tailnet listener exception to "No daemon". No student data, credentials or host paths of private data in source, tests or fixtures: fixtures are synthetic.

## Why Rust

Code with a real effect, the authoritative state and the commands every CLI calls are typed Rust. Errors become compiler errors, and compiler errors are a better feedback loop for agents than a style guide (Bun's account of its rewrite in Rust). The limit is stated by GitHub's port of the Copilot runtime to Rust: most diagnostics were ordinary wiring that any static typing catches, and every regression compiled. So the weight is on tests that hold the specifications and on a protected check, not on the compiler. Shell stays for installers and probes whose shape is still moving; nothing is ported for its own sake.
