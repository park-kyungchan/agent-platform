# The merge-review job

The second reading of merged pull requests, run by hand: `build` writes one Message Batch request per unit merged in a UTC date window (its diff against the first parent, its page from the store, and the numbered requirements of each specification file its page names), routed to a model of another family than its verifier's and to the stronger given model when a changed path is protected; `read` turns the result lines into one finding per pull request with a claimed defect (`detector` `batch:merge-review`, `kind` `drift`, `severity` `info`, at most ten opened a call) and posts one comment on each read pull request, once. Neither program calls a model or the API, runs any command a model wrote, or reads an Academy unit, repository, or path. First user: the Lead, who reproduces a claimed defect in a disposable worktree, raises the finding to `act`, and plans a fix unit that adds its test.

What the batch jobs share is in `evals/batch/README.md`, `evals/batch/lib.py`, and `evals/batch/mining/common.py`; this job's shared code is `common.py`, loaded by its path by both `build` and `read`. Standard library only.

## Shared rules

- Output and refusals: one JSON line on standard output and empty standard error (`evals/batch/lib.py`). A refusal is `{"status":"<code>"}` with exit 2 and nothing written or applied; a store search failure is `{"status":"store_failed"}` with exit 3.
- The window: `--since <YYYY-MM-DD> --until <YYYY-MM-DD>`, UTC dates, `since` not after `until`, at most 31 days apart (else `usage`). A unit is in the window when the first ten characters of its `ended_at` lie within `since..until`, both included.
- The Academy. A unit is the Academy's when its repository's name or its intent's directory is a path of the Academy by `lib.is_academy`, or when its intent is the Academy's (at least one unit of that intent names a repository and every unit of it that names one names a repository of the Academy). Such a unit is counted `academy_skipped`: nothing of its page, diff, or clone is read, and nothing of it enters any written file, request, or comment. A changed path that is the Academy's is left out of the diff, counted `academy_paths`, and never read; a spec path named by the page that is the Academy's is likewise not read, omitted from the request and from `items.json`, and counted `academy_paths`. Every path value (`--repo` clone, `--out`, `--results`, `--items`, `--store`) and every `--repo` repository name is checked first, before any file is read or written and before `ont` or `gh` is called; an Academy path is refused `academy_path` with exit 2.

## The builder

`python3 -I -B evals/batch/merge_review/build --repo <owner>/<name>=<clone> [--repo ...] --since <d> --until <d> --model <id> --strong <id> --out <dir> [--results <file>]... [--max-tokens <n>] [--max-requests <n>] [--store <path>]`

- Store reads, before any write: one `ont search` of `unit` with `{"in":["state",["stopped","accepted","running","ready","draft","merged","done","dropped"]]}` and one of `intent` with `{"in":["status",["draft","assumed","confirmed","met","retired"]]}`, every page; an answer that is not `ok`, a page that is not qualified, or a repeated continuation is `store_failed`, exit 3, nothing written. Items are units of state `merged`, in the window, with `pull_request` and `merged_commit`, ordered by `ended_at`, then key. A unit whose repository no `--repo` names is counted `repo_not_given` and is no item.
- Git reads, from Git's objects of each clone only (no working-tree file read, nothing fetched, nothing mutated). Every `git` call runs with `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, and `GIT_ATTR_NOSYSTEM=1`, and `-c core.quotePath=true -c diff.suppressBlankEmpty=false -c core.attributesFile=/dev/null`. Every `git diff` adds `--attr-source=<merged commit>` before `diff` and the options `D`: `--no-color --no-ext-diff --no-textconv --no-renames --full-index --diff-algorithm=myers --unified=3 --inter-hunk-context=0 --src-prefix=a/ --dst-prefix=b/ --no-relative -O/dev/null --submodule=short --indent-heuristic`. A clone whose `git rev-parse --git-path info/attributes` path exists is refused `repo_unreadable` (with `repository`), exit 2, before any write. The merged commit must be a commit in the clone with a first parent, else `commit_missing` (no item). Changed paths come from `git diff D --name-only -z <first parent> <merged commit>`. The content diff runs `git --literal-pathspecs --attr-source=<merged commit> diff D <first parent> <merged commit> -- <non-Academy paths>`; when no non-Academy path remains, the diff is empty and content `git diff` is not called.
- Named spec files: each `specs` entry (text before `#`) and each match of `specs/[A-Za-z0-9._/-]+\.md` in the page's text members (`objective`, `interface`, `invariants`, `not_built`, `notes`, `verifier_questions`), `read_first` entries, and `acceptance` checks, deduplicated and sorted in byte order. For each non-Academy spec path, `git cat-file` reads the blob when it is a regular file at the merged commit and keeps every line matching `REQ_RE` of `ci/check-spec-trace.py` in file order, or writes `missing` when the path is not a regular file at that commit.
- Protected paths: any non-Academy changed path for which `protected()` of `crates/ap/src/unit/drive.rs` holds (`PROTECTED_EXACT`, `PROTECTED_NAMES`, `PROTECTED_CONFIGS`, `PROTECTED_TREES`, compared without case).
- Model routing: `--model` and `--strong` are Claude API identifiers `claude-<family>-<rest>` of two different families (else `usage`). A unit's verifier names family `f` when `verifier.model` equals `f` or starts with `claude-<f>-`. The reader is `--strong` when a changed path is protected, else `--model`; if the verifier names that reader's family, the other model is chosen.
- One request per item. `params` has exactly `model` (the chosen reader model id), `max_tokens` (`--max-tokens`, 1024 to 8000, default 8000), `system` (`prompt.md`), `messages` (one user message string), and `output_config` (`verdict.schema.json`). The user text holds, in order: repository, pull request number, unit key, class, lane, merged commit, protected flag; the page members `objective`, `interface`, `invariants`, `acceptance`, `not_built`, `write_set`, `read_first`, `specs`, `notes`, `verifier_questions` (each under its name, absent ones omitted); per named spec file its path and requirement lines or `missing`; then the diff. No time, host path, or CLI option enters `params`.
- `verdict.schema.json`: object with `defects`, an array of objects with required string fields `title`, `where`, `clause`, `command`, `why` and `additionalProperties: false`.
- Items whose user text exceeds 300,000 UTF-8 bytes get state `too_large` and no request. `--results` (repeatable) settles keys that have a `succeeded` result line (`result_lines_skipped` counts invalid lines). Among unsettled items in item order, the first `--max-requests` (1 to 100, default 100) get state `requested` and later ones `over_max`.
- Written into `--out` (mode 0700, temp + rename, files mode 0600): `requests-model.jsonl`, `requests-strong.jsonl` (each written even when empty, naming one model only), and `items.json` (`{"job":"merge-review","since","until","model","strong","items":[{"unit","repository","pull_request","commit","verifier","protected","reader","specs","key","state"}]}` where `verifier` is `verifier.model` or `null`, `reader` is `"model"` or `"strong"`).
- Standard output: `{"status":"ok","out","items","requested","settled","too_large","over_max","protected","academy_skipped","academy_paths","repo_not_given","commit_missing","result_lines_skipped"}`, exit 0.

## The reader

`python3 -I -B evals/batch/merge_review/read --items <items.json> --results <file> [--results <file>]... [--store <path>]`

- `items_unreadable` when `--items` does not match the builder's form; `results_unreadable` when a results file cannot be read. Result lines are counted with the seven words (`read`, `refused`, `errored`, `unreadable`, `stale`, `duplicate`, `skipped`), `unreadable` including schema mismatches. Only lines counted `read` cause actions or comments.
- Per read item, defects keep the verdict's order: the first five enter evidence and the pull request comment; the count of all defects enters the summary, comment header, and `defects` total.
- One finding per read item with at least one defect: signature `<owner>/<name>#<pull request>`, native key `batch-merge-review-<first 16 hex of SHA-256(signature)>`, namespace of `id()` in `crates/ap/src/host.rs`, kind `drift`, detector `batch:merge-review`, severity `info`. Summary (from `items.json` alone): `<signature> (unit <unit>, <first 12 hex of commit>): <n> defects claimed against its page by <reader model id>, none reproduced`. Evidence: `key <key>`, then per shown defect `; <i>. <title> at <where>, clause <clause>, reproduce: <command>, why: <why>`, each control character replaced by a space, cut at 2,000 UTF-8 bytes on a character boundary.
- The finding pass: one `ont search` of `finding` with `eq` on `detector`, every page (`store_failed`, exit 3, before any action or comment). Per finding in item order: absent -> `open_finding`, capped at ten `applied` answers per call (further ones counted `over_cap` and not sent); `open` with severity `info` -> `see_finding_again` with `finding` and `evidence` only (never `severity`), only when stored evidence differs; any other state or severity (`open` with `act`/`urgent`, `planned`, `closed`, `dismissed`) -> untouched, counted `left`. No finding is closed, reopened, dismissed, or planned, and no other detector's finding is touched. Each apply uses agent `batch-merge-review` and request id `bmr-<first 40 hex of SHA-256(canonical JSON of [action_type, finding_key, item_key, parameters])>`. The engine (`ONT_BIN`, default `ont`) runs with the caller's environment unchanged (`ONT_ROLE` included); no apply body includes a `role` member.
- Comments, after the finding pass, per read item in item order via `GH_BIN` (default `gh`, argv list, no shell): `GH_BIN pr view <n> --repo <owner>/<name> --json comments`. If any comment's first line equals `<!-- batch:merge-review <signature> -->`, the item is counted `comment_present` and nothing is posted. Otherwise `GH_BIN pr comment <n> --repo <owner>/<name> --body-file -` posts the marker line, header line, up to five defects (each model string sanitized with non-LF control characters turned into spaces and wrapped in its own fenced code block longer than any backtick run it contains), and the closing line `"A claimed defect stays a finding of severity info until its command reproduces it at the merged commit."` (or `"No defect claimed."` when there are no defects). Bodies over 60,000 UTF-8 bytes are cut on a character boundary while keeping the closing line. A `GH_BIN` failure records `{"key": <item key>, "code": "gh_failed"}` in `failed` and continues.
- Standard output: `{"status":"ok","lines","read","refused","errored","unreadable","stale","duplicate","skipped","defects","opened":[],"seen":[],"left","over_cap","already_applied","commented":[],"comment_present","failed":[{"key","code"}]}`, with `opened` and `seen` holding finding keys and `commented` holding item keys; exit 0, or 1 when `failed` is not empty.

## The run

From a checkout at the installed commit, by hand:

```
evals/batch/merge_review/build \
  --repo <owner>/<repo1>=<clone1> --repo <owner>/<repo2>=<clone2> \
  --since <day after last pass's until> --until <YYYY-MM-DD> \
  --model claude-opus-5-5 --strong claude-fable-5-2 \
  --out <dir>
# For each non-empty request file (requests-model.jsonl, requests-strong.jsonl):
ap api batch merge-review --requests <dir>/requests-model.jsonl --out <batch-model>
ap api batch merge-review --requests <dir>/requests-strong.jsonl --out <batch-strong>
ONT_ROLE=lead evals/batch/merge_review/read \
  --items <dir>/items.json \
  --results <batch-model>/results.jsonl --results <batch-strong>/results.jsonl
```

Pass one `--repo` per repository mapped in the policy file's `repositories`, `--model claude-opus-5-5 --strong claude-fable-5-2`, and `--since` the day after the last pass's `until`. Run `ap api batch merge-review` only on each request file that is not empty. Run `read` with `ONT_ROLE=lead` (a program outside `ap` claims its caller's `ONT_ROLE`; `lead`'s grant holds every action); repeat `read` over the same files if `over_cap > 0`.

The Lead's step on each opened finding (each apply with role `lead`, via `ONT_ROLE=lead` or the request member `role`): read the defect's command and run it in a disposable worktree of the merged commit, never in a clone that delivery or a stage uses. When it exits non-zero for the claimed cause, apply `see_finding_again` with severity `act` and its output as evidence, then `plan_finding_as_unit` to a fix unit whose page holds a test of the defect; otherwise apply `dismiss_finding` with the reason.

## The undo

`build` writes only into `--out`: remove that directory. `read` writes only through `open_finding` and `see_finding_again` on detector `batch:merge-review` and one comment per pull request: the Lead dismisses a wrong finding with a reason (the job never changes a dismissed, closed, planned, or non-`info` finding) and deletes a comment by hand. Reverting the pull request removes the job.

## The yield

After two weeks of hand passes, run one `ont search` of `finding` with `eq` on `detector` `batch:merge-review` and one `ont search` of `unit` with `eq` on `state` `merged`. The yield is the share of findings of `batch:merge-review` planned into a unit that merged over all findings of the detector, read by the Lead and stored by no program of this unit.

## What the job does not show

- A verdict is a model's reading of a diff against its unit page and requirement lines, not an execution of the code or the claimed reproduction command; every finding stays `info` until the Lead reproduces it in a disposable worktree.
- Only the first-parent diff, the unit page's members, and the `REQ_RE` lines of named spec files are shown to the model: surrounding source files, helpers, tests outside the diff, and specification prose outside numbered requirement lines are not included.
- Pull requests merged without a unit, Academy units and paths, items whose user text exceeds 300,000 bytes (`too_large`), and items beyond `--max-requests` (`over_max`) are not sent.
- Only the first five defects of an item enter the finding's evidence and the pull request comment; each pull request receives at most one comment, and verdicts from different models are not reconciled.
- A refused, cut, or unparsable answer settles its key in `--results` and is not re-requested until the inputs, prompt, model, or `max_tokens` change; only errored, canceled, or expired lines are requested again.
