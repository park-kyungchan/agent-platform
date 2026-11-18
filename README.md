# Host configuration records

Recorded copies of host configuration files and the Herdr network's files.
Nothing here is built or installed by this repository; agents follow
[`AGENTS.md`](AGENTS.md).

## Contents

| Path | What it is |
| --- | --- |
| `deployments/host-artifacts.json` | The inventory: for each recorded copy, the host file (`source`), the copy (`destination`) and the SHA-256 of the copy's bytes (`sha256`) |
| `deployments/ssh/` | The Tailscale-only sshd drop-in and the `ssh.socket` override |
| `deployments/hindsight/compose.yaml` | The Hindsight compose file |
| `deployments/tsidp/compose.yaml` | The tsidp compose file |
| `deployments/host-cron/` | The Docker prune cron files |
| `deployments/herdr-network/` | The Herdr network's sshd drop-in, its installer, the firewall helper, its unit and contract; its `README.md` maps each file to its host path |

The copies the inventory names are byte copies of the host files as captured on
2026-11-04. Each directory's `README.md` maps its files to their host paths.

## Who uses it

The Lead reads these records. The delivery of host files from this repository
(unit D3) is not built yet; until it is, a merge here changes nothing on the host.

## Check

```bash
./check
```

A healthy run ends with `check: ok`. The check needs no network, no root and no
host path. It parses every tracked JSON file, runs `bash -n` on every tracked shell
script, verifies that every inventory row's copy exists and matches its recorded
SHA-256, and fails on a file under `deployments/` that is not the inventory, a row's
copy, a `README.md`, or under `deployments/herdr-network/`. The GitHub Actions job
`rust` runs the same command and is the required check of `main`.

The check compares each copy with its recorded digest, never with the host. A copy
is refreshed only from the host: its bytes unchanged, with its new digest written
into the inventory in the same commit.

## Not changed here

The login path (the sshd drop-ins, the `ssh.socket` override,
`00-herdr-runtime-key-only.conf` and `deployments/herdr-network/sshd-policy.sh`,
installed as `/usr/local/sbin/herdr-network-sshd`, which writes that drop-in and
reloads `ssh`) is changed only by the user's procedure described in `AGENTS.md`.
Environment files, credentials and host keys are never tracked.
