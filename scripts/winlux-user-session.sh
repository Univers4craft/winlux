#!/bin/sh
# Bureau natif d'un compte, lancé sous l'identité de ce compte.
#
# Deux usages :
# - le bureau principal d'un système installé : l'init démarre Xorg et les
#   services système (winlux-system-services, relais root winlux-root et
#   winlux-power), puis lance ce script par setpriv sous le compte créé à
#   l'installation (WINLUX_PRIMARY_SESSION=1) ;
# - un autre compte ouvert par le changement rapide d'utilisateur (#119) :
#   winlux-sessions démarre un serveur X par compte puis ce script.
#
# On ne démarre ici que ce qui appartient à la session de l'utilisateur, avec
# son propre bus D-Bus, son son (PipeWire), son gestionnaire de fenêtres, sa
# barre des tâches et son bureau. Ce qui touche au système passe par le
# relais root (winlux-control → winlux-root).
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
primary=${WINLUX_PRIMARY_SESSION:-0}
logs=${WINLUX_SESSION_LOGS:-"$XDG_RUNTIME_DIR/winlux"}
mkdir -p "$logs"
cd "$HOME" 2>/dev/null || true

pids=
bus_pid=
cleanup() {
    # Fermer la session : tout ce qu'elle a lancé s'arrête avec elle.
    for pid in $pids; do kill "$pid" 2>/dev/null || true; done
    if test -n "$bus_pid"; then kill "$bus_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
trap 'exit 0' INT TERM HUP
start() {
    # start JOURNAL COMMANDE… : en arrière-plan, arrêté avec la session.
    journal=$1; shift
    "$@" >"$logs/$journal" 2>&1 &
    pids="$pids $!"
}
wait_bus_name() {
    for attempt in $(seq 1 150); do
        if /usr/bin/gdbus call --session --dest org.freedesktop.DBus \
            --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner \
            --timeout 2 "$1" 2>/dev/null | grep -q true; then return 0; fi
        kill -0 "$2" 2>/dev/null || return 1
        sleep 0.2
    done
    return 1
}

# Bus de session propre au compte : ses applications ne parlent pas à celles
# d'un autre bureau.
if test -x /usr/bin/dbus-daemon; then
    bus="$XDG_RUNTIME_DIR/bus"
    rm -f "$bus"
    /usr/bin/dbus-daemon --session --fork --address="unix:path=$bus" --print-pid=1 > "$logs/bus.pid" 2>"$logs/session-bus.log"
    bus_pid=$(cat "$logs/bus.pid" 2>/dev/null || true)
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$bus"
fi
# Le relais root agit sur cet écran pour les réglages d'écran et de pavé
# tactile (xrandr, xinput) : root y est autorisé, personne d'autre.
if test -x /usr/bin/xhost; then /usr/bin/xhost +si:localuser:root >/dev/null 2>&1 || true; fi

# Réglages par défaut du bureau, copiés une fois dans le dossier du compte.
if test -x /usr/bin/winlux-native-config; then /usr/bin/winlux-native-config >"$logs/native-config.log" 2>&1 || true; fi

# Son (#115) : PipeWire du compte, un flux par programme (volume par appli).
if test -x /usr/bin/pipewire && test -x /usr/bin/wireplumber && ! pgrep -u "$uid" -x pipewire >/dev/null 2>&1; then
    start pipewire.log /usr/bin/pipewire
    start wireplumber.log /usr/bin/wireplumber
    if test -x /usr/bin/pipewire-pulse; then start pipewire-pulse.log /usr/bin/pipewire-pulse; fi
fi
# Service de fichiers du bureau (ouverture des fichiers depuis le bureau).
if test -x /usr/bin/Thunar; then
    /usr/bin/Thunar --daemon >"$logs/file-manager.log" 2>&1 &
    thunar_pid=$!
    pids="$pids $thunar_pid"
    if wait_bus_name org.xfce.FileManager "$thunar_pid"; then
        /usr/bin/gdbus call --session --dest org.xfce.FileManager \
            --object-path /org/xfce/FileManager --method org.freedesktop.DBus.Peer.Ping \
            --timeout 90 >>"$logs/file-manager.log" 2>&1 || true
    fi
fi
if test -x /usr/bin/winlux-trust-shortcuts; then /usr/bin/winlux-trust-shortcuts >"$logs/trust-shortcuts.log" 2>&1 || true; fi
if test -x /usr/bin/xfsettingsd; then /usr/bin/xfsettingsd --daemon >"$logs/xfsettings.log" 2>&1 || true; fi
# Thème d'icônes Winlux par défaut, sans écraser un choix de l'utilisateur.
if test -d /usr/share/icons/Winlux && test -x /usr/bin/xfconf-query && ! /usr/bin/xfconf-query -c xsettings -p /Net/IconThemeName >/dev/null 2>&1; then
    /usr/bin/xfconf-query -c xsettings -p /Net/IconThemeName -n -t string -s Winlux || true
fi
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
# Sans connexion automatique, la session s'ouvre verrouillée : le mot de passe
# du compte est demandé avant d'afficher le bureau.
if test "${WINLUX_LOCK_AT_START:-0}" = 1 && test -x /usr/bin/winlux-lock; then
    (setsid /usr/bin/winlux-lock now >"$logs/lock-start.log" 2>&1 &)
fi
# Compositeur facultatif : sans lui, un déplacement laisse des traînées.
if test -x /usr/bin/picom && test "${WINLUX_COMPOSITOR:-1}" != 0 && test -f "$HOME/.config/picom/picom.conf"; then
    start compositor.log /usr/bin/picom --config "$HOME/.config/picom/picom.conf"
fi
if test -x /usr/bin/xfce4-panel; then
    start panel.log /usr/bin/xfce4-panel --disable-wm-check
fi
if test -x /usr/bin/winlux-notifications; then
    start notifications.log /usr/bin/winlux-notifications
fi
has_bluetooth=false
if test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1; then has_bluetooth=true
elif test -d /sys/class/bluetooth && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then has_bluetooth=true; fi
if "$has_bluetooth" && test -x /usr/bin/blueman-applet; then start bluetooth-agent.log /usr/bin/blueman-applet; fi
# Presse-papiers commun : copier dans un .exe, coller dans une appli Linux.
if test -x /usr/bin/winlux; then start clipboard-bridge.log /usr/bin/winlux --clipboard-bridge; fi
# Lecteurs réseau : reconnexion au démarrage et surveillance hors ligne.
if test -x /usr/bin/winlux-drives; then start netdrives.log /usr/bin/winlux-drives watch; fi
if test -x /usr/bin/nm-applet; then start network-agent.log /usr/bin/nm-applet --indicator; fi
# Préférences de l'utilisateur : alignement de la barre, souris, processeur.
if test "$primary" = 1 && test -x /usr/bin/winlux-control; then
    alignment=$(sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
    case "$alignment" in left|center) /usr/bin/winlux-control "align-$alignment" >/dev/null 2>&1 || true ;; esac
    speed=$(sed -n 's/^MOUSE_SPEED=//p' /etc/winlux/input.conf 2>/dev/null || true)
    case "$speed" in
        25) /usr/bin/winlux-control mouse-slow >/dev/null 2>&1 || true ;;
        50) /usr/bin/winlux-control mouse-normal >/dev/null 2>&1 || true ;;
        75) /usr/bin/winlux-control mouse-fast >/dev/null 2>&1 || true ;;
    esac
    if test -s /etc/winlux/cpu.conf; then /usr/bin/winlux-control cpu-restore >/dev/null 2>&1 || true; fi
fi
if test -x /usr/bin/winlux-applications; then /usr/bin/winlux-applications refresh >"$logs/applications.log" 2>&1 || true; fi
if test -x /usr/bin/winlux-session-startup; then /usr/bin/winlux-session-startup >"$logs/startup-apps.log" 2>&1 || true; fi
# Logiciels demandés depuis Bienvenue et interrompus par la fin de session.
if command -v winlux-store >/dev/null 2>&1; then
    (winlux-store starter --resume >"$logs/starter.log" 2>&1 &)
fi
if test "$primary" = 1 && test ! -f "$HOME/.config/winlux/welcome-complete" && test "${WINLUX_NO_WELCOME:-0}" != 1 && test -x /usr/bin/winlux-welcome; then
    (/usr/bin/winlux-welcome >"$logs/welcome.log" 2>&1 &)
fi
# Fond d'écran par écran : l'identité Winlux, sinon le fond de l'image.
default_wallpaper=/usr/share/winlux/wallpapers/winlux-prisme-nuit.png
test -f "$default_wallpaper" || default_wallpaper=/usr/share/winlux/wallpaper.bmp
if test -x /usr/bin/xfconf-query && test -x /usr/bin/xrandr; then
    for output in $(/usr/bin/xrandr --query | awk '$2 == "connected" {print $1}'); do
        property="/backdrop/screen0/monitor$output/workspace0/last-image"
        if ! /usr/bin/xfconf-query -c xfce4-desktop -p "$property" >/dev/null 2>&1; then
            /usr/bin/xfconf-query -c xfce4-desktop -p "$property" -n -t string -s "$default_wallpaper" || true
            /usr/bin/xfconf-query -c xfce4-desktop -p "/backdrop/screen0/monitor$output/workspace0/image-style" -n -t int -s 5 || true
        fi
    done
fi
if test -x /usr/bin/xscreensaver; then
    if test "$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)" -gt 0; then
        start screensaver.log /usr/bin/xscreensaver -no-splash
    else
        start screensaver.log env LIBGL_ALWAYS_SOFTWARE=1 /usr/bin/xscreensaver -no-splash
    fi
fi
# Verrouillage après inactivité et au retour de veille.
if test -x /usr/bin/winlux-lock; then start lock.log /usr/bin/winlux-lock daemon; fi
# Catalogue des applications (raccourcis .lnk, programmes installés).
(
    while :; do
        /usr/bin/winlux-applications refresh >"$logs/catalogue.log" 2>&1 || true
        /usr/bin/winlux-applications window-icons >>"$logs/catalogue.log" 2>&1 || true
        if test "$primary" = 1 && test -O /run/winlux/browser-chooser; then
            rm -f /run/winlux/browser-chooser
            /usr/bin/winlux-shell-app settings 11 >"$logs/browser-settings.log" 2>&1 &
        fi
        sleep 5
    done
) </dev/null >/dev/null 2>&1 &
pids="$pids $!"
if test "$primary" = 1; then
    # État publié pour les Paramètres et la barre des tâches.
    (
        while :; do
            /usr/bin/winlux-control status >/dev/null 2>&1 || true
            sleep 3
        done
    ) </dev/null >/dev/null 2>&1 &
    pids="$pids $!"
    # Écrans : branchement à chaud, capot et taille de la fenêtre VM.
    if test -x /usr/bin/winlux-display; then
        (
            while :; do
                /usr/bin/winlux-display watch 2>>"$logs/display.log" || true
                sleep 1
            done
        ) </dev/null >/dev/null 2>&1 &
        pids="$pids $!"
    fi
fi

# Le bureau (fond d'écran, icônes) est relancé s'il s'arrête ; la session
# vit tant que son gestionnaire de fenêtres vit.
while kill -0 "$wm_pid" 2>/dev/null; do
    if test "$primary" = 1; then
        keyboard_layout=$(sed -n 's/^XKB_LAYOUT=//p' /etc/winlux/input.conf 2>/dev/null || true)
        case "$keyboard_layout" in fr|us|de|es|gb|it|pt|be) /usr/bin/setxkbmap "$keyboard_layout" 2>/dev/null || true ;; esac
        if test -x /usr/bin/winlux-touchpad; then /usr/bin/winlux-touchpad apply 2>/dev/null || true; fi
        ui_scale=$(/usr/bin/winlux-display session-scale 2>/dev/null || true)
        case "$ui_scale" in 1|1.25|1.5|1.75|2|2.5|3) ;; *) ui_scale=1 ;; esac
        WINLUX_THEME=$(sed -n 's/^THEME=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
        export WINLUX_UI_SCALE="$ui_scale" WINLUX_THEME
    fi
    if test -x /usr/bin/xfdesktop; then
        /usr/bin/xfdesktop --disable-wm-check >>"$logs/desktop.log" 2>&1 &
        desktop_pid=$!
        echo "WINLUX_DESKTOP_READY pid=$desktop_pid user=$(id -un)"
        while kill -0 "$desktop_pid" 2>/dev/null && kill -0 "$wm_pid" 2>/dev/null; do sleep 1; done
        kill "$desktop_pid" 2>/dev/null || true
        wait "$desktop_pid" 2>/dev/null || true
    else
        wait "$wm_pid" 2>/dev/null || true
    fi
    sleep 1
done
