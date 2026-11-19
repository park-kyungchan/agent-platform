# `ap deliver` brings the host's main checkouts to `origin/main`

Purpose: a file under a platform repository's main checkout on the host equals `origin/main` after delivery has run, so that an agent that reads it reads the current code. This file holds only that step. The rest of delivery's contract stays in `crates/ap/README.md` "Delivery" until the specification of delivery moves here; a request without `checkouts` behaves as README "Delivery" says.

## Requirements

- R1: The request may carry `checkouts`: an array of 1 to 20 strings, each an absolute path of at most 4,096 bytes with no `.` or `..` component, no control character and no trailing `/`, and no two equal. Anything else refuses `invalid_input` with field `checkouts` (exit 2, nothing written, as for every other member). `checkouts` is one of the members a request may hold, in the timer's form and in a Lead's form, and a rollback request may carry it.
- R2: The step runs only in a call whose `end` is `current` or `installed` and whose request has no `rollback`, after every step that ends the call today and before `last.json` is written, while the call holds the deliver lock. It never runs in a call that ended `refused`, `failed`, `failed_restored`, `not_started` or `unrecorded`, and never in a rollback call. It changes neither `end`, `code` nor the exit.
- R3: For each checkout, in the request's order, the step runs the commands below one by one through the call's `System`, each as `git --no-optional-locks -C <path> -c core.hooksPath=/dev/null -c gc.auto=0 -c maintenance.auto=false <words>`. A command passes only when it exits 0 within its bound and prints what its line asks.
  1. `rev-parse --show-toplevel` (30 seconds) prints exactly `<path>`.
  2. `symbolic-ref --short -q HEAD` (30 seconds) prints `main`.
  3. `status --porcelain --untracked-files=no` (30 seconds) prints nothing. Untracked files do not count.
  4. `fetch --quiet --no-tags origin +refs/heads/main:refs/remotes/origin/main` (60 seconds).
  5. `rev-parse --verify HEAD^{commit}` (the `from`) and `rev-parse --verify refs/remotes/origin/main^{commit}` (the tip), each 30 seconds and 40 hex digits, read once. When they are equal the checkout is `current` and the steps end. Otherwise `merge-base --is-ancestor <from> <tip>` (30 seconds) with the two ids.
  6. `merge --ff-only --quiet <tip>` (600 seconds), with the tip's 40 hex digits and never the ref name, then `rev-parse --verify HEAD^{commit}` (30 seconds) read again.
- R4: The result of a checkout is one of: `not_a_checkout` (step 1 did not pass), `not_on_main` (step 2), `dirty` (step 3 printed something), `fetch_failed` (step 4 failed or timed out), `current` (step 5, equal), `diverged` (the `merge-base` of step 5 did not succeed: `<from>` is not an ancestor of the tip), `forwarded` (step 6, the read after the merge equals the tip) and `git_failed`: step 3 failed, a read of step 5 failed or printed something else than 40 hex digits, or step 6 ended with a `HEAD` that is not the tip. The read after the merge decides, whatever the merge's exit. A result other than `forwarded` and `current` stops the steps of that checkout; the next checkout is still taken.
- R5: No other Git command and no other write touches a checkout: no reset, stash, clean, checkout, commit, push or branch change, and no command after a step that did not pass. A checkout whose result is not `forwarded` is unchanged: its working tree's files, its index's entries, `HEAD` and `refs/heads/*`; step 4 adds objects and moves `refs/remotes/origin/main` and `FETCH_HEAD`.
- R6: The record has the member `checkouts` only when the request holds it: `null` when the step did not run (R2), otherwise an array in the request's order of `{"path", "from", "to", "result"}`. `from` is the `HEAD` read in step 5, `null` when step 5 did not read it. `to` is equal to `from` for `current`; the `HEAD` read after step 6 for `forwarded` and for a `git_failed` of step 6, `null` when that read failed; `null` for every other result. The `invalid_input` record never has the member. The answer on stdout is the same record.
- R7: A request without `checkouts` gives exactly the record and the calls it gave before this specification: no `git -C <checkout>` command and no member `checkouts`.
- R8: `ap deliver check < request.json` runs exactly the request check of `ap deliver` and nothing after it: no state directory, no lock, no fetch, no store action, no `last.json`, no program call. A request that passes exits 0 and prints `{"status":"ok"}`.
- R9: A request the check refuses exits 2 and prints the record `ap deliver` prints for that request (`end` `refused`, `code` `invalid_input`, `detail` `{"field"}`), with the same `at` form and no other difference.
- R10: `ap deliver` without an argument is unchanged, and any other argument, `check <x>` included, is refused `invalid_input` with field `args`.

## Interface

The request member `checkouts`, the record member `checkouts`, the results `forwarded`, `current`, `not_a_checkout`, `not_on_main`, `dirty`, `fetch_failed`, `diverged` and `git_failed`, and the refusal `invalid_input` with field `checkouts`. The form `ap deliver check`.

## Limits

- The fetch of step 4 uses the remote `origin` the checkout names: the checkout of a private repository fetches with the credential helper of the service's user (`HOME=/root`), unlike the fetch of the pair, which reads a public URL.
- A `git_failed` after a merge cut by its bound can leave the checkout part-written (changed tracked files, a lock file). The next run then reads it `dirty` or `git_failed` until a person repairs it. Delivery repairs nothing.
- A checkout read `dirty` is left as it is; saving its edits is a person's step, never delivery's.
