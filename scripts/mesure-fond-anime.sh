#!/bin/sh
# Coût au repos du fond animé (#3079) : temps processeur du serveur X, du
# compositeur, du bureau et de winlux-fond-anime pendant DURÉE secondes, sans
# rien toucher. Rien n'est lancé ni arrêté : on lit seulement /proc.
#
#   sh scripts/mesure-fond-anime.sh [DURÉE]      (60 s par défaut)
#
# À lancer deux fois, animation désactivée puis activée (Paramètres ›
# Personnalisation › Arrière-plan), sur un bureau sans fenêtre ouverte.
set -eu
duration=${1:-60}
hz=$(getconf CLK_TCK)
pids=$(pgrep -x 'Xorg|Xvfb|picom|xfdesktop|winlux-fond-ani' || true)
# Un processus présent au début et à la fin seulement : sinon son départ
# fausserait la différence. Détail par processus, puis le total.
ticks() {
    for pid in $pids; do
        test -r "/proc/$pid/stat" || continue
        # utime et stime (champs 14 et 15), comptés après le nom entre parenthèses.
        sed 's/.*) //' "/proc/$pid/stat" | awk -v p="$pid" '$1 != "Z" {print p, $12 + $13}'
    done
}
# L'animation en marche (un processus fini mais pas encore récolté reste « Z »).
animation_pid() {
    for pid in $(pgrep -x winlux-fond-ani || true); do
        sed 's/.*) //' "/proc/$pid/stat" 2>/dev/null | awk '$1 != "Z" {found = 1} END {exit !found}' && { echo "$pid"; return; }
    done
}
wakeups() {
    pid=$(animation_pid)
    test -n "$pid" && awk '/^voluntary_ctxt_switches/ {print $2}' "/proc/$pid/status" || echo 0
}
for pid in $pids; do printf 'suivi : %s %s\n' "$pid" "$(cat "/proc/$pid/comm")"; done
before=$(ticks); w0=$(wakeups); s0=$(date +%s.%N)
sleep "$duration"
after=$(ticks); w1=$(wakeups); s1=$(date +%s.%N)
d=$(awk -v a="$s0" -v b="$s1" 'BEGIN {print b - a}')
t=0
for pid in $pids; do
    a=$(printf '%s\n' "$before" | awk -v p="$pid" '$1 == p {print $2}')
    b=$(printf '%s\n' "$after" | awk -v p="$pid" '$1 == p {print $2}')
    test -n "$a" && test -n "$b" || continue
    awk -v n="$(cat "/proc/$pid/comm" 2>/dev/null)" -v t="$((b - a))" -v hz="$hz" -v d="$d" \
        'BEGIN {printf "  %s : %.3f %%\n", n, 100 * t / hz / d}'
    t=$((t + b - a))
done
awk -v t="$t" -v hz="$hz" -v d="$d" -v w="$((w1 - w0))" 'BEGIN {
    printf "processeur=%.3f %% d'\''un cœur sur %.0f s (%.2f s de calcul)\n", 100 * t / hz / d, d, t / hz
    printf "reveils_animation=%.1f par seconde\n", w / d
}'
pid=$(animation_pid)
test -n "$pid" && awk '/^VmRSS/ {printf "memoire_animation=%s %s\n", $2, $3}' "/proc/$pid/status" || echo "memoire_animation=0 (arrêtée)"
