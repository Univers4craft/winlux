#!/bin/sh
# Pont entre le bureau Win32 et les services Linux. Les valeurs ecrites sont
# ASCII pour rester lisibles par les anciennes applications TextOutA.
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
config="$root/etc/winlux"
mkdir -p "$run" "$config"
export DISPLAY=${DISPLAY:-:0}
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/winlux/session-bus}

value() { if test -r "$1"; then sed -n "s/^$2=//p" "$1" | head -n 1; fi; }
pref() { value "$config/preferences.conf" "$1"; }
input() { value "$config/input.conf" "$1"; }
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
        devices=0
        if test -r "$root/proc/bus/input/devices"; then devices=$(awk '/^N: Name=/ { n++ } END { print n+0 }' "$root/proc/bus/input/devices"); fi
        printf 'input_devices=%s\n' "$devices"
        printf 'update_auto=%s\n' "$(winlux-update --auto status 2>/dev/null || printf 'indisponible')"
        if test -f "$root/var/lib/winlux/pending-reboot"; then
            printf 'update_action=reboot\n'
        else
            printf 'update_action=%s\n' "$(cat "$run/update-action" 2>/dev/null || true)"
        fi
        printf 'control_available=1\n'
        if have winlux-browsers; then winlux-browsers --status 2>/dev/null || true; fi
        if have winlux-display; then winlux-display status 2>/dev/null || true; fi
        if winlux-update --busy 2>/dev/null; then printf 'update_busy=1\n'; else printf 'update_busy=0\n'; fi
    } > "$run/control.txt.tmp.$$"
    mv "$run/control.txt.tmp.$$" "$run/control.txt"
    # --list lit seulement le manifeste deja en cache, sans acces reseau.
    if have winlux-update; then
        if winlux-update --list > "$run/updates.txt.tmp.$$" 2>/dev/null; then
            mv "$run/updates.txt.tmp.$$" "$run/updates.txt"
        else rm -f "$run/updates.txt.tmp.$$"; fi
    fi
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
        lang-fr|lang-en|lang-es|lang-de)
            lang=${action#lang-}
            save "$config/locale.conf" LANG "$lang" ;;
        tz-*)
            tz=${action#tz-}
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
        update-auto-off) winlux-update --auto disable ;;
        restart-session)
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : relance du bureau bloquée.' >&2; return 1; fi
            # Demande consommee par l'init superviseur. Ne touche pas aux
            # sessions de l'hote qui pourraient aussi executer ce script.
            test -f "$root/etc/winlux/live-session" || { echo 'Redemarrage de session reserve a Winlux Live' >&2; return 1; }
            printf 'restart-session\n' > "$run/session-action" ;;
        poweroff|reboot)
            test -f "$root/etc/winlux/live-session" || { echo 'Arret reserve a Winlux Live' >&2; return 1; }
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : arrêt et redémarrage bloqués.' >&2; return 1; fi
            /usr/bin/winlux-close-session || return
            sync || return
            "$action" ;;
        *) echo "Action inconnue : $action" >&2; return 1 ;;
    esac
}
log="$run/action.log"
case "$action" in update-*) log="$run/update.log" ;; esac
case "$action" in update-check|update-apply|update-rollback) rm -f "$run/update-error-phase" ;; esac
if perform > "$log" 2>&1; then
    message 'Operation terminee.'
    result=0
else
    result=$?
    if test "${action#update-}" != "$action"; then
        detail=$(tail -n 1 "$log")
        message "Echec de la mise à jour : ${detail:-aucun détail dans le journal}"
    else
        message "Echec de l'operation. Le journal indique la cause."
    fi
fi
status
exit "$result"
