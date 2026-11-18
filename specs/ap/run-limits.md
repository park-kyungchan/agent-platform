# A reviewer stage of `ap run` with limits: it writes only its CLI home and a scratch directory

Purpose: a reviewer stage on the Claude CLI that asks for `limits` cannot write the file system outside its account (the CLI's home) and one scratch directory that `ap run` makes for it and removes after it, so that a review cannot change the worktree, the host or another stage's files by mistake. The stage stays the transient scope it is today; the limits come from `bwrap` (bubblewrap) as the scope's command, in front of the CLI.

## Requirements

- R1: A request may carry `limits`, `{"scratch": <absolute path>}`, only with `role` `reviewer` and `runtime` `claude`. Each of these refuses `invalid_input` with field `limits`: another role or runtime; a `limits` that is not an object of exactly the member `scratch`; a `scratch` that is not an absolute path or holds a `..` component; a `scratch` inside the request's `account`, inside an entry of `next_accounts` or inside `cwd`, or one that holds `cwd`, `out` or `prompt` ("inside" and "holds" compare path components as written, equality included); `limits` together with `expect.file`.
- R2: `scratch` must not exist when the call starts, as a directory, a file or a link (`scratch_exists`, field `limits`), and its parent must be a directory that is not a link (`scratch_parent_missing`, field `limits`). These checks are checks of the request: they come before the policy step (`specs/ap/policy.md` R10), `ap unit`'s admission of a stage runs them too, and a refused request makes nothing.
- R3: After the checks of R2 and before the policy step, `ap run` calls `/usr/bin/bwrap --version` once, bound to 10 seconds. A call that fails, times out or does not exit 0 refuses `limits_unavailable` (field `limits`) before anything is made or launched.
- R4: After the claim of `out` and before the first launch, `ap run` makes `scratch` once for the call, by one `mkdir` (not recursive) that fails when the path exists, with mode 0700, then `tmp/` and `target/` in it. Every launch of the call (the accounts of `next_accounts` included) uses that one directory. When one of these three fails, the call ends `failed` with `unmet: launch` and nothing is launched.
- R5: Every launch places, after `systemd-run … --` and before the CLI's argv, `/usr/bin/bwrap --unshare-pid --ro-bind / / --dev /dev --proc /proc --ro-bind /proc/sys /proc/sys --bind <home> <home> --bind <scratch> <scratch> --cap-drop ALL --chdir <cwd> --`, where `<home>` is the account of that launch (the `CLAUDE_CONFIG_DIR` of the CLI: the request's account, an account of `next_accounts`, or the account of a resumed record). `--unshare-pid` is the one `--unshare-*` flag, and there is no `--die-with-parent`.
- R6: The CLI's environment gets `TMPDIR=<scratch>/tmp` and `CARGO_TARGET_DIR=<scratch>/target`; these replace a value of the caller's environment and of the request's `env`.
- R7: The record `<out>.run.json` has the member `limits`, `{"scratch", "writable": [<home>, <scratch>], "bwrap": "/usr/bin/bwrap"}` with the `<home>` of the launch it records, from the record written before the launch on, only when the request has `limits`.
- R8: When the stage has ended while `ap run` waits, at any end and after the call's last launch, `ap run` removes `scratch` and everything under it, only when its own `mkdir` of R4 succeeded in this call, and only when the path is a directory and not a link; the removal follows no link inside it. A directory another call made (its `mkdir` came first) is never removed.
- R9: A request without `limits` gives the same argv, environment, record (no key `limits`) and answer as without this specification, and `bwrap` is not called.

## Interface

The request member: `"limits": {"scratch": "/abs/path"}`. The record member: `"limits": {"scratch": "/abs/path", "writable": ["<account of the launch>", "/abs/path"], "bwrap": "/usr/bin/bwrap"}`. The fallback record of a call whose files cannot be opened keeps its three members (nothing is made for it).

Refusals, all exit 2 before any effect, `{"error":{"code","field","detail"}}` on stderr: `invalid_input`, `scratch_exists`, `scratch_parent_missing`, `limits_unavailable`, each with field `limits`.

Under the limits the root is read-only to the stage's processes, `/proc/sys` too, and they hold no capability (`--cap-drop ALL`). The stage has its own pid namespace (`--unshare-pid`): its `/proc` shows only its own processes, so no other stage and no process of the host is visible there, and `/proc/1` is `bwrap`'s own init in that namespace, whose `/proc/1/root` is the stage's own view of the file system, read-only but for the two paths. The stage's own `/dev` (with `/dev/shm`) and its own `/proc/self` files are writable. The pid namespace changes no control group: the whole process tree stays in the scope, so the scope, its bound, its `BindsTo` and its stop are as without limits, and a stage outlives a killed `ap run` until its own bound. A call whose `ap run` is killed leaves the scratch directory, and its record names it; nothing else removes it. The prompt, the stream and the error file are descriptors `ap run` opened, and `ap run` writes the answer file itself, outside the limits.

## Not included

- Two writes of the Claude CLI outside `<home>` and `<scratch>`, which the CLI catches and warns about: its version lock under `/root/.local/state/claude/locks/`, without which a later update of the CLI may delete the binary a running stage runs from; and the legacy sign-in lock `/root/.claude.lock` beside the home.
- Limits for implementers and fixers, limits as the default, and their use by `ap unit` or `ap unit review`.
- Separate OS users and a change of the permission mode.
- A guard against a stage that means harm: the service manager's socket stays reachable, so the limits guard against a write by mistake only.
