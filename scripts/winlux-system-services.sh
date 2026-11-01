#!/bin/sh
# Services système de Winlux (root) : D-Bus système, polkit, udisks,
# NetworkManager, CUPS et Bluetooth. Winlux n'a pas systemd : l'init les
# démarre ici, une fois, avant le bureau. Le bureau lui-même (session Live
# en root, ou session du compte sur un système installé) ne lance que ce
# qui appartient à l'utilisateur.
set -eu
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
mkdir -p /run/winlux /var/lib/dbus
if ! test -e /var/run && ! test -L /var/run; then ln -s /run /var/run; fi
/usr/bin/dbus-uuidgen --ensure=/etc/machine-id
chmod 644 /etc/machine-id
chmod 755 /var/lib/dbus
ln -sf /etc/machine-id /var/lib/dbus/machine-id
# Hardware services run on the system bus; applications use the session bus.
mkdir -p /run/dbus
chmod 755 /run/dbus
if ! test -S /run/dbus/system_bus_socket; then
    /usr/bin/dbus-daemon --system --fork >/run/winlux/system-bus.log 2>&1
fi
# Winlux uses its own init, so start hardware daemons directly instead of
# relying on systemd activation from the distribution's D-Bus service files.
wait_bus_name() {
    bus=$1; name=$2; service_pid=$3; service_log=$4
    for attempt in $(seq 1 150); do
        if /usr/bin/gdbus call "--$bus" --dest org.freedesktop.DBus \
            --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner \
            --timeout 2 "$name" 2>/dev/null | grep -q true; then return 0; fi
        if ! kill -0 "$service_pid" 2>/dev/null; then
            echo "Service arrêté avant son enregistrement D-Bus : $name" >&2
            cat "$service_log" >&2
            return 1
        fi
        sleep 0.2
    done
    echo "Service D-Bus indisponible : $name ; journal : $service_log" >&2
    cat "$service_log" >&2
    return 1
}
if test -x /usr/lib/polkit-1/polkitd; then
    /usr/lib/polkit-1/polkitd >/run/winlux/polkit.log 2>&1 &
    wait_bus_name system org.freedesktop.PolicyKit1 "$!" /run/winlux/polkit.log
fi
if test -x /usr/libexec/udisks2/udisksd; then
    /usr/libexec/udisks2/udisksd --no-debug >/run/winlux/udisks.log 2>&1 &
    wait_bus_name system org.freedesktop.UDisks2 "$!" /run/winlux/udisks.log
fi
if test -x /usr/sbin/NetworkManager; then
    mkdir -p /etc/netplan /etc/NetworkManager/system-connections
    if test -x /usr/sbin/wpa_supplicant; then
        /usr/sbin/wpa_supplicant -u -s >/run/winlux/wpa-supplicant.log 2>&1 &
    fi
    /usr/sbin/NetworkManager --no-daemon >/run/winlux/network-manager.log 2>&1 &
    network_pid=$!
    echo "$network_pid" >/run/winlux/network-manager.pid
    /usr/bin/winlux-network-watch "$network_pid" >/run/winlux/network-fallback.log 2>&1 &
fi
# Printing (#111): there is no systemd to socket-activate CUPS, start it here.
if test -x /usr/sbin/cupsd && ! test -S /run/cups/cups.sock; then
    mkdir -p /run/cups /var/spool/cups/tmp /var/cache/cups
    /usr/sbin/cupsd -f >/run/winlux/cups.log 2>&1 &
fi
has_bluetooth=false
if test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1; then
    has_bluetooth=true
elif test -d /sys/class/bluetooth && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then
    has_bluetooth=true
fi
if "$has_bluetooth"; then
    for bluetooth in /usr/lib/bluetooth/bluetoothd /usr/libexec/bluetooth/bluetoothd; do
        if test -x "$bluetooth"; then "$bluetooth" -n >/run/winlux/bluetooth.log 2>&1 & break; fi
    done
fi
