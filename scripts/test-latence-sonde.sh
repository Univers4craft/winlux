#!/bin/sh
# Latence d'une application Windows, mesurée de l'extérieur sous Xvfb
# (#2889) : la sonde testc/latence/latence.c (x64 et x86), lancée par le
# runtime puis par winlux-open, chronométrée par scripts/mesure-latence.py
# (lancement -> premier pixel, touche -> pixel, clic -> pixel).
#
# Les chiffres sont affichés pour suivre l'évolution ; le test n'échoue que
# sur une régression grossière, la CI étant bruyante :
#   premier pixel (médiane) > 1500 ms, touche ou clic (médiane) > 50 ms,
#   ou winlux-open plus lent que le lancement direct de plus de 60 ms.
# Prérequis : runtime compilé, MinGW-w64 (x86_64 et i686), libXtst, DISPLAY.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
archs=
for a in x86_64 i686; do
    cc=$a-w64-mingw32-gcc
    command -v "$cc" >/dev/null 2>&1 || continue
    "$cc" -O2 -Wall -Werror -mwindows -o "$work/latence-$a.exe" "$root/testc/latence/latence.c"
    archs="$archs $a"
done
test -n "$archs" || { echo "MinGW-w64 absent : test sauté"; exit 0; }
# winlux-open comme sur le système installé : winlux-applications présent.
mkdir -p "$work/bin" "$work/home"
ln -s "$root/scripts/winlux-applications.py" "$work/bin/winlux-applications"
echc=0
for a in $archs; do
    for mode in direct winlux-open; do
        if [ "$mode" = direct ]; then
            set -- "$winlux" "$work/latence-$a.exe"
        else
            set -- sh "$root/integration/winlux-open" "$work/latence-$a.exe"
        fi
        HOME="$work/home" PATH="$work/bin:$PATH" WINLUX_RUNTIME="$winlux" \
            python3 -I "$root/scripts/mesure-latence.py" --essais 7 --touches 5 --json -- "$@" \
            >"$work/$a-$mode.json" || echc=1
        printf '%s %s : %s\n' "$a" "$mode" "$(cat "$work/$a-$mode.json")"
    done
done
python3 -I - "$work" $archs <<'PY' || echc=1
import json, sys
work, archs = sys.argv[1], sys.argv[2:]
ko = []
for a in archs:
    r = {m: json.load(open(f"{work}/{a}-{m}.json")) for m in ("direct", "winlux-open")}
    for m, v in r.items():
        if v["echecs"] or not v["premier_pixel"]:
            ko.append(f"{a} {m} : sonde jamais peinte"); continue
        if v["premier_pixel"]["mediane"] > 1500:
            ko.append(f"{a} {m} : premier pixel {v['premier_pixel']['mediane']} ms")
        for k in ("touche", "clic"):
            if not v[k] or v[k]["mediane"] > 50:
                ko.append(f"{a} {m} : {k} {v[k] and v[k]['mediane']} ms")
    if not ko:
        surcout = r["winlux-open"]["premier_pixel"]["mediane"] - r["direct"]["premier_pixel"]["mediane"]
        print(f"{a} : winlux-open ajoute {surcout:.1f} ms au premier pixel")
        if surcout > 60:
            ko.append(f"{a} : winlux-open ajoute {surcout:.1f} ms")
if ko:
    sys.exit("ÉCHEC latence (#2889) : " + " ; ".join(ko))
PY
exit "$echc"
