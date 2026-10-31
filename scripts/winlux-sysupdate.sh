#!/bin/sh
# Mises a jour du systeme Linux (APT) : mode automatique, manuel ou planifie.
#   winlux-sysupdate status | --check | --apply | --tick | --busy
#   winlux-sysupdate --mode auto|manual|scheduled
#   winlux-sysupdate --hour up|down | --day next
# --tick est appele periodiquement par la session : il recherche les mises a
# jour une fois par jour et installe selon le mode choisi.
set -eu
umask 022
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
config="$root/etc/winlux/sysupdate.conf"
state="$root/var/lib/winlux/sysupdate.state"
mkdir -p "$run" "${config%/*}" "${state%/*}"

value() { if test -r "$1"; then sed -n "s/^$2=//p" "$1" | head -n 1; fi; }
save() {
    file=$1 key=$2 new_value=$3
    tmp="$file.$$"
    if test -r "$file"; then sed "/^$key=/d" "$file" > "$tmp"; else : > "$tmp"; fi
    printf '%s=%s\n' "$key" "$new_value" >> "$tmp"
    mv "$tmp" "$file"
}
have() { command -v "$1" >/dev/null 2>&1; }
number() { case "$1" in ''|*[!0-9]*) echo "$2" ;; *) echo "$1" ;; esac; }

mode() { m=$(value "$config" MODE); case "$m" in auto|manual|scheduled) echo "$m" ;; *) echo manual ;; esac; }
hour() { h=$(number "$(value "$config" HOUR)" 3); test "$h" -le 23 || h=3; echo "$h"; }
day() { d=$(value "$config" DAY); case "$d" in mon|tue|wed|thu|fri|sat|sun) echo "$d" ;; *) echo daily ;; esac; }
now() { date +%s; }

busy() {
    pid=$(cat "$run/sysupdate.pid" 2>/dev/null || true)
    case "$pid" in ''|*[!0-9]*) return 1 ;; esac
    kill -0 "$pid" 2>/dev/null
}
apt_ready() {
    have apt-get || { echo "APT n'est pas disponible dans ce systeme (session Live minimale)." >&2; return 1; }
    test "$(id -u)" = 0 || { echo 'Droits administrateur requis pour APT.' >&2; return 1; }
}
lock() {
    busy && { echo 'Une mise a jour systeme est deja en cours.' >&2; exit 1; }
    printf '%s\n' "$$" > "$run/sysupdate.pid"
    trap 'rm -f "$run/sysupdate.pid"' EXIT HUP INT TERM
}

check() {
    apt_ready || return
    apt-get update -q || return
    # Simulation : liste exacte de ce que "upgrade" installerait.
    apt-get -s -q upgrade 2>/dev/null \
        | awk '/^Inst / { old=$3; gsub(/[][]/, "", old); new=$4; gsub(/[()]/, "", new); print $2 " " old " -> " new }' \
        > "$run/sysupdates.txt.tmp.$$"
    mv "$run/sysupdates.txt.tmp.$$" "$run/sysupdates.txt"
    count=$(wc -l < "$run/sysupdates.txt" | tr -d ' ')
    save "$state" LAST_CHECK "$(now)"
    save "$state" COUNT "$count"
    if test "$count" = 0; then echo 'Le systeme est a jour.'; else echo "$count paquet(s) a mettre a jour."; fi
}
apply() {
    apt_ready || return
    winlux-update --busy 2>/dev/null && { echo 'Installation Winlux en cours : reessayez ensuite.' >&2; return 1; }
    export DEBIAN_FRONTEND=noninteractive
    apt-get -q -y -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold upgrade || return
    : > "$run/sysupdates.txt"
    save "$state" COUNT 0
    save "$state" LAST_INSTALL "$(now)"
    echo 'Mises a jour systeme installees.'
}
# Creneau planifie : jour (ou tous les jours) et heure choisis, une seule fois.
slot_due() {
    test "$(date +%H | sed 's/^0//')" = "$(hour)" || return 1
    d=$(day)
    test "$d" = daily || test "$(LC_ALL=C date +%a | tr 'A-Z' 'a-z')" = "$d" || return 1
    test "$(value "$state" SLOT)" != "$(date +%Y%m%d)"
}
tick() {
    m=$(mode)
    last=$(number "$(value "$state" LAST_CHECK)" 0)
    if test "$m" = scheduled && slot_due; then
        save "$state" SLOT "$(date +%Y%m%d)"
        check && apply
        winlux-update --apply 2>&1 || true
        return
    fi
    test $(($(now) - last)) -ge 86400 || return 0
    check || return
    if test "$m" = auto && test "$(value "$state" COUNT)" != 0; then apply; fi
}
status() {
    printf 'sysupdate_mode=%s\n' "$(mode)"
    printf 'sysupdate_hour=%s\n' "$(hour)"
    printf 'sysupdate_day=%s\n' "$(day)"
    printf 'sysupdate_count=%s\n' "$(number "$(value "$state" COUNT)" 0)"
    last=$(number "$(value "$state" LAST_CHECK)" 0)
    if test "$last" = 0; then printf 'sysupdate_last=jamais\n'
    else printf 'sysupdate_last=%s\n' "$(date -d "@$last" '+%d/%m/%Y %H:%M' 2>/dev/null || echo inconnue)"; fi
    if have apt-get; then printf 'sysupdate_apt=1\n'; else printf 'sysupdate_apt=0\n'; fi
    if busy; then printf 'sysupdate_busy=1\n'; else printf 'sysupdate_busy=0\n'; fi
}

case "${1:-status}" in
    status) status ;;
    --busy) busy ;;
    --mode)
        case "${2:-}" in auto|manual|scheduled) ;; *) echo 'usage : winlux-sysupdate --mode auto|manual|scheduled' >&2; exit 2 ;; esac
        save "$config" MODE "$2"
        echo "Mode de mise a jour : $2" ;;
    --hour)
        h=$(hour)
        case "${2:-}" in up) h=$(((h + 1) % 24)) ;; down) h=$(((h + 23) % 24)) ;; *) echo 'usage : winlux-sysupdate --hour up|down' >&2; exit 2 ;; esac
        save "$config" HOUR "$h"
        echo "Heure planifiee : ${h}h" ;;
    --day)
        test "${2:-}" = next || { echo 'usage : winlux-sysupdate --day next' >&2; exit 2; }
        case "$(day)" in daily) d=mon ;; mon) d=tue ;; tue) d=wed ;; wed) d=thu ;; thu) d=fri ;; fri) d=sat ;; sat) d=sun ;; *) d=daily ;; esac
        save "$config" DAY "$d"
        echo "Jour planifie : $d" ;;
    --check) lock; check ;;
    --apply) lock; apply ;;
    --tick) lock; tick ;;
    *) echo 'usage : winlux-sysupdate status | --check | --apply | --tick | --mode M | --hour up|down | --day next' >&2; exit 2 ;;
esac
