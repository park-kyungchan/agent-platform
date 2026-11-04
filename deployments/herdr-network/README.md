# Host network policy sources

These files retain the installed host SSH authentication and narrow public
UDP8472 filtering policies. Herdr now runs natively as `herdr server`; the retired
container address, loopback publication and persistent container binds are not a
current deployment contract. The historical `herdr-runtime` include filename is a
native installed path retained for compatibility.

| Source | Installed host path |
| --- | --- |
| `00-herdr-runtime-key-only.conf` | `/etc/ssh/sshd_config.d/00-herdr-runtime-key-only.conf` and `/usr/local/share/herdr-network/00-herdr-runtime-key-only.conf` |
| `sshd-policy.sh` | `/usr/local/sbin/herdr-network-sshd` |
| `firewall-policy.sh` | `/usr/local/sbin/herdr-network-firewall` |
| `herdr-network-firewall.service` | `/etc/systemd/system/herdr-network-firewall.service` |

The SSH helper retains its installed historical key/source preflight. Reviewing
this source does not establish that its former container authentication path is
still live or authorize applying it. Read the current native SSH configuration
and actual caller path before considering any future policy change. SSH host keys,
Tailscale boundaries, native data and unrelated rules retain their owners.

The firewall helper owns only its named chain and commented INPUT jump in each IP
family. Its unit no longer orders before the retired Herdr runtime unit. Stopping
the unit does not withdraw its rule; source tracking performs no lifecycle action.

The current Tailscale bind and socket override byte copies are in `../ssh/`.
