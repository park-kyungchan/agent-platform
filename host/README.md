# Host configuration records

Recorded copies of host configuration files (the login path and the public-interface
firewall among them), the source of the host's policy file with the script that
installs it (`scripts/install-etc.sh`), and the system Git configuration. No program is
built here. Agents follow [`AGENTS.md`](AGENTS.md).

## Contents

| Path | What it is |
| --- | --- |
| `deployments/host-artifacts.json` | The inventory: for each recorded copy, the host file (`source`), the copy (`destination`) and the SHA-256 of the copy's bytes (`sha256`) |
| `deployments/ssh/` | The Tailscale-only sshd drop-in, the plain-ssh drop-in `00-01-ssh-only.conf` and the `ssh.socket` override |
| `deployments/tsidp/compose.yaml` | The tsidp compose file |
| `deployments/host-cron/` | The Docker prune cron files |
| `etc/agent-platform/policy.json` | The source of the host's `/etc/agent-platform/policy.json` (section "The policy file") |
| `scripts/install-etc.sh` | Installs the policy file from a merged commit, checks it against the host, or puts a kept copy back |
| `scripts/test-install-etc.sh` | The tests of `install-etc.sh`, run by `./check` |
| `scripts/retire-hindsight.sh` | Retires the legacy long-term memory service from the host: `check`, `copy`, `delete <copy directory>`, `verify <copy directory>` (section "Retiring the legacy memory service") |
| `scripts/retire-hindsight.json` | The script's list: the containers, volumes, network, directory, copy root and settings entries it removes, and the settings files it only scans |
| `scripts/test-retire-hindsight.sh` | The tests of `retire-hindsight.sh` against a scratch root and a stand-in docker program, run by `./check` |
| `git/` | The system Git configuration for the engineering scopes: the roster check (`git/hooks/pre-commit`), the configuration files and the alias `git task-new` (section "Git configuration") |
| `scripts/test-git-config.sh` | The tests of `git/`, run by `./check` |
| `deployments/host-firewall/` | The nftables table of the public interface `eth0` and the unit that loads it at boot; its `README.md` maps each file to its host path |

The copies the inventory names are byte copies of the host files: `deployments/ssh/00-01-ssh-only.conf`
and `deployments/host-firewall/` as captured on 2026-11-22, the others on 2026-11-04. Each
directory's `README.md` maps its files to their host paths.

## Who uses it

The Lead reads these records. `ap deliver` runs `scripts/install-etc.sh` (`check`,
then `install` of a merged commit), because the host's `/etc/agent-platform/deliver.json`
has the member `etc` since 2026-11-21 22:39Z; the Lead runs it by hand only for a
rollback or a fast-path order. The Lead runs `scripts/retire-hindsight.sh` by hand, once,
in the order of the section "Retiring the legacy memory service"; it reads its list
`scripts/retire-hindsight.json` and nothing runs it on its own. The watch's drift detector compares the host's policy
file with `etc/`. The recorded copies under `deployments/` change on the host only by
the Lead's hand under the procedure in `AGENTS.md`.

## Check

```bash
./check
```

A healthy run ends with `check: ok`. The check needs no network, no root and no
host path. It parses every tracked JSON file, runs `bash -n` on every tracked shell
script, verifies that every inventory row's copy exists and matches its recorded
SHA-256, and fails on a file under `deployments/` that is not the inventory, a row's
copy or a `README.md`, and runs
`scripts/test-git-config.sh`, `scripts/test-install-etc.sh` and
`scripts/test-retire-hindsight.sh` (the four words of the retirement script against a
scratch root, a stand-in docker program and a tar wrapper; no docker and no host file). The GitHub Actions job
`rust` runs the same command and is the required check of `main`.

The check compares each copy with its recorded digest, never with the host. A copy
is refreshed only from the host: its bytes unchanged, with its new digest written
into the inventory in the same commit.

## The policy file

`etc/agent-platform/policy.json` is the source of `/etc/agent-platform/policy.json`.
A change is a pull request; after the merge `ap deliver` runs
`scripts/install-etc.sh` (`check`, then `install <merge>`), because the host's
`/etc/agent-platform/deliver.json` has the member `etc` since 2026-11-21 22:39Z; the
Lead runs it by hand only for a rollback or a fast-path order. The script installs
only that one file:

| Form | What it does |
| --- | --- |
| `install <commit> [--past <commit>:<reason>]...` | Installs the file from a commit on the first-parent history of `origin/main`, after the accepted check (each merge since the installed one has the tree of its second parent and a closed accepting review in the store (a review that `ap unit run` carried to an updated head counts by the pull request of the review it was carried from)) and `ap policy check` on the staged bytes. It keeps the previous bytes in `/var/backups/agent-platform-etc/policy.json.before-<UTC stamp>`, replaces the target by rename, reads it back and records the commit and digest in `/var/lib/agent-platform/etc/installed` |
| `check [<commit>]` | Prints `same` (exit 0) or `differs <target>` (exit 1) against `<commit>`, by default the tip of `origin/main`; it writes nothing but its own clone |
| `rollback <kept file>` | Puts one kept `policy.json.before-<stamp>` back, after `ap policy check`, keeping the current bytes first |

The policy's v3 members: `units.approval` says which unit of class contract merges on its verifier's accept (its head meets no path of `guarded`, and `sample` sets how many accepted merges are drawn for a later look); `work.slots` is how many units the dispatch pass runs at once (6); and `routing` holds the tier `order`, the `large_lines` bound, the `map` of implementer and verifier tiers by kind and size, and the `bar` a tier must meet.
The member `routing.order` names two AGY pairs: `["agy","gemini-4-hydrogen-high"]` stands directly after `["claude","sonnet"]` and `["agy","gemini-4-neon-high"]` directly after `["claude","opus"]`. The order decides: a tier is admitted where its pair stands at or after the row's pair, so Neon is admitted wherever the map gives Sonnet or Opus, and Hydrogen only where it gives Sonnet or a pair before it; no row of the map changes.
A unit whose tier names a runtime outside `routing.order` is refused `tier_not_in_order` until a later policy unit adds it.
Muse left the policy on the user's word of 2026-11-24: its models and its account are no longer in the file.

The script works in its own clone, `/var/lib/agent-platform/clones/agent-vps-operations`
(never in a main checkout), holds the lock `/var/lib/agent-platform/etc/lock`, and
refuses with a code on stderr and exit 2 before any write (`busy`, `not_forward`,
`not_accepted`, `tree_differs`, `verify_failed`, `target_changed`,
`target_not_regular` and others). `AP_ETC_ROOT` prefixes every host path for the
tests. `ap deliver` and the Lead run it; `deliver.json` is not installed from here (its source
is `agent-platform` `deploy/deliver.json`). `AGENTS.md` states the rules, including
the fast path for a change that must take effect at once.

## Retiring the legacy memory service

The user ordered on 2026-11-24 that the legacy long-term memory service be deleted
from the host; a new service is deployed later from nothing.
`scripts/retire-hindsight.sh` does it, and touches only what
`scripts/retire-hindsight.json` lists: the two containers, their two volumes, the
network, the directory `/docker/hindsight`, the images the copy's manifest records
from the two containers (by id), each volume's data directory where
`docker volume inspect` places it (read only), the copy root
`/root/.hindsight-recovery` and the entries that register the service's MCP server
or its skills in the CLIs' settings and in the Academy workspace. The settings files
of the list's `scanned` member are read and never changed: a mention of the service
inside their listed key paths (hook commands, MCP servers at the top and under each
project, permission entries) is a hit, printed as the file and the key's path; a
mention anywhere else is none. The service's banks are the data of the two volumes.

| Word | What it does |
| --- | --- |
| `check` | One line a target, `present` or `absent` (a container with its state and image id), and the scan's hits; changes nothing, exit 0 |
| `copy` | Refuses `container_running`. Makes `<copy root>/final-<UTC time>/` (mode `0700`, and the copy root with mode `0700` when it is missing) with a tar archive of each volume's data directory and of the directory (numeric owners kept), a copy of each settings file an entry names and of the launcher, and `MANIFEST.sha256` over them, which also records each container's image id. It reads every archive back whole (member list and digest), then prints `copy <directory>` and `manifest_sha256 <digest>`; on a failed read-back it removes that directory and exits 1 |
| `delete <copy directory>` | Refuses `copy_not_verified` (not a `final-*` directory directly under the copy root, a manifest that does not verify now, or a target present now that it does not list), `container_running` and `json_unreadable` before any removal. Then removes, in order, the entries, the containers, the network, the volumes, each recorded image by its id (an image docker refuses is printed `kept`), and the directory, one line a step; a target already gone is `gone`; a failed removal stops the call with exit 1 |
| `verify <copy directory>` | Refuses as `delete` does; exit 0 when no container, network, volume, directory or entry of the list is left and the scan has no hit (a recorded image still present is printed `kept` and is no failure), else exit 1 with what is left |

`--root <dir>` puts every path of the list under `<dir>` (for the tests); `--docker
<program>` names the program used as docker. The archives hold the directory's `.env`
files; the script prints paths, names, ids and digests only.

**The Lead's order**, on the host, from a clean worktree at the merge commit:

1. open a finding that names the user's request as the user's word for the listed
   paths in the Academy workspace, and lift the hold on them by that request;
2. `scripts/retire-hindsight.sh check`;
3. `scripts/retire-hindsight.sh copy`;
4. add the copy directory and the manifest's sha256 to the finding;
5. `scripts/retire-hindsight.sh delete <copy directory>`, while no session of a CLI
   whose settings file is an entry runs (it would write the file back from its memory);
6. `scripts/retire-hindsight.sh verify <copy directory>`; a hit in a scanned file is the
   Lead's to remove by hand and to name in the finding.

**Restore** from the copy directory (`sha256sum -c MANIFEST.sha256` there first):
for each volume, `docker volume create <name>`, then
`tar --numeric-owner -xpf volume-<name>.tar -C <its mountpoint>`; the directory with
`tar --numeric-owner -xpf directory.tar -C /docker`, which brings back the compose
file; each settings file or launcher from its `file-<n>-<name>` copy, with the mode
and owner its manifest record names. The images are not in the copy: `docker compose
pull` pulls them again by the names the compose file gives. The copy root stays until
the user says otherwise.

## Not changed here

The login path as it is now (`deployments/ssh/90-tailscale-only.conf`,
`deployments/ssh/00-01-ssh-only.conf`, the `ssh.socket` override and
`deployments/host-firewall/`) is changed only by the user's procedure described in
`AGENTS.md`.
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
