# The monthly API credit: key and limits

## What this is

The host holds one key of the Claude API for batches, paid from a monthly credit, and a set of limits for it. The key is a file that the user writes by hand (step H3 below); the limits are the member `api` of the host's policy file, `etc/agent-platform/policy.json`.

Who uses them: `ap api batch` of lifecycle-platform (unit api-batch, its contract `specs/ap/api.md`). It reads `api` before any call and refuses a batch, before any call, when the settled spend plus the reserved spend plus the batch's worst case passes the job's cap, the day's cap or the stop. Later the timers of unit api-timer use the same member.

A merge of this file or of the policy changes nothing on the host until delivery installs the policy (`ap deliver` runs `scripts/install-etc.sh`, README.md "Who uses it").

## The member `api`

Amounts are whole USD, as JSON integers.

| Name | Value | What it bounds |
| --- | --- | --- |
| `key_file` | `/etc/agent-platform/credentials/anthropic-api-key` | The path of the key file (owner root, mode 0600). The policy holds the path and never a byte of the key |
| `stops_usd.standard` | 160 | The month's stop for the standard queue |
| `stops_usd.soak` | 185 | The month's stop when the soak queue runs too; below the Console workspace's limit of 190 |
| `day_cap_usd` | 40 | The most a day may spend, all jobs together |
| `jobs.drift.cap_usd` | 60 | The month's spend of job `drift` |
| `jobs.drift.models` | `claude-sonnet-5-5`, `claude-opus-5-5` | The Claude API models the job may use |
| `jobs.drift.max_tokens` | 8000 | The output cap of one request |
| `jobs.drift.max_requests` | 200 | The most requests of one batch |
| `jobs.merge-review.cap_usd` | 45 | The month's spend of job `merge-review` |
| `jobs.merge-review.models` | `claude-opus-5-5`, `claude-fable-5-2` | The models the job may use |
| `jobs.merge-review.max_tokens` | 8000 | The output cap of one request |
| `jobs.merge-review.max_requests` | 100 | The most requests of one batch |
| `jobs.mining.cap_usd` | 18 | The month's spend of job `mining` |
| `jobs.mining.models` | `claude-sonnet-5-5`, `claude-fable-5-2` | The models the job may use |
| `jobs.mining.max_tokens` | 8000 | The output cap of one request |
| `jobs.mining.max_requests` | 400 | The most requests of one batch |
| `jobs.evals.cap_usd` | 39 | The month's spend of job `evals` |
| `jobs.evals.models` | `claude-haiku-5-5`, `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-fable-5-2` | The models the job may use |
| `jobs.evals.max_tokens` | 8000 | The output cap of one request |
| `jobs.evals.max_requests` | 300 | The most requests of one batch |

What each job reads:

- `drift`: the numbered requirements and their citing tests.
- `merge-review`: a merged pull request against its page.
- `mining`: the week's review answers and stops.
- `evals`: the instruction suites and the reading of each spec file.

The models are Claude API identifiers, not the CLI's names: `fable`, `opus`, `sonnet` and `haiku` stay in `models.allowed` alone.

The pairs that can never send a full batch. At the batch output price, half the list price (`claude-fable-5-2` 20, `claude-opus-5-5` 10, `claude-sonnet-5-5` 5, `claude-haiku-5-5` 0.25 USD a million tokens), `max_requests` × `max_tokens` of output alone is 64 USD for `mining` and 48 USD for `evals` on `claude-fable-5-2`. Both are above their caps (18 and 39), and 48 is also above the day's cap (40). `ap api batch` therefore refuses such a full batch before any call. A batch of either job on that model is smaller: mining's one synthesis request, evals' spec reading.

## The hand steps

The user acts in H1 and H3, the Lead in H2. No agent opens the Console or handles the key. Every readback runs on the host, outside any stage: in a stage's view `/etc/agent-platform/credentials` is an empty directory of its own (lifecycle-platform `specs/ap/run-limits.md` R10), whatever its mode on the host.

- **H1.** In the Console, a workspace of its own for the host's batches, with a monthly spend limit of 190 USD, and auto-reload off. Readback: the user reads both in the Console and tells the Lead, who records the user's words. Undo: the user's, in the Console.
- **H2** (the Lead, before H3). The directory: `install -d -m 0700 -o root -g root /etc/agent-platform/credentials`, which makes it, or sets an existing one to that owner and mode (no change where it already is so). Readback: `stat -c '%a %U:%G' /etc/agent-platform/credentials` prints `700 root:root`. Undo: `chmod <the mode read before H2> /etc/agent-platform/credentials`, or `rmdir` of a directory H2 made while it is empty.
- **H3.** A key of that workspace, written by the user into `/etc/agent-platform/credentials/anthropic-api-key` (owner and group root, mode `0600`), in the form lifecycle-platform `specs/ap/api.md` states. Readback (the Lead): `stat -c '%a %U:%G' /etc/agent-platform/credentials/anthropic-api-key` prints `600 root:root`, and H2's readback still prints `700 root:root`. Undo: the user disables the key in the Console, then the file is removed.

The key is never in an argument, an environment variable, a record, a prompt, a chat message or a file of a repository, and no command of this README prints, copies or reads a byte of it.

## The Lead's steps after the merge

Delivery installs the policy. Readbacks:

- `ap policy check /etc/agent-platform/policy.json` prints `"status":"ok"` and lists `api` among `readers`.
- `scripts/install-etc.sh check` prints `same`.

Undo: `scripts/install-etc.sh rollback <kept file>`.

## The real run

The Lead runs, on the host:

1. The two readbacks of the section above.
2. The readbacks of H2 and H3.
3. A stage of role `reviewer` and one of role `implementer`, each started by `ap run` with `limits` and asked to run `test -r /etc/agent-platform/credentials/anthropic-api-key; echo $?`. Each prints a non-zero status, because the credential directory is hidden from a stage's view (lifecycle-platform `specs/ap/run-limits.md` R10, unit sec-mount).
