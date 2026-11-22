# Host configuration records

## What this repository holds

- `deployments/host-artifacts.json`: the inventory. Each row names a host file (`source`), its recorded copy in this repository (`destination`) and the SHA-256 of the copy's bytes (`sha256`).
- The recorded copies the inventory names: `deployments/ssh/` (the Tailscale-only sshd drop-in and the `ssh.socket` override), `deployments/hindsight/compose.yaml`, `deployments/tsidp/compose.yaml` and `deployments/host-cron/` (the Docker prune cron files). They are byte copies of the host files as captured on 2026-11-04; the host may have changed since.
- `deployments/herdr-network/`: the Herdr network's files: the sshd drop-in `00-herdr-runtime-key-only.conf`, its installer `sshd-policy.sh`, the firewall helper `firewall-policy.sh`, its unit `herdr-network-firewall.service` and `contract.json`. Its `README.md` maps each file to its installed host path.
- `etc/agent-platform/policy.json`: the source of the host's `/etc/agent-platform/policy.json`, and `scripts/install-etc.sh`, the one script that installs it (section "The policy file under `etc/`").
- `./check` and the workflow `.github/workflows/rust.yml`, whose job `rust` runs `./check`.

The policy file under `etc/` is the only file a script of this repository installs (`scripts/install-etc.sh`). The files under `git/` are installed by the Lead with the commands of `README.md` "Git configuration"; every other file here is a record or a script of the checks. No program is built here.

## Who uses it

The Lead reads these records and runs `scripts/install-etc.sh`. A merge here changes nothing on the host until the Lead runs `install-etc.sh install <merge>`: no timer and no delivery walk reads this repository.

## The check

Run `./check` at the root. It needs no network, no root and no host path, and it reads only the repository's tracked files. It checks that:

1. every tracked `*.json` file parses;
2. `bash -n` passes for every tracked file ending in `.sh` or whose first line is a `#!` line naming `bash` or `sh`, directly or through `env`;
3. every row of `deployments/host-artifacts.json` has `source`, `destination` and `sha256`, its `destination` exists, and the SHA-256 of the destination's bytes equals `sha256`;
4. every tracked file under `deployments/` is the inventory, a row's `destination`, a `README.md`, or under `deployments/herdr-network/`;
5. `scripts/test-git-config.sh` and `scripts/test-install-etc.sh` pass (scratch directories, fakes of `ont` and `ap`, a scratch origin and a scratch root; no network, no root, no host file).

A healthy run ends with the line `check: ok`. A failure prints one `check: <code>@<path>` line per finding and exits non-zero. The job `rust` is the required status check of `main`: keep its job id and name `rust`.

The check compares each copy with its recorded digest, never with the host. A difference between a copy and the host is not a check failure; it is found only by reading the host.

## Changing a record

- Never edit a recorded copy by hand. A copy is refreshed only from the host: copy the host file's bytes unchanged, and write the new SHA-256 of those bytes into its inventory row in the same commit.
- A new recorded copy gets a row in the inventory; a file under `deployments/` without a row fails the check.
- Environment files and their values stay untracked (`.gitignore`). Existing `${VAR}` references in a copy stay as they are.

## The policy file under `etc/`

`etc/agent-platform/policy.json` is the host's policy of models and accounts for `ap run` (and the settings `ap quota`, `ap unit` and `ap work` read from the same file). Its source is this file: the host copy `/etc/agent-platform/policy.json` is installed from a merged commit and by no other means. It is the only file under `etc/`. `deliver.json` is not here: its source is `agent-platform` `deploy/deliver.json`, installed as that repository's `deploy/README.md` says.

- A change of the policy is a pull request that edits the file, then `scripts/install-etc.sh install <merge>`, read back.
- `scripts/install-etc.sh` has three forms. `install <commit>` installs the file from a commit of `origin/main` after the accepted check of every merge since the last installed one (a closed accepting review in the store (a review that `ap unit run` carried to an updated head counts by the pull request of the review it was carried from)) and `ap policy check` on the staged bytes; the previous bytes are kept in `/var/backups/agent-platform-etc/` first, the new bytes replace the target by rename and are read back, and the installed commit and digest are recorded. `check [<commit>]` prints `same` or `differs <target>` and writes nothing but its own clone. `rollback <kept file>` puts one kept file back. The script writes no other path, never runs `systemctl` and never touches a login part.
- Until unit F1 adds `etc` to the merge gate's protected paths, a change under `etc/` is a unit of class `effect`, which the Lead reads.
- Only the Lead runs `install-etc.sh`. The Lead runs `check` before each install and at each STATE checkpoint.
- The fast path: when the user orders a change that must take effect at once, the Lead edits the host file by hand, records the user's request in STATE, and opens the pull request that makes the source equal within the hour. Until it is merged and installed, `install-etc.sh check` prints the difference, and `install` refuses `target_changed` for a hand edit that is not yet in the source.

## Never changed or run here without the user's procedure

The login path: the sshd drop-ins (`deployments/ssh/90-tailscale-only.conf` and `deployments/herdr-network/00-herdr-runtime-key-only.conf`), the `ssh.socket` override (`deployments/ssh/ssh.socket.d/override.conf`), and `deployments/herdr-network/sshd-policy.sh`, installed as `/usr/local/sbin/herdr-network-sshd`, which writes the drop-in `00-herdr-runtime-key-only.conf` and reloads `ssh`.

Do not run `sshd-policy.sh` or `firewall-policy.sh`, and do not install or reload anything from this repository, with one exception: the policy file under `etc/`, installed by `scripts/install-etc.sh` and nothing else. A change of the login path is made only when the user orders it, by the user's procedure and by no timer:

1. the user confirms that the provider's console is the way back in;
2. the new file is written;
3. `sshd -t` passes and `sshd -T` prints the expected setting, both before any reload; on a mismatch the previous bytes return and nothing is reloaded;
4. a timed restore of the previous bytes with a reload is armed;
5. the reload;
6. the timed restore is cancelled only after the user reports a new login from a device. An agent's session on the host proves nothing about a new login.

Never commit a credential, a host key, an environment file or private data.
