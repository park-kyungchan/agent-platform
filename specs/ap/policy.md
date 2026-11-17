# The host's policy of models and accounts for `ap run`

Purpose: one file of the host states which models a stage may run on and which Claude, Codex, Muse or agy accounts a stage of a runtime, role or model may use, and `ap run` enforces it at admission for every stage, so a request written by hand is checked like any other.

## Requirements

- R1: The real host takes the policy file's path from the environment variable `AP_POLICY_FILE` when it is set and not empty, else `/etc/agent-platform/policy.json`, and from no other source.
- R2: When the host names no path or the file does not exist, admission is what it is without a policy and the record's `policy` is null.
- R3: A policy file that is unreadable, is not one JSON object, or holds a malformed member refuses every stage with `policy_invalid` (field `policy`, the reason naming the member).
- R4: The request's `model` must equal an entry of `models.allowed`, else the stage is refused `model_not_allowed` (field `model`).
- R5: The request's `model` must match no pattern of `models.never`, else the stage is refused `model_not_allowed` (field `model`).
- R6: The accounts a stage may use are the list of the first key present among `<runtime>/<role>`, `<runtime>/<model>` and `<runtime>`, in that order.
- R7: A stage for which none of those keys is present is refused `account_not_allowed` (field `account`).
- R8: The request's `account` and each entry of `next_accounts` must be in the stage's list, compared as paths without a trailing `/`, else the stage is refused `account_not_allowed` (field `account` or `next_accounts`).
- R9: A resumed call applies the same account rule to the account its record names.
- R10: The policy's refusals come after every refusal of the request's own shape and before any effect: no claim of `out`, no scope, no file written.
- R11: The record `<out>.run.json` holds `policy`, `{"path", "sha256"}` of the file as read, or null when no file was read; the fallback record of a call whose files cannot be opened has no `policy` (it keeps its three members).
- R12: The refusals that ask the host (`run_still_active`, `worktree_dirty`, `worktree_unreadable`, `systemd_absent` and `bind_inactive`) come after the policy's refusals, so a stage the policy refuses is refused for the policy whatever else is wrong with the host's state.

## Interface

The file is one JSON object. `models` and `accounts` are required; every other top-level member is not read by `ap run`.

- `models`: an object with exactly `allowed` (1 to 64 model names, each as `ap run` takes a model: at most 40 characters of letters, digits and `._-[]`) and `never` (0 to 16 patterns; a pattern is a model name in which `*` stands for any run of characters, at most two `*`).
- `accounts`: an object from a key to an array of 1 to 5 absolute paths. A key is a runtime name (`claude`, `codex`, `muse`, `agy`), or a runtime, `/` and a role of `ap run` (`implementer`, `fixer`, `reviewer`), or a runtime, `/` and a model name. The runtime of a stage is the request's `runtime` (default `claude`).

Refusals, all exit 2 before any effect, `{"error":{"code","field","detail"}}` on stderr:

- `policy_invalid`, field `policy`: `detail` is a sentence that begins with the member (`file`, `models`, `models.allowed`, `models.never`, `accounts` or `accounts.<key>`).
- `model_not_allowed`, field `model`.
- `account_not_allowed`, field `account` (no key for the stage, or the request's `account` not in the list) or `next_accounts` (an entry not in the list).

The record's `policy` is `{"path", "sha256"}` of the file as `ap run` read it, so a stage that ran under another file than the installed one shows it.

## Not included

- No member of the file beyond `models` and `accounts`: the scheduler's numbers and every later member come with their own units.
- No check of the model that served: the record's `served_by` is as it was.
- No change to `ap unit`, to `units.json` or to the composition: a unit's stages meet the policy through `ap run`.
- No installation of the policy file and no default policy: without a file the host allows what `ap run` allowed before.
