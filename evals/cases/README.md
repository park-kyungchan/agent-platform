# evals/cases

`cases/` makes a gate of the routing and intake suites (`evals/README.md`) run only the cases a change can reach. Each case of the two suites is a `case` object of the store (`ontology/README.md`, "Version 20: eval cases") that names the Markdown files holding the facts of the case; `select` prints the cases a change serves, and `reproduce` shows, on a full gate that failed, that the same rule would have named the failing cases with fewer cases than the suites hold. No model is called and no network is used; Git is only read (`ls-tree`, `show`, `diff`, `rev-parse`); no text of a case and no host path of a case file enters a record, a measurement or this repository.

Words. A suite is `routing` or `intake`. A case's facts are its `must` facts (routing) or its `expect.missing` facts (intake). Compared as the suites compare: backticks removed, each run of white space made one space. The Academy rule is the function `is_academy` of `evals/batch/lib.py`: a path with a component beginning `education-` or `edu-`, or `evals` followed by `lesson`.

- `index --repository <owner/name>=<clone>... [--at <owner/name>=<rev>]... --cases <suite>=<file>... [--directory <dir>] [--store <path>]` writes the records and indexes the source `cases`.
- `lost --repository <owner/name>=<clone> --base <rev> --head <rev> [--also <owner/name>=<clone>]... [--accepted <file>]` lists the removed lines of a change that no kept text keeps.
- `select --repository <owner/name>=<clone> --base <rev> --head <rev> --cases <suite>=<file>... [--also <owner/name>=<clone>]... [--accepted <file>] [--directory <dir>] [--store <path>]` prints the cases to run.
- `reproduce --repository <owner/name>=<clone> --base <rev> --head <rev> --cases <suite>=<file>... --gate <file> [--also <owner/name>=<clone>]... [--record <task key>] [--store <path>]` tests the rule on a gate and records the measurement.

Each is run as `python3 -I -B evals/cases/<program> ...`; each prints JSON lines on standard output, keeps standard error empty, and refuses with one line `{"status":"<code>"}` and exit 2 before any effect. The standard library only; the programs load `lib.py` (the rules of the mapping and of the lost lines) and `evals/batch/lib.py` by their paths. The engine is `ONT_BIN` (default `ont`), with `--store` when one is given.

## The mapping

A file holds a fact when every string of the fact, compared as the suites compare, is in the file's text compared the same way. Over the tracked files ending `.md` of the repositories read (a path the Academy rule names is neither opened nor named): `serves` of a case is every `<owner/name>:<path>` that holds one of its facts. `all` is true when the case has no fact, when one of its facts is held by no file (`unhomed` above 0), or when its record would pass 60,000 bytes; `serves` is then empty, so the case serves every path. `map_digest` is the SHA-256 of the bytes of `evals/cases/lib.py`: a change of a rule changes it and asks for a new reproduction.

Every file of a repository is read from Git's objects: nothing is fetched, no checkout's files are read, nothing in a clone changes. Each repository is read at its `--at` revision, else `origin/main`.

## `index`

It writes `<dir>/<suite>.cases.jsonl` for each suite given, one record per case (`type` `case`, `key` the hex SHA-256 of `<suite>:<case id>`, `observed_at` the run's time in whole seconds, and the members of the type), each file under a temporary name in the directory and renamed, mode 0644; the directory is made with mode 0755 when absent; the file of a suite not given stays. A file of the case files is admitted as it is when it is inside this repository's `evals/`, else by its SHA-256 in `evals/<suite>/host-cases.sha256`; its record names it by the repository path or by the short name of that digest line, never by a host path. Then `ont index --source cases`.

`index` holds an exclusive `flock` on `<dir>/.lock` from before it writes a record file until the index has answered, and `select` holds it from before its index until it has read the objects back; a call that has waited 600 seconds for it refuses `busy` and writes nothing.

One line `{"status":"ok"|"index_failed","directory","suites":{<suite>:{"cases","all","unhomed"}},"index":{"status","code","appended","renewed","retracted"}}`. In `suites`, `cases` is the number of records, `all` the number with `all` true and `unhomed` the sum of their unhomed facts. Exit 0, or 3 when the source did not hold.

`--store` goes with `--directory`: `--store` without `--directory`, a `--directory` other than `/var/lib/agent-platform/cases` without `--store`, and `--store` with `--directory /var/lib/agent-platform/cases` are `usage`, so that a scratch store never feeds the host's directory and the host store never reads a scratch one. `reproduce` takes `--store` alone (it writes no record file); `lost` takes neither.

## `lost`

The files read: each path ending `.md` that `git diff --name-status <base> <head>` gives as modified, deleted or renamed (its old path). The lines read: every line of such a file at `<base>` that `git diff -U0 <base> <head>` removes, but a blank line, a fence line (three backticks or `~~~`) and a table rule (only `|`, `-`, `:` and white space). Every `git diff` runs with one `:(exclude)` pathspec for each path of the name-status list that the Academy rule names, so no line of such a path is read.

The kept text: every tracked file of the repository at `<head>`, and of each `--also` clone at `origin/main`, whose name ends `.md`, `.rs`, `.py`, `.sh`, `.toml`, `.json`, `.yml`, `.yaml`, `.service`, `.timer` or `.txt`. A section is the lines from one ATX heading (not inside a fence) to the next in a Markdown file, or a whole file of another kind. A line's text is the line compared as the suites compare, without its list markers (`-`, `*`, `+`, `<digits>.`, `>`) and table pipes; a file's text is its lines' texts joined by one space. A line's terms are its backticked spans and its words that hold a digit, `/`, `.`, `_`, `=` or `@` inside or begin with `--`; its words are its runs of four or more ASCII letters, in lower case, less the common English words of the list `COMMON` in `lib.py`.

A line is `kept` when its text is in the text of one kept file (a moved or rewrapped line is kept), or when every term and every word of it is in one section of one kept file; a line with no term and no word is `skipped`; every other line is lost, whether or not it names a term (the script of the first gate read only the lines that name a backticked term).

Output: one line per lost line `{"path","line","text","digest","accepted"}` (`line` is the number at `<base>`, `text` at most 300 bytes, `digest` the SHA-256 of the line's text), then `{"files","read","kept","skipped","lost","accepted"}`. `--accepted` names a file of digests, one a line, of lines the Lead read and drops on purpose (`accepted_unreadable` when it cannot be read or a line is not a digest). Exit 0 when no lost line is unaccepted, else 1.

## `select`

First the check of `lost`: while a lost line is not accepted it prints `lost`'s lines and `{"status":"lost_facts","lost":<n>}` (`n` the lines not accepted), exits 1, and indexes and selects nothing. Then the index, with the changed repository at `<base>` and each `--also` clone at `origin/main` (an `index_failed` prints the line of `index` and exits 3). Then it reads back the `case` objects of each suite given (`ont search`, every page) and the KPI named `case_map` (of any task; the one with the latest `observed_at`).

The mode is `subset` when the KPI's `value` begins `reproduced <map_digest>`, with the `map_digest` of those objects; else `full`. It is also `full` when the objects cannot be read in full, are not of one `map_digest`, or are not exactly the cases of the files given: nothing the store holds is trusted then. The change: each `<owner/name>:<path>` that `git diff --name-status <base> <head>` names (old and new path), and the lines the change removes or adds. In `subset` a case is selected when its `all` is true, when a path of its `serves` is changed, or when a removed or added line holds the last component of a path of its `serves` (the route to its home changed); in `full` every case is.

One line per suite `{"suite","mode","cases":[<ids>],"args":"--case <id> ..."}` (the ids in the order of the files; an empty selection has empty `args` and starts no reader), then `{"mode","map_digest","selected","of"}`; exit 0.

## `reproduce`

The mapping of the section above at `<base>`, computed without the store, and `select`'s `subset` selection for the change. The failures, read from `--gate` (the saved output of `evals/<suite>/run gate`): the cases in the `lost` of at least two readers whose `ok` is false, or, when only one reader's `ok` is false, that reader's `lost`. A case lost by one reader of several is within the noise of three samples (`evals/README.md`, "What the suite does not show"). `gate_invalid` when no reader has an `ok` and a list `lost`, or the file cannot be read.

`reproduced` is true when the failures are not empty, each of them is selected, and fewer cases are selected than given. One line `{"reproduced","map_digest","failures","selected","cases","missed":[<ids>]}` (counts, except `missed`); exit 0 when reproduced, else 1.

With `--record <task key>`: the task's full id from one `ont search` of `task`, then the KPI named `case_map` from one `ont search` of `kpi` by `eq` on `name`, kept when its `task` is that task; when there is none, `define_kpi` with a fixed `definition` naming this command and the value's form, request id `cases-kpi-<first 32 hex of the SHA-256 of "<task key>:case_map">`. The value is `reproduced <map_digest> failures <f> selected <s>/<n>` or `not_reproduced <map_digest> failures <f> selected <s>/<n>`; the evidence is `evals/cases/reproduce gate sha256:<first 16 hex of the gate file's SHA-256> <first 12 of base>..<first 12 of head>` (no path). When the KPI's `value` and `evidence` already equal these, nothing is applied and the line's `recorded` is `already`; otherwise `record_measurement` with them, `observed_at` the run's time to the second, request id `cases-<first 32 hex of the SHA-256 of the canonical JSON of value, evidence and observed_at>`, and `recorded` is `applied`. A refusal or a failure of the store under `--record` (also a task the store does not have, with `code` null) prints the result line, then `{"status":"record_failed","code":<the store's code, or null>}`, and exits 3. Without `--record` the line has no `recorded` and no store is read.

## Refusals

Before any effect, one line and exit 2: `usage` (an unknown flag, a missing or empty value, a repeated single flag, a suite that is not `routing` or `intake`, a name that is not `owner/name`, the combinations of `--store` and `--directory` above); `academy_path` (the Academy rule asked of every repository name and every clone, case file, `--directory`, `--gate` and `--accepted` path, first as given, then resolved one component at a time); `repo_unreadable` (a clone or a revision Git cannot read); `cases_unreadable`, `cases_digest_unknown`, `cases_invalid`, `case_id_twice` (the routing suite's refusals, for both suites, over the case files of a suite together); `accepted_unreadable`; `gate_invalid`. After a check and before the write: `busy`. `directory_unwritable` when the directory cannot be made or written; any other failure of a program is `internal_error`.

## The order for a content unit

1. `select` first, with the changed repository and its clones: it stops while a removed line has lost its fact, and otherwise prints the cases to run. Read each lost line; either restore the fact, or put its digest in the accepted file for a line dropped on purpose.
2. Start the readers of each suite with the `--case` arguments it printed (an empty selection starts none). In `full` mode, the whole suite.
3. After a full gate with a failure, `reproduce --record <task key>` on that change and gate: it measures whether the rule would have named the failing cases. A `reproduced` value for the current `map_digest` is what makes `select` print `subset`.

The real run: `reproduce --record` on the routing gate of the change 5317582..5732af6 of agent-platform (`/var/lib/agent-platform/evals/routing/gate-c3-pre/gate.json`) or on the next full gate that failed; then `select` for the next content unit prints `subset` with fewer cases than the suites hold.

## The undo

Remove `/var/lib/agent-platform/cases/*.cases.jsonl` and run `ont index --source cases`: the complete listing retracts every `case` object, and `select` prints `full`.

## What it does not show

- A file that holds a fact is not shown to be where a reader reads it: the mapping says where the strings are, not that the reader loads that file.
- The lost rule calls a fact reworded across two sections lost, and keeps a line whose words occur in one section by chance (a whole file of another kind is one section).
- Most intake cases have no fact (`expect.missing` is empty unless a case's order skips a stage), so they are `all` and every unit selects them.
- A reproduction on one change says the rule would have named that gate's failures; it is one sample, and a failure of one reader of three is within the noise.
- The rule names the cases a change serves by their facts; a change whose effect is to make a reader choose differently, with no fact in a changed file, is not reached by it.
