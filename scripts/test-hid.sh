#!/bin/sh
# Manettes et périphériques HID (#271) : compile testc/msvc/hid_msvc.c avec
# clang (cible MSVC) si besoin, puis le lance sous Winlux face à un faux
# appareil hidraw (sysfs factice + FIFO à la place de /dev/hidraw0) :
#   - setupapi trouve l'interface HID et son chemin \\?\hid#vid_…#hidraw0#{…} ;
#   - CreateFileW l'ouvre, hid.dll rend attributs, nom et capacités ;
#   - WriteFile / ReadFile en OVERLAPPED passent l'octet d'identifiant de
#     rapport que Windows ajoute et que hidraw omet.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/hid_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/hid_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dev="$work/sys/hidraw0/device"
mkdir -p "$dev" "$work/dev"
printf 'DRIVER=hid-generic\nHID_ID=0003:0000045E:0000028E\nHID_NAME=Winlux Manette test\nHID_UNIQ=00:11:22:33:44:55\n' >"$dev/uevent"
# Manette : 16 boutons, 4 axes de 8 bits, sortie de 2 octets, sans
# identifiant de rapport.
printf '\005\001\011\005\241\001\005\011\031\001\051\020\025\000\045\001\165\001\225\020\201\002\005\001\011\060\011\061\011\062\011\065\025\000\046\377\000\165\010\225\004\201\002\006\000\377\011\001\165\010\225\002\221\002\300' >"$dev/report_descriptor"
mkfifo "$work/dev/hidraw0"

cd "$work"
set +e
WINLUX_GUI=headless WINLUX_HIDRAW_SYSFS="$work/sys" WINLUX_HIDRAW_DEV="$work/dev" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "hid_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "hid_msvc : manette trouvée par setupapi, ouverte, rapports lus et écrits"
