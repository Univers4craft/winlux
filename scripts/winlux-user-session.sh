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
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin:$PATH:/usr/games:/usr/local/games"
uid=$(id -u)
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$uid}
if ! mkdir -p "$XDG_RUNTIME_DIR" 2>/dev/null || ! test -w "$XDG_RUNTIME_DIR"; then
    XDG_RUNTIME_DIR=$(mktemp -d "/tmp/winlux-runtime-$uid.XXXXXX")
fi
chmod 700 "$XDG_RUNTIME_DIR"
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
export XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
# Liens web des programmes qui lisent $BROWSER (outils en ligne de commande,
# Python webbrowser) : le navigateur installé, comme xdg-open et les .exe.
export BROWSER="${BROWSER:-/usr/bin/winlux-browser}"
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

# Gestionnaire de fenêtres lancé tout de suite : il démarre pendant le son
# et le service de fichiers, attendu seulement avant la barre des tâches.
rc="$HOME/.config/openbox/rc.xml"
if test -f "$rc"; then
    /usr/bin/openbox --sm-disable --config-file "$rc" >"$logs/window-manager.log" 2>&1 &
else
    /usr/bin/openbox --sm-disable >"$logs/window-manager.log" 2>&1 &
fi
wm_pid=$!
pids="$pids $wm_pid"
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
    # Une ouverture demandée par le bureau avant que Thunar ait pris son nom
    # attend simplement sur le bus : le bureau ne l'attend donc pas. Seuls
    # les tests automatiques, qui cliquent dès la première seconde, attendent
    # sa réponse complète (Ping).
    thunar_ready() {
        wait_bus_name org.xfce.FileManager "$thunar_pid" && \
            /usr/bin/gdbus call --session --dest org.xfce.FileManager \
                --object-path /org/xfce/FileManager --method org.freedesktop.DBus.Peer.Ping \
                --timeout 90 >>"$logs/file-manager.log" 2>&1
    }
    if grep -qw 'winlux.native-smoke=1' /proc/cmdline 2>/dev/null; then
        thunar_ready || true
    else
        (thunar_ready </dev/null >/dev/null 2>&1 &)
    fi
fi
# Raccourcis du Bureau approuvés en arrière-plan : seule leur ouverture en
# dépend, pas l'affichage du bureau.
if test -x /usr/bin/winlux-trust-shortcuts; then (/usr/bin/winlux-trust-shortcuts </dev/null >"$logs/trust-shortcuts.log" 2>&1 &); fi
if test -x /usr/bin/xfsettingsd; then /usr/bin/xfsettingsd --daemon >"$logs/xfsettings.log" 2>&1 || true; fi
# Thème d'icônes Winlux par défaut, sans écraser un choix de l'utilisateur.
# Le défaut XFCE (elementary-xfce-dark) n'est pas livré : un thème configuré
# mais absent laissait les silhouettes noires d'Adwaita.
icon_theme=$(/usr/bin/xfconf-query -c xsettings -p /Net/IconThemeName 2>/dev/null || true)
if test -d /usr/share/icons/Winlux && test -x /usr/bin/xfconf-query && { test -z "$icon_theme" || ! test -d "/usr/share/icons/$icon_theme"; }; then
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
# Fond d'écran par écran, fixé avant la barre des tâches : l'identité Winlux, sinon le fond de l'image.
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
# Bureau de Windows 11 : la Corbeille seule (#30). Les autres icônes
# d'xfdesktop s'activent dans Paramètres ; un choix existant est gardé.
if test -x /usr/bin/xfconf-query; then
    for icon in trash:true filesystem:false home:false removable:false; do
        property="/desktop-icons/file-icons/show-${icon%%:*}"
        /usr/bin/xfconf-query -c xfce4-desktop -p "$property" >/dev/null 2>&1 ||
            /usr/bin/xfconf-query -c xfce4-desktop -p "$property" -n -t bool -s "${icon#*:}" || true
    done
fi
# Compositeur facultatif : sans lui, un déplacement laisse des traînées.
if test -x /usr/bin/picom && test "${WINLUX_COMPOSITOR:-1}" != 0 && test -f "$HOME/.config/picom/picom.conf"; then
    start compositor.log /usr/bin/picom --config "$HOME/.config/picom/picom.conf"
    compositor_pid=$!
    # Coins carrés des fenêtres agrandies : l'état est recopié sur le cadre
    # openbox, seul lu par les règles de picom (#582).
    if test -x /usr/bin/winlux-maximized-watch; then start maximized-watch.log /usr/bin/winlux-maximized-watch; fi
fi
# Icône du clavier tactile (tablette sans clavier) écrite AVANT le lancement
# de la barre : un xfce4-panel déjà lancé ne chargeait pas le greffon ajouté
# ensuite, et une tablette restait sans aucun moyen de taper.
if test -x /usr/bin/winlux-osk; then WINLUX_OSK_NO_RESTART=1 /usr/bin/winlux-osk panel >"$logs/osk-panel.log" 2>&1 || true; fi
# Tablettes et convertibles : l'écran suit l'accéléromètre (s'arrête aussitôt
# sans accéléromètre ou sur un PC qui n'est pas une tablette).
if test -x /usr/bin/winlux-rotation; then start rotation.log /usr/bin/winlux-rotation watch; fi
if test -x /usr/bin/xfce4-panel; then
    start panel.log /usr/bin/xfce4-panel --disable-wm-check
fi
# Clavier ou souris USB branché pendant la session : disposition (AZERTY…)
# et réglages réappliqués, sinon un clavier branché à chaud tape en QWERTY.
if test -x /usr/bin/winlux-input-watch; then
    start input-watch.log /usr/bin/winlux-input-watch
fi
if test -x /usr/bin/winlux-notifications; then
    start notifications.log /usr/bin/winlux-notifications
fi
# Préférences de l'utilisateur : alignement de la barre, souris, processeur,
# restaurées en arrière-plan pendant que le bureau s'affiche.
# Alignement de la barre : dans CHAQUE session, y compris celle d'un compte
# ouvert par « Changer d'utilisateur ». Sans cela, ce compte gardait la copie
# figée de xfce4-panel.xml (séparateur 3 étiré, contrepoids 13 au bout) : le
# bouton Démarrer tombait vers x≈838, ni à gauche ni au milieu.
if test "$primary" != 1 && test -x /usr/bin/winlux-control; then (
    export WINLUX_CONTROL_NO_STATUS=1
    alignment=$(sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
    case "$alignment" in left|center) /usr/bin/winlux-control "align-$alignment" ;; esac
    ) </dev/null >"$logs/session-settings.log" 2>&1 &
fi
if test "$primary" = 1 && test -x /usr/bin/winlux-control; then (
    # L'état est publié par la boucle de la session : pas de sondes ici.
    export WINLUX_CONTROL_NO_STATUS=1
    alignment=$(sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
    case "$alignment" in left|center) /usr/bin/winlux-control "align-$alignment" >/dev/null 2>&1 || true ;; esac
    speed=$(sed -n 's/^MOUSE_SPEED=//p' /etc/winlux/input.conf 2>/dev/null || true)
    case "$speed" in
        25) /usr/bin/winlux-control mouse-slow >/dev/null 2>&1 || true ;;
        50) /usr/bin/winlux-control mouse-normal >/dev/null 2>&1 || true ;;
        75) /usr/bin/winlux-control mouse-fast >/dev/null 2>&1 || true ;;
    esac
    if test -s /etc/winlux/cpu.conf; then /usr/bin/winlux-control cpu-restore >/dev/null 2>&1 || true; fi
    ) </dev/null >"$logs/session-settings.log" 2>&1 &
fi
if test "$primary" = 1 && test ! -f "$HOME/.config/winlux/welcome-complete" && test "${WINLUX_NO_WELCOME:-0}" != 1 && test -x /usr/bin/winlux-welcome; then
    # Bienvenue s'ouvre une fois le bureau dessiné : ouverte avant, sa
    # fenêtre restait parfois noire (cadre vide au centre de l'écran).
    (
        for attempt in $(seq 1 100); do
            /usr/bin/xdotool search --onlyvisible --classname xfdesktop >/dev/null 2>&1 && break
            sleep 0.2
        done
        exec /usr/bin/winlux-welcome
    ) </dev/null >"$logs/welcome.log" 2>&1 &
fi
# Bureau visible : xdotool trouve la fenêtre de xfdesktop (20 s au plus).
wait_desktop() {
    for attempt in $(seq 1 100); do
        /usr/bin/xdotool search --onlyvisible --classname xfdesktop >/dev/null 2>&1 && return 0
        sleep 0.2
    done
    return 0
}
# Démarrage décalé : ce qui n'est pas le bureau lui-même (agents de la zone
# de notification, presse-papiers, lecteurs réseau, programmes au démarrage,
# économiseur et verrouillage, catalogue, relevé d'état, suivi des écrans)
# attend que le bureau soit affiché, pour ne pas lui disputer le processeur.
# Ce sous-shell suit ses propres processus et les arrête avec la session.
(
    pids=
    bus_pid=
    trap cleanup EXIT
    trap 'exit 0' INT TERM HUP
    if test -x /usr/bin/xdotool; then wait_desktop; fi
    has_bluetooth=false
    if test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1; then has_bluetooth=true
    elif test -d /sys/class/bluetooth && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then has_bluetooth=true; fi
    if "$has_bluetooth" && test -x /usr/bin/blueman-applet; then start bluetooth-agent.log /usr/bin/blueman-applet; fi
    # Presse-papiers commun : copier dans un .exe, coller dans une appli Linux.
    if test -x /usr/bin/winlux; then start clipboard-bridge.log /usr/bin/winlux --clipboard-bridge; fi
    # Historique du presse-papiers (Windows + V).
    if test -x /usr/bin/winlux-clipboard; then start clipboard-history.log /usr/bin/winlux-clipboard daemon; fi
    # Lecteurs réseau : reconnexion au démarrage et surveillance hors ligne.
    if test -x /usr/bin/winlux-drives; then start netdrives.log /usr/bin/winlux-drives watch; fi
    if test -x /usr/bin/nm-applet; then start network-agent.log /usr/bin/nm-applet --indicator; fi
    # Téléphone connecté : seulement si un téléphone est associé à ce compte.
    if test -x /usr/bin/winlux-phone; then start phone.log /usr/bin/winlux-phone serve --if-paired; fi
    # Programmes « au démarrage » : lancés une fois le bureau affiché.
    if test -x /usr/bin/winlux-session-startup; then (/usr/bin/winlux-session-startup </dev/null >"$logs/startup-apps.log" 2>&1 &); fi
    # Logiciels demandés depuis Bienvenue et interrompus par la fin de session.
    if command -v winlux-store >/dev/null 2>&1; then
        (winlux-store starter --resume >"$logs/starter.log" 2>&1 &)
    fi
    if test -x /usr/bin/xscreensaver; then
        if test "$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)" -gt 0; then
            start screensaver.log /usr/bin/xscreensaver -no-splash
        else
            start screensaver.log env LIBGL_ALWAYS_SOFTWARE=1 /usr/bin/xscreensaver -no-splash
        fi
    fi
    # Horloge : alarmes, minuteurs et fin des sessions de concentration, même fenêtre fermée.
    if test -x /usr/bin/winlux-alarms; then start alarms.log /usr/bin/winlux-alarms daemon; fi
    # Éclairage nocturne : planning et couleurs de l'écran (rien sans rampe gamma).
    if test -x /usr/bin/winlux-ambiance; then start ambiance.log /usr/bin/winlux-ambiance daemon; fi
    # Ancrage à la souris : bords, coins, barre des dispositions, survol d'Agrandir (#460).
    if test -x /usr/bin/winlux-snap; then start snap-watch.log /usr/bin/winlux-snap watch; fi
    # Touche Windows seule : ouvre ou ferme Démarrer, sans gêner Windows+Gauche (#605).
    if test -x /usr/bin/winlux-super-key; then start super-key.log /usr/bin/winlux-super-key; fi
    # Verrouillage après inactivité et au retour de veille.
    if test -x /usr/bin/winlux-lock; then start lock.log /usr/bin/winlux-lock daemon; fi
    # Catalogue des applications (raccourcis .lnk, programmes installés) et
    # relevé d'état en basse priorité : ils ne passent jamais devant le bureau.
    status_nice=
    if test -x /usr/bin/nice; then status_nice='/usr/bin/nice -n 10'; fi
    # Catalogue résident : refait seulement quand un raccourci, le registre ou
    # un navigateur change, au lieu de relancer Python toutes les 5 s (#214).
    $status_nice /usr/bin/winlux-applications watch </dev/null >>"$logs/catalogue.log" 2>&1 &
    pids="$pids $!"
    (
        while :; do
            if test "$primary" = 1 && test -O /run/winlux/browser-chooser; then
                rm -f /run/winlux/browser-chooser
                /usr/bin/winlux-shell-app settings 11 >"$logs/browser-settings.log" 2>&1 &
            fi
            # Lien demandé sans navigateur : ouvert dès qu'un navigateur est installé.
            if test -f "/run/winlux/browser-pending-$(id -u)"; then /usr/bin/winlux-browser --pending >/dev/null 2>&1 & fi
            sleep 5
        done
    ) </dev/null >/dev/null 2>&1 &
    pids="$pids $!"
    if test "$primary" = 1; then
        # État publié pour les Paramètres et la barre des tâches.
        (
            while :; do
                WINLUX_CONTROL_PERIODIC=1 $status_nice /usr/bin/winlux-control status >/dev/null 2>&1 || true
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

    wait
) </dev/null >/dev/null 2>&1 &
pids="$pids $!"

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
# Le bureau (fond d'écran, icônes) est relancé s'il s'arrête ; la session
# vit tant que son gestionnaire de fenêtres vit.
while kill -0 "$wm_pid" 2>/dev/null; do
    if test "$primary" = 1; then
        keyboard_layout=$(sed -n 's/^XKB_LAYOUT=//p' /etc/winlux/input.conf 2>/dev/null || true)
        # Clavier et pavé tactile appliqués pendant que le bureau se dessine.
        (
            case "$keyboard_layout" in fr|us|de|es|gb|it|pt|be) /usr/bin/setxkbmap "$keyboard_layout" 2>/dev/null || true ;; esac
            if test -x /usr/bin/winlux-touchpad; then /usr/bin/winlux-touchpad apply 2>/dev/null || true; fi
            # Souris : précision, défilement, gaucher, double-clic, couleur, traînée.
            if test -x /usr/bin/winlux-mouse; then /usr/bin/winlux-mouse apply 2>/dev/null || true; fi
        ) </dev/null >/dev/null 2>&1 &
        ui_scale=$(/usr/bin/winlux-display session-scale 2>/dev/null || true)
        case "$ui_scale" in 1|1.25|1.5|1.75|2|2.5|3) ;; *) ui_scale=1 ;; esac
        WINLUX_THEME=$(sed -n 's/^THEME=//p' /etc/winlux/preferences.conf 2>/dev/null || true)
        export WINLUX_UI_SCALE="$ui_scale" WINLUX_THEME
        gtk_scale "$ui_scale"
    fi
    if test -x /usr/bin/xfdesktop; then
        /usr/bin/xfdesktop --disable-wm-check >>"$logs/desktop.log" 2>&1 &
        desktop_pid=$!
        echo "WINLUX_DESKTOP_READY pid=$desktop_pid user=$(id -un)"
        # picom garde des fenêtres noires quand le fond du bureau change sous
        # lui : remise à zéro (SIGUSR1) une fois la fenêtre de xfdesktop là.
        if test -n "${compositor_pid:-}" && test -x /usr/bin/wmctrl; then
            (
                for attempt in $(seq 1 240); do
                    /usr/bin/wmctrl -l -x 2>/dev/null | grep -q ' xfdesktop\.Xfdesktop ' && break
                    kill -0 "$desktop_pid" 2>/dev/null || exit 0
                    sleep 0.5
                done
                sleep 1
                kill -USR1 "$compositor_pid" 2>/dev/null || true
            ) </dev/null >/dev/null 2>&1 &
        fi
        while kill -0 "$desktop_pid" 2>/dev/null && kill -0 "$wm_pid" 2>/dev/null; do sleep 1; done
        kill "$desktop_pid" 2>/dev/null || true
        wait "$desktop_pid" 2>/dev/null || true
    else
        wait "$wm_pid" 2>/dev/null || true
    fi
    sleep 1
done
