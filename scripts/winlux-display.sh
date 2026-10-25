#!/bin/sh
# Display policy shared by settings and the Xorg supervisor; no eval.
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
config="$root/etc/winlux/display.conf"
export DISPLAY=${DISPLAY:-:0}
get() { sed -n "s/^$1=//p" "$config" 2>/dev/null | head -n 1; }
save() {
    mkdir -p "${config%/*}"
    if test -f "$config"; then sed "/^$1=/d" "$config" >"$config.new.$$"; else : >"$config.new.$$"; fi
    printf '%s=%s\n' "$1" "$2" >>"$config.new.$$"
    mv "$config.new.$$" "$config"
}
query=$(xrandr --query) || { echo 'Affichage Xorg indisponible.' >&2; exit 1; }
output=$(printf '%s\n' "$query" | awk '/ connected/ {print $1; exit}')
test -n "$output" || { echo 'Aucun écran connecté.' >&2; exit 1; }
modes=$(printf '%s\n' "$query" | awk -v output="$output" '/ connected/ {active=($1==output); next} active && /^[[:space:]]+[0-9]+x[0-9]+/ {print $1}')
current=$(printf '%s\n' "$query" | awk -v output="$output" '/ connected/ {active=($1==output); next} active && /\*/ {print $1; exit}')
case "${1:-status}" in
    status)
        printf 'display_output=%s\ndisplay_current=%s\n' "$output" "$current"
        printf 'display_modes=%s\n' "$(printf '%s\n' "$modes" | awk 'NF {printf "%s%s", sep, $1; sep=","} END {print ""}')"
        printf 'display_policy=%s\ndisplay_scale=%s\n' "$(get POLICY || true)" "$(get UI_SCALE || true)" ;;
    follow) xrandr --output "$output" --auto; save POLICY follow ;;
    fixed) save MODE "$current"; save POLICY fixed ;;
    mode)
        test "$#" -eq 2 || exit 2
        printf '%s\n' "$modes" | grep -Fxq "$2" || { echo 'Définition non proposée par cet écran.' >&2; exit 1; }
        # Persist only after the X server accepts this real advertised mode.
        xrandr --output "$output" --mode "$2"
        save MODE "$2"; save POLICY fixed ;;
    scale)
        case "${2:-}" in auto|1|1.25|1.5|2) save UI_SCALE "$2" ;; *) exit 2 ;; esac
        echo 'Échelle enregistrée. Relancez le bureau pour appliquer.' ;;
    sync)
        policy=$(get POLICY || true)
        if test "$policy" = fixed; then
            wanted=$(get MODE || true)
            if test -n "$wanted" && test "$wanted" != "$current" && printf '%s\n' "$modes" | grep -Fxq "$wanted"; then xrandr --output "$output" --mode "$wanted"; fi
        else
            preferred=$(printf '%s\n' "$query" | awk -v output="$output" '/ connected/ {active=($1==output); next} active && /\+/ {print $1; exit}')
            if test -n "$preferred" && test "$preferred" != "$current"; then xrandr --output "$output" --auto; fi
        fi ;;
    *) exit 2 ;;
esac
