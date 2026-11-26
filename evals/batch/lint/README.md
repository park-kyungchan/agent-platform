# The spec-lint job

Job J4 of the API credit, run by hand once a week: each specification file of the platform repositories read by a model for ambiguity, contradiction and gaps, through the Message Batches API. The two programs here only build requests and read results: they call no model, open no socket and hold no key. `ap api batch evals` (unit api-batch, policy unit api-policy) sends the batch. The first user is the Lead at the end of a week, who turns each finding into a spec amendment unit of class `contract` or dismisses it with a reason. The shared rules of the Academy, the key, the request and result lines and the output form are those of `evals/batch/README.md`; `lib.py` holds them, `evals/batch/evals/common.py` holds the counting of result lines and the finding pass, and `common.py` here holds what the two programs add.

## The builder

`python3 -I -B evals/batch/lint/build --repo <owner>/<name>=<clone> [--repo ...] --model <id> --out <dir> [--rev <rev>] [--results <file>]... [--max-tokens <n>]`

- Before any read: `academy_path` when a repository's name is the Academy's, or when the path of a clone, of `--out` or of a `--results` file is. `--rev` (default `origin/main`) is resolved once per clone to a commit; `repo_unreadable` (with the member `repository`) when the clone or the revision cannot be read. `--max-tokens` is an integer from 1024 to 8000, default 4000; anything else is `usage`.
- The files: every regular file whose path begins with `specs/` and ends with `.md`, read from Git's objects at that commit (`git ls-tree`, `git cat-file --batch`): no file of a checkout is read, nothing is fetched and nothing in a clone changes. Git is the only program started. A tracked path of the Academy is not opened and is counted `academy_skipped`. A file's rules: `R<digits>` of each line that `REQ_RE` of `ci/check-spec-trace.py` matches, a later line of the same rule in the file none; a test fails when the pattern is not that of the check.
- One request per file. `params` has exactly `model`, `max_tokens`, `system` (the text of `prompt.md`), `messages` (one `user` message whose content is one string: the repository, the path and the file whole between `<spec path="...">` and `</spec>`) and `output_config`, `{"format":{"type":"json_schema","schema":<verdict.schema.json>}}`. The `custom_id` is the key of `params`, so it does not change with the commit while the file does not. A file whose user text is over 300,000 bytes gets no request and the state `too_large`.
- `--results` (repeatable) names files of result lines: a key is settled when one of them holds a line of that key with `result.type` `succeeded` (a refusal and a cut answer are settled, an error is asked again at the next build). A file that cannot be read is `results_unreadable`; a line that is no result line is skipped and counted.
- Written into `--out` (mode 0700 when made; files 0600, each renamed): `requests.jsonl`, one line per requested file, and `files.json`, `{"job":"spec-lint","model","files":[{"key","repository","commit","path","rules":["R1"],"state"}]}`, by repository, then path, `state` one of `requested`, `settled`, `too_large`. The same commits, options and result files give byte-equal files.
- Standard output: `{"status":"ok","out","files","requested","settled","too_large","academy_skipped","result_lines_skipped"}`, exit 0.

## The verdict

`verdict.schema.json`: an object with exactly `items`, an array of objects with exactly `kind` (`ambiguity`, `contradiction` or `gap`), `rules` (array of strings), `quote` and `why` (strings); every member required, no other property at either level. `prompt.md` defines the kinds (`ambiguity`: a sentence that admits two readings that act differently; `contradiction`: two sentences of the file that cannot both hold; `gap`: a case the file's own rules leave without an answer) and asks per item the rules it concerns, a verbatim quote of at most one sentence and one sentence why, and no item for a matter of wording.

## The reader

`python3 -I -B evals/batch/lint/read --files <files.json> --results <file> [--store <path>]`; the engine is `ONT_BIN`, default `ont`, called with `--store` when given and every apply with `--client batch-spec-lint`; `ONT_AGENT` and `ONT_ROLE` are not passed to it, so that a request id names the same action whoever runs the reader; each apply carries the fixed member `role` `api` (`specs/ap/roles.md` R3).

- `academy_path` as for the builder for both paths and the store's, before either file is read, and for a repository or path that `files.json` names; `files_unreadable` when the file is not of the builder's form; `results_unreadable`.
- Of the lines of one key the first whose `result.type` is `succeeded` is the one counted, else the first; every other line of that key is `duplicate`, so the reader reads what the builder settles. Each result line is counted in exactly one of `skipped` (no result line), `stale` (its key is of no file of `files.json`), `duplicate`, `errored` (`errored`, `canceled` or `expired`), `refused` (`succeeded` with `stop_reason` `refusal`), `unreadable` (`succeeded` with a `stop_reason` other than `end_turn`, or the `text` blocks of `message.content` joined are not JSON of `verdict.schema.json`), `read`. `lines` counts the lines that are not empty. Only a `read` line gives a signal; any other line causes no action.
- One finding per file, anchor and kind. Per read file, per item in order: the anchor is the first of the item's `rules` that `files.json` gives the file, else `file`; signature `<owner>/<name>:<path>#<anchor>:<kind>`; native key `batch-spec-lint-` and the first 16 hex digits of the SHA-256 of the signature; kind `note`, detector `batch:spec-lint`, severity `info`; summary `<signature>: the weekly reading of the specifications found this <kind>`; evidence `key <key> commit <commit>; rules <the item's rules, comma-separated>; quote: <quote>; why: <why>`, each control character a space, the whole cut at 2,000 bytes on a character boundary. A later item of a signature already met in the call adds nothing. Nothing the model wrote decides a key, a detector, a kind, a severity or a summary beyond the kind of an item and a rule that `files.json` gives its file.
- The pass is `finding_pass` of `evals/batch/evals/common.py`, unchanged, with the agent `batch-spec-lint` and the request id prefix `bl-`; the key that enters a request id is the file's. One `ont search` of `finding` by detector (every page; an answer that is not `ok` and qualified is `{"status":"store_failed"}`, exit 3, before any action; the search is made only when a signal exists). A signal with no finding: `open_finding`; a finding `open` or `planned`: `see_finding_again`, only when the stored evidence differs; `closed` or `dismissed`: nothing (counted `left`). At most ten `open_finding` are applied a call and the rest are counted `over_cap`, so the same call again opens the next ten. Nothing is closed or reopened and no finding of another detector is read for change.
- Standard output: `{"status":"ok","lines","read","refused","errored","unreadable","stale","duplicate","skipped","items","opened":[],"seen":[],"left","over_cap","already_applied","failed":[{"key","code"}]}`; `items` is the number of signals the pass was given (the distinct signatures of the read files); exit 0, or 1 when `failed` is not empty, 3 on `store_failed`.

## A pass by hand

Weekly, from a checkout at the installed commit:

```
evals/batch/lint/build --repo park-kyungchan/lifecycle-platform=<clone> --model claude-fable-5-2 --out <dir>
ap api batch evals --requests <dir>/requests.jsonl --out <batch dir>
evals/batch/lint/read --files <dir>/files.json --results <batch dir>/results.jsonl
```

Read the same result file again until `over_cap` is 0. A later build given an earlier result file asks only the files that changed or ended with an error.

## The undo

`build` writes only into `--out`: remove that directory. `read` writes no file and applies only `open_finding` and `see_finding_again` of detector `batch:spec-lint`. The Lead dismisses a finding that is wrong, with a reason (the job never changes it again), or closes it; the programs write nothing else. Reverting the pull request that added these files removes the job, and its findings stay in the store as records until the Lead closes them.

## What the job does not show

- A reading is one answer of one model: a finding is a lead for the Lead to read, which is why the severity is `info`, and the same file asked of two models gives two keys that are not reconciled.
- The anchor of a finding is the first rule the model names that the file holds; the rule that matters most may be another, and an item that names no rule of the file is anchored at `file`.
- An unchanged file is asked again at every weekly build unless `--results` names an earlier result file that settles its key; a key whose answer was refused, cut or unparsable is settled too and is counted at every read.
- Only files under `specs/` are read: a README, `content/` or code that contradicts a specification is not seen, and a file over 300,000 bytes is not asked.
- The job opens no finding about its own reading, closes none that the file no longer shows, and does not amend a specification.
