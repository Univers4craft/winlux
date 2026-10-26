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
        printf 'align=%s\n' "$(pref TASKBAR_ALIGN)"
        printf 'theme=%s\n' "$(pref THEME)"
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
        if test -s "$config/update.token" && ! test -L "$config/update.token"; then printf 'update_auth=configured\n'; else printf 'update_auth=missing\n'; fi
        if test -f "$root/var/lib/winlux/pending-reboot"; then
            printf 'update_action=reboot\n'
        else
            printf 'update_action=%s\n' "$(cat "$run/update-action" 2>/dev/null || true)"
        fi
        printf 'control_available=1\n'
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
        network-renew) winlux-network restart ;;
        volume-up|volume-down|volume-mute)
            if have wpctl; then
                case "$action" in volume-up) wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+ ;; volume-down) wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- ;; *) wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle ;; esac
            elif have amixer; then
                case "$action" in volume-up) amixer set Master 5%+ ;; volume-down) amixer set Master 5%- ;; *) amixer set Master toggle ;; esac
            else echo 'Aucun mixeur audio installe.' >&2; return 1; fi ;;
        keyboard-fr|keyboard-us)
            layout=${action#keyboard-}
            have setxkbmap || { echo 'setxkbmap indisponible' >&2; return 1; }
            setxkbmap -layout "$layout" || return
            save "$config/input.conf" XKB_LAYOUT "$layout" ;;
        tap-on|tap-off)
            tap=0; test "$action" != tap-on || tap=1
            apply_tap "$tap" || return
            save "$config/input.conf" TAP_TO_CLICK "$tap" ;;
        align-left|align-center) save "$config/preferences.conf" TASKBAR_ALIGN "${action#align-}" ;;
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
        update-save-token)
            request="$run/update-token-request"
            test -f "$request" && ! test -L "$request" || { echo 'Demande de jeton absente ou invalide.' >&2; return 1; }
            chmod 600 "$request" || return
            length=$(wc -c < "$request")
            if test "$length" -eq 0; then
                rm -f "$config/update.token" "$request" || return
                echo 'Acces prive GitHub efface.'
            elif test "$length" -gt 255 || test "$(wc -l < "$request")" -ne 0 || LC_ALL=C grep -q '[^A-Za-z0-9_]' "$request"; then
                rm -f "$request"
                echo 'Format de jeton invalide. Utilisez un jeton GitHub sans espace.' >&2
                return 1
            else
                printf '\n' >> "$request" || return
                mv -f "$request" "$config/update.token" || return
                chmod 600 "$config/update.token" || return
                echo 'Acces prive GitHub enregistre.'
            fi ;;
        restart-session)
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : relance du bureau bloquée.' >&2; return 1; fi
            # Demande consommee par l'init superviseur. Ne touche pas aux
            # sessions de l'hote qui pourraient aussi executer ce script.
            test -f "$root/etc/winlux/live-session" || { echo 'Redemarrage de session reserve a Winlux Live' >&2; return 1; }
            printf 'restart-session\n' > "$run/session-action" ;;
        poweroff|reboot)
            test -f "$root/etc/winlux/live-session" || { echo 'Arret reserve a Winlux Live' >&2; return 1; }
            if winlux-update --busy 2>/dev/null; then echo 'Installation en cours : arrêt et redémarrage bloqués.' >&2; return 1; fi
            sync || return
            "$action" ;;
        *) echo "Action inconnue : $action" >&2; return 1 ;;
    esac
}
log="$run/action.log"
case "$action" in update-*) log="$run/update.log" ;; esac
if perform > "$log" 2>&1; then
    message 'Operation terminee.'
    result=0
else
    result=$?
    message "Echec de l'operation. Le journal indique la cause."
fi
status
exit "$result"
