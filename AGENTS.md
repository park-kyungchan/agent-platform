# Working in agent-platform

Shared rules of this host: `/root/.agents/AGENTS.md` (its source is `content/AGENTS.md` here). This page adds only what is particular to this repository. What the repository is: `README.md`.

## What is here

| Path | What it is | Its check |
| --- | --- | --- |
| `content/` | The shared instructions every coding CLI of the host loads: `AGENTS.md`, `runtime-references/`, `skills/`. Installed to `/root/.agents` by the Lead | `ci/check-content.py`: checksums in `content/SHA256SUMS` and `crates/ap/src/native/content_inventory.rs`, routes, words the glossary replaces, limits |
| `crates/ap` | The command `ap`: `work start`, `work wait`, `work close`, `review`, `render`, `install`, `doctor`, `pane` | Its tests; `crates/ap/README.md` is the contract |
| `ontology/work.schema.json` | The schema of the Lead's own records in the Ontology store. Frozen: no new version without a decision of the Lead | `ontology/rehearsal.sh` installs it into a disposable store of the engine revision in `ontology/ENGINE-PIN` and asserts every action answer that `ap` parses |
| `memory/` | Originals of engineering memory records | `ci/check-memory-records.py` and its tests |
| `scripts/install-ap.sh` | Installs `ap` on the host from a commit on `main`; `check`, `rollback` | `scripts/test-install-ap.sh` |

## The check

`./check` at the repository root, until it prints `check: ok`. The CI job `rust` runs the same file and is the required check of `main`. `./check content` runs only the steps without a Rust build and is enough for a change of `content/`, `memory/` or documents. On the host the script takes the build lock by itself (do not wrap it in `flock`) and builds into `target/` inside your worktree.

## Rules of this repository

- Work in a Git worktree of your task on a branch from `origin/main`, never in the main checkout.
- `ap` stays small. It keeps no registry of what Git, GitHub, Herdr or the file system already list, and it is not extended without a named user in a domain's work.
- A file added to or removed from `content/` is added to or removed from `content/SHA256SUMS` and `content_inventory.rs` in the same order; every changed file gets its new checksum. Never edit the installed copy under `/root/.agents`.
- A change of the engine revision in `ontology/ENGINE-PIN` is a change of a contract between two repositories: it comes with a passing rehearsal and says in the pull request which answer shapes changed.
- No model name, date, version, session or pane id in `content/AGENTS.md` or in a skill's frontmatter. No credential and no student data anywhere.

## Pull request

One pull request per unit of work, able to merge alone; its description is your plan and your proof. Never force-push. Review policy: `REVIEW.md`. Installation on the host (`ap`, the content) is a separate step of the Lead: merged is not installed.
