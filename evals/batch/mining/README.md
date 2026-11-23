# The mining job

The question: why was work blocked or stopped this week, and could a deterministic check have refused it earlier? The job reads the closed page, plan and verifier reviews and the stops of units of a UTC date window from the store, asks a model to give each item one cause from a closed list and to say whether a machine check could have refused it, counts the items by cause, asks a second question about the three largest causes, and opens at most three findings (detector `batch:mining`, kind `lesson`, severity `info`) with their counts and their items' keys. Its first user is the Lead at the end of a wave: a cause a machine could have refused becomes a unit that builds the check; any other becomes one changed sentence of the page reference (`content/skills/lead/references/unit-page.md`).

Three programs carry it by hand: `build`, `synth` and `read`. None calls a model or the network and none holds a key; `ap api batch mining` sends both batches and writes the result lines. What the jobs share is in `evals/batch/README.md` and `lib.py` (the Academy rule, the output and refusal form, the request and result lines, the key, the reading of a result line, the calls of the engine); this job's own shared code is `common.py`, loaded by its path. Standard library only.

## Shared rules

- The output of a program is one JSON line on standard output and standard error stays empty. A refusal is `{"status":"<code>"}` with exit 2 and nothing written or applied; a store that cannot be read is `store_failed`, exit 3. The engine is `ONT_BIN` (default `ont`), called with `--store` when given; `ONT_AGENT` and `ONT_ROLE` are not passed to it.
- The window: `--since <YYYY-MM-DD> --until <YYYY-MM-DD>`, UTC dates, since not after until and at most 31 days apart (else `usage`). A time is in the window when its first ten characters lie within since..until, both included, as `evals/readout.py` reads its window.
- The Academy. An item is the Academy's when the review's repository, its unit's repository or its intent's directory is a path of the Academy by `lib.is_academy`, or when its intent is the Academy's: at least one unit of that intent names a repository and every unit of it that names one names a repository of the Academy. A review's unit is its unit; its intent is that unit's intent or, for kind `plan`, the review's own intent (an intent the review itself names is asked as well). A stop's unit is the row's `parameters.unit`. A report path that is a path of the Academy is the same. Such an item is counted `academy_skipped` (one per review or stop, since its report is not opened to count its lines), is in no file written, and its report is never opened, examined or listed. Every path given as `--out`, `--results`, `--items`, `--synthesis` or `--store` is asked first, before any file is read or written and before the engine is called, and one of the Academy is refused `academy_path` before any file system call on it.

## The builder

`python3 -I -B evals/batch/mining/build --since <d> --until <d> --model <id> --out <dir> [--results <file>]... [--max-tokens <n>] [--max-requests <n>] [--store <path>]`

- The reads, before any write: one `ont search` of `review` with the filter `{"eq":["status","closed"]}`, one of `unit` with `{"in":["state",[<the eight states>]]}` and one of `intent` with `{"in":["status",[<the five statuses>]]}`, each over every page; then `ont log` `{"action_type":"stop_unit","outcome":"applied","since":"<since>T00:00:00Z","until":"<the day after until>T00:00:00Z","parameters":true,"limit":1000}` and its continuation to the end. An answer that is not `ok`, a search page that is not qualified, a log answer without `rows` or `withheld`, a log row without `action_id`, `submitted_at`, `parameters.cause` or an object `parameters`, or a continuation given twice, is `store_failed`, exit 3, with nothing written. A log answer's `withheld` is counted `stops_withheld`.
- The items come from the reviews whose `closed_at` and the log rows whose `submitted_at` is in the window. A review of another kind than `page`, `plan` or `packet`, with verdict `abandoned`, or with `carried_from`, gives no item.
  - `blocking`: per review of kind `page` or `plan`, each line of its report that `BLOCKING_LINE` of `evals/readout.py` matches. Its text runs to the line before the next line that the pattern matches, that begins `note:` after the same marks, or that begins `What I ran` or `What I could not check` after any `#`, `*` and spaces (the sections the frames put after the findings), or to the report's end, cut at 6,000 bytes on a character boundary. Its id is `<review>#<n>`, `n` from 1 in the report's order; its name is the word after `blocking ` when `BLOCKING_KINDS` of `evals/readout.py` lists it for the review's kind, else null. The builder holds the pattern and the lists, and a test fails when they are not those of `evals/readout.py`.
  - `repair`: a review of kind `packet` with verdict `repair`, and a review of kind `page` or `plan` with verdict `repair` whose report has no blocking line. Text: the report from its third line, cut at 12,000 bytes; id the review; name null.
  - `stop`: each log row; text `cause: <cause>` and `detail: <detail>` on two lines; id the row's `action_id`; name the cause.
  - A report is read only when its path is absolute, names a regular file reached without a link and holds at most 1,048,576 bytes (read as UTF-8, invalid bytes replaced); else the review is counted `report_unreadable` and gives no item. The lines of a report are the lines of its text without the empty one after a final line break.
- One request per item. `params` has exactly `model` (as given), `max_tokens` (`--max-tokens`, an integer from 256 to 8000, default 1024), `system` (the text of `classify.md`, a blank line, then the causes of `causes.json`, one line each, `<cause>: <means>`, in its order), `messages` (one `user` message whose content is one string: the item's kind and name, the review's kind and verdict, the unit's class and lane, one per line, `none` where absent, a blank line, then the item's text) and `output_config` `{"format":{"type":"json_schema","schema":<classify.schema.json>}}`. What the program adds to the item's own text holds no time, no host path and no review, unit or intent key (the item's text is the report's or the log's, as written, and may hold them); the key is the one of `lib.key`, and two items of equal `params` share one request.
- `classify.schema.json`: an object with exactly `cause` (a string, one of the causes of `causes.json` in its order), `machine` (a boolean: a check that reads only the request, the page, the base or the head, with no model, could have refused the item before a stage read it), `check` (a string: that check in one sentence, where it runs and what it refuses; empty when `machine` is false) and `why` (a string, one sentence); every member required, no other.
- `--results` (repeatable) settles a key as the drift builder does: a key is settled when a file holds a line of that key with `result.type` `succeeded`, so a refusal is settled and an error is asked again; a line that is no result line is counted `result_lines_skipped`. The items in the order of their time (`closed_at` or `submitted_at`), then id; the first `--max-requests` (an integer from 1 to 100,000, default 400, the policy's `max_requests` of job `mining`) keys that are not settled are requested, the later ones are `over_max`.
- Written into `--out` (made with mode 0700 when absent; `out_unwritable`), each file under a temporary name and renamed, mode 0600, after every read succeeded. `requests.jsonl`: one request line per requested key in item order (an empty file when there is none). `items.json`: `{"job":"mining","since","until","model","items":[{"item","kind","name","review","unit","intent","at","text","key","state"}]}`, `review`, `unit` and `intent` null where absent, `text` as cut above, `state` one of `requested`, `settled`, `over_max`.
- Standard output: `{"status":"ok","out","items","requested","settled","over_max","blocking","repair","stop","academy_skipped","report_unreadable","stops_withheld","result_lines_skipped"}`, exit 0. `requested`, `settled` and `over_max` count keys, `items` and the three kinds count items.

## The synthesizer

`python3 -I -B evals/batch/mining/synth --items <items.json> --results <file> [--results <file>]... --model <id> --out <dir> [--max-tokens <n>]`

- `items_unreadable` when the file is not of the builder's form; `results_unreadable`. Each result line is counted in exactly one of: `skipped` (no result line), `stale` (its key is of no item), `duplicate` (of the lines of one key the first whose `result.type` is `succeeded` is the one counted, else the first), `errored`, `refused`, `unreadable` (a cut or unparsable answer, or one that is not of `classify.schema.json`, a cause outside `causes.json` among them), `read`. An item whose key is read is classified; every other item is counted unclassified.
- The classified items are grouped by cause and ordered by count, high first, then by the order of `causes.json`; the first three are the week's causes, each with `count`, `machine` (the number of its items whose `machine` is true) and `items` (their ids in item order). `remedy` is `check` when `machine` is at least half of `count`, else `sentence`. `synthesis.json`: `{"job":"mining","since","until","model","key","classified","unclassified","counts":{<the seven words>},"causes":[{"cause","count","machine","remedy","items"}]}`; `key` is the key of the synthesis request, null when there is no cause.
- With at least one cause, `requests.jsonl` holds one request: `params` as the builder's with `synthesize.md` as `system`, `max_tokens` default 4000 (256 to 8000), a user text that holds the window and then, per cause, its cause, means, count, machine and remedy and, for at most its first 20 items, the id, check, why and the first 1,500 bytes of the item's text from `items.json` (cut on a character boundary); `output_config` with `synthesis.schema.json`: `{"causes":[{"cause","change","why"}]}`, every member required and no other: `change` for remedy `check` is the check to build (where it runs and what it refuses), for `sentence` the one sentence of the page reference to add or replace, written out. With no cause the file is empty.
- Standard output: `{"status":"ok","out","causes","classified","unclassified","requested"}` and the seven counts, exit 0. The synthesizer calls no engine.

## The reader

`python3 -I -B evals/batch/mining/read --synthesis <synthesis.json> --results <file> [--store <path>]`

- `synthesis_unreadable` when the file is not of the synthesizer's form; `results_unreadable`. The lines of the key of `synthesis.json` are counted with the words above (a line of any other key is `stale`); only a line counted `read` causes actions, so a refused, errored, cut, stale or duplicate line, or none, causes none and exits 0 without a call of the engine.
- One finding per cause of `synthesis.json`. Signature `mining:<since>..<until>:<cause>`. Native key `batch-mining-` and the first 16 hex digits of the SHA-256 of the signature; the id's namespace is the one of `id()` in `crates/ap/src/host.rs` (`lib.NAMESPACE`). Kind `lesson`, detector `batch:mining`, severity `info`. Summary, from `synthesis.json` alone: `<since>..<until> <cause>: <count> of <classified> classified items, <machine> of them a machine could have refused (<remedy>)`. Evidence: `items <the first ten ids, comma-separated>; change: <change>; why: <why>`, `change` and `why` from the first entry of the verdict whose cause is the cause (`change: none; why: none` without one), each control character a space, the whole cut at 2,000 bytes on a character boundary.
- Per finding, by its present state (one `ont search` of `finding` with `eq` on `detector`, every page; `store_failed`, exit 3, before any action): absent, `open_finding`; `open` or `planned`, `see_finding_again` only when the stored evidence differs; `closed` or `dismissed`, nothing (counted `left`). No finding is closed or reopened and none of another detector is changed. Each apply is `ont action apply` with `--client batch-mining` and the agent `batch-mining`; its request id is `bm-` and the first 40 hex digits of the SHA-256 of the JSON, in the key's form, of the array of the action, the finding's key, the synthesis key and the parameters. An answer `already_applied` wrote nothing: it is counted `already_applied` and is in neither list. A refusal or failure is listed in `failed` (as the drift reader's), and the call goes on.
- Standard output: `{"status":"ok","lines","read","refused","errored","unreadable","stale","duplicate","skipped","opened":[],"seen":[],"left","already_applied","failed":[{"key","code"}]}`, exit 0, or 1 when `failed` is not empty.

## The closed list

`causes.json` is an array of `{"cause","means"}` in this order, and `classify.schema.json` has the same causes as the enum of `cause`; a test holds the three equal.

- `base_claim`: the page, plan or stage states a fact about the base, a path, line, name, behaviour or command form, that the base does not hold
- `outside_write_set`: the change needs a file outside the write set, or a pattern is not of the two forms
- `class_or_lane`: the class or lane does not admit the paths or the effect the change needs
- `contradiction`: two rules of the page, or of the page and another page or a specification, cannot both hold
- `weak_check`: an acceptance check passes without the change, cannot pass with it, or tests nothing the page changes
- `unguarded_effect`: an effect on a real target with no guard
- `plan_order`: a dependency missing, circular or wrong, a criterion no unit reaches, or two units that cannot both hold
- `build_defect`: the head does not do what its page says, or breaks a test, an invariant or a contract
- `environment`: the host, a service, a limit, the network or the hosting service stopped the work, not the page or the head
- `other`: none of the causes above

## The run

From a checkout at the installed commit, by hand, in this order:

```
build --since <d> --until <d> --model claude-sonnet-5-5 --out <dir1> [--results <earlier result files>...]
ap api batch mining --requests <dir1>/requests.jsonl --out <dir2>
synth --items <dir1>/items.json --results <dir2>/results.jsonl --model claude-fable-5-2 --out <dir3>
ap api batch mining --requests <dir3>/requests.jsonl --out <dir4>
read --synthesis <dir3>/synthesis.json --results <dir4>/results.jsonl
```

Read the three findings in `findings` of `ap status`: plan each as a unit or dismiss it with a reason. The first real week's run needs `ap api batch` and the policy's job `mining` delivered. The tests are `ci/test_batch_mining.py`.

## The undo

The Lead dismisses a finding with a reason, and it is never changed again by the job. The programs write nothing else: `build` and `synth` write only into `--out` (remove the directory) and `read` writes only through `open_finding` and `see_finding_again`; reverting the pull request that added these files removes the job, and its findings stay in the store as records.

## What the job does not show

- A cause is a model's reading of the text of one report or one stop, not a run of anything; `machine` is its claim that a check would have refused the item, not a check that exists. The findings are leads for the Lead to read, which is why the severity is `info`.
- Only the text of a report is read: not the page, the base, the diff or the session. A report that cannot be read gives no item (`report_unreadable`), and the rows the log withheld give none (`stops_withheld`).
- A key whose answer was cut, refused or does not parse is settled, and so is not asked again until its input, the prompt, the model or `max_tokens` changes; an error is asked again at the next build. The same item asked of two models gives two keys, and the synthesizer acts on the result files it is given.
- The three largest causes are all the job reads: a smaller cause, and the items of a cause beyond its first twenty, are in `synthesis.json` only as counts and ids.
- A finding's evidence changed from A to B and later back to A answers `already_applied` and leaves B stored, since the request id of A was used with the same content.
- A window run again after the Lead dismissed its finding opens nothing for that cause (`left`); a later window has its own signature and its own finding.
