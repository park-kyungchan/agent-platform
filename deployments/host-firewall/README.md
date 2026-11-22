# Host firewall sources

Read-only byte copies captured on 2026-11-22. Tracking does not authorize installation.

| Source | Host path |
| --- | --- |
| `host-firewall.nft` | `/etc/host-firewall.nft` |
| `host-firewall.service` | `/etc/systemd/system/host-firewall.service` |

What the files set:

- The table `inet hostfw` hooks `input` before the default filter priority, with policy accept.
- On `eth0` (the public interface) only replies to the host's own connections
  (`ct state established,related`), ICMP and UDP 41641 (Tailscale) are admitted;
  invalid packets and everything else are dropped.
- Every other interface is untouched (`iifname != "eth0" accept`).
- The unit `host-firewall.service` loads the table at boot (`nft -f /etc/host-firewall.nft`,
  before `network-online.target` and `tailscaled.service`); stopping it deletes the table.

Undo: `systemctl disable --now host-firewall.service`.

The login path and this firewall change only by the user's procedure in `AGENTS.md`.
