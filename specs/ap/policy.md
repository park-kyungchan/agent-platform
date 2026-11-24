# The host's policy of models and accounts for `ap run`

Purpose: one file of the host states which models a stage may run on and which Claude, Codex, Muse or agy accounts a stage of a runtime, role or model may use, and `ap run` enforces it at admission for every stage, so a request written by hand is checked like any other.

## Requirements

- R1: The real host takes the policy file's path from the environment variable `AP_POLICY_FILE` when it is set and not empty, else `/etc/agent-platform/policy.json`, and from no other source.
- R2: When the host names no path or no file exists at the path, whatever its bytes, admission is what it is without a policy and the record's `policy` is null.
- R3: A policy file that is unreadable, is not one JSON object, holds a malformed member, or exists at a path that is not valid UTF-8 (so the record cannot name it) refuses every stage with `policy_invalid` (field `policy`, the reason naming the member).
- R4: The request's `model` must equal an entry of `models.allowed`, else the stage is refused `model_not_allowed` (field `model`).
- R5: The request's `model` must match no pattern of `models.never`, else the stage is refused `model_not_allowed` (field `model`).
- R6: The accounts a stage may use are the list of the first key present among `<runtime>/<role>`, `<runtime>/<model>` and `<runtime>`, in that order.
- R7: A stage for which none of those keys is present is refused `account_not_allowed` (field `account`).
- R8: The request's `account` and each entry of `next_accounts` must be in the stage's list, compared as paths without a trailing `/`, else the stage is refused `account_not_allowed` (field `account` or `next_accounts`).
- R9: A resumed call applies the same account rule to the account its record names.
- R10: The policy step is the last check of admission: its refusals come after every refusal `ap run` had without a policy (the request's own members and the checks of the host's state, which only read) and before any effect, so nothing is claimed, launched or written before one.
- R11: The record `<out>.run.json` holds `policy`, `{"path", "sha256"}` of the file as read, or null when no file was read; the fallback record of a call whose files cannot be opened has no `policy` (it keeps its three members).
- R12: `ap policy check [<file>]` reads `<file>`, else the path of R1, once, and runs over those bytes every reader `ap` has of the policy file, the functions the verbs call, in this order and stopping at the first refusal: `models` (the stage admission's `models`, `accounts` and `api_key_accounts`), `quota` (`ap quota`'s), `units` (`ap unit run`'s), `reviews` (`ap unit review`'s), `repositories` (the clone of `ap unit ready` and `ap unit run`, on each key of `repositories` in the order of the parsed object; an absent member passes, a member that is not an object is refused as the clone's reader refuses it, and an entry without `clone` is `repository_not_mapped`, field `repository`), `work` (`ap work`'s), `check` (`ap check`'s: the member `hosts`, `hosts.pc` and `hosts.pc.stages` among it, `specs/ap/check.md` R16), `writer` (the function `writer` of `crates/ap/src/unit/page_write.rs` that `ap unit write` and `ap work` call, `specs/ap/unit-write.md` R2; an absent member passes), `api` (`ap api batch`'s, the member `api`, `specs/ap/api.md` R3; an absent member passes) and `client` (the remote client's, the member `store_url`, R19; an absent member passes).
- R13: When every reader passes, the check exits 0 and prints on stdout `{"status":"ok","path":<the path read>,"sha256":<the digest of the bytes checked>,"readers":["models","quota","units","reviews","repositories","work","check","writer","api","client"]}`.
- R14: A refusal of a reader exits 2 and prints on stderr `{"error":{"code":<the reader's own code>,"field":<its field>,"detail":<its detail>,"reader":<the reader's name>}}`: the code, field and detail the verb of that reader gives for the same file. A file that is unreadable, not JSON, not one JSON object or too large is the refusal of `models`; a call with another form is `usage`, field `args`, `reader` null, exit 2.
- R15: An absent file is refused `policy_absent`, field `policy`, `detail` null and `reader` null, exit 2.
- R16: The check writes nothing, starts nothing, applies no store action and calls no network: the readers get the one read of the file and no program call.
- R17: The member `api_key_accounts`, optional and top-level, names the accounts whose stages run on the Anthropic API credit (`specs/ap/run.md` R54): an object of 1 to 5 entries, each key an account directory (an absolute path without a NUL byte, without a `..` component and without a final `/`) and each value a key file of the form of `api.key_file` (`specs/ap/api.md` R3: an absolute path directly under `/etc/agent-platform/credentials/`, exactly one component more, not `.` or `..`). Any other form is `policy_invalid` (field `policy`) with a `detail` that begins with `api_key_accounts` (the member: not an object, or fewer than 1 or more than 5 entries) or `api_key_accounts.<key>` (an entry). It is read by the reader of the stage admission, the one `ap run` calls and `ap policy check` runs as its reader `models` (R12): the check names no new reader, and a file with a valid member answers as the same file without it.
- R18: A value of `api_key_accounts` equal to `api.key_file`, when the policy has the member `api` with a string `key_file`, is `policy_invalid` with a `detail` that begins with `api_key_accounts.<key>`: the key of a key account is a stage key of its own, never the key whose spend the credit's ledger counts (`specs/ap/api.md`). A key of `api_key_accounts` need not be listed in `accounts`; the account rule (R6 to R9) is unchanged and alone decides which account a stage may use.
- R19: The member `store_url`, optional and top-level, names the remote `ont serve` endpoint (`specs/ap/client.md`): a string of exact form `http://<tailnet-ip>:<port>`, where `<tailnet-ip>` is an IPv4 literal in `100.64.0.0/10` (`octets[0] == 100 && (octets[1] & 0xc0) == 0x40`) or a bracketed IPv6 literal in `[fd7a:115c:a1e0::/48]` (`segments[0..3] == [0xfd7a, 0x115c, 0xa1e0]`), `<port>` is an integer `1..=65535`, with no DNS hostname, no userinfo (`@`), no path, no query, no fragment, and no whitespace or control characters. It is validated by the reader `client` of `ap policy check` (R12, after `api`); any other form is `policy_invalid` (`field: "policy"`, `detail` starting with `"store_url"`). Reading `store_url` for remote mode does not require `models` or `accounts`.

## Interface

The file is one JSON object. `models` and `accounts` are required; every other top-level member is not read by `ap run`, but the optional `api_key_accounts`. The member `quota` (the runtime CPU quota of a slice) is read only by `ap quota` and by the dispatch pass `ap work`, which starts no unit over its limit: `specs/ap/quota.md`, `specs/ap/work.md`.

- `models`: an object with exactly `allowed` (1 to 64 model names, each as `ap run` takes a model: at most 40 characters of letters, digits and `._-[]`) and `never` (0 to 16 patterns; a pattern is a model name in which `*` stands for any run of characters, at most two `*`).
- `accounts`: an object from a key to an array of 1 to 5 absolute paths. A key is a runtime name (`claude`, `codex`, `muse`, `agy`), or a runtime, `/` and a role of `ap run` (`implementer`, `fixer`, `reviewer` or `writer`, so `accounts` takes the key `<runtime>/writer`), or a runtime, `/` and a model name. The runtime of a stage is the request's `runtime` (default `claude`).

The member `work` (the dispatch pass's slots, slice and prefix, and `work.pages` and `work.page_prefix`, the writers in flight and the prefix of their units) is read only by `ap work`: `specs/ap/work.md`.

The member `writer` (the model, effort, mode, window, bound and pages directory of a page's writer) is read only by `ap unit write` and `ap work`: `specs/ap/unit-write.md` R2, `specs/ap/work.md` R24.

The member `hosts` (the local PC that `ap check` sends a check to: `hosts.pc` with exactly `ssh`, `work` and `busy_load`, and optionally `min_available_mb`, the PC's available memory in MB a call needs, 2,048 when absent, and `stages`, the stage account's `ssh` and `work` where a look may run) is read by `ap check` and by `ap run` for a request with `host` (`specs/ap/run.md` R57): `specs/ap/check.md` R16. A file without it is read by every verb as before, and a request without `host` reads no `hosts`.

The member `api_key_accounts` (account directories that run on a key file of their own) is read by `ap run` (R17, R18; `specs/ap/run.md` R54).

The member `api` (the key file, the stops of the two queues, the day's cap and the jobs of the API credit) is read only by `ap api batch`: `specs/ap/api.md` R3. A file without it is read by every verb as before.

The member `store_url` (the tailnet HTTP URL of `ont serve`) is read only by the remote-enabled client verbs (`ap status`, `ap request`, `ap intent`) and the VPS-only driver guard: R19, `specs/ap/client.md`. A file without it is read by every verb as before.

Refusals, all exit 2 before any effect, `{"error":{"code","field","detail"}}` on stderr:

- `policy_invalid`, field `policy`: `detail` is a sentence that begins with the member (`file`, `models`, `models.allowed`, `models.never`, `accounts`, `accounts.<key>` or `store_url`).
- `model_not_allowed`, field `model`.
- `account_not_allowed`, field `account` (no key for the stage, or the request's `account` not in the list) or `next_accounts` (an entry not in the list).

The record's `policy` is `{"path", "sha256"}` of the file as `ap run` read it, so a stage that ran under another file than the installed one shows it.

## Not included

- No member of the file beyond `models`, `accounts` and `api_key_accounts` is read by `ap run` (`quota` is read by `ap quota` and `ap work` alone, `work` by `ap work` alone, `hosts` by `ap check` and by `ap run` for a request with `host`, `api` by `ap api batch` alone, `store_url` by the remote client verbs and driver guard alone): the scheduler's numbers and every later member come with their own units.
- No check of the model that served: the record's `served_by` is as it was.
- No change to `ap unit`, to `units.json` or to the composition: a unit's stages meet the policy through `ap run`.
- `ap policy check` does not run the checks of `ap unit run` that depend on a unit (the account key of a tier, `account_not_mapped`; the required `reviews.task`): they are not properties of the file alone.
- No installation of the policy file and no default policy: without a file the host allows what `ap run` allowed before.
