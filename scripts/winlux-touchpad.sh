#!/bin/sh
# Pavé tactile (pilote libinput de Xorg) : lecture des capacités réelles,
# réglages et réapplication (début de session, reprise de veille, nouveau
# pavé branché). Seuls les réglages que le pavé accepte sont annoncés : les
# Paramètres cachent le reste.
#   winlux-touchpad status
#   winlux-touchpad set tap|natural|twofinger|clickfinger|dwt 0|1
#   winlux-touchpad set speed 0..100
#   winlux-touchpad apply
set -eu
umask 022
root=${WINLUX_CONTROL_ROOT:-}
config="$root/etc/winlux/input.conf"
export DISPLAY=${DISPLAY:-:0}

get() { sed -n "s/^$1=//p" "$config" 2>/dev/null | head -n 1; }
save() {
    mkdir -p "${config%/*}"
    if test -f "$config"; then sed "/^$1=/d" "$config" >"$config.new.$$"; else : >"$config.new.$$"; fi
    printf '%s=%s\n' "$1" "$2" >>"$config.new.$$"
    mv "$config.new.$$" "$config"
}

# Pavés tactiles : les périphériques libinput qui savent « taper pour cliquer ».
touchpads() {
    command -v xinput >/dev/null 2>&1 || return 0
    for id in $(xinput --list --id-only 2>/dev/null); do
        if xinput list-props "$id" 2>/dev/null | grep -q 'libinput Tapping Enabled ('; then
            printf '%s\n' "$id"
        fi
    done
}

# Valeurs actuelles d'une propriété libinput du premier pavé (vide si absente).
prop() {
    xinput list-props "$1" 2>/dev/null | sed -n "s/^[[:space:]]*libinput $2 ([0-9]*):[[:space:]]*//p" | head -n 1 | tr -d ' '
}

# Clé de configuration, propriété libinput et valeur pour 0/1.
apply_one() {
    id=$1 key=$2 value=$3
    case "$key" in
        tap) xinput set-prop "$id" 'libinput Tapping Enabled' "$value" ;;
        natural) xinput set-prop "$id" 'libinput Natural Scrolling Enabled' "$value" ;;
        dwt) xinput set-prop "$id" 'libinput Disable While Typing Enabled' "$value" ;;
        # Défilement : deux doigts, sinon le bord du pavé.
        twofinger)
            if test "$value" = 1; then xinput set-prop "$id" 'libinput Scroll Method Enabled' 1 0 0
            else xinput set-prop "$id" 'libinput Scroll Method Enabled' 0 1 0; fi ;;
        # Clic : deux doigts = clic droit (clickfinger), sinon zones du pavé.
        clickfinger)
            if test "$value" = 1; then xinput set-prop "$id" 'libinput Click Method Enabled' 0 1
            else xinput set-prop "$id" 'libinput Click Method Enabled' 1 0; fi ;;
        speed)
            # 0..100 → -1..1 (50 = vitesse par défaut).
            accel=$(awk -v v="$value" 'BEGIN { printf "%.2f", (v - 50) / 50 }')
            xinput set-prop "$id" 'libinput Accel Speed' "$accel" ;;
    esac
}

conf_key() {
    case "$1" in
        tap) echo TAP_TO_CLICK ;; natural) echo TOUCHPAD_NATURAL ;; dwt) echo TOUCHPAD_DWT ;;
        twofinger) echo TOUCHPAD_TWOFINGER ;; clickfinger) echo TOUCHPAD_CLICKFINGER ;; speed) echo TOUCHPAD_SPEED ;;
    esac
}

apply_saved() {
    for id in $(touchpads); do
        for key in tap natural dwt twofinger clickfinger speed; do
            value=$(get "$(conf_key "$key")")
            test -n "$value" || continue
            apply_one "$id" "$key" "$value" 2>/dev/null || true
        done
    done
}

status() {
    id=$(touchpads | head -n 1)
    if test -z "$id"; then
        printf 'touchpad_present=0\n'
        return 0
    fi
    printf 'touchpad_present=1\n'
    v=$(prop "$id" 'Tapping Enabled'); printf 'touchpad_tap=%s\n' "${v%%,*}"
    v=$(prop "$id" 'Natural Scrolling Enabled'); test -z "$v" || printf 'touchpad_natural=%s\n' "${v%%,*}"
    v=$(prop "$id" 'Disable While Typing Enabled'); test -z "$v" || printf 'touchpad_dwt=%s\n' "${v%%,*}"
    # Méthodes disponibles : « two-finger, edge, button » et « buttonareas, clickfinger ».
    avail=$(prop "$id" 'Scroll Methods Available')
    case "$avail" in 1,*)
        v=$(prop "$id" 'Scroll Method Enabled'); printf 'touchpad_twofinger=%s\n' "${v%%,*}" ;;
    esac
    avail=$(prop "$id" 'Click Methods Available')
    case "$avail" in *,1)
        v=$(prop "$id" 'Click Method Enabled'); printf 'touchpad_clickfinger=%s\n' "${v##*,}" ;;
    esac
    v=$(prop "$id" 'Accel Speed')
    test -z "$v" || printf 'touchpad_speed=%s\n' "$(awk -v a="$v" 'BEGIN { printf "%d", a * 50 + 50.5 }')"
    printf 'touchpad_gestures=%s\n' "$(xinput --version 2>/dev/null | grep -q 'XI version on server: 2\.[4-9]' && echo 1 || echo 0)"
}

case "${1:-status}" in
    status) status ;;
    apply) apply_saved ;;
    set)
        test "$#" -eq 3 || exit 2
        key=$2 value=$3
        case "$key" in
            tap|natural|twofinger|clickfinger|dwt) case "$value" in 0|1) ;; *) exit 2 ;; esac ;;
            speed) case "$value" in ''|*[!0-9]*) exit 2 ;; esac; test "$value" -le 100 || exit 2 ;;
            *) exit 2 ;;
        esac
        ids=$(touchpads)
        test -n "$ids" || { echo 'Aucun pavé tactile détecté.' >&2; exit 1; }
        for id in $ids; do apply_one "$id" "$key" "$value"; done
        save "$(conf_key "$key")" "$value" ;;
    *) exit 2 ;;
esac
