#!/bin/sh
set -eu

runtime_home=/home/palantirkc
runtime_project="$runtime_home/code/shared/agent-platform"
runtime_host_key=/var/lib/herdr-ssh/ssh_host_ed25519_key
runtime_authorized_keys=/etc/ssh/authorized_keys/palantirkc

if [ "$(id -u palantirkc)" != 1000 ] || [ "$(id -g palantirkc)" != 1000 ]; then
    echo 'The palantirkc account must use UID and GID 1000.' >&2
    exit 1
fi
if [ ! -d "$runtime_project" ]; then
    echo 'Provision the persistent agent-platform directory before starting Herdr.' >&2
    exit 1
fi
if [ ! -s "$runtime_authorized_keys" ] || [ -L "$runtime_authorized_keys" ]; then
    echo 'Stage the selected public key in the regular authorized_keys bind file.' >&2
    exit 1
fi
if [ -L /var/lib/herdr-ssh ] || [ -L "$runtime_host_key" ]; then
    echo 'SSH host key storage must not use symlinks.' >&2
    exit 1
fi

install -d --mode=0755 /run/sshd
install -d --owner=0 --group=0 --mode=0700 /var/lib/herdr-ssh
if [ ! -f "$runtime_host_key" ]; then
    ssh-keygen -q -t ed25519 -N '' -f "$runtime_host_key"
fi
chmod 0600 "$runtime_host_key"
chmod 0644 "$runtime_host_key.pub"
chown 0:0 "$runtime_host_key" "$runtime_host_key.pub"

runuser --user palantirkc -- test -w "$runtime_home"
runuser --user palantirkc -- test -w "$runtime_project"
/usr/sbin/sshd -t -f /etc/ssh/sshd_config
exec /usr/bin/supervisord -c /etc/supervisor/herdr-runtime.conf
