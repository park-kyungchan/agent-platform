#!/bin/sh
set -eu
umask 077

network_template=/usr/local/share/herdr-network/00-herdr-runtime-key-only.conf
network_destination=/etc/ssh/sshd_config.d/00-herdr-runtime-key-only.conf
network_key_fingerprint=SHA256:Ldr0Fx8mBbSxRJuK2W46wZLRMPCmRoNloQtb9K8C9Ss
network_authorized_keys=/root/.ssh/authorized_keys
network_created=0
network_validated=0
network_staging=

fail() {
    echo "$*" >&2
    exit 1
}

require_host() {
    [ "$(id -u)" = 0 ] || fail 'Run this host policy helper as root.'
    [ "$(hostname -s)" = srv1873899 ] || fail 'This policy is pinned to srv1873899.'
    [ -f "$network_template" ] && [ ! -L "$network_template" ] \
        || fail 'The regular source-owned SSH policy template is missing.'
    [ ! -L "$network_destination" ] || fail 'Refusing a symlink at the SSH policy destination.'
}

require_registered_key() {
    [ -f "$network_authorized_keys" ] && [ ! -L "$network_authorized_keys" ] \
        || fail 'The existing regular root authorized_keys file is required.'
    ssh-keygen -lf "$network_authorized_keys" \
        | awk -v expected="$network_key_fingerprint" '$2 == expected { found = 1 } END { exit !found }' \
        || fail 'The independently verified Herdr public-key fingerprint is not registered.'
}

install_owned_include() {
    network_staging=$(mktemp /etc/ssh/sshd_config.d/.herdr-key-only.XXXXXX)
    if ! install -o root -g root -m 0644 "$network_template" "$network_staging"; then
        rm "$network_staging"
        network_staging=
        fail 'Could not stage the complete SSH include.'
    fi
    if ! ln -T "$network_staging" "$network_destination"; then
        rm "$network_staging"
        network_staging=
        fail 'The SSH include destination appeared during staging; no existing file was replaced.'
    fi
    network_created=1
    rm "$network_staging"
    network_staging=
}

validate_effective_policy() {
    /usr/sbin/sshd -t
    for network_context in global herdr-source; do
        if [ "$network_context" = global ]; then
            network_effective=$(/usr/sbin/sshd -T)
        else
            network_effective=$(/usr/sbin/sshd -T -C user=root,addr=172.16.4.2,host=172.16.4.2,laddr=100.95.133.99,lport=22)
        fi
        printf '%s\n' "$network_effective" | awk '
            $1 == "pubkeyauthentication" { pubkey = ($2 == "yes") }
            $1 == "passwordauthentication" { password = ($2 == "no") }
            $1 == "kbdinteractiveauthentication" { keyboard = ($2 == "no") }
            $1 == "authenticationmethods" { methods = ($2 == "publickey") }
            $1 == "permitrootlogin" { rootkey = ($2 == "prohibit-password" || $2 == "without-password") }
            END { exit !(pubkey && password && keyboard && methods && rootkey) }
        ' || fail "Effective SSH key-only policy did not pass for $network_context. Check native Include/Match precedence before reloading."
        printf 'Verified disk-effective SSH policy: %s\n' "$network_context"
    done
}

show_effective_policy() {
    /usr/sbin/sshd -T | awk '$1 ~ /^(port|listenaddress|pubkeyauthentication|passwordauthentication|kbdinteractiveauthentication|authenticationmethods|permitrootlogin)$/ { print }'
}

remove_unvalidated_candidate() {
    network_exit=$?
    if [ -n "$network_staging" ] && [ -f "$network_staging" ]; then
        rm "$network_staging"
    fi
    if [ "$network_exit" -ne 0 ] && [ "$network_created" = 1 ] && [ "$network_validated" = 0 ]; then
        if [ -f "$network_destination" ] && cmp -s "$network_template" "$network_destination"; then
            rm "$network_destination"
            echo 'Removed only this unvalidated candidate include; SSH was not reloaded.' >&2
        fi
    fi
}

case "${1:-}" in
    apply)
        require_host
        require_registered_key
        trap remove_unvalidated_candidate 0
        trap 'exit 130' INT
        trap 'exit 143' TERM
        if [ -e "$network_destination" ]; then
            cmp -s "$network_template" "$network_destination" \
                || fail 'An SSH include with different contents exists; inspect it before replacement.'
        else
            install_owned_include
        fi
        validate_effective_policy
        network_validated=1
        systemctl reload ssh.service \
            || fail 'Validated policy remains installed. Inspect native SSH state before retrying an uncertain reload.'
        show_effective_policy
        echo 'SSH reload returned successfully; independently verify a fresh container-origin public-key connection.'
        ;;
    check)
        require_host
        require_registered_key
        [ -f "$network_destination" ] && cmp -s "$network_template" "$network_destination" \
            || fail 'The expected source-owned include is not installed.'
        validate_effective_policy
        show_effective_policy
        echo 'This checks disk-effective configuration; fresh SSH authentication is a separate runtime check.'
        ;;
    revert)
        require_host
        if [ ! -e "$network_destination" ]; then
            echo 'The source-owned SSH include is already absent.'
            exit 0
        fi
        cmp -s "$network_template" "$network_destination" \
            || fail 'Refusing to remove an SSH include whose contents changed.'
        rm "$network_destination"
        if ! /usr/sbin/sshd -t; then
            install_owned_include
            fail 'Predecessor SSH configuration failed validation; restored this include without reloading SSH.'
        fi
        systemctl reload ssh.service \
            || fail 'The include is removed but reload is uncertain; inspect native SSH state before another action.'
        show_effective_policy
        echo 'Removed only the owned include and reloaded the remaining native SSH configuration.'
        ;;
    *)
        echo 'Usage: herdr-network-sshd apply|check|revert' >&2
        exit 2
        ;;
esac
