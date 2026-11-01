#!/bin/sh
# Bureau natif d'un autre compte (changement rapide d'utilisateur, #119).
#
# winlux-sessions démarre un serveur X par compte puis lance ce script sous
# l'identité du compte. Les services système (D-Bus système, réseau, son,
# impression...) tournent déjà pour le premier bureau : on ne démarre ici que
# ce qui appartient à la session de l'utilisateur, avec son propre bus D-Bus,
# son propre gestionnaire de fenêtres, sa barre des tâches et son bureau.
set -eu
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
uid=$(id -u)
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$uid}
if ! mkdir -p "$XDG_RUNTIME_DIR" 2>/dev/null || ! test -w "$XDG_RUNTIME_DIR"; then
    XDG_RUNTIME_DIR=$(mktemp -d "/tmp/winlux-runtime-$uid.XXXXXX")
fi
chmod 700 "$XDG_RUNTIME_DIR"
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
export XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
export XDG_CONFIG_DIRS=/etc/xdg
export XDG_DATA_DIRS="$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share"
export WINLUX_NATIVE_SHELL=1
if test -z "${LANG:-}"; then
    if test -d /usr/lib/locale/fr_FR.utf8; then export LANG=fr_FR.UTF-8; else export LANG=C.UTF-8; fi
fi
logs="$XDG_RUNTIME_DIR/winlux"
mkdir -p "$logs"

pids=
bus_pid=
cleanup() {
    # Fermer la session : tout ce qu'elle a lancé s'arrête avec elle.
    for pid in $pids; do kill "$pid" 2>/dev/null || true; done
    if test -n "$bus_pid"; then kill "$bus_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
trap 'exit 0' INT TERM HUP

# Bus de session propre au compte : ses applications ne parlent pas à celles
# de l'autre bureau.
if test -x /usr/bin/dbus-daemon; then
    bus="$XDG_RUNTIME_DIR/bus"
    rm -f "$bus"
    /usr/bin/dbus-daemon --session --fork --address="unix:path=$bus" --print-pid=1 > "$logs/bus.pid" 2>"$logs/session-bus.log"
    bus_pid=$(cat "$logs/bus.pid" 2>/dev/null || true)
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$bus"
fi

# Réglages par défaut du bureau, copiés une fois dans le dossier du compte.
if test -x /usr/bin/winlux-native-config; then /usr/bin/winlux-native-config >"$logs/native-config.log" 2>&1 || true; fi

if test -x /usr/bin/xfsettingsd; then /usr/bin/xfsettingsd --daemon >"$logs/xfsettings.log" 2>&1 || true; fi
rc="$HOME/.config/openbox/rc.xml"
if test -f "$rc"; then
    /usr/bin/openbox --sm-disable --config-file "$rc" >"$logs/window-manager.log" 2>&1 &
else
    /usr/bin/openbox --sm-disable >"$logs/window-manager.log" 2>&1 &
fi
wm_pid=$!
pids="$pids $wm_pid"
for attempt in $(seq 1 50); do
    /usr/bin/wmctrl -m >/dev/null 2>&1 && break
    kill -0 "$wm_pid" 2>/dev/null || { cat "$logs/window-manager.log" >&2; exit 1; }
    sleep 0.1
done
if test -x /usr/bin/xfce4-panel; then
    /usr/bin/xfce4-panel --disable-wm-check >"$logs/panel.log" 2>&1 &
    pids="$pids $!"
fi
if test -x /usr/bin/winlux-notifications; then
    /usr/bin/winlux-notifications >"$logs/notifications.log" 2>&1 &
    pids="$pids $!"
fi
if test -x /usr/bin/winlux-session-startup; then /usr/bin/winlux-session-startup >"$logs/startup-apps.log" 2>&1 || true; fi
if test -x /usr/bin/xscreensaver; then
    /usr/bin/xscreensaver -no-splash >"$logs/screensaver.log" 2>&1 &
    pids="$pids $!"
fi

# Le bureau (fond d'écran, icônes) est relancé s'il s'arrête ; la session
# vit tant que son gestionnaire de fenêtres vit.
while kill -0 "$wm_pid" 2>/dev/null; do
    if test -x /usr/bin/xfdesktop; then
        /usr/bin/xfdesktop --disable-wm-check >>"$logs/desktop.log" 2>&1 &
        desktop_pid=$!
        while kill -0 "$desktop_pid" 2>/dev/null && kill -0 "$wm_pid" 2>/dev/null; do sleep 1; done
        kill "$desktop_pid" 2>/dev/null || true
        wait "$desktop_pid" 2>/dev/null || true
    else
        wait "$wm_pid" 2>/dev/null || true
    fi
    sleep 1
done
