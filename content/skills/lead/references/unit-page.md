# Writing a unit's page

The page is the unit object (`crates/ap/README.md` "The unit object"), written by `ap unit write <key>` (`specs/ap/unit-write.md`) or by a page file with the typed block (`ap unit <page>`). `ap unit page <key>` renders it with the repository's `AGENTS.md`, its map and the role's frame (`crates/ap/src/unit/frames/`): the frame tells the worker and the verifier how they work, so a page restates no shared rule.

## 1. The fields

| Field | How it is written |
| --- | --- |
| `objective` | The state after the merge, as sentences that are true or false; who uses the result first and the real run that shows it |
| `interface` | The exact flags, members, codes and names its acceptance checks test, and what others build on (a command's ends and record; a schema; a port's signature; the shape of an answer), fixed by you after reading the code it constrains |
| `invariants` | Rules a test can break, worded as rules, never as a list of names |
| `acceptance` | Executables, each `{check, kind}`: `fails_on_base` (a test or command that fails on the base and passes after) or `invariant` (passes on both); `ap unit ready` runs them on the base first |
| `not_built` | The neighbouring things left out on purpose |
| `write_set` | The area: crates and directories as patterns (`dir/**`), never lines |
| `read_first` | First word `path` or `path:a-b`, relative to the repository root, of the base `ap unit ready` and `ap unit review` fetch (`read_first_missing`, `read_first_range`), then what it is for |
| `specs` | `<path>#R<n>` or `<path>`; a clause whose citing test is in the write set and whose file is not is named (`clause_missing`, `clause_unnamed`) |
| `notes` | What you run before or after the merge; no claim about the base (section 3) |
| `verifier_questions` | Things to run with inputs of the verifier's own, never a verdict |
| `class`, `implementer`, `verifier`, `estimate_lines`, `depends_on` | Section 4 |

When a script can do all of a unit, write the script. A page states the objective, the interface and executable acceptance, and prescribes no implementation: write no types, functions or line numbers; the session that compiles and tests writes the plan. The implementing stage writes the README section and the tests before the code.

## 2. One good page (invented values)

```text
Unit status-holds: class plain, lane objective, agent-platform
Objective: `ap status --text` prints the count of active holds on its first line, so a Lead
  sees a hold before any effect; first use: the Lead's first `ap status` of the next turn.
Invariants: the JSON form of `ap status` is unchanged.
Acceptance:
  fails_on_base: cargo test --locked -p ap --test status holds_count_on_first_line
  invariant:     cargo test --locked -p ap --test status json_form
Write set: crates/ap/src/status.rs, crates/ap/tests/status.rs, crates/ap/README.md
Implementer: claude, sonnet, high. Verifier: claude, opus, xhigh. Estimate: 60 lines.
```

## 3. The defects that caused stops

- A claim about the base that no machine ran: the worker builds on it and stops when the code disagrees. Make it an `invariant` check or leave it out.
- A contract in prose: every durable contract is checked by a machine (a type, a test, a schema check, a CI job); a consumer pins the contract it cites and runs the contract's vectors in its own tests. Where code calls a schema, it calls through types generated from that schema, never through names in strings.
- An acceptance check that cannot fail: it passes on the base, or it tests nothing the page changes. `ap unit ready` refuses the first; read each check for the second.
- An invariant that lists by name the existing tests allowed to change: write "assertions on the value this page changes may change, each named in the pull request".
- A write set that leaves out a file the rules need, or a requirement from an earlier review that needs a file outside it: widen it, or order a unit before it.

## 4. Size, class and tiers

- A unit is one pull request that can merge alone, about 600 changed lines outside tests as the target; a larger change is split into pull requests. When the delivered size passes one and a half times the estimate, stop adding and find the one choice that causes it.
- Class: `plain`; `contract` (changes a specification, a schema, a command's members, a store format, a check or a workflow); `effect` (acts on a real target outside a pull request: the host's configuration, GitHub settings, data). The lane `small` takes only `plain`; changing a check is a `contract` unit (`protected_path_changed`).
- A required check or a branch rule is changed only by you, through `github/apply` (class `effect`), after the job passes on main.
- Tiers: the verifier is never below the implementer; name the model on every start, and the effort where the CLI has a flag; a subagent of your own session is started with an explicit model. A Claude stage gets a compaction window above the peak of its kind of stage (the account's window, or `compact_window` where the request takes one), so that it never compacts; a stage whose session record holds a `compact_boundary` event is a finding. The cost of a route and the implementer's tier: `evals/readout.py`, run from a checkout at `origin/main`.
- A page is large (the user's rule) at an estimate of 500 changed lines outside tests, or 2,000 removed. Its implementer is Opus at effort high for Rust a verb runs with a real effect, the store's schema, a contract other code builds on, or delivery; it is `muse-wonder-1.0` for content, maps, specifications, removals and plain code. Its bound is four hours (`timeout_seconds` 14400).
- Reviewers: a `contract` or `effect` page by Opus at effort high; a plan by Opus, or by `gemini-4-argon-high` on the user's word; a design attack (`references/judging.md` section 1) by Fable through `ap run`. A reviewer's model must be in the policy's `reviews.models` (a change of it is class `effect`). A later look resumes the reviewer's session (`resume`, at most three looks, `specs/ap/unit-review.md` R8) or gives a new one the earlier reports (`earlier`).

## 5. The allowed models

The host's policy file (member `models`) is the authority: `ap run` refuses any other model (`model_not_allowed`), and every `muse-spark-*` and `*-contributor` model.

| CLI | Model | What it is for |
| --- | --- | --- |
| Claude Code | Fable | Design and plan work; the design attack |
| Claude Code | Opus | The Lead; verifiers; reviewers of pages and plans; implementers of large pages (section 4) |
| Claude Code | Sonnet | Implementers of pages under the bar: plain code, small contracts |
| Claude Code | Haiku | Mechanical work; a weak reader in the suites |
| Muse Code | `muse-wonder-1.0` | Between Opus and Sonnet: large pages (section 4); contributor mode off, set by `ap run` |
| Muse Code | `muse-ember-1.0` | About Haiku's level: mechanical work, a weak reader |
| Antigravity | `gemini-4-hydrogen-<effort>` (Hydrogen) | Between Opus and Sonnet; the effort is part of the model's name |
| Antigravity | `gemini-4-neon-<effort>` (Neon) | A reviewer on the user's word; the effort is part of the name |
| Antigravity | `gemini-4-argon-high` (Argon) | A reviewer of a plan or a design on the user's word |
| Codex | none | Paused: no model of it is in the policy |

## 6. The request forms

Where a form here differs, its specification holds.

- `ap unit add|amend|ready|drop < request.json`: the op is the first argument, never a member (`specs/ap/unit.md` Interface).
- `ap unit review < request.json`; `ap unit run <key>`; `ap unit approve|reject < request.json` (`specs/ap/unit-review.md`, `specs/ap/unit-run.md`).
- `ap request < request.json` and `ap intent < request.json`: the op is the member `op` (R2 of `specs/ap/request.md` and `specs/ap/intent.md`).
- `ont action validate`: the body of `ont action apply`; an `op`, where given, is `"op":"validate_action"` (`ont help`).
- Bounds: `specs/ap/unit.md` R7 to R14.
