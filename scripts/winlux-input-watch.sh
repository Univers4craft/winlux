#!/bin/sh
# Périphériques d'entrée branchés pendant la session (clavier ou souris USB,
# station d'accueil, clavier de tablette détachable).
#
# Le serveur X donne à un clavier branché à chaud sa disposition par défaut
# (QWERTY), pas celle appliquée par setxkbmap au démarrage : un clavier USB
# branché après l'ouverture de session tapait donc en QWERTY dans une session
# AZERTY. Toutes les 2 s, si la liste des périphériques d'entrée change, la
# disposition choisie (XKB_LAYOUT de /etc/winlux/input.conf) et les réglages
# du pavé tactile et de la souris sont réappliqués.
#
#   winlux-input-watch           surveille (session graphique, $DISPLAY)
#   winlux-input-watch --once    réapplique une fois
# WINLUX_INPUT_ROOT déplace /proc et /etc (tests) ; WINLUX_INPUT_INTERVAL.
set -u
root=${WINLUX_INPUT_ROOT:-}
interval=${WINLUX_INPUT_INTERVAL:-2}

apply() {
    layout=$(sed -n 's/^XKB_LAYOUT=//p' "$root/etc/winlux/input.conf" 2>/dev/null | tail -n 1)
    case "$layout" in
        fr|us|de|es|gb|it|pt|be|ch) setxkbmap "$layout" 2>/dev/null || true ;;
    esac
    if command -v winlux-touchpad >/dev/null 2>&1; then winlux-touchpad apply >/dev/null 2>&1 || true; fi
    if command -v winlux-mouse >/dev/null 2>&1; then winlux-mouse apply >/dev/null 2>&1 || true; fi
}

devices() {
    # Noms et gestionnaires seulement : les compteurs ne changent pas la liste.
    grep -E '^(N|H):' "$root/proc/bus/input/devices" 2>/dev/null | cksum
}

if test "${1:-}" = --once; then
    apply
    exit 0
fi
# Une seule surveillance par affichage (session relancée).
lock="${XDG_RUNTIME_DIR:-/tmp}/winlux-input-watch-$(printf '%s' "${DISPLAY:-none}" | tr -c 'A-Za-z0-9' '_').pid"
if test -r "$lock" && kill -0 "$(cat "$lock" 2>/dev/null)" 2>/dev/null; then exit 0; fi
echo $$ >"$lock" 2>/dev/null || true
last=$(devices)
while sleep "$interval"; do
    now=$(devices)
    if test "$now" != "$last"; then
        # Le serveur X ajoute le périphérique juste après le noyau.
        sleep 1
        apply
        last=$(devices)
    fi
done
