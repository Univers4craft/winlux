#!/bin/sh
# Only use DHCP for a wired carrier that NetworkManager does not manage.
set -u
# resolv.conf réécrit par NetworkManager à chaque bail : toujours lisible.
perms() { if test -x /usr/bin/winlux-network-perms; then /usr/bin/winlux-network-perms >/dev/null 2>&1 || true; fi; }
sleep 5
perms
sleep 15
while :; do
    perms
    if ! ip -4 route show default 2>/dev/null | grep -q '^default '; then
        for entry in /sys/class/net/*; do
            interface=${entry##*/}
            case "$interface" in lo|*[!a-zA-Z0-9_.:-]*) continue ;; esac
            test -e "$entry/device" && test ! -d "$entry/wireless" || continue
            managed=$(nmcli -g GENERAL.MANAGED device show "$interface" 2>/dev/null || true)
            case "$managed" in yes|true) continue ;; esac
            # Une carte éteinte n'annonce jamais de porteuse : l'allumer d'abord.
            if test "$(cat "$entry/operstate" 2>/dev/null || true)" = down; then
                ip link set "$interface" up 2>/dev/null || true
                sleep 3
            fi
            test "$(cat "$entry/carrier" 2>/dev/null || true)" = 1 || continue
            echo "Secours DHCP Ethernet : $interface"
            WINLUX_NETWORK_FORCE_DHCP=1 WINLUX_NETWORK_INTERFACE="$interface" /usr/bin/winlux-network start || true
        done
    fi
    sleep 10
done
