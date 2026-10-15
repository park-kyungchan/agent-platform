# Herdr runtime deployment

This directory defines one dedicated Herdr and Codex container. Hermes remains a separate deployment. These files do not configure a custom agent scheduler, copy Codex authentication, or provision the common Harness.

The host source projection is `/docker/herdr-runtime/deployment`. The historical
`build` path is a relative compatibility symlink to that directory. Persistent
Home and SSH keys retain their existing native paths. This separates deployment
source from runtime data without duplicating the deployment tree or changing the
Compose project/container identity. The path index is owned by `vps-operations`;
common naming and ownership semantics are recorded in `agent-platform`.

## Inputs and persistence

The image pins Herdr 0.9.5, Codex CLI 0.162.1, Bun 1.4.3, and Node.js 22.23.3. `versions.json` records their upstream identities. Debian packages resolve at build time, so deployment verification must record the resulting image ID.

Bun 1.4.3 is the verified stable release selected for the shared VPS runtime. The
Dockerfile retains its exact upstream image digest so future agents inherit the
same version after provisioning. A newer stable release requires an explicit pin
update and verification; a floating `latest` tag would make rebuilds drift. The
repository's package manager and runtime baseline use this version too. GitHub
source CI retains its separately pinned compatibility runtime; changing that
workflow requires workflow-scoped publication authority.
`bun-types` is independently pinned API type metadata; it does not select the
executable used by agents. Changing these source pins alone does not install Bun
in an existing runtime or authorize container recreation.

Provision these host paths before starting Compose:

- `/docker/herdr-runtime/home`: persistent home owned by UID/GID 1000. Provision the common `agent-platform` checkout at `home/code/shared/agent-platform` and the selected Codex configuration and skills separately.
- `/docker/herdr-runtime/ssh-host-keys`: root-owned directory, mode 0700. The entrypoint generates one Ed25519 host key if none exists.
- `/docker/herdr-runtime/authorized_keys`: regular root-owned file, mode 0644, containing only the selected public key. It is mounted read-only outside the user home.

No host Docker socket, Hermes files, or local Codex credentials are mounted. The container publishes SSH only on host loopback `127.0.0.1:2222`. External clients reach it through the existing Tailscale SSH endpoint and native OpenSSH ProxyCommand. The container no longer needs the Tailscale IP to exist before binding its SSH publication. The boot unit retains its normal Docker/network ordering and no longer invokes the retired address-readiness helper.

The dedicated native Compose network pins subnet `172.16.4.0/24`, gateway `172.16.4.1`, and container address `172.16.4.2`. This preserves the source address used by the independently generated Herdr-to-host SSH identity. Recreating this network requires an explicit check that its only member is the owned Herdr container.

## Connection

Create a WSL2 SSH alias with the selected existing key and a separately verified container host fingerprint:

```sshconfig
Host srv1873899 srv1873899-herdr
    HostName 127.0.0.1
    Port 2222
    User palantirkc
    IdentityFile ~/.ssh/<selected-existing-key>
    IdentitiesOnly yes
    StrictHostKeyChecking yes
    HostKeyAlias srv1873899-herdr
    ProxyCommand ssh -o BatchMode=yes -o StrictHostKeyChecking=yes -o HostKeyAlias=srv1873899 -o ConnectTimeout=10 root@100.95.133.99 /usr/bin/nc %h %p
```

The primary WSL2 entrypoint is `herdr --remote srv1873899`. It opens the remote environment with one user-facing Codex Orchestrator. The Orchestrator uses the Herdr skill to open Codex worker panes and native Hermes TUI panes or tabs on demand. `srv1873899-herdr` remains a compatible alias to the same container. Claude and Antigravity support is planned; neither is installed by this deployment. The current paused Work still requires manual resume.

Both user-facing aliases target the container. The outer ProxyCommand deliberately uses the literal host address `root@100.95.133.99` and the existing host trust alias; it must not recursively invoke either container alias. Do not add `100.95.133.99` to the container alias's Host pattern. Saved-machine registration is a separate local step; preserve the existing saved-machine identity when changing its display/entry alias.

The alias uses the already verified Tailscale SSH endpoint on port 22 to carry the container SSH connection to VPS loopback. `%h` expands to `127.0.0.1` on the VPS side of the ProxyCommand. Direct Desktop-to-VPS port 2222 previously timed out while the host-local port returned the expected SSH banner; the cause was not promoted to a verified ACL diagnosis. Loopback publication removes that extra tail-address listener and keeps the existing tailnet access policy. The final SSH endpoint and pinned host key still belong to the container.

The container starts SSH. The first native remote attachment or saved-machine setup starts the Herdr detached daemon under the authenticated user. Herdr owns this daemon's startup and socket discovery. Supervisor does not start `herdr server`: foreground headless mode does not establish the detached session identity required by Herdr 0.9.5 remote attachment. No private daemon flag, injected capability value, or custom startup scheduler is used.

SSH uses public-key authentication only. Root login and SSH forwarding are disabled. Herdr's SSH command and SFTP workflows remain available. The Herdr API socket is internal to the container user home; no API TCP port is published.

## Codex inside panes

From a Herdr pane, run:

```bash
herdr-codex
```

`herdr-codex` calls `codex-herdr`, which executes the installed native CLI as `codex --no-daemon`. This retains the pane environment in the pane-owned Codex process. Neither helper changes the generic `codex` executable.

When launching a named agent through Herdr, pass the native option explicitly:

```bash
herdr agent start builder --kind codex --pane <returned-pane-id> -- --no-daemon
```

Herdr 0.9.5 has no per-kind default launch-arguments setting identified in its configuration contract. A plain `herdr agent start ... --kind codex` therefore does not inherit the helper's option. Common Harness instructions should require the command above. `--no-daemon` is incompatible with Codex's `--remote` and `codex agents`; Herdr remote attachment is a different command and remains compatible.

Model, reasoning effort, context limits, approvals, hooks, skills, and login are provisioned separately by the owning agent. Authenticate this VPS Codex environment independently.

## Lifecycle and verification

Tini manages child reaping. Supervisor runs root SSH in the foreground. SSH startup retries are limited to three; later unexpected SSH exits can restart. Herdr's native remote bootstrap starts the detached daemon as UID/GID 1000. A deliberate successful Herdr shutdown remains stopped until a later native attachment starts it again.

Docker health reports SSH bootstrap readiness. `healthy` permits a cold state with no Herdr daemon and therefore does not establish that Herdr or any Codex process is running. When a daemon already exists, the health check additionally requires the pinned version, compatible endpoint, detached session identity, and saved-machine lifecycle capabilities. Health checks do not submit model work, start Herdr, or restart the container. Deployment acceptance must separately observe the running daemon after native remote attachment.

Client detach preserves running panes. Container or VPS restart ends the original processes. The container returns with SSH bootstrap readiness; Herdr starts again on the next native remote attachment. Persistent home files can support layout and native conversation restoration; this is not uninterrupted execution or autonomous job resumption. `unless-stopped` is a container restart policy, not a job continuation guarantee.

Before accepting deployment, verify:

1. Compose renders the exact three mounts, host-loopback SSH binding, pinned bridge/source address, 4 GiB memory limit, 1.5 CPU limit, and bounded container logs.
2. SSH login reports UID/GID 1000, the dedicated container hostname, the expected tool versions, and Herdr endpoint compatibility.
3. Non-interactive SSH and interactive pane shells resolve Bun, Node, Codex, and both Codex helpers.
4. Herdr and a temporary pane process have the same identities before and after a test client detach and reconnect.
5. Record whether native Herdr Codex integration was selected and activated. This deployment does not copy or activate WSL hooks or hook trust. If integration is added later, verify its version, native loading and trust separately; default native screen detection is not proof of hook integration.
6. Existing Hermes runtime identity and native serving remain unchanged.

## Applying the loopback and network successor

The deployment owner must first finish admitted artifact transfer, inspect the fresh Herdr pane/agent/process state, and establish that no active job or authentication flow will be interrupted. Preserve every existing persistent bind and all Hermes containers. The current source key is restricted to `172.16.4.2`, so apply the static source-address pin with the publication change.

Validate the successor on the VPS:

```bash
docker compose -f /docker/herdr-runtime/deployment/compose.yaml config --quiet
docker network inspect herdr-runtime_default
docker inspect herdr-runtime
```

Compare the live container ID and network membership with the just-reviewed preflight. If Compose needs to replace the dedicated network, its only member must be that owned Herdr container. After the no-job gate, recreate only this project; never add `--volumes`, `--remove-orphans`, or another Compose project:

```bash
docker compose -f /docker/herdr-runtime/deployment/compose.yaml down
docker compose -f /docker/herdr-runtime/deployment/compose.yaml up -d --no-build
```

Install the successor boot unit and run `systemctl daemon-reload`. The boot unit is a startup-order record; it is not a job supervisor. After confirming no unit still references `/usr/local/sbin/herdr-runtime-network-ready`, remove only that retired helper at its recorded hash. Do not change host SSH socket/boot routing as part of this update.

The client owner changes only this alias's destination to `127.0.0.1`, retaining its existing Tailscale SSH ProxyCommand, selected public-key identity, pinned container host key, and saved-machine identity. Verify native SSH/SFTP and Herdr attachment, then separately verify independent Herdr-to-host BatchMode SSH, the existing Desktop HTTPS route, and unchanged Hermes native owner/serving. A recreated container starts a new Herdr daemon on the next attachment; this is not process continuity.

For reversal, restore the reviewed predecessor Compose publication and client alias together. Keep all persistent binds and the source-address pin unless their own authorization is being reversed. Validate the predecessor and repeat the same owned-container/no-job gate before its recreation. Restoring a saved layout or native conversation does not restart an unfinished Work automatically.

## Primary references

- [Herdr remote access](https://herdr.dev/docs/persistence-remote/)
- [Herdr session persistence limits](https://herdr.dev/docs/session-state/)
- [Codex CLI options](https://learn.chatgpt.com/docs/cli/reference)
- [Official Node.js image](https://hub.docker.com/_/node)
- [Docker multi-process containers](https://docs.docker.com/engine/containers/multi-service_container/)
- [Supervisor configuration](https://supervisord.org/configuration.html)
