#!/bin/sh
set -eu

network_chain=HERDR-NET-PUBLIC
network_comment=herdr-network-public-vxlan
network_interface=eth0

fail() {
    echo "$*" >&2
    exit 1
}

require_host() {
    [ "$(id -u)" = 0 ] || fail 'Run this host firewall helper as root.'
    [ "$(hostname -s)" = srv1873899 ] || fail 'This policy is pinned to srv1873899.'
    /usr/sbin/iptables -w 5 -S INPUT >/dev/null
    /usr/sbin/ip6tables -w 5 -S INPUT >/dev/null
}

chain_exists() {
    "$network_binary" -w 5 -S "$network_chain" >/dev/null 2>&1
}

has_jump() {
    "$network_binary" -w 5 -C INPUT -i "$network_interface" -p udp -m udp --dport 8472 \
        -m comment --comment "$network_comment" -j "$network_chain" >/dev/null 2>&1
}

has_drop() {
    "$network_binary" -w 5 -C "$network_chain" -m comment --comment "$network_comment" -j DROP >/dev/null 2>&1
}

assert_owned_chain() {
    if ! chain_exists; then
        return
    fi
    network_rule_count=$("$network_binary" -w 5 -S "$network_chain" | awk '$1 == "-A" { n++ } END { print n+0 }')
    if [ "$network_rule_count" -ne 0 ]; then
        [ "$network_rule_count" = 1 ] && has_drop \
            || fail 'The owned firewall chain contains unexpected rules; no changes were made to it.'
    fi
    network_reference_count=$("$network_binary" -w 5 -S | awk -v chain="$network_chain" '$NF == chain && ($(NF-1) == "-j" || $(NF-1) == "-g") { n++ } END { print n+0 }')
    if [ "$network_reference_count" -ne 0 ]; then
        [ "$network_reference_count" = 1 ] && has_jump \
            || fail 'The owned firewall chain has unexpected references; inspect them before changing it.'
    fi
}

jump_is_first() {
    network_first_rule=$("$network_binary" -w 5 -S INPUT | awk '$1 == "-A" && !seen { print; seen=1 }')
    network_owned_reference=$("$network_binary" -w 5 -S INPUT | awk -v chain="$network_chain" '$NF == chain && $(NF-1) == "-j" { print }')
    [ -n "$network_owned_reference" ] && [ "$network_first_rule" = "$network_owned_reference" ]
}

check_family() {
    assert_owned_chain
    has_drop && has_jump && jump_is_first \
        || fail "$network_binary does not have the exact owned public-UDP8472 block first in INPUT."
    "$network_binary" -w 5 -S "$network_chain"
    "$network_binary" -w 5 -S INPUT | awk -v chain="$network_chain" '$NF == chain && $(NF-1) == "-j" { print }'
}

case "${1:-}" in
    apply|check|remove)
        network_action=$1
        require_host
        if [ "$network_action" != remove ]; then
            [ -d "/sys/class/net/$network_interface" ] || fail 'The reviewed public eth0 interface is absent.'
        fi
        # Validate both existing owned-chain namespaces before any policy change.
        for network_binary in /usr/sbin/iptables /usr/sbin/ip6tables; do
            assert_owned_chain
        done
        for network_binary in /usr/sbin/iptables /usr/sbin/ip6tables; do
            case "$network_action" in
                apply)
                    if ! chain_exists; then
                        "$network_binary" -w 5 -N "$network_chain"
                    fi
                    if ! has_drop; then
                        "$network_binary" -w 5 -A "$network_chain" -m comment --comment "$network_comment" -j DROP
                    fi
                    if has_jump; then
                        if ! jump_is_first; then
                            "$network_binary" -w 5 -D INPUT -i "$network_interface" -p udp -m udp --dport 8472 \
                                -m comment --comment "$network_comment" -j "$network_chain"
                            "$network_binary" -w 5 -I INPUT 1 -i "$network_interface" -p udp -m udp --dport 8472 \
                                -m comment --comment "$network_comment" -j "$network_chain"
                        fi
                    else
                        "$network_binary" -w 5 -I INPUT 1 -i "$network_interface" -p udp -m udp --dport 8472 \
                            -m comment --comment "$network_comment" -j "$network_chain"
                    fi
                    check_family
                    ;;
                check)
                    check_family
                    ;;
                remove)
                    if has_jump; then
                        "$network_binary" -w 5 -D INPUT -i "$network_interface" -p udp -m udp --dport 8472 \
                            -m comment --comment "$network_comment" -j "$network_chain"
                    fi
                    if chain_exists; then
                        if has_drop; then
                            "$network_binary" -w 5 -D "$network_chain" -m comment --comment "$network_comment" -j DROP
                        fi
                        "$network_binary" -w 5 -X "$network_chain"
                    fi
                    printf 'Removed only owned rules: %s\n' "$network_binary"
                    ;;
            esac
        done
        ;;
    *)
        echo 'Usage: herdr-network-firewall apply|check|remove' >&2
        exit 2
        ;;
esac
