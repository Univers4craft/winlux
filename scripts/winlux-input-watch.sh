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
# La liste comparée est celle du noyau ET celle du serveur X (xinput) : le
# noyau voit le clavier avant que X ne l'ajoute (pilote, udev), parfois plus
# d'une seconde avant. Réappliquer sur le seul changement du noyau laissait le
# clavier arrivé ensuite dans X avec la disposition par défaut (QWERTY). La
# liste de X n'est relue que dans les secondes qui suivent un changement du
# noyau : au repos, aucun programme n'est lancé (#3130).
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

# Liste du noyau (noms et gestionnaires seulement : les compteurs ne changent
# pas la liste), lue sans lancer de programme : elle est relue à chaque tour.
kernel_devices() {
    kernel=
    test -r "$root/proc/bus/input/devices" || return 0
    while IFS= read -r line || test -n "$line"; do
        case $line in N:*|H:*) kernel="$kernel$line
" ;; esac
    done <"$root/proc/bus/input/devices"
}
# Liste du serveur X : elle demande xinput (un programme et un aller-retour X).
x_devices() { xinput list --id-only 2>/dev/null; }
# Secondes depuis le démarrage, sans programme.
now_seconds() { read -r up _ </proc/uptime 2>/dev/null || up=0; now=${up%%.*}; }

if test "${1:-}" = --once; then
    apply
    exit 0
fi
# Une seule surveillance par affichage (session relancée).
lock="${XDG_RUNTIME_DIR:-/tmp}/winlux-input-watch-$(printf '%s' "${DISPLAY:-none}" | tr -c 'A-Za-z0-9' '_').pid"
if test -r "$lock" && kill -0 "$(cat "$lock" 2>/dev/null)" 2>/dev/null; then exit 0; fi
echo $$ >"$lock" 2>/dev/null || true
# Au repos, seule la liste du noyau est relue (#3130) : xinput, grep et cksum
# toutes les 2 s faisaient cinq programmes et un réveil de X par tour. Après
# un changement du noyau, la liste de X est suivie à chaque tour pendant
# WATCH_X secondes, le temps que X ajoute le périphérique (plus d'une seconde
# parfois), et la disposition est réappliquée quand elle change.
watch_x=${WINLUX_INPUT_WATCH_X:-15}
kernel_devices
last_kernel=$kernel
last_x=$(x_devices)
x_until=0
while sleep "$interval"; do
    kernel_devices
    if test "$kernel" != "$last_kernel"; then
        last_kernel=$kernel
        # Le serveur X ajoute le périphérique juste après le noyau.
        sleep 1
        apply
        last_x=$(x_devices)
        now_seconds
        x_until=$((now + watch_x))
        continue
    fi
    test "$x_until" -gt 0 || continue
    now_seconds
    if test "$now" -gt "$x_until"; then x_until=0; continue; fi
    x=$(x_devices)
    if test "$x" != "$last_x"; then
        sleep 1
        apply
        last_x=$(x_devices)
    fi
done
