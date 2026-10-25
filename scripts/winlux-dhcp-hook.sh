#!/bin/sh
# BusyBox udhcpc lease hook. DHCP data is passed as arguments, never evaluated.
set -eu
PATH=/usr/bin:/bin:/usr/sbin:/sbin
task_root=${WINLUX_NETWORK_ROOT:-}
run="$task_root/run/winlux"
mkdir -p "$run" "$task_root/etc"
ipcmd() { if test -n "${WINLUX_IP:-}"; then "$WINLUX_IP" "$@"; else /bin/busybox ip "$@"; fi; }
ipv4() {
    printf '%s\n' "$1" | awk -F. 'NF!=4 {exit 1} {for(i=1;i<=4;i++) if($i!~/^[0-9]+$/ || $i>255) exit 1}'
}
case "${interface:-}" in ''|*[!a-zA-Z0-9_.:-]*) exit 2 ;; esac
case "${1:-}" in
    deconfig)
        ipcmd -4 addr flush dev "$interface"
        ;;
    bound|renew)
        ipv4 "${ip:-}" || exit 2
        ipv4 "${subnet:-255.255.255.0}" || exit 2
        prefix=$(/bin/busybox ipcalc -p "$ip" "${subnet:-255.255.255.0}" | cut -d= -f2)
        case "$prefix" in ''|*[!0-9]*) exit 2 ;; esac
        test "$prefix" -le 32 || exit 2
        ipcmd -4 addr flush dev "$interface"
        ipcmd addr add "$ip/$prefix" dev "$interface"
        ipcmd link set "$interface" up
        for gateway in ${router:-}; do
            ipv4 "$gateway" || continue
            ipcmd route replace default via "$gateway" dev "$interface"
            break
        done
        resolver="$task_root/etc/resolv.conf.new"
        printf '# Configured by Winlux DHCP on %s\n' "$interface" >"$resolver"
        for server in ${dns:-}; do
            ipv4 "$server" || continue
            printf 'nameserver %s\n' "$server" >>"$resolver"
        done
        if grep -q '^nameserver ' "$resolver"; then mv "$resolver" "$task_root/etc/resolv.conf"; else rm -f "$resolver"; fi
        printf 'interface=%s\nip=%s\n' "$interface" "$ip" >"$run/network-lease.txt"
        echo "DHCP $interface: $ip/$prefix"
        ;;
    nak|leasefail) echo "DHCP $interface: ${1}" ;;
    *) exit 0 ;;
esac
