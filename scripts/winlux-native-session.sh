#!/bin/sh
# Start native services once. The init shell supervises xfdesktop separately.
set -eu
# Jeux Debian (/usr/games) et logiciels locaux : sans eux dans PATH, GIO
# masquait leur raccourci (Démineur installé depuis la Boutique introuvable).
export PATH="/usr/local/sbin:/usr/local/bin:$PATH:/usr/games:/usr/local/games"
export DISPLAY=${DISPLAY:-:0}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/0}
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/winlux/session-bus
export XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
# Liens web des programmes qui lisent $BROWSER (outils en ligne de commande,
# Python webbrowser) : le navigateur installé, comme xdg-open et les .exe.
export BROWSER="${BROWSER:-/usr/bin/winlux-browser}"
export XDG_CONFIG_DIRS=/etc/xdg
export XDG_DATA_DIRS="$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share"
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
# Services sans écran (bus D-Bus, polkit, udisks, réseau, impression, son) :
# l'init les lance avec --early pendant que Xorg démarre, en parallèle.
early_services() {
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
    /usr/bin/winlux-native-config
    # Services système (D-Bus système, polkit, udisks, réseau, impression,
    # Bluetooth) : partagés avec le bureau du compte sur un système installé.
    /usr/bin/winlux-system-services
    # Sound (#115): PipeWire with its session manager and Pulse server gives every
    # .exe its own stream (per-app volume). ALSA alone stays the fallback.
    if test -x /usr/bin/pipewire && test -x /usr/bin/wireplumber; then
        /usr/bin/pipewire >/run/winlux/pipewire.log 2>&1 &
        /usr/bin/wireplumber >/run/winlux/wireplumber.log 2>&1 &
        if test -x /usr/bin/pipewire-pulse; then /usr/bin/pipewire-pulse >/run/winlux/pipewire-pulse.log 2>&1 & fi
    fi
}
early_done=/run/winlux/session-early.done early_failed=/run/winlux/session-early.failed
if test "${1:-}" = --early; then
    rm -f "$early_done" "$early_failed"
    trap 'if test -e "$early_done"; then :; else touch "$early_failed"; fi' EXIT
    early_services
    touch "$early_done"
    exit 0
fi
# Lancement par l'init : attendre la partie déjà partie avec Xorg.
early_pid=$(cat /run/winlux/session-early.pid 2>/dev/null || true)
case "$early_pid" in
    ''|*[!0-9]*) ;;
    *) while ! test -e "$early_done" && ! test -e "$early_failed" && kill -0 "$early_pid" 2>/dev/null; do sleep 0.05; done ;;
esac
if test -e "$early_failed"; then
    echo "Services de session en échec (voir /run/winlux/native-early.log)" >&2
    cat /run/winlux/native-early.log >&2 2>/dev/null || true
    exit 1
fi
test -e "$early_done" || early_services
# Gestionnaire de fenêtres lancé d'emblée : il est prêt quand la barre arrive.
/usr/bin/openbox --sm-disable --config-file "$HOME/.config/openbox/rc.xml" >/run/winlux/window-manager.log 2>&1 &
echo $! > /run/winlux/window-manager.pid
# Start the desktop file service after its hardware dependencies are ready.
# An early GVfs activation could otherwise time out on software-emulated CPUs.
/usr/bin/Thunar --daemon >/run/winlux/file-manager.log 2>&1 &
# Thunar s'initialise pendant que la suite de la session démarre ; on
# l'attend seulement avant de rendre la main au bureau (xfdesktop).
thunar_pid=$!
has_bluetooth=false
if test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1; then
    has_bluetooth=true
elif test -d /sys/class/bluetooth && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then
    has_bluetooth=true
fi
/usr/bin/xfsettingsd --daemon >/run/winlux/xfsettings.log 2>&1
# Thème d'icônes Winlux par défaut, sans écraser un choix de l'utilisateur.
# Le défaut d'XFCE (elementary-xfce-dark) n'est pas livré : sans thème réel,
# les icônes retombaient sur les silhouettes noires d'Adwaita (Installer…).
icon_theme=$(/usr/bin/xfconf-query -c xsettings -p /Net/IconThemeName 2>/dev/null || true)
if test -d /usr/share/icons/Winlux && { test -z "$icon_theme" || ! test -d "/usr/share/icons/$icon_theme"; }; then
    /usr/bin/xfconf-query -c xsettings -p /Net/IconThemeName -n -t string -s Winlux || true
fi
# Thème GTK : le défaut d'XFCE (Greybird) n'est pas livré et GTK retombait
# sur Adwaita clair, même en thème sombre (Boutique, Jeux, éditeur…). Un thème
# absent est remplacé par Adwaita.
gtk_theme=$(/usr/bin/xfconf-query -c xsettings -p /Net/ThemeName 2>/dev/null || true)
# Thème absent (défaut XFCE Greybird, « Adwaita-dark » d'anciennes versions) :
# le thème Winlux du compte est créé d'après Personnalisation (clair ou
# sombre), sinon Adwaita. Une installation neuve restait claire en mode sombre.
case "$gtk_theme" in Adwaita|HighContrast|HighContrastInverse) ;; *)
    # Thèmes du compte (Winlux-dark-1… écrits par Personnalisation) compris.
    if test -z "$gtk_theme" || ! { test -d "/usr/share/themes/$gtk_theme/gtk-3.0" || test -d "$HOME/.local/share/themes/$gtk_theme/gtk-3.0" || test -d "$HOME/.themes/$gtk_theme/gtk-3.0"; }; then
        mode=dark
        if grep -qx 'THEME=light' /etc/winlux/preferences.conf 2>/dev/null; then mode=light; fi
        if ! { test -x /usr/bin/winlux-personalize && WINLUX_PERSONALIZE_NO_SYSTEM=1 /usr/bin/winlux-personalize theme "$mode" >/dev/null 2>&1; }; then
            /usr/bin/xfconf-query -c xsettings -p /Net/ThemeName -n -t string -s Adwaita 2>/dev/null || true
        fi
    fi ;;
esac
if "$has_bluetooth" && test -x /usr/bin/blueman-applet; then
    /usr/bin/blueman-applet >/run/winlux/bluetooth-agent.log 2>&1 &
fi
/usr/bin/winlux-notifications >/run/winlux/notifications.log 2>&1 &
# Fond d'écran fixé avant la barre des tâches : le bureau arrive avec lui.
# Xfconf activates its own session-bus service. Set wallpaper per real output.
# Default: the Winlux identity wallpaper, else the build's wallpaper.bmp.
default_wallpaper=/usr/share/winlux/wallpapers/winlux-prisme-nuit.png
test -f "$default_wallpaper" || default_wallpaper=/usr/share/winlux/wallpaper.bmp
for output in $(/usr/bin/xrandr --query | awk '$2 == "connected" {print $1}'); do
    property="/backdrop/screen0/monitor$output/workspace0/last-image"
    if ! /usr/bin/xfconf-query -c xfce4-desktop -p "$property" >/dev/null 2>&1; then
        /usr/bin/xfconf-query -c xfce4-desktop -p "$property" -n -t string -s "$default_wallpaper"
        /usr/bin/xfconf-query -c xfce4-desktop -p "/backdrop/screen0/monitor$output/workspace0/image-style" -n -t int -s 5
    fi
done
# Bureau de Windows 11 : la Corbeille seule (#30). Les autres icônes
# d'xfdesktop s'activent dans Paramètres ; un choix existant est gardé.
for icon in trash:true filesystem:false home:false removable:false; do
    property="/desktop-icons/file-icons/show-${icon%%:*}"
    /usr/bin/xfconf-query -c xfce4-desktop -p "$property" >/dev/null 2>&1 ||
        /usr/bin/xfconf-query -c xfce4-desktop -p "$property" -n -t bool -s "${icon#*:}" || true
done
# Échelle des applications GTK (#29), comme XFCE : facteur entier pour GTK
# (200 % en 4K), le reste par la densité des polices (150 % → 144 ppp).
# Un réglage fait à la main dans xsettings est gardé.
gtk_scale() {
    test -x /usr/bin/xfconf-query || return 0
    marker="$HOME/.local/share/winlux/gtk-scale"
    case "$1" in 1|1.25|1.5|1.75|2|2.5|3) ;; *) return 0 ;; esac
    factor=$(awk -v s="$1" 'BEGIN { print (s >= 2) ? 2 : 1 }')
    dpi=$(awk -v s="$1" -v f="$factor" 'BEGIN { printf "%d", 96 * s / f + 0.5 }')
    current=$(/usr/bin/xfconf-query -c xsettings -p /Xft/DPI 2>/dev/null || true)
    previous=$(cat "$marker" 2>/dev/null || true)
    if test -n "$current" && test "$current" != "${previous#*:}"; then return 0; fi
    test "$previous" = "$factor:$dpi" && test "$current" = "$dpi" && return 0
    for setting in "/Gdk/WindowScalingFactor:$factor" "/Xft/DPI:$dpi"; do
        /usr/bin/xfconf-query -c xsettings -p "${setting%:*}" -s "${setting##*:}" 2>/dev/null ||
            /usr/bin/xfconf-query -c xsettings -p "${setting%:*}" -n -t int -s "${setting##*:}" || true
    done
    mkdir -p "${marker%/*}" && printf '%s:%s\n' "$factor" "$dpi" >"$marker"
}
gtk_scale "${WINLUX_UI_SCALE:-$(/usr/bin/winlux-display session-scale 2>/dev/null || echo 1)}"
# Presse-papiers commun : copier dans un .exe, coller dans une appli Linux, et inversement.
if test -x /usr/bin/winlux; then /usr/bin/winlux --clipboard-bridge >/run/winlux/clipboard-bridge.log 2>&1 & fi
# Session live (root) : mêmes services que la session d'un compte
# (winlux-user-session.sh). Sans eux, Windows+V restait vide, les alarmes de
# l'Horloge ne sonnaient pas fenêtre fermée et l'éclairage nocturne ne
# s'appliquait pas.
if test -x /usr/bin/winlux-clipboard; then /usr/bin/winlux-clipboard daemon >/run/winlux/clipboard-history.log 2>&1 & fi
if test -x /usr/bin/winlux-alarms; then /usr/bin/winlux-alarms daemon >/run/winlux/alarms.log 2>&1 & fi
if test -x /usr/bin/winlux-ambiance; then /usr/bin/winlux-ambiance daemon >/run/winlux/ambiance.log 2>&1 & fi
if test -x /usr/bin/winlux-snap; then /usr/bin/winlux-snap watch >/run/winlux/snap-watch.log 2>&1 & fi
# Lecteurs réseau : reconnexion au démarrage et surveillance hors ligne.
if test -x /usr/bin/winlux-drives; then /usr/bin/winlux-drives watch >/run/winlux/netdrives.log 2>&1 & fi
# Dossiers partagés : état publié pour l'Explorateur, serveur relancé si besoin.
if test -x /usr/bin/winlux-share; then /usr/bin/winlux-share ensure >/dev/null 2>&1 & fi
/usr/bin/nm-applet --indicator >/run/winlux/network-agent.log 2>&1 &
# Téléphone connecté : seulement si un téléphone est associé à ce compte.
if test -x /usr/bin/winlux-phone; then /usr/bin/winlux-phone serve --if-paired >/run/winlux/phone.log 2>&1 & fi
alignment=$(sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
# Réglages restaurés sans le rapport d'état complet de winlux-control
# (l'init le rafraîchit toutes les 3 s) : 18 s de gagnées en VM émulée.
# En arrière-plan : xfconf garde l'alignement d'une session à l'autre et le
# panneau suit en direct toute correction ; souris et processeur n'ont pas
# besoin d'attendre non plus.
speed=$(sed -n 's/^MOUSE_SPEED=//p' /etc/winlux/input.conf 2>/dev/null || true)
# Icône du clavier tactile (tablette sans clavier) écrite avant le lancement
# de la barre, plus bas : un xfce4-panel déjà lancé ne chargeait pas le greffon.
if test -x /usr/bin/winlux-osk; then WINLUX_OSK_NO_RESTART=1 /usr/bin/winlux-osk panel || true; fi
(
    export WINLUX_CONTROL_NO_STATUS=1
    case "$alignment" in left|center) /usr/bin/winlux-control "align-$alignment" ;; esac
    # Restore the user's input choice on each session, without resetting defaults.
    case "$speed" in
        25) /usr/bin/winlux-control mouse-slow || true ;;
        50) /usr/bin/winlux-control mouse-normal || true ;;
        75) /usr/bin/winlux-control mouse-fast || true ;;
    esac
    # Souris : précision, défilement, gaucher, double-clic, couleur, traînée.
    if test -x /usr/bin/winlux-mouse; then /usr/bin/winlux-mouse apply || true; fi
    # Profil processeur et courbe de ventilateurs choisis dans les Parametres.
    if test -s /etc/winlux/cpu.conf; then /usr/bin/winlux-control cpu-restore || true; fi
) </dev/null >/run/winlux/session-settings.log 2>&1 &
# Shared MIME and desktop caches are generated when building the image,
# rather than scanning the full database on every cold boot.
# Catalogue des programmes Windows (menu Démarrer) : son premier passage se
# fait en parallèle de la barre des tâches au lieu de la retarder ; les
# raccourcis de la session précédente restent affichés entre-temps.
# Un seul processus résident, en basse priorité : il refait le catalogue
# quand un raccourci, le registre ou un navigateur change, au lieu de tout
# relire toutes les 5 s (#214). Son journal est borné par lui-même.
nice -n 10 /usr/bin/winlux-applications watch </dev/null >>/run/winlux/applications.log 2>&1 &
echo $! > /run/winlux/catalogue.pid
# Demandes ponctuelles des navigateurs : deux tests de fichier, pas de Python.
(
    while :; do
        if test -f /run/winlux/browser-chooser; then
            rm -f /run/winlux/browser-chooser
            /usr/bin/winlux-shell-app settings 11 >/run/winlux/browser-settings.log 2>&1 &
        fi
        # Lien demandé sans navigateur : ouvert dès qu'un navigateur est installé.
        if test -f "/run/winlux/browser-pending-$(id -u)"; then /usr/bin/winlux-browser --pending >/dev/null 2>&1 & fi
        sleep 5
    done
) </dev/null >/run/winlux/catalogue.log 2>&1 &
for attempt in $(seq 1 30); do
    /usr/bin/wmctrl -m >/dev/null 2>&1 && break
    sleep 0.1
done
/usr/bin/wmctrl -m >/dev/null
# Compositeur facultatif (absent des anciennes images) : sans lui, chaque
# fenêtre découverte doit se repeindre et un déplacement laisse des traînées.
if test -x /usr/bin/picom && test "${WINLUX_COMPOSITOR:-1}" != 0; then
    /usr/bin/picom --config "$HOME/.config/picom/picom.conf" >/run/winlux/compositor.log 2>&1 &
    # Le bureau (xfdesktop) est lancé ensuite par l'init : son arrivée laissait
    # noires les fenêtres déjà ouvertes (Bienvenue au premier démarrage).
    # Une remise à zéro de picom, une fois le bureau affiché, les repeint.
    (
        tries=0
        while ! pgrep -x xfdesktop >/dev/null 2>&1 && test "$tries" -lt 60; do sleep 0.5; tries=$((tries + 1)); done
        sleep 2
        pkill -USR1 -x picom 2>/dev/null
    ) >/dev/null 2>&1 &
fi
/usr/bin/xfce4-panel --disable-wm-check >/run/winlux/panel.log 2>&1 &
echo $! > /run/winlux/panel.pid
# Clavier ou souris USB branché pendant la session : disposition et réglages
# réappliqués (un clavier branché à chaud tapait en QWERTY).
if test -x /usr/bin/winlux-input-watch; then /usr/bin/winlux-input-watch >/run/winlux/input-watch.log 2>&1 & fi
wait_bus_name session org.xfce.FileManager "$thunar_pid" /run/winlux/file-manager.log
# Name ownership precedes GTK/GVfs initialization. Une fois le nom pris, une
# demande d'ouverture envoyée par le bureau attend simplement dans la file de
# Thunar : la réponse complète n'est attendue avant le bureau que pour les
# tests automatiques, qui cliquent dès la première seconde.
thunar_ping() {
    /usr/bin/gdbus call --session --dest org.xfce.FileManager \
        --object-path /org/xfce/FileManager --method org.freedesktop.DBus.Peer.Ping \
        --timeout 90 >>/run/winlux/file-manager.log 2>&1
}
if grep -qw 'winlux.native-smoke=1' /proc/cmdline; then
    thunar_ping || { cat /run/winlux/file-manager.log >&2; exit 1; }
else
    thunar_ping </dev/null >/dev/null 2>&1 &
fi
# Raccourcis du Bureau approuvés en arrière-plan : seule leur ouverture
# en dépend, pas l'affichage du bureau.
/usr/bin/winlux-trust-shortcuts </dev/null >/run/winlux/trust-shortcuts.log 2>&1 &
# Programmes « au démarrage » : lancés après le bureau, comme sous Windows.
/usr/bin/winlux-session-startup >/run/winlux/startup-apps.log 2>&1 &
# Logiciels demandés depuis Bienvenue et interrompus par la fin de session.
if command -v winlux-store >/dev/null 2>&1; then
    winlux-store starter --resume >/run/winlux/starter.log 2>&1 &
fi
if test ! -f "$HOME/.config/winlux/welcome-complete" && test "${WINLUX_NO_WELCOME:-0}" != 1 && ! grep -qw 'winlux.native-smoke=1' /proc/cmdline; then
    # Bienvenue s'ouvre une fois le bureau dessiné : ouverte avant, sa
    # fenêtre restait parfois noire (cadre vide au centre de l'écran).
    (
        for attempt in $(seq 1 100); do
            /usr/bin/xdotool search --onlyvisible --classname xfdesktop >/dev/null 2>&1 && break
            sleep 0.2
        done
        exec /usr/bin/winlux-welcome
    ) </dev/null >/run/winlux/welcome.log 2>&1 &
fi
gpu_count=$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)
if test "$gpu_count" -gt 0; then
    /usr/bin/xscreensaver -no-splash -verbose >/run/winlux/screensaver.log 2>&1 &
else
    LIBGL_ALWAYS_SOFTWARE=1 /usr/bin/xscreensaver -no-splash -verbose >/run/winlux/screensaver.log 2>&1 &
fi
echo $! > /run/winlux/screensaver.pid
# Verrouillage après inactivité et au retour de veille (Paramètres > Confidentialité et sécurité).
if ! { test -f /run/winlux/lock-daemon.pid && kill -0 "$(cat /run/winlux/lock-daemon.pid)" 2>/dev/null; }; then
    /usr/bin/winlux-lock daemon >/run/winlux/lock.log 2>&1 &
    echo $! > /run/winlux/lock-daemon.pid
fi
