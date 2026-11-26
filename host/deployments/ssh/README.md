# Host configuration sources

Read-only byte copies. Tracking does not authorize installation.

| Copy | Host path | Captured |
| --- | --- | --- |
| `90-tailscale-only.conf` | `/etc/ssh/sshd_config.d/90-tailscale-only.conf` | 2026-11-04 |
| `ssh.socket.d/override.conf` | `/etc/systemd/system/ssh.socket.d/override.conf` | 2026-11-04 |
| `00-01-ssh-only.conf` | `/etc/ssh/sshd_config.d/00-01-ssh-only.conf` | 2026-11-22 |

`00-01-ssh-only.conf` sets: `AllowUsers root`; `DisableForwarding yes` with
`AllowTcpForwarding`, `AllowStreamLocalForwarding`, `AllowAgentForwarding` and
`X11Forwarding` all `no`; `PermitTunnel no`; `MaxAuthTries 3`; `LoginGraceTime 30`;
`ClientAliveInterval 300` and `ClientAliveCountMax 2`. The user enters by plain ssh
as root with password and TOTP.

`00-00-password-totp.conf` is a host file that is not recorded here (it holds no
secret). It sorts first, so authentication is unchanged by `00-01-ssh-only.conf`.

Environment files and their values remain untracked. Existing `${VAR}` references require the host-owned environment at installation.
