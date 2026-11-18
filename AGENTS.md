# Host configuration records

## What this repository holds

- `deployments/host-artifacts.json`: the inventory. Each row names a host file (`source`), its recorded copy in this repository (`destination`) and the SHA-256 of the copy's bytes (`sha256`).
- The recorded copies the inventory names: `deployments/ssh/` (the Tailscale-only sshd drop-in and the `ssh.socket` override), `deployments/hindsight/compose.yaml`, `deployments/tsidp/compose.yaml` and `deployments/host-cron/` (the Docker prune cron files). They are byte copies of the host files as captured on 2026-11-04; the host may have changed since.
- `deployments/herdr-network/`: the Herdr network's files: the sshd drop-in `00-herdr-runtime-key-only.conf`, its installer `sshd-policy.sh`, the firewall helper `firewall-policy.sh`, its unit `herdr-network-firewall.service` and `contract.json`. Its `README.md` maps each file to its installed host path.
- `./check` and the workflow `.github/workflows/rust.yml`, whose job `rust` runs `./check`.

Nothing here is installed by this repository and no program is built here.

## Who uses it

The Lead reads these records. The delivery of host files from this repository (unit D3) is not built yet: until it is, a merge here changes nothing on the host.

## The check

Run `./check` at the root. It needs no network, no root and no host path, and it reads only the repository's tracked files. It checks that:

1. every tracked `*.json` file parses;
2. `bash -n` passes for every tracked file ending in `.sh` or whose first line is a `#!` line naming `bash` or `sh`, directly or through `env`;
3. every row of `deployments/host-artifacts.json` has `source`, `destination` and `sha256`, its `destination` exists, and the SHA-256 of the destination's bytes equals `sha256`;
4. every tracked file under `deployments/` is the inventory, a row's `destination`, a `README.md`, or under `deployments/herdr-network/`.

A healthy run ends with the line `check: ok`. A failure prints one `check: <code>@<path>` line per finding and exits non-zero. The job `rust` is the required status check of `main`: keep its job id and name `rust`.

The check compares each copy with its recorded digest, never with the host. A difference between a copy and the host is not a check failure; it is found only by reading the host.

## Changing a record

- Never edit a recorded copy by hand. A copy is refreshed only from the host: copy the host file's bytes unchanged, and write the new SHA-256 of those bytes into its inventory row in the same commit.
- A new recorded copy gets a row in the inventory; a file under `deployments/` without a row fails the check.
- Environment files and their values stay untracked (`.gitignore`). Existing `${VAR}` references in a copy stay as they are.

## Never changed or run here without the user's procedure

The login path: the sshd drop-ins (`deployments/ssh/90-tailscale-only.conf` and `deployments/herdr-network/00-herdr-runtime-key-only.conf`), the `ssh.socket` override (`deployments/ssh/ssh.socket.d/override.conf`), and `deployments/herdr-network/sshd-policy.sh`, installed as `/usr/local/sbin/herdr-network-sshd`, which writes the drop-in `00-herdr-runtime-key-only.conf` and reloads `ssh`.

Do not run `sshd-policy.sh` or `firewall-policy.sh`, and do not install or reload anything from this repository. A change of the login path is made only when the user orders it, by the user's procedure and by no timer:

1. the user confirms that the provider's console is the way back in;
2. the new file is written;
3. `sshd -t` passes and `sshd -T` prints the expected setting, both before any reload; on a mismatch the previous bytes return and nothing is reloaded;
4. a timed restore of the previous bytes with a reload is armed;
5. the reload;
6. the timed restore is cancelled only after the user reports a new login from a device. An agent's session on the host proves nothing about a new login.

Never commit a credential, a host key, an environment file or private data.
