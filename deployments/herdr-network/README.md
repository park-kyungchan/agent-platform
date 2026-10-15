# Herdr network policy candidate

This source-owned candidate preserves the existing Windows Desktop Hermes route and native service owners. It makes host OpenSSH use public-key authentication and blocks only public-interface UDP8472. It neither removes Flannel/K3s/CNI state nor changes Tailscale SSH, Serve, host SSH listen/socket/boot routing, or Hermes. The root deployment owner applies it after fresh preflight; these source files do not grant execution authority.

## Preconditions

Independent Herdr-origin authentication is already verified with the persistent, container-generated key fingerprint `SHA256:Ldr0Fx8mBbSxRJuK2W46wZLRMPCmRoNloQtb9K8C9Ss`. Its host entry allows only source `172.16.4.2`, with standard `restrict,pty` options. Keep the native Compose address pin when recreating Herdr. Do not copy a WSL private key, forward a required WSL agent, mount the Docker socket, or generate a replacement identity as a retry.

The existing host OpenSSH key is `SHA256:UdCSDc3uPy5xd4wy/aiyJoqy/ega46Q4zP27whF+JVQ`. Container-origin traffic to `100.95.133.99:22` reaches this OpenSSH server. Tailnet-origin traffic to the same endpoint is handled by Tailscale SSH and uses a separate authentication/trust path. Do not use a successful Tailscale SSH login as proof that the independent OpenSSH key works.

Inspect current native identities, retain the working admin session, and confirm a second fresh container-origin public-key login before applying host authentication hardening. The installer verifies that the expected public-key fingerprint remains registered; that is an additional precondition, not a replacement for the observed login.

## Install source-owned files

Stage this reviewed directory as `/docker/operations/herdr-network` on the VPS, then run the following as host root. Verify the staged file hashes against the reviewed source before installation.

```bash
reviewed_source=/docker/operations/herdr-network
install -d -o root -g root -m 0755 /usr/local/share/herdr-network
install -o root -g root -m 0644 "$reviewed_source/00-herdr-runtime-key-only.conf" /usr/local/share/herdr-network/00-herdr-runtime-key-only.conf
install -o root -g root -m 0755 "$reviewed_source/sshd-policy.sh" /usr/local/sbin/herdr-network-sshd
install -o root -g root -m 0755 "$reviewed_source/firewall-policy.sh" /usr/local/sbin/herdr-network-firewall
install -o root -g root -m 0644 "$reviewed_source/herdr-network-firewall.service" /etc/systemd/system/herdr-network-firewall.service
systemctl daemon-reload
```

The SSH helper installs only `00-herdr-runtime-key-only.conf` in the native include directory. It refuses a different existing file at that name. OpenSSH include ordering and Match sections are checked with the real host binary after installation, before reload. An early filename alone does not establish effective policy.

## Apply and read back

```bash
/usr/local/sbin/herdr-network-sshd apply
/usr/local/sbin/herdr-network-sshd check
systemctl enable --now herdr-network-firewall.service
/usr/local/sbin/herdr-network-firewall check
systemctl show ssh.socket -p Listen -p ActiveState
systemctl show herdr-network-firewall.service -p ActiveState -p SubState
iptables -S HERDR-NET-PUBLIC
ip6tables -S HERDR-NET-PUBLIC
```

`apply` runs `sshd -t` and the real `sshd -T` globally and for `root` from `172.16.4.2`. Required values are public-key enabled, password/keyboard-interactive disabled, `AuthenticationMethods publickey`, and root public-key login retained. Only then does it reload `ssh.service`. It does not restart the socket, change port22, rotate keys, or rewrite any authorized_keys.

From the recreated Herdr container, run a fresh read-only login using its native SSH configuration:

```bash
docker exec --user 1000:1000 --env HOME=/home/palantirkc herdr-runtime ssh -o BatchMode=yes -o StrictHostKeyChecking=yes srv1873899 id -u
```

Expect `0`. Separately verify that a new connection offers only public-key authentication, the host Ed25519 fingerprint is unchanged, the existing Windows Desktop `/hermes/api` route and native Hermes serving still work, and Herdr SSH/SFTP and remote attachment use the loopback successor. Do not start a model turn or resume the paused Work for these checks.

The firewall helper owns one named chain and one commented INPUT jump in each IP family. The jump matches only `eth0`, UDP, destination8472. It is placed first so later permissive legacy chains cannot bypass it. Loopback, Tailscale and Docker-bridge ingress, TCP22/443, and Tailscale UDP41641 do not match. No other table, default policy, chain or legacy network is flushed or removed.

IPv4 and IPv6 are separate native effects. If an apply fails, inspect both families before deciding whether to retry or reverse. The helper refuses unexpected contents or references under its chain name. Rule checks do not establish Hostinger firewall configuration or that an external probe reached the VPS.

## Reversal

Withdraw only the owned firewall policy:

```bash
systemctl disable --now herdr-network-firewall.service
/usr/local/sbin/herdr-network-firewall remove
```

Stopping the oneshot unit alone deliberately leaves the filtering in place. `remove` deletes only the exact owned INPUT jump and commented DROP, then its empty owned chain. It does not flush shared rules or remove VXLAN/CNI devices.

Restore the predecessor native SSH authentication policy:

```bash
/usr/local/sbin/herdr-network-sshd revert
```

This removes only the unchanged owned include, validates the remaining native SSH configuration, then reloads. If the predecessor fails syntax validation, the owned include is restored without reload. An uncertain reload must be inspected before another action. No private/public keys are removed or rotated by reversal.

Reversing the Herdr loopback publication requires its separately reviewed client-alias/Compose predecessor and the same fresh no-job/owned-network gate. Preserve the address pin and all persistent binds unless their own authorization is reversed. Do not restore the obsolete Tailscale address-readiness helper merely to change authentication policy.

## References

- [OpenSSH configuration](https://man.openbsd.org/sshd_config.5)
- [OpenSSH authorized_keys options](https://man.openbsd.org/sshd.8#AUTHORIZED_KEYS_FILE_FORMAT)
- [Tailscale SSH and standard SSH separation](https://tailscale.com/docs/features/tailscale-ssh)
- [Docker port publication](https://docs.docker.com/engine/network/port-publishing/)
- [K3s VXLAN network requirements](https://docs.k3s.io/installation/requirements)
