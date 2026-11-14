# evals

`readout.py` reads the cost of the same work (K1) and the field reading of the implementer tier (K2) from records that already exist; its prices are `prices.tsv`.

## The engineering suite

`engineering/` holds the first cut of the suite: past units of the platform repositories that a stage of a chosen model runs again, judged by the tests the unit was accepted with.

- A case is one file `engineering/cases/<id>.json`: the merged pull request it was taken from, its `base` (the first parent of the merge commit) and accepted `head`, the `tests` (every file that judges), the `commands` the evaluator runs, and the SHA-256 of the page given to the candidate. Its exact form and the call that runs it: `crates/ap/README.md`, section "One case".
- The rule of `tests`, for every case: every path the pull request changed under a `tests` directory, and, where `./check` is a command, `check` and each script it sources. Every path of `tests` is a regular file at `head`: a symbolic link is no judging file, and a case that names one is refused (and the evaluator exits 2 for it, writing nothing). A case whose base `check` runs installs or touches the host (as `ap-158`) is judged by the cargo commands of its check instead.
- The pages are host files written by the Lead, `evals/engineering/pages/<id>.md` in the Lead's task directory below the work root; their text never enters this repository, only its digest (`page_sha256`), and a case file holds no host path.
- `engineering/evaluate <case> <clone>` is the evaluator: it restores the files of `tests` from `head`, runs `commands` in the copy and exits 0 (passed), 1 (failed) or 2 (it could not judge). The restore is total: whatever the candidate left at a judging path or at one of its parents (a file, a link, a directory) is replaced, and nothing is written through a link. Exit 1 is a statement about the candidate and nothing else: an error of the evaluator itself is exit 2. No model judges.
- `engineering/prompt.md` is the suite's prompt to the candidate; the call appends the block of facts.
- The validity pair: a case is valid when its call without a stage ends `failed` at `base` and `passed` at `head`. A case with another pair counts in no reading.
- What a case record means as a row of a reading (the target, the cost, pass, fail, no result, the rerun of a no-result row): `crates/ap/README.md`, section "One case", the last paragraph.
