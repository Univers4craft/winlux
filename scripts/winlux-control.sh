#!/bin/sh
# Pont entre le bureau Win32 et les services Linux. Les valeurs ecrites sont
# ASCII pour rester lisibles par les anciennes applications TextOutA.
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
# Le module d'alimentation du runtime suit la meme racine (tests).
if test -n "$root"; then export WINLUX_POWER_ROOT="$root"; fi
config="$root/etc/winlux"
test -d "$run" && test -d "$config" || mkdir -p "$run" "$config"
export DISPLAY=${DISPLAY:-:0}
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/winlux/session-bus}

# Première valeur de CLE= dans un fichier, lue sans lancer de programme
# (relevé toutes les 3 s). 3e argument : valeur mise à la place
# d'une clé présente mais vide (ancien « | sed 's/^$/…/' »).
value() {
    test -r "$1" || return 0
    while IFS= read -r _vl || test -n "$_vl"; do
        case $_vl in "$2="*) _vl=${_vl#"$2="}; printf '%s\n' "${_vl:-${3:-}}"; return 0 ;; esac
    done < "$1"
}
# Première ligne d'un fichier (valeurs de /run, /sys, /proc), sinon la
# valeur par défaut si le fichier manque ; sans lancer cat ni head.
first() {
    if test -r "$1"; then _fl=; IFS= read -r _fl < "$1" || true; printf '%s\n' "$_fl"
    else printf '%s\n' "${2-}"; fi
}
pref() { value "$config/preferences.conf" "$@"; }
overlay() { value "$config/overlay.conf" "$@"; }
# Les jeux lancés par l'utilisateur lisent ce fichier : il reste lisible.
save_overlay() { save "$config/overlay.conf" "$1" "$2" && chmod 644 "$config/overlay.conf"; }
cpu_pref() { value "$config/cpu.conf" "$1"; }
input() { value "$config/input.conf" "$@"; }
privacy() { v=$(value "$config/privacy.conf" "$1"); printf '%s\n' "${v:-$2}"; }
# Bureau sous le compte de l'utilisateur (système installé) : /etc reste à
# root, le relais winlux-root écrit les préférences et exécute les actions
# système. Jamais dans les tests (WINLUX_CONTROL_ROOT) ni en root.
relay_socket=${WINLUX_ROOT_SOCKET:-/run/winlux-root.sock}
relayed_session() { test -z "$root" && test "$(id -u)" != 0 && test -S "$relay_socket"; }
save() {
    file=$1 key=$2 new_value=$3
    if relayed_session && test "${file%/*}" = "$config"; then
        "${WINLUX_ROOT_TOOL:-winlux-root}" conf "${file##*/}" "$key" "$new_value"
        return
    fi
    tmp="$file.$$"
    if test -r "$file"; then sed "/^$key=/d" "$file" > "$tmp" || return; else : > "$tmp" || return; fi
    printf '%s=%s\n' "$key" "$new_value" >> "$tmp" || return
    mv "$tmp" "$file"
}
have() { command -v "$1" >/dev/null 2>&1; }
# Relance de la barre des tâches (xfce4-panel -r). Le panneau relancé
# abandonne (« There is already a running instance ») s'il trouve encore
# l'ancien sur le bus de session : sur une machine chargée, la barre
# disparaissait jusqu'à la session suivante (#848). On attend qu'un nouveau
# panneau tienne une seconde, sinon on en lance un.
restart_panel() {
    uid=$(id -u)
    old=" $(pgrep -u "$uid" -x xfce4-panel | tr '\n' ' ') "
    xfce4-panel -r >/dev/null 2>&1 || true
    last= stable=0
    for attempt in $(seq 1 50); do
        sleep 0.2
        alive=
        for pid in $(pgrep -u "$uid" -x xfce4-panel); do
            case "$old" in *" $pid "*) continue ;; esac
            case "$(ps -o stat= -p "$pid" 2>/dev/null)" in Z*|'') continue ;; esac
            alive=$pid
        done
        if test -n "$alive" && test "$alive" = "$last"; then stable=$((stable + 1)); else stable=0; fi
        last=$alive
        # Toujours là une seconde après : il a passé le contrôle d'instance.
        test "$stable" -ge 5 && return 0
    done
    setsid -f xfce4-panel --disable-wm-check </dev/null >/dev/null 2>&1 || true
}
trust_desktop() {
    chmod 755 "$1" || return
    if have gio; then gio set -t string "$1" metadata::trusted true 2>/dev/null || true; fi
}
# Messages, journal et verrou des actions : propres à chaque compte.
# /run/winlux est collant (1777) : un second compte (« Changer
# d'utilisateur ») ne pouvait ni remplacer action.txt du premier (mv :
# « Opération non permise ») ni écrire son action.log, et winlux-control
# s'arrêtait avant d'agir (barre jamais centrée chez lui).
uid=$(id -u)
if { test -z "$root" || test "${WINLUX_CONTROL_PER_ACCOUNT:-0}" = 1; } && test "$uid" != 0; then
    mine="$run/control-$uid"
    if ! test -d "$mine"; then
        mkdir -p "$mine"
        chmod 755 "$mine" 2>/dev/null || true
    fi
else
    mine=$run
fi
message() {
    action_tmp=$(mktemp "$mine/action.txt.XXXXXX" 2>/dev/null) || return 0
    printf '%s\n' "$*" > "$action_tmp" && chmod 644 "$action_tmp" && mv "$action_tmp" "$mine/action.txt" && return 0
    rm -f "$action_tmp" 2>/dev/null || true
}
# Fichier temporaire à nom imprévisible (#644) : un compte ne peut ni le
# créer d'avance dans /run/winlux (collant) ni le faire publier à sa place.
fresh() { mktemp "$1.XXXXXX"; }
# Fichier d'état partagé (relevé de l'init, en root) : un compte qui ne peut
# pas le remplacer laisse la version de l'init, sans faire échouer l'action.
publish() { chmod 644 "$1" 2>/dev/null; mv "$1" "$2" 2>/dev/null || { rm -f "$1" 2>/dev/null; true; }; }
# Relevé périodique (boucle de l'init, toutes les 3 s) : les sondes lentes
# dont la réponse change rarement (deux sont en Python) sont reprises du
# relevé de moins d'une minute. Toute action de winlux-control, dont chaque
# réglage changé dans Paramètres, refait un relevé complet.
nl='
'
fresh_cache=unset
cached() {
    key=$1; shift
    file="$mine/status-cache/$key"
    if test "${WINLUX_CONTROL_PERIODIC:-0}" = 1 && test -f "$file"; then
        # Un seul find par relevé pour toutes les sondes en cache.
        if test "$fresh_cache" = unset; then
            fresh_cache=$(find "$mine/status-cache" -maxdepth 1 -type f -mmin -1 2>/dev/null || true)
        fi
        case "$nl$fresh_cache$nl" in
            *"$nl$file$nl"*)
                while IFS= read -r _cl || test -n "$_cl"; do printf '%s\n' "$_cl"; done < "$file"
                return 0 ;;
        esac
    fi
    # Session du compte : /run/winlux peut ne pas lui être inscriptible,
    # la valeur est alors calculée à chaque relevé, sans cache.
    cache_tmp=
    if mkdir -p "$mine/status-cache" 2>/dev/null && cache_tmp=$(mktemp "$file.XXXXXX" 2>/dev/null) \
        && "$@" >"$cache_tmp" 2>/dev/null; then
        chmod 644 "$cache_tmp" 2>/dev/null
        mv "$cache_tmp" "$file" && cat "$file" && return 0
    fi
    test -z "$cache_tmp" || rm -f "$cache_tmp" 2>/dev/null || true
    "$@" 2>/dev/null || true
}
status() {
    control_tmp=$(fresh "$run/control.txt") || return
    # Calculé ici, hors des $(…) : un sous-shell perdrait la valeur.
    if test "${WINLUX_CONTROL_PERIODIC:-0}" = 1; then
        fresh_cache=$(find "$mine/status-cache" -maxdepth 1 -type f -mmin -1 2>/dev/null || true)
    fi
    {
        printf 'version=%s\n' "$(first "$root/usr/share/winlux/version" developpement)"
        if test -r /proc/sys/kernel/osrelease; then printf 'kernel=%s\n' "$(first /proc/sys/kernel/osrelease)"
        else printf 'kernel=%s\n' "$(uname -r)"; fi
        persistent=0
        if test -r "$root/proc/mounts"; then
            while read -r _dev _mnt _fs _rest; do
                test "$_mnt" = / || continue
                case $_fs in ext2|ext3|ext4|btrfs|xfs|f2fs) persistent=1 ;; esac
            done < "$root/proc/mounts"
        fi
        printf 'persistent=%s\n' "$persistent"
        printf 'layout=%s\n' "$(input XKB_LAYOUT fr)"
        printf 'tap=%s\n' "$(input TAP_TO_CLICK 0)"
        printf 'mouse_speed=%s\n' "$(input MOUSE_SPEED 50)"
        printf 'align=%s\n' "$(pref TASKBAR_ALIGN)"
        printf 'theme=%s\n' "$(pref THEME)"
        printf 'lang=%s\n' "$(value "$config/locale.conf" LANG fr)"
        # Le fuseau réel : /etc/timezone, sinon la cible de /etc/localtime,
        # sinon UTC (jamais « Paris » affiché pour un système resté en UTC).
        if test -r /etc/timezone; then tz=$(first /etc/timezone); else tz=$({ readlink /etc/localtime 2>/dev/null | sed -n 's#^.*/zoneinfo/##p' | grep . ; } || printf 'UTC'); fi; printf 'timezone=%s\n' "$tz"
        printf 'network='; if have ip; then
            # Champs lus par le shell, 1re ligne seulement (5e de la route,
            # 4e de l'adresse), comme l'ancien awk.
            route=$(ip -4 route show default 2>/dev/null) || route=
            set -f
            # shellcheck disable=SC2086
            set -- ${route%%"$nl"*}
            set +f
            iface=${5:-}
            if test -n "$iface"; then
                address=$(ip -4 -o addr show dev "$iface" 2>/dev/null) || address=
                set -f
                # shellcheck disable=SC2086
                set -- ${address%%"$nl"*}
                set +f
                addr=${4:-}
                printf '%s %s\n' "$iface" "$addr"
                printf 'network_connected=1\n'
            else printf 'Hors ligne - aucune route par defaut\nnetwork_connected=0\n'; fi
        else printf 'Outil ip indisponible\nnetwork_connected=0\n'; fi
        printf 'audio='
        if have wpctl; then
            volume=$(wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null) || volume=
            printf '%s\n' "${volume:-Aucun peripherique audio}"
        elif have amixer; then
            amixer get Master 2>/dev/null | awk -F '[][]' '/%/ { print $2 " " $4; found=1; exit } END { if (!found) print "Aucun controle Master" }'
        else printf 'Audio non disponible (aucun mixeur installe)\n'; fi
        # Aucune valeur de batterie inventee : en VM, CA signifie que
        # l'hyperviseur ne presente pas de batterie a l'invite.
        present=0
        for bat in "$root"/sys/class/power_supply/BAT*; do
            test -r "$bat/capacity" || continue
            capacity=$(first "$bat/capacity")
            case "$capacity" in ''|*[!0-9]*) continue ;; esac
            test "$capacity" -le 100 || continue
            printf 'battery=%s%% %s\nbattery_level=%s\n' "$capacity" "$(first "$bat/status")" "$capacity"
            present=1; break
        done
        if test "$present" = 0; then printf 'battery=Alimentation CA - aucune batterie exposee\n'; fi
        printf 'battery_present=%s\n' "$present"
        # Etat detaille, modes et delais : module d'alimentation du runtime.
        if have winlux; then winlux --power status 2>/dev/null || true; fi
        devices=0 pads=0
        if test -r "$root/proc/bus/input/devices"; then
            while IFS= read -r _il; do
                case $_il in
                    'N: Name='*) devices=$((devices + 1)) ;;
                    'H: Handlers='*' js'[0-9]*) pads=$((pads + 1)) ;;
                esac
            done < "$root/proc/bus/input/devices"
        fi
        printf 'input_devices=%s\n' "$devices"
        # Périphériques des programmes Windows : manettes (XInput/DirectInput),
        # webcams (Media Foundation) et micro (waveIn/WASAPI).
        printf 'gamepads=%s\n' "$pads"
        cams=0
        for cam in "$root"/sys/class/video4linux/video*; do
            test -r "$cam/name" || continue
            # Un seul nœud de capture par caméra UVC (index 0).
            test "$(first "$cam/index" 0)" = 0 || continue
            cams=$((cams + 1))
        done
        printf 'cameras=%s\n' "$cams"
        printf 'microphone='
        if have wpctl && mic=$(wpctl inspect @DEFAULT_AUDIO_SOURCE@ 2>/dev/null | sed -n '/.*node.description = "\(.*\)".*/{s//\1/p;q;}') && test -n "$mic"; then
            printf '%s\n' "$mic"
        elif test -r "$root/proc/asound/pcm" && grep -q 'capture' "$root/proc/asound/pcm"; then
            grep 'capture' "$root/proc/asound/pcm" | head -n 1 | cut -d: -f3 | sed 's/^ *//'
        else printf 'Aucun micro detecte\n'; fi
        bluetooth="$run/bluetooth.txt"
        # Sans adaptateur, l'état « absent » déjà publié reste vrai : le relevé
        # périodique ne relance pas winlux-bluetooth pour rien.
        adapter=0
        for _bt in "$root"/sys/class/bluetooth/*; do test -e "$_bt" && adapter=1; break; done
        if have winlux-bluetooth && { test "$adapter" = 1 || test "${WINLUX_CONTROL_PERIODIC:-0}" != 1 \
                || test "$(value "$bluetooth" present)" != 0; }; then
            winlux-bluetooth status 2>/dev/null || true
        fi
        printf 'bluetooth_present=%s\n' "$(value "$bluetooth" present 0)"
        printf 'bluetooth_powered=%s\n' "$(value "$bluetooth" powered 0)"
        printf 'bluetooth_connected=%s\n' "$(value "$bluetooth" connected 0)"
        # Réglage changé seulement par une action (qui refait un relevé complet).
        auto=$(cached update-auto winlux-update --auto status); printf 'update_auto=%s\n' "${auto:-indisponible}"
        if test -f "$root/var/lib/winlux/pending-reboot"; then
            printf 'update_action=reboot\n'
        else
            printf 'update_action=%s\n' "$(first "$run/update-action")"
        fi
        # Overlay de performances (Paramètres > Graphismes) ; vide = défaut.
        setting=$(overlay OVERLAY_ENABLED); printf 'overlay_enabled=%s\n' "${setting:-0}"
        printf 'overlay_hotkey=%s\n' "$(overlay OVERLAY_HOTKEY)"
        printf 'overlay_position=%s\n' "$(overlay OVERLAY_POSITION)"
        setting=$(overlay OVERLAY_AUTOSHOW); printf 'overlay_autoshow=%s\n' "${setting:-0}"
        for metric in fps frametime cpu gpu; do
            case $metric in fps) key=FPS ;; frametime) key=FRAMETIME ;; cpu) key=CPU ;; *) key=GPU ;; esac
            setting=$(overlay "OVERLAY_$key")
            printf 'overlay_%s=%s\n' "$metric" "${setting:-1}"
        done
        gpu_count=$(first "$root/run/winlux/hw/gpu-count" 0)
        printf 'gpu_count=%s\n' "$gpu_count"
        printf 'virtual_machine=%s\n' "$(first "$root/run/winlux/hw/virtual-machine")"
        test "$(first "$root/run/winlux/hw/has-nvidia")" = 1 && printf 'gpu_nvidia=%s\n' "$(first "$root/run/winlux/hw/nvidia-driver" nouveau)"
        test "$(first "$root/run/winlux/hw/has-amd")" = 1 && printf 'gpu_amd=%s\n' "$(first "$root/run/winlux/hw/amd-driver" amdgpu)"
        test "$(first "$root/run/winlux/hw/has-intel")" = 1 && printf 'gpu_intel=i915\n'
        gpu_mode=$(first "$root/run/winlux/hw/gpu-mode")
        test -n "$gpu_mode" && printf 'gpu_mode=%s\n' "$gpu_mode"
        printf 'cpu_sockets=%s\n' "$(first "$root/run/winlux/hw/cpu-sockets" 1)"
        printf 'cpu_threads=%s\n' "$(first "$root/run/winlux/hw/cpu-threads" 1)"
        printf 'wifi=%s\n' "$(first "$root/run/winlux/hw/has-wifi" 0)"
        printf 'touchpad=%s\n' "$(first "$root/run/winlux/hw/has-touchpad" 0)"
        # Réglages réellement acceptés par le pavé (rien sans pavé tactile).
        if have winlux-touchpad; then cached touchpad winlux-touchpad status; fi
        printf 'webcam=%s\n' "$(first "$root/run/winlux/hw/has-webcam" 0)"
        welcome=$(pref WELCOME_DONE); printf 'welcome_done=%s\n' "${welcome:-0}"
        welcome=$(pref WELCOME_STEP); printf 'welcome_step=%s\n' "${welcome:-0}"
        # Vie privee : rien n'est envoye par defaut, la localisation est coupee.
        printf 'privacy_telemetry=%s\n' "$(privacy TELEMETRY off)"
        printf 'privacy_microphone=%s\n' "$(privacy MICROPHONE allow)"
        printf 'privacy_camera=%s\n' "$(privacy CAMERA allow)"
        printf 'privacy_location=%s\n' "$(privacy LOCATION deny)"
        printf 'privacy_location_history=%s\n' "$(privacy LOCATION_HISTORY off)"
        # Verrouillage de l'ecran (delai, sortie de veille, compte protege).
        if have winlux-lock; then cached lock env WINLUX_LOCK_ROOT="$root" winlux-lock status; fi
        # Signaler un probleme : reglages lus par winlux-report (envoi automatique
        # des plantages seulement si AUTO=on, ou jeton personnel avec AUTO=default).
        if have winlux-report; then cached report env WINLUX_REPORT_CONF="$config/report.conf" winlux-report --status; fi
        # Mises à jour du système (apt) : lentes à interroger, changent rarement.
        if have winlux-sysupdate; then cached sysupdate winlux-sysupdate status; fi
        printf 'control_available=1\n'
        if have winlux-browsers; then cached browsers winlux-browsers --status; fi
        if have winlux-display; then winlux-display status 2>/dev/null || true; fi
        if winlux-update --busy 2>/dev/null; then printf 'update_busy=1\n'; else printf 'update_busy=0\n'; fi
    } > "$control_tmp"
    publish "$control_tmp" "$run/control.txt"
    cpu_status
    drivers_status
    # --list lit seulement le manifeste deja en cache, sans acces reseau.
    # Refait seulement si l'une de ses sources a change depuis (les dossiers
    # d'etat comptent : un fichier supprime change leur date).
    if have winlux-update && ! updates_current; then
        updates_tmp=$(fresh "$run/updates.txt") || return 0
        if winlux-update --list > "$updates_tmp" 2>/dev/null; then
            publish "$updates_tmp" "$run/updates.txt"
        else rm -f "$updates_tmp"; fi
        # Rien à décrire sans rapport (moteur absent ou muet) : pas de second programme.
        if test -s "$run/updates.txt"; then update_state; fi
    fi
}
# État de la page Mises à jour (clé=valeur, #875-#882) : refait avec updates.txt.
update_state() {
    have winlux-update || return 0
    state_tmp=$(fresh "$run/update-state.txt") || return 0
    # Ancien moteur sans --state : un état minimal, pour ne pas relancer
    # --list à chaque relevé (le fichier manquant le redemanderait).
    winlux-update --state > "$state_tmp" 2>/dev/null || printf 'backend=legacy\n' > "$state_tmp"
    publish "$state_tmp" "$run/update-state.txt"
}
updates_current() {
    test -s "$run/updates.txt" || return 1
    test -s "$run/update-state.txt" || ! have winlux-update || return 1
    for source in "$root/usr/share/winlux/version" "$root/var/lib/winlux" "$root/var/cache/winlux" \
        "$root/var/lib/winlux/available-manifest" "$root/var/lib/winlux/update-trial"; do
        test -e "$source" || continue
        test "$source" -nt "$run/updates.txt" && return 1
    done
    return 0
}
# Capacites processeur detectees par le runtime : chaque section des
# Parametres n'apparait que si son cap_*=1 (garde-fous materiels).
cpu_status() {
    have winlux || { rm -f "$run/cpu.txt"; return 0; }
    cpu_tmp=$(fresh "$run/cpu.txt") || return 0
    # Capacités et temps de démarrage ne changent pas pendant la session.
    cached cpu-report winlux --cpu-report "${root:-/}" > "$cpu_tmp"
    if test -s "$cpu_tmp"; then
        hybrid=$(cpu_pref HYBRID_POLICY) fan=$(cpu_pref FAN_CURVE)
        printf 'hybrid_policy=%s\nfan_curve=%s\n' "${hybrid:-auto}" "${fan:-auto}" >> "$cpu_tmp"
        # Temps du dernier démarrage (repères notés par l'init).
        cached boot-time winlux --boot-time "${root:-/}" >> "$cpu_tmp"
        # Mode jeu : réglage, capacités réelles et partie en cours.
        winlux --gamemode status "${root:-/}" >> "$cpu_tmp" 2>/dev/null || true
        publish "$cpu_tmp" "$run/cpu.txt"
    else rm -f "$cpu_tmp"; fi
}
# Pilotes du matériel réel (Paramètres > Matériel et pilotes, Graphismes,
# Processeur) : relevé sysfs de winlux-drivers, repris s'il a moins d'une minute.
drivers_status() {
    have winlux-drivers || { rm -f "$run/drivers.txt"; return 0; }
    drivers_tmp=$(fresh "$run/drivers.txt") || return 0
    if cached drivers env WINLUX_DRIVERS_ROOT="${root:-/}" winlux-drivers status > "$drivers_tmp" 2>/dev/null; then
        publish "$drivers_tmp" "$run/drivers.txt"
    else rm -f "$drivers_tmp"; fi
}
fan_daemon() {
    pid=$(cat "$run/fan-daemon.pid" 2>/dev/null || true)
    case "$pid" in ''|*[!0-9]*) ;; *) if kill -0 "$pid" 2>/dev/null; then return 0; fi ;; esac
    winlux --fan-daemon "${root:-/}" >> "$run/fan.log" 2>&1 &
    printf '%s\n' "$!" > "$run/fan-daemon.pid"
}
apply_tap() {
    have xinput || { echo 'xinput indisponible : reglage du pave tactile impossible' >&2; return 1; }
    found=0
    for id in $(xinput --list --id-only); do
        if xinput list-props "$id" 2>/dev/null | grep -q 'libinput Tapping Enabled ('; then
            xinput set-prop "$id" 'libinput Tapping Enabled' "$1" || return
            found=1
        fi
    done
    test "$found" = 1 || { echo 'Aucun pave tactile compatible expose par le materiel virtuel' >&2; return 1; }
}
case "${1:-status}" in
    status) status; exit 0 ;;
esac
# Action executee en root par le relais winlux-root pour le bureau : ni
# verrou, ni message, ni etat (le bureau s'en charge de son cote).
relayed=0
if test "${WINLUX_CONTROL_RELAYED:-}" = 1 && test "$(id -u)" = 0; then relayed=1; umask 022; fi
if test "$relayed" = 0; then
# Une seule action a la fois. Le bureau reste reactif pendant les appels
# reseau et voit le journal quand la commande detachee se termine.
if ! mkdir "$mine/control.lock" 2>/dev/null; then message 'Une opération est déjà en cours. Consultez le journal.'; exit 1; fi
trap 'rmdir "$mine/control.lock" 2>/dev/null || true' EXIT HUP INT TERM
fi
action=$1
# Alias accepte par les menus XFCE (whiskermenu) : « shutdown » = poweroff.
case "$action" in shutdown) action=poweroff ;; esac
test "$relayed" = 1 || message 'Opération en cours…'
# Actions du bureau lui-meme (X, son, dossier personnel) : sous le compte.
# Les autres touchent au systeme et passent par le relais root.
local_action() {
    case "$action" in
        browser-launch-*|keyboard-*|lang-*|locale-*|tap-*|mouse-*|align-*|desktop-icon-*|theme-*|welcome-*) return 0 ;;
        privacy-*|lock-*|overlay-*|game-mode-*|cpu-hybrid-*|volume-*|report-open|restart-session|poweroff|reboot) return 0 ;;
        # Réglages du compte (#1100) : en root, HOME=/root exportait ceux de root.
        transfer-*) return 0 ;;
    esac
    return 1
}
perform() {
    case "$action" in
        browser-install-brave|browser-install-chrome|browser-install-edge|browser-install-firefox|browser-install-opera|browser-install-operagx|browser-install-tor|browser-install-vivaldi) winlux-browsers --install "${action#browser-install-}" ;;
        browser-default-brave|browser-default-chrome|browser-default-edge|browser-default-firefox|browser-default-opera|browser-default-operagx|browser-default-tor|browser-default-vivaldi) winlux-browsers --default "${action#browser-default-}" ;;
        browser-launch-brave|browser-launch-chrome|browser-launch-edge|browser-launch-firefox|browser-launch-opera|browser-launch-operagx|browser-launch-tor|browser-launch-vivaldi) winlux-browsers --launch "${action#browser-launch-}" >"$run/browser.log" 2>&1 & ;;
        # Boutiques de logiciels (docs/BOUTIQUES.md) : 75 = reporté faute de réseau
        # (Paramètres l'explique, l'init reprend au démarrage suivant).
        store-catalog-add-*|store-catalog-remove-*)
            id=${action#store-catalog-add-}; verb=--add
            case "$action" in store-catalog-remove-*) id=${action#store-catalog-remove-}; verb=--remove ;; esac
            case "$id" in ''|*[!a-z0-9-]*) message 'Boutique inconnue'; exit 2 ;; esac
            code=0; winlux-store-catalogs "$verb" "$id" || code=$?
            return "$code" ;;
        network-renew) winlux-network restart ;;
        network-scan)
            # Write available networks to a file the desktop shell can read.
            out="$run/networks.txt"
            networks_tmp=$(fresh "$out") || return
            if command -v nmcli >/dev/null 2>&1 && nmcli -t -f RUNNING general 2>/dev/null | grep -qx running; then
                nmcli -t -f SSID,SIGNAL,SECURITY,IN-USE device wifi list --rescan yes 2>/dev/null | head -n 20 > "$networks_tmp" || true
            elif command -v iwlist >/dev/null 2>&1; then
                iface=$(ip -4 route show default 2>/dev/null | awk 'NR==1{print $5}')
                test -n "$iface" || iface=$(ls /sys/class/net/ 2>/dev/null | grep -v '^lo$' | head -n 1)
                iwlist "$iface" scan 2>/dev/null | awk '
                    /Cell / { if (ssid!="") print ssid ":" signal ":" enc ":" use; ssid=""; signal="0"; enc=""; use="" }
                    /ESSID:/ { gsub(/.*ESSID:"|"$/,""); ssid=$0 }
                    /Signal level=/ { gsub(/.*Signal level=/,""); gsub(/ .*/,""); signal=int(($0+100)*2); if(signal>100) signal=100 }
                    /Encryption key:on/ { enc="WPA" }
                    END { if (ssid!="") print ssid ":" signal ":" enc ":" use }
                ' | head -n 20 > "$networks_tmp" || true
            else
                printf '' > "$networks_tmp"
            fi
            # Active connection info
            {
                iface=$(ip -4 route show default 2>/dev/null | awk 'NR==1{print $5}')
                if test -n "$iface"; then
                    addr=$(ip -4 -o addr show dev "$iface" 2>/dev/null | awk 'NR==1{print $4}')
                    is_wifi=0; test ! -d "/sys/class/net/$iface/wireless" || is_wifi=1
                    ssid=""
                    if test "$is_wifi" = 1 && command -v nmcli >/dev/null 2>&1; then
                        ssid=$(nmcli -t -f GENERAL.CONNECTION device show "$iface" 2>/dev/null | sed 's/^GENERAL.CONNECTION://')
                    fi
                    printf 'connected=1\niface=%s\naddr=%s\nwifi=%s\nssid=%s\n' "$iface" "$addr" "$is_wifi" "$ssid"
                else
                    printf 'connected=0\niface=\naddr=\nwifi=0\nssid=\n'
                fi
            } >> "$networks_tmp"
            publish "$networks_tmp" "$out" ;;
        network-connect)
            ssid=${2:-}
            test -n "$ssid" || { echo 'SSID manquant' >&2; return 1; }
            password=${3:-}
            if command -v nmcli >/dev/null 2>&1; then
                if test -n "$password"; then
                    nmcli device wifi connect "$ssid" password "$password" || return
                else
                    nmcli device wifi connect "$ssid" || return
                fi
            else
                echo 'NetworkManager requis pour la connexion Wi-Fi' >&2; return 1
            fi ;;
        network-disconnect)
            if command -v nmcli >/dev/null 2>&1; then
                iface=$(ip -4 route show default 2>/dev/null | awk 'NR==1{print $5}')
                test -n "$iface" && nmcli device disconnect "$iface" || true
            fi ;;
        bluetooth-on|bluetooth-off|bluetooth-scan) winlux-bluetooth "${action#bluetooth-}" ;;
        bluetooth-pair|bluetooth-connect|bluetooth-disconnect|bluetooth-remove)
            winlux-bluetooth "${action#bluetooth-}" "${2:-}" ;;
        volume-up|volume-down|volume-mute|volume-set)
            # volume-set N : niveau absolu en pourcentage (curseur du bureau).
            level="${2:-}"
            if test "$action" = volume-set && test -z "$level"; then echo 'volume-set : niveau manquant' >&2; return 0; fi
            case "$level" in *[!0-9]*) level=50 ;; esac
            test "$level" -le 100 || level=100
            if have wpctl; then
                case "$action" in volume-up) wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+ ;; volume-down) wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- ;; volume-set) wpctl set-volume @DEFAULT_AUDIO_SINK@ "$level%" && wpctl set-mute @DEFAULT_AUDIO_SINK@ 0 ;; *) wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle ;; esac
            elif have amixer; then
                case "$action" in volume-up) amixer set Master 5%+ ;; volume-down) amixer set Master 5%- ;; volume-set) amixer set Master "$level%" unmute ;; *) amixer set Master toggle ;; esac
            else echo 'Aucun mixeur audio installe.' >&2; return 1; fi ;;
        keyboard-fr|keyboard-us|keyboard-de|keyboard-es|keyboard-gb|keyboard-it|keyboard-pt|keyboard-be)
            layout=${action#keyboard-}
            have setxkbmap || { echo 'setxkbmap indisponible' >&2; return 1; }
            setxkbmap -layout "$layout" || return
            save "$config/input.conf" XKB_LAYOUT "$layout" ;;
        lang-fr|lang-en|lang-es|lang-de|locale-fr|locale-en|locale-es|locale-de)
            lang=${action#*-}
            save "$config/locale.conf" LANG "$lang" ;;
        tz-*|timezone-*)
            tz=${action#tz-}; tz=${tz#timezone-}
            case "$tz" in *[!A-Za-z0-9/_+-]*) echo "Fuseau invalide : $tz" >&2; return 1 ;; esac
            test -f "/usr/share/zoneinfo/$tz" || { echo "Fuseau inconnu : $tz" >&2; return 1; }
            # timedatectl a besoin de systemd, absent de Winlux.
            if have timedatectl && test -d /run/systemd/system; then
                timedatectl set-timezone "$tz" || return
            else
                ln -sf "/usr/share/zoneinfo/$tz" /etc/localtime || return
                printf '%s\n' "$tz" > /etc/timezone || return
                chmod 644 /etc/timezone 2>/dev/null || true
            fi
            save "$config/locale.conf" TIMEZONE "$tz" ;;
        touchpad-set) winlux-touchpad set "${2:-}" "${3:-}" ;;
        tap-on|tap-off)
            tap=0; test "$action" != tap-on || tap=1
            apply_tap "$tap" || return
            save "$config/input.conf" TAP_TO_CLICK "$tap" ;;
        mouse-slow|mouse-normal|mouse-fast)
            case "$action" in mouse-slow) speed=25; acceleration=-0.5 ;; mouse-normal) speed=50; acceleration=0 ;; mouse-fast) speed=75; acceleration=0.5 ;; esac
            have xinput || { echo 'xinput indisponible' >&2; return 1; }
            found=0
            for id in $(xinput --list --id-only); do
                if xinput list-props "$id" 2>/dev/null | grep -q 'libinput Accel Speed ('; then
                    xinput set-prop "$id" 'libinput Accel Speed' "$acceleration" || return
                    found=1
                fi
            done
            test "$found" = 1 || { echo 'Aucune souris avec acceleration libinput exposee' >&2; return 1; }
            save "$config/input.conf" MOUSE_SPEED "$speed" ;;
        align-left|align-center)
            alignment=${action#align-}
            if have xfconf-query; then
                # XFConf conserve les propriétés en mémoire : modifier le XML
                # derrière son dos ne change pas le panneau en cours.
                for spec in 'plugin-8:string:separator' 'plugin-8/style:uint:0' 'plugin-8/expand:bool:true' 'plugin-9:string:genmon' \
                    'plugin-9/command:string:/usr/bin/winlux-tray notifications' 'plugin-9/use-label:bool:false' \
                    'plugin-9/update-period:int:5000' 'plugin-13:string:genmon' 'plugin-13/command:string:/usr/bin/winlux-tray balance' \
                    'plugin-13/use-label:bool:false' 'plugin-13/update-period:int:2000'; do
                    property=${spec%%:*}; rest=${spec#*:}; kind=${rest%%:*}; setting=${rest#*:}
                    if ! xfconf-query -c xfce4-panel -p "/plugins/$property" >/dev/null 2>&1; then
                        xfconf-query -c xfce4-panel -p "/plugins/$property" -n -t "$kind" -s "$setting" || return
                    fi
                done
                # Garder la zone de notification des programmes Windows (#154).
                tray=
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-10 2>/dev/null)" = systray; then tray='-t int -s 10'; fi
                # Valeurs déjà en place (cas de chaque ouverture de session) :
                # rien n'est réécrit, la barre ne reconstruit pas ses greffons.
                # Bouton Widgets (winlux-widgets set button on) : à gauche de
                # la barre centrée, près des icônes de la barre à gauche.
                widgets=
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-11 2>/dev/null)" = genmon; then widgets=11; fi
                # Bouton de l'assistant IA (#286), présent seulement une fois activé.
                assistant=
                assistant=$(sed -n 's/^panel_plugin=//p' "${XDG_CONFIG_HOME:-$HOME/.config}/winlux/assistant.conf" 2>/dev/null || true)
                case "$assistant" in
                    ''|*[!0-9]*) assistant= ;;
                    *) test "$(xfconf-query -c xfce4-panel -p "/plugins/plugin-$assistant" 2>/dev/null)" = launcher || assistant= ;;
                esac
                # Icône du clavier tactile (winlux-osk panel), avant le réseau.
                keyboard=
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-12 2>/dev/null)" = genmon; then keyboard=12; fi
                # Boutons Rechercher et Vue des tâches (#459), juste après
                # Démarrer comme dans Windows 11 : numéros libres de 14 à 29.
                # La recherche est un genmon `winlux-tray search` depuis #703
                # (zone « Rechercher » ou loupe), un lanceur avant.
                search= taskview=
                for number in $(seq 14 29); do
                    case "$(xfconf-query -c xfce4-panel -p "/plugins/plugin-$number/items" 2>/dev/null)" in
                        *winlux-search.desktop*) search=$number ;;
                        *winlux-taskview.desktop*) taskview=$number ;;
                    esac
                    # La barre recopie souvent le lanceur sous un nom numéroté
                    # (launcher-15/17941231871.desktop) : Vue des tâches et
                    # Rechercher se reconnaissent alors à leur fichier source (#885).
                    for launcher in "${XDG_CONFIG_HOME:-$HOME/.config}/xfce4/panel/launcher-$number"/*.desktop; do
                        test -f "$launcher" || continue
                        case "$(sed -n 's/^\(X-XFCE-Source\|Exec\)=//p' "$launcher" 2>/dev/null)" in
                            *winlux-taskview*) taskview=$number ;;
                            *winlux-search*) search=$number ;;
                        esac
                    done
                    case "$(xfconf-query -c xfce4-panel -p "/plugins/plugin-$number/command" 2>/dev/null)" in
                        *'winlux-tray search'*) search=$number ;;
                    esac
                done
                # Personnalisation > Barre des tâches (#703) : recherche masquée,
                # barre classique ou flottante.
                taskbar_conf=${XDG_CONFIG_HOME:-$HOME/.config}/winlux/taskbar.conf
                if test "$(sed -n 's/^SEARCH=//p' "$taskbar_conf" 2>/dev/null | head -n 1)" = hidden; then search=; fi
                floating=0
                if test "$(sed -n 's/^STYLE=//p' "$taskbar_conf" 2>/dev/null | head -n 1)" = floating; then floating=1; fi
                buttons="${search:+$search }${taskview:+$taskview }"
                # Bloc de droite (#703) : réseau, son et batterie réunis dans le
                # genmon 4 ; 5 et 6 ne restent que sur un compte pas encore
                # migré. L'heure puis la cloche, comme Windows 11.
                status=4
                for number in 5 6; do
                    if test "$(xfconf-query -c xfce4-panel -p "/plugins/plugin-$number" 2>/dev/null)" = genmon; then status="$status $number"; fi
                done
                right="$status 7 9"
                # Greffon 13 : contrepoids de largeur égale au bloc de droite
                # (winlux-tray balance), sans quoi le groupe centré tombe trop
                # à gauche de la moitié de ce bloc. Barre à gauche : il reste
                # à la même place, large d'un pixel, pour ne pas relancer la
                # barre (#920).
                status_ids=
                if test "$floating" = 1; then
                    # Barre flottante (#889), maquette 5-B : deux capsules. Au
                    # centre (ou à gauche) Démarrer, Rechercher, Vue des tâches
                    # et les fenêtres ; en bas à droite, un second panneau avec
                    # la zone de notification, l'état, l'heure et la cloche ; pas
                    # de contrepoids 13. Les greffons sont posés sur toute la
                    # largeur du panneau, marge transparente comprise : aux
                    # deux bouts, un séparateur (3, 8, 40, 41) occupe cette
                    # marge et un second (44, 45, 42, 43) garde le coin arrondi
                    # libre, qu'un greffon externe (genmon) peindrait en carré.
                    ids="3 44 ${widgets:+11 }1 ${buttons}2 ${assistant:+$assistant }45 8"
                    status_ids="40 42 ${tray:+10 }${keyboard:+12 }$right 43 41"
                    for number in 40 41 42 43 44 45; do
                        for spec in "plugin-$number:string:separator" "plugin-$number/style:uint:0" "plugin-$number/expand:bool:false"; do
                            property=${spec%%:*}; rest=${spec#*:}; kind=${rest%%:*}; setting=${rest#*:}
                            if ! xfconf-query -c xfce4-panel -p "/plugins/$property" >/dev/null 2>&1; then
                                xfconf-query -c xfce4-panel -p "/plugins/$property" -n -t "$kind" -s "$setting" || return
                            fi
                        done
                    done
                elif test "$alignment" = center; then
                    ids="${widgets:+11 }3 13 1 ${buttons}2 8 ${assistant:+$assistant }${tray:+10 }${keyboard:+12 }$right"
                else
                    # Même ordre qu'au centre (#920) : la bascule ne change que
                    # plugin-3/expand et la largeur du contrepoids (1 px à
                    # gauche), sans relancer la barre. Seul le bouton Widgets
                    # change de côté, comme sous Windows 11.
                    ids="3 13 1 ${buttons}2 8 ${widgets:+11 }${assistant:+$assistant }${tray:+10 }${keyboard:+12 }$right"
                fi
                current=$(xfconf-query -c xfce4-panel -p /panels/panel-1/plugin-ids 2>/dev/null | grep -E '^[0-9]+$' | tr '\n' ' ' || true)
                panels=$(xfconf-query -c xfce4-panel -p /panels 2>/dev/null | grep -E '^[0-9]+$' | tr '\n' ' ' || true)
                status_current=
                case " $panels" in *' 2 '*) status_current=$(xfconf-query -c xfce4-panel -p /panels/panel-2/plugin-ids 2>/dev/null | grep -E '^[0-9]+$' | tr '\n' ' ' || true) ;; esac
                if test "$current" != "$ids " || test "$status_current" != "${status_ids:+$status_ids }"; then
                    if test -n "$status_ids"; then
                        # Panneau 2 : longueur ajustée à son contenu, ancré en bas
                        # à droite (p=4), même hauteur que la barre.
                        size=$(xfconf-query -c xfce4-panel -p /panels/panel-1/size 2>/dev/null || echo 40)
                        for spec in "position:string:p=4;x=0;y=0" "position-locked:bool:true" "length:uint:1" \
                            "length-adjust:bool:true" "size:uint:$size" "nrows:uint:1" "mode:uint:0"; do
                            property=${spec%%:*}; rest=${spec#*:}; kind=${rest%%:*}; setting=${rest#*:}
                            xfconf-query -c xfce4-panel -p "/panels/panel-2/$property" -s "$setting" 2>/dev/null ||
                                xfconf-query -c xfce4-panel -p "/panels/panel-2/$property" -n -t "$kind" -s "$setting" || return
                        done
                        set --
                        for id in $status_ids; do set -- "$@" -t int -s "$id"; done
                        xfconf-query -c xfce4-panel -p /panels/panel-2/plugin-ids -n -a "$@" 2>/dev/null ||
                            xfconf-query -c xfce4-panel -p /panels/panel-2/plugin-ids -a "$@" || return
                        test "$panels" = "1 2 " || xfconf-query -c xfce4-panel -p /panels -a -t int -s 1 -t int -s 2 || return
                    elif test -n "$status_current"; then
                        # Retour au style classique : le panneau 2 disparaît, son
                        # contenu revient dans la barre (ids ci-dessus).
                        xfconf-query -c xfce4-panel -p /panels -a -t int -s 1 || return
                    fi
                    set --
                    for id in $ids; do set -- "$@" -t int -s "$id"; done
                    xfconf-query -c xfce4-panel -p /panels/panel-1/plugin-ids -a "$@" || return
                    # Une barre déjà lancée ne charge pas un greffon ajouté
                    # après coup, et ne déplace pas non plus correctement ceux
                    # qu'elle a déjà : le contrepoids 13 restait au bout de la
                    # barre, zone de notification décalée de sa largeur et
                    # Démarrer au quart de l'écran (#651). Relance dès que l'ordre
                    # change ; à l'ouverture de session, il est déjà en place.
                    if test -n "$current" && pgrep -u "$(id -u)" -x xfce4-panel >/dev/null 2>&1; then
                        restart_panel
                    fi
                fi
                expand=false; test "$alignment" != center || expand=true
                # Bascule en direct (#920) : l'alignement est enregistré d'abord,
                # puis le contrepoids (genmon 13) est rafraîchi tout de suite,
                # sans attendre sa période de 2 s, juste avant d'étirer ou non
                # le séparateur 3 : les deux changements arrivent ensemble.
                save "$config/preferences.conf" TASKBAR_ALIGN "$alignment"
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-3/expand 2>/dev/null || true)" != "$expand" &&
                    pgrep -u "$(id -u)" -x xfce4-panel >/dev/null 2>&1; then
                    xfce4-panel --plugin-event=genmon-13:refresh:bool:true >/dev/null 2>&1 || true
                fi
                # Barre flottante : elle épouse son contenu (aucun séparateur
                # extensible) et se centre d'elle-même.
                spread=true
                if test "$floating" = 1; then expand=false; spread=false; fi
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-3/expand 2>/dev/null || true)" != "$expand"; then
                    xfconf-query -c xfce4-panel -p /plugins/plugin-3/expand -s "$expand" || return
                fi
                if test "$(xfconf-query -c xfce4-panel -p /plugins/plugin-8/expand 2>/dev/null || true)" != "$spread"; then
                    xfconf-query -c xfce4-panel -p /plugins/plugin-8/expand -s "$spread" || return
                fi
                # Longueur et ancrage de la barre : capsule à la longueur de son
                # contenu (au centre, ou en bas à gauche) en flottant, toute la
                # largeur sinon. Réécrits seulement s'ils changent.
                if test "$floating" = 1; then
                    length=1 adjust=true position='p=10;x=0;y=0'
                    test "$alignment" = center || position='p=8;x=0;y=0'
                else
                    length=100 adjust=false position='p=10;x=0;y=0'
                fi
                for spec in "length:uint:$length" "length-adjust:bool:$adjust" "position:string:$position"; do
                    property=${spec%%:*}; rest=${spec#*:}; kind=${rest%%:*}; setting=${rest#*:}
                    if test "$(xfconf-query -c xfce4-panel -p "/panels/panel-1/$property" 2>/dev/null || true)" != "$setting"; then
                        xfconf-query -c xfce4-panel -p "/panels/panel-1/$property" -s "$setting" 2>/dev/null ||
                            xfconf-query -c xfce4-panel -p "/panels/panel-1/$property" -n -t "$kind" -s "$setting" || return
                    fi
                done
            fi
            # Windows + X s'ouvre au-dessus du bouton Démarrer, pas sous le
            # pointeur : position réécrite dans le rc.xml du compte.
            if have winlux-run; then winlux-run place-menu "$alignment" >/dev/null 2>&1 || true; fi
            save "$config/preferences.conf" TASKBAR_ALIGN "$alignment" ;;
        desktop-icon-recycle)
            mkdir -p "$HOME/Bureau"
            printf '%s\n' '[Desktop Entry]' 'Type=Link' 'Name=Corbeille' 'Icon=user-trash' 'URL=trash:///' > "$HOME/Bureau/winlux-recycle-bin.desktop"
            trust_desktop "$HOME/Bureau/winlux-recycle-bin.desktop" ;;
        desktop-icon-explorer)
            mkdir -p "$HOME/Bureau"
            cp /usr/share/applications/winlux-explorer.desktop "$HOME/Bureau/winlux-explorer.desktop"
            trust_desktop "$HOME/Bureau/winlux-explorer.desktop" ;;
        desktop-icon-computer)
            mkdir -p "$HOME/Bureau"
            printf '%s\n' '[Desktop Entry]' 'Type=Link' 'Name=Ce PC' 'Icon=computer' 'URL=file:///' > "$HOME/Bureau/winlux-computer.desktop"
            trust_desktop "$HOME/Bureau/winlux-computer.desktop" ;;
        desktop-icon-documents)
            mkdir -p "$HOME/Bureau"
            printf '%s\n' '[Desktop Entry]' 'Type=Link' 'Name=Documents' 'Icon=folder-documents' "URL=file://$HOME/Documents" > "$HOME/Bureau/winlux-documents.desktop"
            trust_desktop "$HOME/Bureau/winlux-documents.desktop" ;;
        theme-dark|theme-light) save "$config/preferences.conf" THEME "${action#theme-}" ;;
        gpu-set)
            # Le runtime revalide contre ce que le pilote expose ; ici on ne
            # laisse passer que des jetons simples, jamais un chemin.
            card=${2:-} setting=${3:-} amount=${4:-}
            case "$card" in card[0-9]|card[0-9][0-9]) ;; *) echo 'Carte graphique invalide' >&2; return 1 ;; esac
            case "$setting" in power-limit|perf|oc-sclk|oc-reset) ;; *) echo 'Reglage GPU inconnu' >&2; return 1 ;; esac
            case "$amount" in ''|*[!a-z0-9]*) echo 'Valeur GPU invalide' >&2; return 1 ;; esac
            have winlux || { echo 'Runtime winlux introuvable' >&2; return 1; }
            winlux --gpu-set "$card" "$setting" "$amount" ;;
        overlay-on|overlay-off)
            on=0; test "$action" != overlay-on || on=1
            save_overlay OVERLAY_ENABLED "$on" ;;
        overlay-auto-on|overlay-auto-off)
            on=0; test "$action" != overlay-auto-on || on=1
            save_overlay OVERLAY_AUTOSHOW "$on" ;;
        overlay-key-shift-f12|overlay-key-ctrl-shift-o|overlay-key-alt-f12) save_overlay OVERLAY_HOTKEY "${action#overlay-key-}" ;;
        overlay-pos-top-left|overlay-pos-top-right|overlay-pos-bottom-left|overlay-pos-bottom-right) save_overlay OVERLAY_POSITION "${action#overlay-pos-}" ;;
        overlay-fps-on|overlay-fps-off|overlay-frametime-on|overlay-frametime-off|overlay-cpu-on|overlay-cpu-off|overlay-gpu-on|overlay-gpu-off)
            metric=${action#overlay-}; state=${metric##*-}; metric=${metric%-*}
            on=0; test "$state" != on || on=1
            save_overlay "OVERLAY_$(printf '%s' "$metric" | tr 'a-z' 'A-Z')" "$on" ;;
        cpu-profile-performance|cpu-profile-balanced|cpu-profile-powersave)
            have winlux || { echo 'Runtime winlux introuvable' >&2; return 1; }
            winlux --cpu-profile "${action#cpu-profile-}" "${root:-/}" || return
            save "$config/cpu.conf" CPU_PROFILE "${action#cpu-profile-}" ;;
        game-mode-auto|game-mode-off)
            save "$config/cpu.conf" GAME_MODE "${action#game-mode-}" ;;
        # Pilotes : installation depuis les dépôts Ubuntu (système installé).
        drivers-refresh) winlux-drivers refresh ;;
        drivers-install-modules|drivers-install-nvidia|drivers-install-mesa|drivers-install-vaapi|drivers-install-microcode|\
        drivers-install-amd-pstate|drivers-install-fwupd|drivers-install-fingerprint|drivers-install-scanner|drivers-install-printer-drivers)
            winlux-drivers install "${action#drivers-install-}" ;;
        drivers-remove-nvidia) winlux-drivers remove nvidia ;;
        drivers-firmware-check) winlux-drivers firmware-check ;;
        drivers-firmware-update) winlux-drivers firmware-update ;;
        drivers-auto-on) winlux-drivers set-auto 1 ;;
        drivers-auto-off) winlux-drivers set-auto 0 ;;
        cpu-hybrid-auto|cpu-hybrid-heavy|cpu-hybrid-performance|cpu-hybrid-efficiency)
            save "$config/cpu.conf" HYBRID_POLICY "${action#cpu-hybrid-}" ;;
        cpu-fan-auto|cpu-fan-quiet|cpu-fan-balanced|cpu-fan-performance)
            have winlux || { echo 'Runtime winlux introuvable' >&2; return 1; }
            save "$config/cpu.conf" FAN_CURVE "${action#cpu-fan-}" || return
            # Le demon rend lui-meme la main au firmware en mode auto.
            test "$action" = cpu-fan-auto || fan_daemon ;;
        cpu-restore)
            profile=$(cpu_pref CPU_PROFILE)
            case "$profile" in performance|balanced|powersave) winlux --cpu-profile "$profile" "${root:-/}" || true ;; esac
            case "$(cpu_pref FAN_CURVE)" in quiet|balanced|performance) fan_daemon ;; esac ;;
        display-follow|display-fixed) winlux-display "${action#display-}" ;;
        # Écrans multiples : « display-set layout extend », « display-set
        # scale-step HDMI-1 up »… (arguments revalidés par winlux-display).
        display-set) shift; winlux-display "$@" ;;
        display-mode-*) mode=${action#display-mode-}; case "$mode" in *[!0-9x]*|'') return 1 ;; esac; winlux-display mode "$mode" ;;
        display-scale-auto) winlux-display scale auto ;;
        display-scale-up|display-scale-down)
            scale=$(value "$config/display.conf" UI_SCALE)
            case "$action:$scale" in
                display-scale-up:1) scale=1.25 ;; display-scale-up:1.25) scale=1.5 ;; display-scale-up:1.5|display-scale-up:2) scale=2 ;;
                display-scale-down:2) scale=1.5 ;; display-scale-down:1.5) scale=1.25 ;; display-scale-down:1.25|display-scale-down:1) scale=1 ;;
                *) scale=1.25 ;;
            esac
            winlux-display scale "$scale" ;;
        update-check)
            winlux-update --check || return
            updates_tmp=$(fresh "$run/updates.txt") || return
            winlux-update --list > "$updates_tmp" || { rm -f "$updates_tmp"; return 1; }
            publish "$updates_tmp" "$run/updates.txt"
            update_state ;;
        update-apply) if winlux-update --apply; then code=0; else code=$?; fi; update_state; return $code ;;
        update-rollback) if winlux-update --rollback; then code=0; else code=$?; fi; update_state; return $code ;;
        update-auto-on) winlux-update --auto enable && update_state ;;
        update-pause-1|update-pause-2|update-pause-3|update-pause-4|update-pause-5)
            winlux-update --pause "${action#update-pause-}" && update_state ;;
        update-resume) winlux-update --resume && update_state ;;
        # Heures d'activité « update-hours-8-18 » : début et fin revalidés par winlux-update.
        update-hours-[0-9]-[0-9]|update-hours-[0-9]-[0-9][0-9]|update-hours-[0-9][0-9]-[0-9]|update-hours-[0-9][0-9]-[0-9][0-9])
            hours=${action#update-hours-}
            winlux-update --active-hours "${hours%-*}" "${hours#*-}" && update_state ;;
        sysupdate-mode-auto|sysupdate-mode-manual|sysupdate-mode-scheduled)
            winlux-sysupdate --mode "${action#sysupdate-mode-}" || return
            # Le mode s'applique aussi aux mises a jour de Winlux lui-meme.
            if test "$action" = sysupdate-mode-auto; then winlux-update --auto enable; else winlux-update --auto disable; fi ;;
        sysupdate-hour-up|sysupdate-hour-down) winlux-sysupdate --hour "${action#sysupdate-hour-}" ;;
        sysupdate-day-next) winlux-sysupdate --day next ;;
        sysupdate-check) winlux-sysupdate --check ;;
        sysupdate-apply) winlux-sysupdate --apply ;;
        welcome-done) save "$config/preferences.conf" WELCOME_DONE 1 ;;
        welcome-reset) save "$config/preferences.conf" WELCOME_DONE 0 && save "$config/preferences.conf" WELCOME_STEP 0 ;;
        # Etape atteinte : la visite reprend la ou elle a ete fermee.
        welcome-step-[0-9]|welcome-step-[0-9][0-9]) save "$config/preferences.conf" WELCOME_STEP "${action#welcome-step-}" ;;
        privacy-telemetry-on|privacy-telemetry-off) save "$config/privacy.conf" TELEMETRY "${action#privacy-telemetry-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-microphone-allow|privacy-microphone-deny) save "$config/privacy.conf" MICROPHONE "${action#privacy-microphone-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-camera-allow|privacy-camera-deny) save "$config/privacy.conf" CAMERA "${action#privacy-camera-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-location-allow|privacy-location-deny) save "$config/privacy.conf" LOCATION "${action#privacy-location-}" && chmod 644 "$config/privacy.conf" ;;
        lock-now) if have winlux-lock; then (setsid winlux-lock now >/dev/null 2>&1 &); else echo 'winlux-lock absent' >&2; return 1; fi ;;
        lock-after-0|lock-after-60|lock-after-180|lock-after-300|lock-after-600|lock-after-900|lock-after-1800|lock-after-3600)
            save "$config/lock.conf" LOCK_AFTER "${action#lock-after-}" && chmod 644 "$config/lock.conf" ;;
        lock-resume-on) save "$config/lock.conf" LOCK_ON_RESUME 1 && chmod 644 "$config/lock.conf" ;;
        lock-resume-off) save "$config/lock.conf" LOCK_ON_RESUME 0 && chmod 644 "$config/lock.conf" ;;
        privacy-history-on|privacy-history-off) save "$config/privacy.conf" LOCATION_HISTORY "${action#privacy-history-}" && chmod 644 "$config/privacy.conf" ;;
        update-auto-off) winlux-update --auto disable && update_state ;;
        report-offer-on|report-offer-off) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "offer=${action#report-offer-}" ;;
        report-redact-on|report-redact-off) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "redact=${action#report-redact-}" ;;
        report-mode-auto|report-mode-browser|report-mode-file) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "mode=${action#report-mode-}" ;;
        report-auto-on|report-auto-off) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "auto=${action#report-auto-}" ;;
        # Jeton GitHub du système et rapports à envoyer : lus sur l'entrée
        # standard (jamais en argument), traités par root seul.
        report-token-set) winlux-report --set-token ;;
        report-token-forget) winlux-report --forget-token ;;
        report-send) winlux-report --send-stdin ;;
        report-open) (setsid winlux-report >/dev/null 2>&1 &) ;;
        restart-session)
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : relance du bureau bloquée.' >&2; return 1; fi
            test -f "$root/etc/winlux/live-session" || test -f "$root/etc/winlux/install-config.json" || { echo 'Redemarrage de session reserve a Winlux' >&2; return 1; }
            "${WINLUX_POWER:-/usr/bin/winlux-power}" screen restart-session >/dev/null 2>&1 || true
            printf 'restart-session\n' > "$run/session-action" ;;
        power-profile-performance|power-profile-balanced|power-profile-saver)
            winlux --power profile "${action#power-profile-}" ;;
        power-screen-ac|power-screen-dc|power-sleep-ac|power-sleep-dc)
            winlux --power set "${action#power-}" next ;;
        power-lid) winlux --power set lid next ;;
        # Valeur choisie dans un menu déroulant (#895) : revalidée par winlux --power.
        power-set-screen-ac-[0-9]*|power-set-screen-dc-[0-9]*|power-set-sleep-ac-[0-9]*|power-set-sleep-dc-[0-9]*)
            setting=${action#power-set-}; value=${setting##*-}
            case "$value" in *[!0-9]*|'') return 1 ;; esac
            winlux --power set "${setting%-*}" "$value" ;;
        power-set-lid-nothing|power-set-lid-sleep|power-set-lid-hibernate|power-set-lid-shutdown|power-set-lid-display-off)
            winlux --power set lid "${action#power-set-lid-}" ;;
        power-set-button-nothing|power-set-button-sleep|power-set-button-hibernate|power-set-button-shutdown|power-set-button-display-off)
            winlux --power set button "${action#power-set-button-}" ;;
        power-button) winlux --power set button next ;;
        # Même chemin que les réglages rapides et les touches (winlux-brightness).
        power-brightness-up)
            if have winlux-brightness; then winlux-brightness set +10 --osd >/dev/null; else winlux --power set brightness +; fi ;;
        power-brightness-down)
            if have winlux-brightness; then winlux-brightness set -10 --osd >/dev/null; else winlux --power set brightness -; fi ;;
        power-suspend) winlux --power suspend ;;
        power-hibernate) winlux --power hibernate ;;
        power-hibernate-setup) winlux --power hibernate-setup ;;
        poweroff|reboot)
            test -f "$root/etc/winlux/live-session" || test -f "$root/etc/winlux/install-config.json" || { echo 'Arret reserve a Winlux' >&2; return 1; }
            if winlux-update --busy 2>/dev/null || winlux-sysupdate --busy 2>/dev/null; then echo 'Installation en cours : arrêt et redémarrage bloqués.' >&2; return 1; fi
            # Les commandes poweroff/reboot de l'image sont celles de systemd,
            # qui n'est pas l'init de Winlux : elles échouaient sans rien
            # éteindre. winlux-power ferme les applications derrière l'écran
            # « Arrêt en cours » puis signale l'init (USR2 arrêt, TERM redémarrage).
            "${WINLUX_POWER:-/usr/bin/winlux-power}" now "$action" ;;
        journal-status|journal-list|journal-errors|journal-crashes|journal-clear|journal-rotate)
            winlux-journal "${action#journal-}" "$2" ;;
        journal-export) winlux-journal export "$2" ;;
        restore-create) winlux-restore create "$2" ;;
        restore-list|restore-status) winlux-restore "${action#restore-}" ;;
        restore-restore|restore-delete) winlux-restore "${action#restore-}" "$2" ;;
        backup-status|backup-request|backup-list|backup-prune) winlux-backup "${action#backup-}" "$2" ;;
        backup-drives) winlux-backup drives ;;
        # Paramètres natifs › Sauvegarde : arguments directs (chemins avec
        # espaces) ; le relais vérifie que le compte est celui qui demande.
        backup-set|backup-add|backup-remove|backup-share|backup-browse|backup-versions)
            shift; winlux-backup "${action#backup-}" "$@" ;;
        backup-restore)
            # La restauration peut être longue : détachée comme la sauvegarde.
            shift
            mkdir -p "$run/backup"
            (winlux-backup restore "$@" >"$run/backup/$1.restore.log" 2>&1 &) ;;
        backup-run)
            # Une sauvegarde peut durer : elle ne bloque pas les autres réglages.
            winlux-backup status "$2"
            (winlux-backup run "$2" >"$run/backup/$2.log" 2>&1 &) ;;
        session-*) shift; winlux-sessions "${action#session-}" "$@" ;;
        remote-*) shift; winlux-remote-config "${action#remote-}" "$@" ;;
        # Le dossier à partager est le dossier courant (fixé par l'Explorateur).
        share-*) shift; winlux-share "${action#share-}" "$@" ;;
        transfer-*) shift; WINLUX_TRANSFER_RUN="$mine" winlux-transfer "${action#transfer-}" "$@" ;;
        # Téléphone connecté (winlux-phone) : ports du protocole KDE Connect
        # ouverts sur les réseaux privés et de domaine seulement, jamais public.
        phone-firewall-on|phone-firewall-off)
            fw="$config/firewall.conf" tmp="$config/firewall.conf.$$"
            if test -r "$fw"; then grep -v '^rule=944[78]	' "$fw" > "$tmp" || true; else : > "$tmp"; fi
            if test "$action" = phone-firewall-on; then
                printf 'rule=9447\tTelephone connecte Winlux (TCP)\tin\tallow\ttcp\t1716,1739-1764\t\tprivate,domain\t1\n' >> "$tmp"
                printf 'rule=9448\tTelephone connecte Winlux (UDP)\tin\tallow\tudp\t1716\t\tprivate,domain\t1\n' >> "$tmp"
            fi
            mv "$tmp" "$fw"; chmod 644 "$fw" ;;
        # Type de réseau (#1103) : Public ferme le partage et la découverte,
        # Privé les rouvre ; profile= de firewall.conf, lu par le pare-feu.
        firewall-profile-private|firewall-profile-public)
            fw="$config/firewall.conf" tmp="$config/firewall.conf.$$"
            if test -r "$fw"; then grep -v '^profile=' "$fw" > "$tmp" || true; else : > "$tmp"; fi
            printf 'profile=%s\n' "${action#firewall-profile-}" >> "$tmp"
            mv "$tmp" "$fw"; chmod 644 "$fw"
            if have winlux; then winlux --firewall apply || true; fi
            if have winlux-share; then winlux-share status >/dev/null 2>&1 || true; fi ;;
        # Comptes locaux (Paramètres › Comptes) ; mot de passe sur l'entrée standard.
        account-add|account-password|account-role|account-remove) shift; winlux-accounts "${action#account-}" "$@" ;;
        *) echo "Action inconnue : $action" >&2; return 1 ;;
    esac
}
if test "$relayed" = 1; then
    perform "$@"
    exit $?
fi
if relayed_session && ! local_action; then
    perform() { "${WINLUX_ROOT_TOOL:-winlux-root}" control "$@"; }
fi
log="$mine/action.log"
case "$action" in update-*) log="$mine/update.log" ;; sysupdate-*) log="$mine/sysupdate.log" ;; esac
case "$action" in update-check|update-apply|update-rollback) rm -f "$run/update-error-phase" ;; esac
if perform "$@" > "$log" 2>&1; then
    message 'Opération terminée.'
    result=0
else
    result=$?
    if test "$result" = 75; then
        # Reporté faute de réseau (navigateur, boutique) : pas une panne.
        message 'Pas de connexion Internet : ce sera fait automatiquement au prochain démarrage connecté.'
    elif test "${action#update-}" != "$action" || test "${action#sysupdate-}" != "$action"; then
        detail=$(tail -n 1 "$log")
        message "Echec de la mise à jour : ${detail:-aucun détail dans le journal}"
    else
        message "Échec de l’opération. Le journal indique la cause."
    fi
fi
# Au démarrage de session, l'init rafraîchit déjà l'état toutes les 3 s :
# inutile de relancer ici la vingtaine de sondes de status (plusieurs
# secondes par appel) entre chaque réglage restauré.
if test "${WINLUX_CONTROL_NO_STATUS:-0}" != 1; then status; fi
exit "$result"
