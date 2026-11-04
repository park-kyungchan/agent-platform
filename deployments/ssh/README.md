# Host configuration sources

Read-only byte copies captured on 2026-11-04. Tracking does not authorize installation.

- `90-tailscale-only.conf` → `/etc/ssh/sshd_config.d/90-tailscale-only.conf`
- `ssh.socket.d/override.conf` → `/etc/systemd/system/ssh.socket.d/override.conf`

Environment files and their values remain untracked. Existing `${VAR}` references require the host-owned environment at installation.
