#!/bin/sh
# Only use DHCP for a wired carrier that NetworkManager does not manage.
set -u
# resolv.conf réécrit par NetworkManager à chaque bail : toujours lisible.
perms() { if test -x /usr/bin/winlux-network-perms; then /usr/bin/winlux-network-perms >/dev/null 2>&1 || true; fi; }
# Au repos, winlux-network-perms (une vingtaine de programmes : stat, chmod,
# cat…) n'est relancé que si un fichier qu'il corrige a été réécrit depuis
# son dernier passage, et au moins toutes les 10 min (#3130). Repère dans
# /run (à root), pas dans /run/winlux collant.
done_mark=/run/winlux-network-perms.done
perms_due() {
    test -e "$done_mark" || return 0
    test "$rounds" -ge 60 && return 0
    for watched in /etc/resolv.conf /etc/timezone /etc/hostname /etc/default/locale /dev/rfkill \
        /etc/polkit-1/rules.d /etc/polkit-1/rules.d/50-winlux-network.rules \
        /usr/share/winlux/native/network/50-winlux-network.rules; do
        test "$watched" -nt "$done_mark" && return 0
    done
    return 1
}
perms_if_due() {
    rounds=$((rounds + 1))
    perms_due || return 0
    rounds=0
    : >"$done_mark" 2>/dev/null || true
    perms
}
# Route IPv4 par défaut, lue dans /proc sans lancer ip ni grep.
has_default_route() {
    while read -r _iface dest _gw _flags _ref _use _metric mask _rest; do
        test "$dest" = 00000000 && test "$mask" = 00000000 && return 0
    done </proc/net/route 2>/dev/null
    return 1
}
rounds=0
sleep 5
perms
: >"$done_mark" 2>/dev/null || true
sleep 15
while :; do
    perms_if_due
    if ! has_default_route; then
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
