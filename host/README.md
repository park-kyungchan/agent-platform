# The host's configuration

`host/` of agent-platform holds the files that configure the host: the source of
the policy file `/etc/agent-platform/policy.json` with the script that installs it,
the recorded copies of host configuration files (the login path and the
public-interface firewall among them) with their inventory, the system Git
configuration, and the script that retired the legacy memory service. No program is
built here. Its history is that of the former operations repository, merged in.
The repository's `AGENTS.md`, `REVIEW.md` and `./check` hold for it; this file adds
only the rules of these files.

## Contents

| Path | What it is |
| --- | --- |
| `deployments/host-artifacts.json` | The inventory: for each recorded copy, the host file (`source`), the copy (`destination`, relative to `host/`) and the SHA-256 of the copy's bytes (`sha256`) |
| `deployments/ssh/` | The Tailscale-only sshd drop-in, the plain-ssh drop-in `00-01-ssh-only.conf` and the `ssh.socket` override |
| `deployments/tsidp/compose.yaml` | The tsidp compose file |
| `deployments/host-cron/` | The Docker prune cron files |
| `deployments/host-firewall/` | The nftables table of the public interface `eth0` and the unit that loads it at boot; its `README.md` maps each file to its host path |
| `deployments/api-credit/`, `deployments/pc-account/`, `deployments/pc-stage-account/` | Notes on the API credit's key file and the PC's accounts (`README.md` only) |
| `etc/agent-platform/policy.json` | The source of the host's `/etc/agent-platform/policy.json` (section "The policy file") |
| `scripts/install-etc.sh` | Installs the policy file from a merged commit, checks it against the host, or puts a kept copy back |
| `scripts/test-install-etc.sh` | The tests of `install-etc.sh` |
| `scripts/retire-hindsight.sh`, `scripts/retire-hindsight.json` | The retirement of the legacy long-term memory service and its list (section "Retiring the legacy memory service") |
| `scripts/test-retire-hindsight.sh` | The tests of `retire-hindsight.sh` against a scratch root and a stand-in docker program |
| `git/` | The system Git configuration: the roster check (`git/hooks/pre-commit`), the configuration files and the alias `git task-new` (section "Git configuration") |
| `scripts/test-git-config.sh` | The tests of `git/` |

The copies the inventory names are byte copies of the host files:
`deployments/ssh/00-01-ssh-only.conf` and `deployments/host-firewall/` as captured
on 2026-11-22, the others on 2026-11-04. Each directory's `README.md` maps its
files to their host paths.

## Who uses it

`ap deliver` runs `scripts/install-etc.sh` (`check`, then `install` of the tip it
delivered) when the host's request names this repository in its member `etc`
(`specs/ap/deliver.md` R71 to R74, `deploy/README.md` "The configuration files"); the
Lead runs it by hand only for a rollback, a fast-path order or a merge the
installer cannot judge. The watch's detector `drift` compares the inventory's host
files with their digests and the host's `/etc/` with `etc/` of the commit `ap` was
installed from (`specs/ap/watch.md` R27, R29, R34). The Lead installs `git/` by hand
(section "Git configuration") and ran `scripts/retire-hindsight.sh` once. The
recorded copies under `deployments/` change on the host only by the Lead's hand,
under the procedure below.

## The check

The repository's `./check` runs, without a Rust build, the steps of this tree
(`ci/check-host.py`): every tracked JSON file under `host/` parses, every shell
script passes `bash -n`, every inventory row's copy exists and matches its recorded
SHA-256, and every file under `deployments/` is the inventory, a row's copy or a
`README.md`; then `scripts/test-git-config.sh`, `scripts/test-install-etc.sh` and
`scripts/test-retire-hindsight.sh` (scratch directories, fakes of `ont`, `ap` and
docker; no network, no root, no host file). The check compares each copy with its
recorded digest, never with the host: a difference between a copy and the host is
found only by reading the host (the watch).

## Changing a record

- Never edit a recorded copy by hand. A copy is refreshed only from the host: copy
  the host file's bytes unchanged, and write the new SHA-256 of those bytes into its
  inventory row in the same commit.
- A new recorded copy gets a row in the inventory; a file under `deployments/`
  without a row fails the check.
- Environment files and their values stay untracked (`host/.gitignore`). Existing
  `${VAR}` references in a copy stay as they are. Never commit a credential, a host
  key, an environment file or private data.

## The policy file

`etc/agent-platform/policy.json` is the host's policy of models and accounts for `ap
run` (and the settings `ap quota`, `ap unit` and `ap work` read from the same file).
The host copy `/etc/agent-platform/policy.json` is installed from a merged commit and
by no other means, but the fast path below. It is the only file under `etc/`.
`deliver.json` is not here: its source is `deploy/deliver.json`, installed as
`deploy/README.md` says. A change of the policy is a pull request that edits the
file, together with the code that reads a new member; the next delivery installs
both.

| Form | What it does |
| --- | --- |
| `install <commit> [--past <commit>:<reason>]...` | Installs the file from `<commit>`, read from the objects of the Git repository of the working directory (no clone, no fetch, no remote), after the accepted check and `ap policy check` on the staged bytes. The accepted check starts at the commit the installer recorded: it must be an ancestor of `<commit>` through any parent (a commit of the former operations repository is, through the merge that imported its history) and not `<commit>` itself (`not_forward`); every commit of the first-parent walk from `<commit>` back to that record, or to the first merge whose first parent does not hold it, must be a merge with the tree of its second parent and a closed accepting review of its head in the store (a review that `ap unit run` carried to an updated head counts by the pull request of the review it was carried from); without a record `<commit>` alone. When the target already holds the bytes it writes nothing and keeps no copy; else it keeps the previous bytes in `/var/backups/agent-platform-etc/policy.json.before-<UTC stamp>`, replaces the target by rename and reads it back. Either way it records the commit and digest in `/var/lib/agent-platform/etc/installed` |
| `check [<commit>]` | Prints `same` (exit 0) or `differs <target>` (exit 1) against `<commit>`, by default the working directory's `HEAD`; it writes nothing but its lock |
| `rollback <kept file>` | Puts one kept `policy.json.before-<stamp>` back, after `ap policy check`, keeping the current bytes first; it needs no repository |

The script holds the lock `/var/lib/agent-platform/etc/lock` and refuses with a code
on stderr and exit 2 before any write (`busy`, `not_forward`, `not_accepted`,
`tree_differs`, `record_invalid`, `commit_unknown`, `verify_failed`,
`target_changed`, `target_not_regular` and others). `AP_ETC_ROOT` prefixes every host
path for the tests. It writes no other path, never runs `systemctl` and never
touches a login part.

The policy's v3 members: `units.approval` says which unit of class contract merges on its verifier's accept (its head meets no path of `guarded`, and `sample` sets how many accepted merges are drawn for a later look); `work.slots` is how many units the dispatch pass runs at once (6); and `routing` holds the tier `order`, the `large_lines` bound, the `map` of implementer and verifier tiers by kind and size, and the `bar` a tier must meet.
The member `routing.order` names two AGY pairs: `["agy","gemini-4-hydrogen-high"]` stands directly after `["claude","sonnet"]` and `["agy","gemini-4-neon-high"]` directly after `["claude","opus"]`. The order decides: a tier is admitted where its pair stands at or after the row's pair, so Neon is admitted wherever the map gives Sonnet or Opus, and Hydrogen only where it gives Sonnet or a pair before it; no row of the map changes.
A unit whose tier names a runtime outside `routing.order` is refused `tier_not_in_order` until a later policy unit adds it.
Muse left the policy on the user's word of 2026-11-24: its models and its account are no longer in the file.

- `host/` is a protected tree (`specs/ap/unit-run.md` R36): a change under it is a
  unit of class `contract` or `effect`, which the Lead reads.
- The Lead runs `install-etc.sh check` before a hand install and at each STATE
  checkpoint.
- The fast path: when the user orders a change that must take effect at once, the
  Lead edits the host file by hand, records the user's request in STATE, and opens
  the pull request that makes the source equal within the hour. Until it is merged
  and installed, `install-etc.sh check` prints the difference, and `install` refuses
  `target_changed` for a hand edit that is not yet in the source.

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

**The Lead's order**, on the host, from a clean worktree at the merge commit (the paths below are under `host/`):

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

## The login path

The sshd drop-ins (`deployments/ssh/90-tailscale-only.conf` and
`deployments/ssh/00-01-ssh-only.conf`), the `ssh.socket` override
(`deployments/ssh/ssh.socket.d/override.conf`) and `deployments/host-firewall/` are
records of the login path as it is now. Nothing of this tree installs or reloads
them, and no agent runs a script of the login path; the policy file under `etc/` is
the only file a script of this tree installs. A change of the login path is made
only when the user orders it, by the user's procedure and by no timer:

1. the user confirms that the provider's console is the way back in;
2. the new file is written;
3. `sshd -t` passes and `sshd -T` prints the expected setting, both before any
   reload; on a mismatch the previous bytes return and nothing is reloaded;
4. a timed restore of the previous bytes with a reload is armed;
5. the reload;
6. the timed restore is cancelled only after the user reports a new login from a
   device. An agent's session on the host proves nothing about a new login.

## Git configuration

`git/` holds what the Lead installs into `/usr/local/lib/agents-git/` and
`/etc/gitconfig`. Nothing here installs it; the commands below run in `host/`.

| File | What it is |
| --- | --- |
| `git/system.gitconfig` | The lines of `/etc/gitconfig`: the alias file for every repository, and the `includeIf "gitdir:…"` lines for the scopes below |
| `git/work.gitconfig` | For a task directory: `core.hooksPath` (the hook directory), `push.default = upstream`, `agents.roster` (a glob of the Academy's roster files) |
| `git/clones.gitconfig` | For the two clones: `core.hooksPath` and `agents.roster`, no `push.default` (a branch made by `ap unit run` tracks `origin/main`, and `upstream` would push it to `main`) |
| `git/alias.gitconfig` | The alias `git task-new <name>` |
| `git/hooks/pre-commit` | The roster check |

**Scopes.** The task directories `work/ws-*`, `work/ap-*`, `work/ont-*` and
`work/mem-*`, and the clones `platforms/ontology-platform`,
`platforms/agent-vps-operations` and `platforms/agent-platform`, each with its
linked worktrees (the patterns end in a slash and carry no `.git`, so Git
matches the main checkout's `.git` and every `.git/worktrees/<n>`).
`agent-platform` is named so that a commit under `engine/` and `host/` runs the
roster check; every linked worktree of its main checkout is matched with it,
the worktrees of `ap unit run` among them, which therefore need a readable
roster. `education-platform` is not named: the Academy's own repositories stay
out of this configuration. The configuration is the system file, not
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
