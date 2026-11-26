# The evals job

Job J5 of the API credit, run by hand: the routing suite and the Lead suite (`evals/README.md`) read through the Message Batches API, three samples a case or more, so that the effect of a change of `content/` on them is read with its spread (one sample cannot tell a change from noise). The three programs here only build requests and read results: they call no model, open no socket and hold no key. `ap api batch evals` (unit api-batch, policy unit api-policy) sends each batch. The suites' own runs on the subscription (`evals/routing/run`, `evals/lead/run`) are unchanged, and so is their gate; a pass of this job is a reading beside them. The reading of the specifications (J4) is not here. The shared rules of the Academy, the key, the request and result lines and the output form are those of `evals/batch/README.md`; `lib.py` holds them and `common.py` here holds what the three programs add.

## What a pass is

- A pass is one suite, one commit of `content/` and one answering model. Its instruction text is read from Git's objects: `--clone <lifecycle-platform clone> [--rev <rev>]` (default `origin/main`, resolved once to a commit; `repo_unreadable` when the clone or the revision cannot be read). The files are those `content/SHA256SUMS` lists at that commit, read with `git cat-file`: no file of a checkout is read, nothing is fetched and nothing in the clone changes. A file whose bytes differ from their listed SHA-256 is `content_sums_differ`. The text is `AGENTS.md` first, then the other files by path as bytes, each preceded by one line `=== <path below content/> ===`. The pass's instructions are `{"commit","sums"}`, `sums` the SHA-256 of `content/SHA256SUMS` at that commit.
- The text is the one block of `system` of every answer request, marked for the one-hour cache (`{"type":"ephemeral","ttl":"1h"}`), so that the requests of a pass share one cached prefix: `ap api batch` reserves such a request at the rate `cache_write_1h` and settles it by the results' usage. There is no tool and no harness: a reader answers from the text alone.
- An answer request's `params` has exactly `model`, `max_tokens` (`--max-tokens`, 256 to 8000, default 4000), `system` and `messages` (one user message whose content is one string). The samples `s` = 1 to `--samples` (3 to 9, default 3) are requests of their own with equal params; the `custom_id` of sample `s` is the SHA-256 (64 hex) of the JSON, in the key's form, of `[params, s]`. A scoring request's `custom_id` is the key of its params. A request whose system and user text pass 300,000 bytes together is not written and its items take the state `too_large`.
- Every path given (`--clone`, `--cases`, `--out`, `--results`, `--items`, `--blind`, `--scores`, `--baseline`, `--store`) is asked as the Academy rule says, all of them before any file is read or written; one of the Academy is refused `academy_path`. Before a Lead case directory is hashed or read its names are listed (no file opened), and a relative path the rule names, such as one below `TASK/evals/lesson/`, is refused `academy_path`.

## The builder

`python3 -I -B evals/batch/evals/build --suite routing|lead --clone <dir> [--rev <rev>] --model <id> --out <dir> [--cases <path>]... [--samples <n>] [--max-tokens <n>] [--results <file>]...`

- `routing`: the cases are those of `load_cases` of the routing module (`evals/routing/cases.json` and each `--cases` host file, admitted by its digest, with that function's refusals). The user text is `answer.md` with the case's situation put in: the opening of the routing suite's prompt, one sentence that the shared instructions in force are the system text laid out as `/root/.agents`, "Do not do the assignment.", and the prompt's last paragraph (the form of the answer) word for word. No sentence of that prompt about the file system, the host, the store, the Academy's paths or a function or command to ask first is in it: a batch reader has no tool.
- `lead`: exactly one `--cases`, the directory of the Lead cases, checked as `check_cases` of the Lead module checks them without its account: each id of `evals/lead/cases.sha256` present with the digest `tree_digest` gives (`case_digest_differs`), no other directory (`case_unknown`). The user text is `lead.md`, then `PROMPT.txt` with `@TASK@` replaced by `task`, then each regular file below `TASK/` by relative path as bytes, each under one line `=== task/<relative path> ===`, read as UTF-8 with invalid bytes replaced. No byte of `EXPECTED.md` is in `requests.jsonl` or `items.json`: the file is read only to compute the case digest.
- `--results` (repeatable) names files of result lines: a key is settled when one of them holds a line of that key with `result.type` `succeeded` (a refusal is settled, an error is asked again at the next build). A file that cannot be read is `results_unreadable`; a line that is no result line is skipped and counted.
- Written into `--out` (mode 0700, files 0600, each renamed): `requests.jsonl`, one line per requested key by case id, then sample, and `items.json`, `{"job":"evals","suite","model","instructions","case_files","samples","items":[{"case","sample","key","state"}]}`, `state` one of `requested`, `settled`, `too_large`; `case_files` is the digests of `load_cases` for the routing suite and `{"cases.sha256": <SHA-256 of evals/lead/cases.sha256>}` for the Lead suite. The same commit, case files, options and result files give byte-equal files.
- Standard output: `{"status":"ok","out","suite","cases","requested","settled","too_large","system_bytes","result_lines_skipped"}`, exit 0.

## The blind scorer

`python3 -I -B evals/batch/evals/blind --items <items.json of suite lead> --results <file>... --cases <dir> --model <id> --out <dir> [--max-tokens <n>]`

- The cases are checked as for the builder; `items_unreadable` when the file is not the form above (or is of the routing suite); `results_unreadable`.
- Each item whose answer line is counted `read` (a free answer is `unreadable` when its `stop_reason` is not `end_turn` or its text is empty) gives one scoring request: `system` is the text of `evals/lead/scoring.md`, then `blind.md`; the user text is the case's `EXPECTED.md` under a line `Rubric:` and the answer under a line `Answer:`; `output_config` is `{"format":{"type":"json_schema","schema":<blind.schema.json>}}`. Nothing in `params` names the case id, the answering model, the sample, a key, the commit or the instructions, so equal answers of one case share one request. `blind.schema.json` is an object with exactly `result` (one of `caught`, `partly`, `missed`), `quote` and `clause`, all required.
- Written: `requests.jsonl` and `blind.json`, `{"job":"evals","suite":"lead","model","items":<SHA-256 of items.json>,"answers":[{"case","sample","key","score_key"}]}`, `score_key` null for an answer not read. Standard output: `{"status":"ok","out","answers","requested","unanswered","lines"}` and the counts below.

## The scorer

`python3 -I -B evals/batch/evals/score --items <items.json> --results <file>... --out <dir> [--cases <path>]... [--blind <blind.json> --scores <file>...] [--baseline <score.json>] [--record <task key>] [--store <path>]`

- `items_unreadable`, `results_unreadable`; for the Lead suite `--blind` and at least one `--scores` are required (`usage`) and `blind_unreadable` is a blind.json that is not the form above, or whose `items` is not the SHA-256 of the `--items` given. For the routing suite the `--cases` are given again, and digests other than those of `items.json` are `case_sets_differ`.
- A sample passes when its answer line is counted `read` and, for the routing suite, `passes()` of the routing module holds for its text; for the Lead suite when the line of its `score_key` is counted `read` with `result` `caught` (`partly` adds nothing, `evals/lead/scoring.md`). A sample whose answer (or, for the Lead suite, whose score) is not counted `read`, a `too_large` one included, fails and is counted `unanswered`; a read answer that does not pass fails and is not.
- The counts: of the lines of one key the first whose type is `succeeded` is the one counted, else the first; every other line of that key is `duplicate`. Each line is counted in exactly one of `skipped` (no result line), `stale` (its key is of no item), `duplicate`, `errored` (`errored`, `canceled` or `expired`), `refused` (`stop_reason` `refusal`), `unreadable` and `read`. A line not counted `read` gives no pass, no caught and no action. For the Lead suite `counts` is that of the answers and `score_counts` that of the score lines.
- `score.json`, written into `--out` (mode 0600, renamed): `{"job":"evals","suite","instructions","case_files","items":<SHA-256 of items.json>,"scorer":<blind.json's model or null>,"readers":{<model>:{"n","passes","samples","mean","majority","unanswered","spread"}}}`. `n`, `passes`, `samples`, `mean`, `majority` and `unanswered` are what `score_reader` of the routing module computes (`mean` is the mean number of passed cases a sample). `spread` is 2 x sqrt(the sum over the cases of p(1-p) x 2/n), `p` a case's passes over `n`, rounded to four places: the bound of the routing gate for a second pass with the same rates, in the unit of `mean`. So `evals/routing/run gate --baseline <score.json> --candidate <score.json>` judges two passes, and the same inputs give a byte-equal file.
- `--baseline <score.json>`: a score.json of this program (`score_unreadable`) of the same suite, case files and reader (`case_sets_differ`); `lost` is the ids of the cases in its majority and not in this pass's.
- Standard output: `{"status":"ok","out","suite","reader","cases","n","mean","spread","unanswered","lost":[],"counts":{...}}`, with `--record` also `"recorded"`; exit 0, 1 when `recorded.failed` is not empty, 3 on `store_failed` or `unknown_task` (nothing is applied and no `score.json` is written then).

## The record

- `--record <task key>`; without it no engine is started. The engine is `ONT_BIN` (default `ont`), started with the program's environment unchanged, so it claims the role the caller's `ONT_ROLE` names; the program sends no member role. A run without `ONT_ROLE` is refused `not_granted`: each apply is listed in `recorded.failed` (exit 1) and the run is made again with `ONT_ROLE=lead`; its request ids make the repeat safe.
- Every read comes before the first apply. The task: one search of `task`, every page, the object whose native key is the key (a page that is not qualified is used when it holds the key); none is `unknown_task`. The KPI `batch evals <suite> <reader>`: one search of `kpi` on its name, every page, kept when its task is that task; when there is none, `define_kpi` with a fixed definition that names this program, the value's form and the spread's rule. With `--baseline`, one search of `finding` by detector `batch:evals`. An answer that is not `ok`, or a page of `kpi` or `finding` that is not qualified, is `store_failed`, before any action.
- One measurement per pass and reader (`record_measurement`): value `mean <m> spread <s> samples <c1> ... <cn> majority <k>/<cases> n <n> unanswered <u>`; `observed_at` the call's UTC time; evidence `evals/batch/evals/score sha256:<first 16 hex of score.json's SHA-256> content sha256:<first 16 hex of sums>`, for the Lead suite followed by ` scorer <model>`. Nothing is applied when the KPI's evidence already equals it (`already`); an `older_measurement` is counted `older`.
- With `--baseline`, one finding per lost case: signature `evals:<suite>:<reader>:<case>:<first 16 hex of this pass's sums>` (so a case lost under another content is a finding of its own, and a finding dismissed for one unit hides no later one); native key `batch-evals-` and the first 16 hex of the SHA-256 of the signature; kind `note`, detector `batch:evals`, severity `info`; summary `<suite> case <case> lost for <reader>: passed in <b> of <n> samples of the baseline and <c> of <n> of this pass`; evidence `score sha256:<16 hex> baseline sha256:<16 hex> content sha256:<16 hex>`.
- The pass over the findings (`finding_pass` of `common.py`, which unit batch-lint calls unchanged): absent, `open_finding`; `open` or `planned`, `see_finding_again` only when the stored evidence differs; `closed` or `dismissed`, nothing (counted `left`). At most ten `open_finding` are applied a call and the rest are counted `over_cap`, so the same call again opens the next ten.
- Each apply has the agent `batch-evals` and the request id `be-` and the first 40 hex of the SHA-256 of the JSON, in the key's form, of the array of the action, the KPI's name or the finding's key, the SHA-256 of `score.json` and the parameters. `already_applied`, a refusal (the code, or `failed[0]` of a `submission_criteria`) and a failure are counted and listed as for the other jobs. `recorded` is `{"kpi":"found"|"defined","measurement":"applied"|"already"|"older"|"failed","opened":[],"seen":[],"left","over_cap","already_applied","failed":[{"key","code"}]}`.
- The program writes only inside `--out` and, with `--record`, applies only `define_kpi`, `record_measurement`, `open_finding` and `see_finding_again`.

## A pass by hand

From a checkout at the installed commit. A pass of the routing suite, once for each model (`claude-haiku-5-5`, `claude-sonnet-5-5`):

```
evals/batch/evals/build --suite routing --clone <clone> --model <model> --out <dir>
ap api batch evals --requests <dir>/requests.jsonl --out <batch dir>
ONT_ROLE=lead evals/batch/evals/score --items <dir>/items.json --results <batch dir>/results.jsonl --out <score dir> --record <task key>
```

The Lead suite (the cases directory is a host directory the Lead names):

```
evals/batch/evals/build --suite lead --clone <clone> --cases <cases> --model claude-sonnet-5-5 --out <dir>
ap api batch evals --requests <dir>/requests.jsonl --out <batch dir>
evals/batch/evals/blind --items <dir>/items.json --results <batch dir>/results.jsonl --cases <cases> --model claude-opus-5-5 --out <blind dir>
ap api batch evals --requests <blind dir>/requests.jsonl --out <scores dir>
ONT_ROLE=lead evals/batch/evals/score --items <dir>/items.json --results <batch dir>/results.jsonl --blind <blind dir>/blind.json --scores <scores dir>/results.jsonl --out <score dir> --record <task key>
```

For a unit that changes `content/`: the same passes at the unit's head commit and at its base, `score --baseline <the base's score.json>`, then `evals/routing/run gate --baseline <base score.json> --candidate <head score.json>`. A `build` given the result files of an earlier pass of the same commit asks only what changed or ended with an error.

## The undo

`build` and `blind` write only into `--out`: remove that directory. `score` writes `score.json` and, with `--record`, the four actions of detector `batch:evals`. The Lead dismisses a finding that is wrong, with a reason (the job never changes it again); a measurement is history and stays. Reverting the pull request that added these files removes the job.

## What the job does not show

- A pass reads the instruction text alone: a fact whose only home is outside `content/` (a repository, the store, a task directory) can be named in an answer but not quoted, so a routing case that needs one is lost on every pass whatever the text says.
- The Lead's answers are scored by a model, blind to the answering model and the sample: its quote is the evidence to read, and two scorers may differ.
- At three samples the loss of one case is not reliably caught, as `evals/README.md` says of the suites: a spread that covers the means is not a proof that no case was lost, and a `lost` case that the spread covers is a lead for more samples (`--samples 9`).
- A lost-case finding is per case and per content of the pass; it does not say that the change of `content/` caused it, only that the case's majority was lost between the two passes.
- A batch pass is not the subscription's run: it is another model call path, with no tool, and its gate is `evals/routing/run gate` on score files, not a replacement for the suites' runs on the instructions installed after delivery.
