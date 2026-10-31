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
mkdir -p "$run" "$config"
export DISPLAY=${DISPLAY:-:0}
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/winlux/session-bus}

value() { if test -r "$1"; then sed -n "s/^$2=//p" "$1" | head -n 1; fi; }
pref() { value "$config/preferences.conf" "$1"; }
overlay() { value "$config/overlay.conf" "$1"; }
# Les jeux lancés par l'utilisateur lisent ce fichier : il reste lisible.
save_overlay() { save "$config/overlay.conf" "$1" "$2" && chmod 644 "$config/overlay.conf"; }
cpu_pref() { value "$config/cpu.conf" "$1"; }
input() { value "$config/input.conf" "$1"; }
privacy() { v=$(value "$config/privacy.conf" "$1"); printf '%s\n' "${v:-$2}"; }
save() {
    file=$1 key=$2 new_value=$3
    tmp="$file.$$"
    if test -r "$file"; then sed "/^$key=/d" "$file" > "$tmp" || return; else : > "$tmp" || return; fi
    printf '%s=%s\n' "$key" "$new_value" >> "$tmp" || return
    mv "$tmp" "$file"
}
have() { command -v "$1" >/dev/null 2>&1; }
trust_desktop() {
    chmod 755 "$1" || return
    if have gio; then gio set -t string "$1" metadata::trusted true 2>/dev/null || true; fi
}
message() { printf '%s\n' "$*" > "$run/action.txt.tmp.$$"; mv "$run/action.txt.tmp.$$" "$run/action.txt"; }
status() {
    {
        printf 'version=%s\n' "$(head -n 1 "$root/usr/share/winlux/version" 2>/dev/null || printf 'developpement')"
        printf 'kernel=%s\n' "$(uname -r)"
        persistent=0
        if test -r "$root/proc/mounts"; then
            persistent=$(awk '$2 == "/" && $3 ~ /^(ext[234]|btrfs|xfs|f2fs)$/ { saved=1 } END { print saved+0 }' "$root/proc/mounts")
        fi
        printf 'persistent=%s\n' "$persistent"
        printf 'layout=%s\n' "$(input XKB_LAYOUT | sed 's/^$/fr/')"
        printf 'tap=%s\n' "$(input TAP_TO_CLICK | sed 's/^$/0/')"
        printf 'mouse_speed=%s\n' "$(input MOUSE_SPEED | sed 's/^$/50/')"
        printf 'align=%s\n' "$(pref TASKBAR_ALIGN)"
        printf 'theme=%s\n' "$(pref THEME)"
        printf 'lang=%s\n' "$(value "$config/locale.conf" LANG | sed 's/^$/fr/')"
        printf 'timezone=%s\n' "$(cat /etc/timezone 2>/dev/null || printf 'Europe/Paris')"
        printf 'network='; if have ip; then
            iface=$(ip -4 route show default 2>/dev/null | awk 'NR==1 { print $5 }')
            if test -n "$iface"; then
                addr=$(ip -4 -o addr show dev "$iface" 2>/dev/null | awk 'NR==1 { print $4 }')
                printf '%s %s\n' "$iface" "$addr"
                printf 'network_connected=1\n'
            else printf 'Hors ligne - aucune route par defaut\nnetwork_connected=0\n'; fi
        else printf 'Outil ip indisponible\nnetwork_connected=0\n'; fi
        printf 'audio='
        if have wpctl; then
            wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null | head -n 1 || printf 'Aucun peripherique audio\n'
        elif have amixer; then
            amixer get Master 2>/dev/null | awk -F '[][]' '/%/ { print $2 " " $4; found=1; exit } END { if (!found) print "Aucun controle Master" }'
        else printf 'Audio non disponible (aucun mixeur installe)\n'; fi
        # Aucune valeur de batterie inventee : en VM, CA signifie que
        # l'hyperviseur ne presente pas de batterie a l'invite.
        present=0
        for bat in "$root"/sys/class/power_supply/BAT*; do
            test -r "$bat/capacity" || continue
            capacity=$(cat "$bat/capacity")
            case "$capacity" in ''|*[!0-9]*) continue ;; esac
            test "$capacity" -le 100 || continue
            printf 'battery=%s%% %s\nbattery_level=%s\n' "$capacity" "$(cat "$bat/status" 2>/dev/null || true)" "$capacity"
            present=1; break
        done
        if test "$present" = 0; then printf 'battery=Alimentation CA - aucune batterie exposee\n'; fi
        printf 'battery_present=%s\n' "$present"
        # Etat detaille, modes et delais : module d'alimentation du runtime.
        if have winlux; then winlux --power status 2>/dev/null || true; fi
        devices=0
        if test -r "$root/proc/bus/input/devices"; then devices=$(awk '/^N: Name=/ { n++ } END { print n+0 }' "$root/proc/bus/input/devices"); fi
        printf 'input_devices=%s\n' "$devices"
        # Périphériques des programmes Windows : manettes (XInput/DirectInput),
        # webcams (Media Foundation) et micro (waveIn/WASAPI).
        pads=0
        if test -r "$root/proc/bus/input/devices"; then pads=$(awk '/^H: Handlers=/ && / js[0-9]/ { n++ } END { print n+0 }' "$root/proc/bus/input/devices"); fi
        printf 'gamepads=%s\n' "$pads"
        cams=0
        for cam in "$root"/sys/class/video4linux/video*; do
            test -r "$cam/name" || continue
            # Un seul nœud de capture par caméra UVC (index 0).
            test "$(cat "$cam/index" 2>/dev/null || echo 0)" = 0 || continue
            cams=$((cams + 1))
        done
        printf 'cameras=%s\n' "$cams"
        printf 'microphone='
        if have wpctl && mic=$(wpctl inspect @DEFAULT_AUDIO_SOURCE@ 2>/dev/null | sed -n 's/.*node.description = "\(.*\)"/\1/p' | head -n 1) && test -n "$mic"; then
            printf '%s\n' "$mic"
        elif test -r "$root/proc/asound/pcm" && grep -q 'capture' "$root/proc/asound/pcm"; then
            grep 'capture' "$root/proc/asound/pcm" | head -n 1 | cut -d: -f3 | sed 's/^ *//'
        else printf 'Aucun micro detecte\n'; fi
        bluetooth="$run/bluetooth.txt"
        if have winlux-bluetooth; then winlux-bluetooth status 2>/dev/null || true; fi
        printf 'bluetooth_present=%s\n' "$(value "$bluetooth" present | sed 's/^$/0/')"
        printf 'bluetooth_powered=%s\n' "$(value "$bluetooth" powered | sed 's/^$/0/')"
        printf 'bluetooth_connected=%s\n' "$(value "$bluetooth" connected | sed 's/^$/0/')"
        printf 'update_auto=%s\n' "$(winlux-update --auto status 2>/dev/null || printf 'indisponible')"
        if test -f "$root/var/lib/winlux/pending-reboot"; then
            printf 'update_action=reboot\n'
        else
            printf 'update_action=%s\n' "$(cat "$run/update-action" 2>/dev/null || true)"
        fi
        # Overlay de performances (Paramètres > Graphismes) ; vide = défaut.
        setting=$(overlay OVERLAY_ENABLED); printf 'overlay_enabled=%s\n' "${setting:-0}"
        printf 'overlay_hotkey=%s\n' "$(overlay OVERLAY_HOTKEY)"
        printf 'overlay_position=%s\n' "$(overlay OVERLAY_POSITION)"
        setting=$(overlay OVERLAY_AUTOSHOW); printf 'overlay_autoshow=%s\n' "${setting:-0}"
        for metric in fps frametime cpu gpu; do
            setting=$(overlay "OVERLAY_$(printf '%s' "$metric" | tr 'a-z' 'A-Z')")
            printf 'overlay_%s=%s\n' "$metric" "${setting:-1}"
        done
        gpu_count=$(cat "$root/run/winlux/hw/gpu-count" 2>/dev/null || echo 0)
        printf 'gpu_count=%s\n' "$gpu_count"
        test "$(cat "$root/run/winlux/hw/has-nvidia" 2>/dev/null)" = 1 && printf 'gpu_nvidia=%s\n' "$(cat "$root/run/winlux/hw/nvidia-driver" 2>/dev/null || echo nouveau)"
        test "$(cat "$root/run/winlux/hw/has-amd" 2>/dev/null)" = 1 && printf 'gpu_amd=%s\n' "$(cat "$root/run/winlux/hw/amd-driver" 2>/dev/null || echo amdgpu)"
        test "$(cat "$root/run/winlux/hw/has-intel" 2>/dev/null)" = 1 && printf 'gpu_intel=i915\n'
        gpu_mode=$(cat "$root/run/winlux/hw/gpu-mode" 2>/dev/null || true)
        test -n "$gpu_mode" && printf 'gpu_mode=%s\n' "$gpu_mode"
        printf 'cpu_sockets=%s\n' "$(cat "$root/run/winlux/hw/cpu-sockets" 2>/dev/null || echo 1)"
        printf 'cpu_threads=%s\n' "$(cat "$root/run/winlux/hw/cpu-threads" 2>/dev/null || echo 1)"
        printf 'wifi=%s\n' "$(cat "$root/run/winlux/hw/has-wifi" 2>/dev/null || echo 0)"
        printf 'touchpad=%s\n' "$(cat "$root/run/winlux/hw/has-touchpad" 2>/dev/null || echo 0)"
        printf 'webcam=%s\n' "$(cat "$root/run/winlux/hw/has-webcam" 2>/dev/null || echo 0)"
        welcome=$(pref WELCOME_DONE); printf 'welcome_done=%s\n' "${welcome:-0}"
        # Vie privee : rien n'est envoye par defaut, la localisation est coupee.
        printf 'privacy_telemetry=%s\n' "$(privacy TELEMETRY off)"
        printf 'privacy_microphone=%s\n' "$(privacy MICROPHONE allow)"
        printf 'privacy_camera=%s\n' "$(privacy CAMERA allow)"
        printf 'privacy_location=%s\n' "$(privacy LOCATION deny)"
        printf 'privacy_location_history=%s\n' "$(privacy LOCATION_HISTORY off)"
        # Signaler un probleme : reglages lus par winlux-report (rien n'est envoye seul).
        if have winlux-report; then WINLUX_REPORT_CONF="$config/report.conf" winlux-report --status 2>/dev/null || true; fi
        if have winlux-sysupdate; then winlux-sysupdate status 2>/dev/null || true; fi
        printf 'control_available=1\n'
        if have winlux-browsers; then winlux-browsers --status 2>/dev/null || true; fi
        if have winlux-display; then winlux-display status 2>/dev/null || true; fi
        if winlux-update --busy 2>/dev/null; then printf 'update_busy=1\n'; else printf 'update_busy=0\n'; fi
    } > "$run/control.txt.tmp.$$"
    mv "$run/control.txt.tmp.$$" "$run/control.txt"
    cpu_status
    # --list lit seulement le manifeste deja en cache, sans acces reseau.
    if have winlux-update; then
        if winlux-update --list > "$run/updates.txt.tmp.$$" 2>/dev/null; then
            mv "$run/updates.txt.tmp.$$" "$run/updates.txt"
        else rm -f "$run/updates.txt.tmp.$$"; fi
    fi
}
# Capacites processeur detectees par le runtime : chaque section des
# Parametres n'apparait que si son cap_*=1 (garde-fous materiels).
cpu_status() {
    have winlux || { rm -f "$run/cpu.txt"; return 0; }
    if winlux --cpu-report "${root:-/}" > "$run/cpu.txt.tmp.$$" 2>/dev/null; then
        hybrid=$(cpu_pref HYBRID_POLICY) fan=$(cpu_pref FAN_CURVE)
        printf 'hybrid_policy=%s\nfan_curve=%s\n' "${hybrid:-auto}" "${fan:-auto}" >> "$run/cpu.txt.tmp.$$"
        mv "$run/cpu.txt.tmp.$$" "$run/cpu.txt"
    else rm -f "$run/cpu.txt.tmp.$$"; fi
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
# Une seule action a la fois. Le bureau reste reactif pendant les appels
# reseau et voit le journal quand la commande detachee se termine.
if ! mkdir "$run/control.lock" 2>/dev/null; then message 'Une operation est deja en cours. Consultez le journal.'; exit 1; fi
trap 'rmdir "$run/control.lock" 2>/dev/null || true' EXIT HUP INT TERM
action=$1
message 'Operation en cours...'
perform() {
    case "$action" in
        browser-install-edge|browser-install-chrome|browser-install-firefox) winlux-browsers --install "${action#browser-install-}" ;;
        browser-default-edge|browser-default-chrome|browser-default-firefox) winlux-browsers --default "${action#browser-default-}" ;;
        browser-launch-edge|browser-launch-chrome|browser-launch-firefox) winlux-browsers --launch "${action#browser-launch-}" >"$run/browser.log" 2>&1 & ;;
        network-renew) winlux-network restart ;;
        network-scan)
            # Write available networks to a file the desktop shell can read.
            out="$run/networks.txt"
            if command -v nmcli >/dev/null 2>&1 && nmcli -t -f RUNNING general 2>/dev/null | grep -qx running; then
                nmcli -t -f SSID,SIGNAL,SECURITY,IN-USE device wifi list --rescan yes 2>/dev/null | head -n 20 > "$out.tmp.$$" || true
            elif command -v iwlist >/dev/null 2>&1; then
                iface=$(ip -4 route show default 2>/dev/null | awk 'NR==1{print $5}')
                test -n "$iface" || iface=$(ls /sys/class/net/ 2>/dev/null | grep -v '^lo$' | head -n 1)
                iwlist "$iface" scan 2>/dev/null | awk '
                    /Cell / { if (ssid!="") print ssid ":" signal ":" enc ":" use; ssid=""; signal="0"; enc=""; use="" }
                    /ESSID:/ { gsub(/.*ESSID:"|"$/,""); ssid=$0 }
                    /Signal level=/ { gsub(/.*Signal level=/,""); gsub(/ .*/,""); signal=int(($0+100)*2); if(signal>100) signal=100 }
                    /Encryption key:on/ { enc="WPA" }
                    END { if (ssid!="") print ssid ":" signal ":" enc ":" use }
                ' | head -n 20 > "$out.tmp.$$" || true
            else
                printf '' > "$out.tmp.$$"
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
            } >> "$out.tmp.$$"
            mv "$out.tmp.$$" "$out" ;;
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
        volume-up|volume-down|volume-mute)
            if have wpctl; then
                case "$action" in volume-up) wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+ ;; volume-down) wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- ;; *) wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle ;; esac
            elif have amixer; then
                case "$action" in volume-up) amixer set Master 5%+ ;; volume-down) amixer set Master 5%- ;; *) amixer set Master toggle ;; esac
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
            if have timedatectl; then
                timedatectl set-timezone "$tz" || return
            else
                ln -sf "/usr/share/zoneinfo/$tz" /etc/localtime || return
                printf '%s\n' "$tz" > /etc/timezone || return
            fi
            save "$config/locale.conf" TIMEZONE "$tz" ;;
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
                for spec in 'plugin-8:string:separator' 'plugin-8/style:uint:0' 'plugin-8/expand:bool:true' 'plugin-9:string:genmon'; do
                    property=${spec%%:*}; rest=${spec#*:}; kind=${rest%%:*}; setting=${rest#*:}
                    if ! xfconf-query -c xfce4-panel -p "/plugins/$property" >/dev/null 2>&1; then
                        xfconf-query -c xfce4-panel -p "/plugins/$property" -n -t "$kind" -s "$setting" || return
                    fi
                done
                xfconf-query -c xfce4-panel -p /panels/panel-1/plugin-ids -a -t int -s 3 -t int -s 1 -t int -s 2 -t int -s 8 -t int -s 4 -t int -s 5 -t int -s 6 -t int -s 9 -t int -s 7 || return
                expand=false; test "$alignment" != center || expand=true
                xfconf-query -c xfce4-panel -p /plugins/plugin-3/expand -s "$expand" || return
            fi
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
            winlux-update --list > "$run/updates.txt.tmp.$$" || return
            mv "$run/updates.txt.tmp.$$" "$run/updates.txt" ;;
        update-apply) winlux-update --apply ;;
        update-rollback) winlux-update --rollback ;;
        update-auto-on) winlux-update --auto enable ;;
        sysupdate-mode-auto|sysupdate-mode-manual|sysupdate-mode-scheduled)
            winlux-sysupdate --mode "${action#sysupdate-mode-}" || return
            # Le mode s'applique aussi aux mises a jour de Winlux lui-meme.
            if test "$action" = sysupdate-mode-auto; then winlux-update --auto enable; else winlux-update --auto disable; fi ;;
        sysupdate-hour-up|sysupdate-hour-down) winlux-sysupdate --hour "${action#sysupdate-hour-}" ;;
        sysupdate-day-next) winlux-sysupdate --day next ;;
        sysupdate-check) winlux-sysupdate --check ;;
        sysupdate-apply) winlux-sysupdate --apply ;;
        welcome-done) save "$config/preferences.conf" WELCOME_DONE 1 ;;
        welcome-reset) save "$config/preferences.conf" WELCOME_DONE 0 ;;
        privacy-telemetry-on|privacy-telemetry-off) save "$config/privacy.conf" TELEMETRY "${action#privacy-telemetry-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-microphone-allow|privacy-microphone-deny) save "$config/privacy.conf" MICROPHONE "${action#privacy-microphone-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-camera-allow|privacy-camera-deny) save "$config/privacy.conf" CAMERA "${action#privacy-camera-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-location-allow|privacy-location-deny) save "$config/privacy.conf" LOCATION "${action#privacy-location-}" && chmod 644 "$config/privacy.conf" ;;
        privacy-history-on|privacy-history-off) save "$config/privacy.conf" LOCATION_HISTORY "${action#privacy-history-}" && chmod 644 "$config/privacy.conf" ;;
        update-auto-off) winlux-update --auto disable ;;
        report-offer-on|report-offer-off) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "offer=${action#report-offer-}" ;;
        report-redact-on|report-redact-off) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "redact=${action#report-redact-}" ;;
        report-mode-auto|report-mode-browser|report-mode-file) WINLUX_REPORT_CONF="$config/report.conf" winlux-report --set "mode=${action#report-mode-}" ;;
        report-open) (setsid winlux-report >/dev/null 2>&1 &) ;;
        restart-session)
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : relance du bureau bloquée.' >&2; return 1; fi
            test -f "$root/etc/winlux/live-session" || { echo 'Redemarrage de session reserve a Winlux Live' >&2; return 1; }
            /usr/bin/winlux-shutdown-screen restart-session >/dev/null 2>&1 || true
            printf 'restart-session\n' > "$run/session-action" ;;
        power-profile-performance|power-profile-balanced|power-profile-saver)
            winlux --power profile "${action#power-profile-}" ;;
        power-screen-ac|power-screen-dc|power-sleep-ac|power-sleep-dc)
            winlux --power set "${action#power-}" next ;;
        power-lid) winlux --power set lid next ;;
        power-button) winlux --power set button next ;;
        power-brightness-up) winlux --power set brightness + ;;
        power-brightness-down) winlux --power set brightness - ;;
        power-suspend) winlux --power suspend ;;
        power-hibernate) winlux --power hibernate ;;
        power-hibernate-setup) winlux --power hibernate-setup ;;
        poweroff|reboot)
            test -f "$root/etc/winlux/live-session" || { echo 'Arret reserve a Winlux Live' >&2; return 1; }
            if winlux-update --busy 2>/dev/null || winlux-sysupdate --busy 2>/dev/null; then echo 'Installation en cours : arrêt et redémarrage bloqués.' >&2; return 1; fi
            /usr/bin/winlux-shutdown-screen "$action" >/dev/null 2>&1 || true
            /usr/bin/winlux-close-session || return
            sync || return
            "$action" ;;
        journal-status|journal-list|journal-errors|journal-crashes|journal-clear|journal-rotate)
            winlux-journal "${action#journal-}" "$2" ;;
        journal-export) winlux-journal export "$2" ;;
        restore-create) winlux-restore create "$2" ;;
        restore-list|restore-status) winlux-restore "${action#restore-}" ;;
        restore-restore|restore-delete) winlux-restore "${action#restore-}" "$2" ;;
        *) echo "Action inconnue : $action" >&2; return 1 ;;
    esac
}
log="$run/action.log"
case "$action" in update-*) log="$run/update.log" ;; sysupdate-*) log="$run/sysupdate.log" ;; esac
case "$action" in update-check|update-apply|update-rollback) rm -f "$run/update-error-phase" ;; esac
if perform "$@" > "$log" 2>&1; then
    message 'Operation terminee.'
    result=0
else
    result=$?
    if test "${action#update-}" != "$action" || test "${action#sysupdate-}" != "$action"; then
        detail=$(tail -n 1 "$log")
        message "Echec de la mise à jour : ${detail:-aucun détail dans le journal}"
    else
        message "Echec de l'operation. Le journal indique la cause."
    fi
fi
status
exit "$result"
