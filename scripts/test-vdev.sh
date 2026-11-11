#!/bin/sh
# Appareil virtuel d'un pilote constructeur (#1533) : vdev_msvc.exe ouvre
# \\.\WinluxEssai et lui parle avec DeviceIoControl (synchrone, OVERLAPPED,
# port de complétion). Le journal doit nommer l'appareil et le code inconnu
# sans jamais recopier le contenu des tampons (sauf WINLUX_VDEV_DUMP=1).
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/vdev_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/vdev_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

run() {
    name=$1
    shift
    set +e
    env WINLUX_GUI=headless WINLUX_VDEV_ESSAI=1 WINLUX_TRACE=1 WINLUX_LOG_FILE="$work/$name.log" "$@" \
        timeout 60 "$runtime" "$exe" >"$work/$name.txt" 2>&1
    status=$?
    set -e
    test "$status" -eq 0 || {
        tail -40 "$work/$name.txt"
        echo "vdev_msvc.exe ($name) : le test n° $status a échoué" >&2
        exit 1
    }
}

run normal
cat "$work"/normal.* >"$work/normal.all" 2>/dev/null || true
grep -q 'WinluxEssai : DeviceIoControl 0x80002FFC (type 0x8000, fonction 0xBFF.*entrée 6 o.*code inconnu' "$work/normal.all" || {
    echo "journal : ligne du code inconnu absente" >&2
    grep -i vdev "$work/normal.all" | head >&2
    exit 1
}
if grep -q 'entrée = ' "$work/normal.all"; then
    echo "journal : contenu des tampons écrit sans WINLUX_VDEV_DUMP=1" >&2
    exit 1
fi

run dump WINLUX_VDEV_DUMP=1
cat "$work"/dump.* >"$work/dump.all" 2>/dev/null || true
grep -q 'fonction 0xBFF.*entrée = 010203040506' "$work/dump.all" || {
    echo "journal : WINLUX_VDEV_DUMP=1 n'ajoute pas l'entrée en hexadécimal" >&2
    exit 1
}
echo "vdev_msvc : \\\\.\\Nom, DeviceIoControl synchrone, OVERLAPPED et port de complétion, code inconnu journalisé sans contenu"
