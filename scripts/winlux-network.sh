#!/bin/sh
# DHCP service for the live image and the persistent Winlux root.
set -eu
PATH=/usr/bin:/bin:/usr/sbin:/sbin
task_root=${WINLUX_NETWORK_ROOT:-}
run="$task_root/run/winlux"
sys="$task_root/sys/class/net"
hook=${WINLUX_DHCP_HOOK:-/usr/share/winlux/udhcpc.script}
mkdir -p "$run"
if test -z "$task_root" && command -v nmcli >/dev/null 2>&1 && nmcli -t -f RUNNING general 2>/dev/null | grep -qx running; then
    case "${1:-status}" in
        start) exec nmcli networking on ;;
        restart) nmcli networking off; exec nmcli networking on ;;
        stop) exec nmcli networking off ;;
        status) exec nmcli device status ;;
    esac
fi
ipcmd() { if test -n "${WINLUX_IP:-}"; then "$WINLUX_IP" "$@"; else /bin/busybox ip "$@"; fi; }
dhcp() { if test -n "${WINLUX_UDHCPC:-}"; then "$WINLUX_UDHCPC" "$@"; else /bin/busybox udhcpc "$@"; fi; }
stop() {
    for file in "$run"/dhcp-*.pid; do
        test -f "$file" || continue
        pid=$(cat "$file")
        case "$pid" in ''|*[!0-9]*) continue ;; esac
        kill "$pid" 2>/dev/null || true
        rm -f "$file"
    done
}
case "${1:-status}" in
    start|restart)
        if test "${1:-}" = restart; then stop; fi
        ipcmd link set lo up
        found=0
        for entry in "$sys"/*; do
            test -d "$entry" || continue
            interface=${entry##*/}
            case "$interface" in lo|*[!a-zA-Z0-9_.:-]*) continue ;; esac
            # Ethernet, Wi-Fi and virtual NICs all expose this interface.  A
            # Wi-Fi radio still needs an association before DHCP can succeed.
            ipcmd link set "$interface" up || continue
            if test -s "$run/dhcp-$interface.pid"; then
                pid=$(cat "$run/dhcp-$interface.pid")
                case "$pid" in ''|*[!0-9]*) ;; *) kill -0 "$pid" 2>/dev/null && continue ;; esac
            fi
            found=1
            dhcp -b -t 3 -T 2 -i "$interface" -p "$run/dhcp-$interface.pid" -s "$hook" >>"$run/network.log" 2>&1 &
        done
        test "$found" = 0 || echo "DHCP started; see /run/winlux/network.log"
        ;;
    stop) stop ;;
    status)
        ipcmd -4 addr show
        ipcmd route show
        ;;
    *) echo "usage: winlux-network start|restart|stop|status" >&2; exit 2 ;;
esac
