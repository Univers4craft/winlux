#!/bin/sh
# Start native services once. The init shell supervises xfdesktop separately.
set -eu
export DISPLAY=${DISPLAY:-:0}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/0}
mkdir -p "$XDG_RUNTIME_DIR" /run/winlux /var/lib/dbus
chmod 700 "$XDG_RUNTIME_DIR"
/usr/bin/dbus-uuidgen --ensure=/etc/machine-id
ln -sf /etc/machine-id /var/lib/dbus/machine-id
if ! test -S /run/winlux/session-bus; then
    /usr/bin/dbus-daemon --session --fork --address=unix:path=/run/winlux/session-bus --print-address=1 > /run/winlux/dbus-address 2>/run/winlux/session-bus.log
fi
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/winlux/session-bus
export XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
export XDG_CONFIG_DIRS=/etc/xdg XDG_DATA_DIRS=/usr/local/share:/usr/share
/usr/bin/winlux-native-config
# Restore the user's input choice on each session, without resetting defaults.
speed=$(sed -n 's/^MOUSE_SPEED=//p' /etc/winlux/input.conf 2>/dev/null || true)
case "$speed" in
    25) /usr/bin/winlux-control mouse-slow || true ;;
    50) /usr/bin/winlux-control mouse-normal || true ;;
    75) /usr/bin/winlux-control mouse-fast || true ;;
esac
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
            /usr/bin/winlux-shell-app settings 10 >/run/winlux/browser-settings.log 2>&1 &
        fi
        sleep 5
    done
) </dev/null >/run/winlux/catalogue.log 2>&1 &
echo $! > /run/winlux/catalogue.pid
