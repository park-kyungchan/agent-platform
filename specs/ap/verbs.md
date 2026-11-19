# The verbs of `ap`: the list, each verb's help, the version and the unknown verb

Purpose: an agent of any CLI learns from `ap` itself which verbs it has, the request form of each and where its contract is, and an argument list `ap` does not know is refused with one fixed code, so that a caller never mistakes a verb that is gone for one that ran.

## Requirements

- R1: `ap --help` and `ap -h`, each the one argument, print the same text on standard output and exit 0: the line `Usage: ap <verb> [< request.json]`, the line `Verbs; the section is of crates/ap/README.md:`, then one line per verb the command dispatches, `  <verb>: ap <form> (README "<heading>")`, where `<form>` is the verb's request form and `<heading>` the `##` heading of `crates/ap/README.md` whose section routes to the verb's contract. Every verb the dispatch knows is listed once, and every heading named is a `##` heading of that README.
- R2: `ap <verb> --help`, exactly these two arguments, prints that verb's own usage on standard output and exits 0, for every listed verb; `version`, `accept` and `row`, which have no usage of their own, answer with their line of R1's list.
- R3: `ap version` and `ap --version`, each the one argument, print one line `ap <Cargo package version> <40-hex commit>[-dirty]` and exit 0, with nothing on standard error; the commit is `unknown` when Git was unavailable at build time, and it is the commit of the tree the binary was built from (with `-dirty` when that tree had changes).
- R4: Any other argument list is refused before any effect: no argument, an argument `ap` dispatches no verb for (the earlier installer's verbs `install`, `doctor` and `render` among them: they were removed and are refused like any unknown verb), `--help`, `-h` or `--version` with a further argument, and `version` with a further argument other than the one `--help` of R2 (a listed verb with other arguments is that verb's to refuse). The refusal prints `{"error":{"code":"usage","field":"args"}}` and a newline on standard error, nothing on standard output, and exits 2.
- R5: A failed write of the answer of R1 to R3 to standard output exits 4 with `{"error":{"code":"io_error","field":"stdout"}}` on standard error.

## Interface

`ap --help`, `ap -h`, `ap <verb> --help`, `ap version`, `ap --version`. The verbs and their contracts:

| Verb | Contract |
| --- | --- |
| `version` | this file |
| `run` | `specs/ap/run.md`, with `specs/ap/run-limits.md`, `specs/ap/run-object.md` and `specs/ap/policy.md` |
| `unit` (the store forms `add`, `amend`, `ready`, `drop`, `page`, `review`, `run`, `approve`, `reject`) | `specs/ap/unit.md`, `specs/ap/unit-review.md`, `specs/ap/unit-run.md` |
| `deliver` | `specs/ap/deliver.md` |
| `request` | `specs/ap/request.md` |
| `intent` | `specs/ap/intent.md` |
| `status` | `specs/ap/status.md` |
| `quota` | `specs/ap/quota.md` |
| `work` | `specs/ap/work.md` |
| `policy` | `specs/ap/policy.md` R12 to R16 |
| `watch` | `specs/ap/watch.md` |
| `gate`, `review`, `accept`, `backlog`, `state`, `row`, and `unit` with a page or a request on standard input | their sections of `crates/ap/README.md` (verbs to be removed; no specification) |

Each verb's own refusals and exits are in its contract; this file holds only the argument lists above that reach no verb.

## Not included

- The revision is derived from the enclosing Git repository at build time: a source tarball nested inside another Git repository reports the outer repository's revision.
- The install of `ap` (`specs/ap/deliver.md`; `scripts/install-ap.sh` is the step it calls) and a change of `ap --help`, whose lines still name the README's headings.
