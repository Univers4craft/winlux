#!/bin/sh
# Start native services once. The init shell supervises xfdesktop separately.
set -eu
export DISPLAY=${DISPLAY:-:0}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/0}
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
mkdir -p "$XDG_RUNTIME_DIR" /run/winlux /var/lib/dbus
if ! test -e /var/run && ! test -L /var/run; then ln -s /run /var/run; fi
chmod 700 "$XDG_RUNTIME_DIR"
/usr/bin/dbus-uuidgen --ensure=/etc/machine-id
chmod 644 /etc/machine-id
chmod 755 /var/lib/dbus
ln -sf /etc/machine-id /var/lib/dbus/machine-id
if ! test -S /run/winlux/session-bus; then
    /usr/bin/dbus-daemon --session --fork --address=unix:path=/run/winlux/session-bus --print-address=1 > /run/winlux/dbus-address 2>/run/winlux/session-bus.log
fi
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/winlux/session-bus
export XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
export XDG_CONFIG_DIRS=/etc/xdg
export XDG_DATA_DIRS="$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share"
/usr/bin/winlux-native-config
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
# Start the desktop file service after its hardware dependencies are ready.
# An early GVfs activation could otherwise time out on software-emulated CPUs.
/usr/bin/Thunar --daemon >/run/winlux/file-manager.log 2>&1 &
wait_bus_name session org.xfce.FileManager "$!" /run/winlux/file-manager.log
# Name ownership precedes GTK/GVfs initialization. Wait for an actual method
# reply so the desktop cannot send file-launch requests to a busy startup loop.
/usr/bin/gdbus call --session --dest org.xfce.FileManager \
    --object-path /org/xfce/FileManager --method org.freedesktop.DBus.Peer.Ping \
    --timeout 90 >>/run/winlux/file-manager.log 2>&1 || {
        cat /run/winlux/file-manager.log >&2
        exit 1
    }
/usr/bin/winlux-trust-shortcuts
has_bluetooth=false
if test -d /sys/class/bluetooth && ls /sys/class/bluetooth/ >/dev/null 2>&1 && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then
    has_bluetooth=true
fi
if "$has_bluetooth"; then
    for bluetooth in /usr/lib/bluetooth/bluetoothd /usr/libexec/bluetooth/bluetoothd; do
        if test -x "$bluetooth"; then "$bluetooth" -n >/run/winlux/bluetooth.log 2>&1 & break; fi
    done
fi
/usr/bin/xfsettingsd --daemon >/run/winlux/xfsettings.log 2>&1
if "$has_bluetooth" && test -x /usr/bin/blueman-applet; then
    /usr/bin/blueman-applet >/run/winlux/bluetooth-agent.log 2>&1 &
fi
/usr/bin/winlux-notifications >/run/winlux/notifications.log 2>&1 &
/usr/bin/nm-applet --indicator >/run/winlux/network-agent.log 2>&1 &
alignment=$(sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
case "$alignment" in left|center) /usr/bin/winlux-control "align-$alignment" ;; esac
# Restore the user's input choice on each session, without resetting defaults.
speed=$(sed -n 's/^MOUSE_SPEED=//p' /etc/winlux/input.conf 2>/dev/null || true)
case "$speed" in
    25) /usr/bin/winlux-control mouse-slow || true ;;
    50) /usr/bin/winlux-control mouse-normal || true ;;
    75) /usr/bin/winlux-control mouse-fast || true ;;
esac
# Profil processeur et courbe de ventilateurs choisis dans les Parametres.
if test -s /etc/winlux/cpu.conf; then /usr/bin/winlux-control cpu-restore || true; fi
for query in /usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders /usr/bin/gdk-pixbuf-query-loaders; do
    if test -x "$query"; then "$query" --update-cache; break; fi
done
# Shared MIME and desktop caches are generated when building the image,
# rather than scanning the full database on every cold boot.
/usr/bin/winlux-applications refresh
/usr/bin/openbox --sm-disable --config-file "$HOME/.config/openbox/rc.xml" >/run/winlux/window-manager.log 2>&1 &
echo $! > /run/winlux/window-manager.pid
for attempt in $(seq 1 30); do
    /usr/bin/wmctrl -m >/dev/null 2>&1 && break
    sleep 0.1
done
/usr/bin/wmctrl -m >/dev/null
/usr/bin/xfce4-panel --disable-wm-check >/run/winlux/panel.log 2>&1 &
echo $! > /run/winlux/panel.pid
/usr/bin/winlux-session-startup >/run/winlux/startup-apps.log 2>&1
if test ! -f "$HOME/.config/winlux/welcome-complete" && test "${WINLUX_NO_WELCOME:-0}" != 1 && ! grep -qw 'winlux.native-smoke=1' /proc/cmdline; then
    /usr/bin/winlux-welcome >/run/winlux/welcome.log 2>&1 &
fi
# Xfconf activates its own session-bus service. Set wallpaper per real output.
for output in $(/usr/bin/xrandr --query | awk '$2 == "connected" {print $1}'); do
    property="/backdrop/screen0/monitor$output/workspace0/last-image"
    if ! /usr/bin/xfconf-query -c xfce4-desktop -p "$property" >/dev/null 2>&1; then
        /usr/bin/xfconf-query -c xfce4-desktop -p "$property" -n -t string -s /usr/share/winlux/wallpaper.bmp
        /usr/bin/xfconf-query -c xfce4-desktop -p "/backdrop/screen0/monitor$output/workspace0/image-style" -n -t int -s 5
    fi
done
LIBGL_ALWAYS_SOFTWARE=1 /usr/bin/xscreensaver -no-splash -verbose >/run/winlux/screensaver.log 2>&1 &
echo $! > /run/winlux/screensaver.pid
(
    while :; do
        /usr/bin/winlux-applications refresh >/run/winlux/applications.log 2>&1 || true
        /usr/bin/winlux-applications window-icons >>/run/winlux/applications.log 2>&1 || true
        if test -f /run/winlux/browser-chooser; then
            rm -f /run/winlux/browser-chooser
            /usr/bin/winlux-shell-app settings 11 >/run/winlux/browser-settings.log 2>&1 &
        fi
        sleep 5
    done
) </dev/null >/run/winlux/catalogue.log 2>&1 &
echo $! > /run/winlux/catalogue.pid
