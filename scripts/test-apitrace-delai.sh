#!/bin/sh
# WINLUX_APITRACE_DELAY (#3219) : rien n'est journalisé avant le délai, puis
# tout l'est. Sans délai, le journal commence dès le démarrage.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
command -v "$cc" >/dev/null || { echo "$cc absent : test sauté"; exit 0; }
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$cc" -O1 -o "$work/appels.exe" "$root/testc/trace/appels_reguliers.c"
premier() { grep -h -o '@[0-9.]*$' "$1"/*.log | head -n 1 | tr -d @; }
nombre() { cat "$1"/*.log | grep -c 'Sleep' || true; }
mkdir "$work/sans" "$work/avec"
(cd "$work" && WINLUX_APITRACE=1 WINLUX_APITRACE_DIR="$work/sans" "$winlux" appels.exe) >/dev/null 2>&1
(cd "$work" && WINLUX_APITRACE=1 WINLUX_APITRACE_DELAY=1 WINLUX_APITRACE_DIR="$work/avec" "$winlux" appels.exe) >/dev/null 2>&1
s0=$(premier "$work/sans"); a0=$(premier "$work/avec")
ns=$(nombre "$work/sans"); na=$(nombre "$work/avec")
echo "sans délai : première ligne à ${s0:-?} s, $ns appels de Sleep ; délai 1 s : première ligne à ${a0:-?} s, $na appels de Sleep"
awk -v s="${s0:-9}" -v a="${a0:-0}" -v ns="$ns" -v na="$na" 'BEGIN {
    if (s >= 0.5) { print "ÉCHEC : sans délai, le journal devrait commencer au démarrage"; exit 1 }
    if (a < 0.95) { print "ÉCHEC : avec un délai d une seconde, une ligne écrite à " a " s"; exit 1 }
    if (na < 5 || na >= ns) { print "ÉCHEC : après le délai, " na " appels de Sleep journalisés (sans délai : " ns ")"; exit 1 }
}'
echo "WINLUX_APITRACE_DELAY : rien avant le délai, les appels suivants journalisés (#3219)"
