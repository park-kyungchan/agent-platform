# Host configuration records

Recorded copies of host configuration files, the Herdr network's files, and the
source of the host's policy file with the script that installs it
(`scripts/install-etc.sh`), and the system Git configuration. No program is
built here. Agents follow [`AGENTS.md`](AGENTS.md).

## Contents

| Path | What it is |
| --- | --- |
| `deployments/host-artifacts.json` | The inventory: for each recorded copy, the host file (`source`), the copy (`destination`) and the SHA-256 of the copy's bytes (`sha256`) |
| `deployments/ssh/` | The Tailscale-only sshd drop-in and the `ssh.socket` override |
| `deployments/hindsight/compose.yaml` | The Hindsight compose file |
| `deployments/tsidp/compose.yaml` | The tsidp compose file |
| `deployments/host-cron/` | The Docker prune cron files |
| `etc/agent-platform/policy.json` | The source of the host's `/etc/agent-platform/policy.json` (section "The policy file") |
| `scripts/install-etc.sh` | Installs the policy file from a merged commit, checks it against the host, or puts a kept copy back |
| `scripts/test-install-etc.sh` | The tests of `install-etc.sh`, run by `./check` |
| `git/` | The system Git configuration for the engineering scopes: the roster check (`git/hooks/pre-commit`), the configuration files and the alias `git task-new` (section "Git configuration") |
| `scripts/test-git-config.sh` | The tests of `git/`, run by `./check` |
| `deployments/herdr-network/` | The Herdr network's sshd drop-in, its installer, the firewall helper, its unit and contract; its `README.md` maps each file to its host path |

The copies the inventory names are byte copies of the host files as captured on
2026-11-04. Each directory's `README.md` maps its files to their host paths.

## Who uses it

The Lead reads these records and runs `scripts/install-etc.sh`. A merge here
changes nothing on the host until the Lead runs `install-etc.sh install <merge>`;
no timer reads this repository.

## Check

```bash
./check
```

A healthy run ends with `check: ok`. The check needs no network, no root and no
host path. It parses every tracked JSON file, runs `bash -n` on every tracked shell
script, verifies that every inventory row's copy exists and matches its recorded
SHA-256, and fails on a file under `deployments/` that is not the inventory, a row's
copy, a `README.md`, or under `deployments/herdr-network/`, and runs
`scripts/test-git-config.sh` and `scripts/test-install-etc.sh`. The GitHub Actions job
`rust` runs the same command and is the required check of `main`.

The check compares each copy with its recorded digest, never with the host. A copy
is refreshed only from the host: its bytes unchanged, with its new digest written
into the inventory in the same commit.

## The policy file

`etc/agent-platform/policy.json` is the source of `/etc/agent-platform/policy.json`.
A change is a pull request; after the merge the Lead runs
`scripts/install-etc.sh install <merge>`. The script installs only that one file:

| Form | What it does |
| --- | --- |
| `install <commit> [--past <commit>:<reason>]...` | Installs the file from a commit on the first-parent history of `origin/main`, after the accepted check (each merge since the installed one has the tree of its second parent and a closed accepting review in the store) and `ap policy check` on the staged bytes. It keeps the previous bytes in `/var/backups/agent-platform-etc/policy.json.before-<UTC stamp>`, replaces the target by rename, reads it back and records the commit and digest in `/var/lib/agent-platform/etc/installed` |
| `check [<commit>]` | Prints `same` (exit 0) or `differs <target>` (exit 1) against `<commit>`, by default the tip of `origin/main`; it writes nothing but its own clone |
| `rollback <kept file>` | Puts one kept `policy.json.before-<stamp>` back, after `ap policy check`, keeping the current bytes first |

The script works in its own clone, `/var/lib/agent-platform/clones/agent-vps-operations`
(never in a main checkout), holds the lock `/var/lib/agent-platform/etc/lock`, and
refuses with a code on stderr and exit 2 before any write (`busy`, `not_forward`,
`not_accepted`, `tree_differs`, `verify_failed`, `target_changed`,
`target_not_regular` and others). `AP_ETC_ROOT` prefixes every host path for the
tests. Only the Lead runs it; `deliver.json` is not installed from here (its source
is `agent-platform` `deploy/deliver.json`). `AGENTS.md` states the rules, including
the fast path for a change that must take effect at once.

## Not changed here

The login path (the sshd drop-ins, the `ssh.socket` override,
`00-herdr-runtime-key-only.conf` and `deployments/herdr-network/sshd-policy.sh`,
installed as `/usr/local/sbin/herdr-network-sshd`, which writes that drop-in and
reloads `ssh`) is changed only by the user's procedure described in `AGENTS.md`.
Environment files, credentials and host keys are never tracked.

## Git configuration

`git/` holds what the Lead installs into `/usr/local/lib/agents-git/` and
`/etc/gitconfig`. Nothing here installs it.

| File | What it is |
| --- | --- |
| `git/system.gitconfig` | The lines of `/etc/gitconfig`: the alias file for every repository, and the `includeIf "gitdir:…"` lines for the scopes below |
| `git/work.gitconfig` | For a task directory: `core.hooksPath` (the hook directory), `push.default = upstream`, `agents.roster` (a glob of the Academy's roster files) |
| `git/clones.gitconfig` | For the two clones: `core.hooksPath` and `agents.roster`, no `push.default` (a branch made by `ap unit run` tracks `origin/main`, and `upstream` would push it to `main`) |
| `git/alias.gitconfig` | The alias `git task-new <name>` |
| `git/hooks/pre-commit` | The roster check |

**Scopes.** The task directories `work/ws-*`, `work/ap-*`, `work/ont-*` and
`work/mem-*`, and the clones `platforms/ontology-platform` and
`platforms/agent-vps-operations`, each with its linked worktrees (the patterns
end in a slash and carry no `.git`, so Git matches the main checkout's `.git`
and every `.git/worktrees/<n>`). `agent-platform` and `education-platform` are
not named: a worktree of each lies in an Academy directory, and the Academy's
files stay out of this configuration. `agent-platform` gets one more
`includeIf` line later. The configuration is the system file, not
`/root/.gitconfig`, so that a session whose `HOME` is not `/root` reads it too.

**A repository inside a scope directory is a linked worktree, never a full
clone.** A full clone inside `work/ws-*` would match the task-directory pattern
and get `push.default = upstream` and the hook; a linked worktree's Git
directory lies in its clone and matches only the clone's line.

**The check** (`git/hooks/pre-commit`) reads the roster files named by
`agents.roster`, each a JSON array of objects. The terms are the values of
`student_name` and `student_id` of at least 2 characters (a number counts as its
decimal text). It refuses with `roster_unreadable` when the key is absent, the
glob names no file, a file is unreadable or not a JSON array of objects, or an
object yields no term (an empty array passes beside other files, but the files
together must yield a term). It refuses with `private_term_staged` when a staged
path holds a term (one line with a count, no path) or an added line holds a term
(one line per path with the count). It prints no term, no line of a diff and no
roster content; it makes no network call. Terms are written only to a `0600`
temporary file that a `trap` removes, and reach `grep` only through `-f`.
It reports `staged_unreadable` when Git cannot read the staged changes.

**The alias** `git task-new <name>` (`<name>` is `ws-`, `ap-`, `ont-` or `mem-`
and lower-case words joined by `-`, at most 64 characters) makes
`/home/palantirkc/work/<name>`, runs `git init -b main`, adds the remote
`origin` (`AGENTS_TASK_REMOTE`, default the program repository), makes one empty
commit `task <name> started` and pushes `main` to the branch `task/<name>`. It
writes nothing else. Its own commit runs the check: while the roster cannot be
read it stops after `remote add`, the directory holds only `.git`, and the name
answers `task-new: exists` until that directory is removed. `AGENTS_WORK_ROOT`
overrides the base directory (for tests).

**Install and undo** (the Lead, after a merge):

```bash
install -d -m 0755 /usr/local/lib/agents-git/hooks
install -m 0644 git/work.gitconfig git/clones.gitconfig git/alias.gitconfig /usr/local/lib/agents-git/
install -m 0755 git/hooks/pre-commit /usr/local/lib/agents-git/hooks/pre-commit
install -m 0644 git/system.gitconfig /etc/gitconfig
# undo
rm -f /etc/gitconfig && rm -rf /usr/local/lib/agents-git
```

`install` of `/etc/gitconfig` assumes the file is absent; if it exists, merge the
lines of `git/system.gitconfig` into it instead, and the undo removes those lines.
Compare each installed file with `cmp`. Read the result with
`git config --show-scope --show-origin --get core.hooksPath` in a scope directory.

**`--no-verify` is not used by an agent.** If the Academy's capture moves,
change the glob: `git config --system agents.roster '<new glob>'` (read back
with `git config --show-origin --get agents.roster` in a scope directory; undone
with `git config --system --unset agents.roster`). The new value is written at
the end of `/etc/gitconfig`, after the `includeIf` lines, so it wins.

**What the check does not see.** `git merge --no-ff`, `cherry-pick`, `rebase`,
`revert`, `am`, `commit-tree` and `stash` (they make commits without running
`pre-commit`, or run it on content already written); `--no-verify`;
`-c core.hooksPath=…`; `-c agents.roster=…`; `GIT_CONFIG_NOSYSTEM`; commit
messages; compressed content (a packed archive, an image). A merge on GitHub
makes no local commit.
