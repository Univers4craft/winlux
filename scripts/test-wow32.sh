#!/bin/sh
# Programmes Windows 32 bits (x86) sous Winlux (#232) : construit
# testc/wow32/hello32.exe avec clang (cible i686-pc-windows-msvc) si besoin,
# puis vérifie qu'il démarre et que ses quinze vérifications passent :
# TEB 32 bits sur FS, PEB, pile, dernière erreur, mémoire sous 2 Go,
# argument et retour de 64 bits, pseudo-handle -1, ligne de commande,
# environnement, nettoyage de pile stdcall. Deux fois de plus : image
# déplacée (relocations HIGHLOW) et base de FS rétablie par arch_prctl,
# comme sur un processeur sans FSGSBASE. Enfin, un accès à NULL dans le
# code x86 doit finir au rapport de plantage (code 0xC0000005, octet bas 5).
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$out/hello32.exe" || test "$root/testc/wow32/hello32.c" -nt "$out/hello32.exe" \
    || test "$root/testc/wow32/build.sh" -nt "$out/hello32.exe"; then
    sh "$root/testc/wow32/build.sh" "$out"
fi
test -f "$out/hello32.exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

run() {
    name=$1; shift
    set +e
    env WINLUX_GUI=headless WINLUX_WOW32_TEST=ok "$@" timeout 60 "$runtime" "$out/$name.exe" argument-de-test >"$work/out.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'Tout est correct' "$work/out.txt"; then
        cat "$work/out.txt"
        echo "$name.exe ($*) : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
    grep -q 'Bonjour depuis un programme Windows 32 bits' "$work/out.txt"
}

run hello32
echo "hello32 : programme x86 lancé, 15 vérifications passées"
run hello32_reloc
echo "hello32_reloc : image déplacée sous 2 Go, relocations HIGHLOW"
run hello32 WINLUX_WOW32_NO_FSGSBASE=1
echo "hello32 : FS rétabli par arch_prctl (sans FSGSBASE)"

set +e
WINLUX_GUI=headless WINLUX_CRASH_DIR="$work/crash" timeout 60 "$runtime" "$out/hello32.exe" plantage >"$work/crash.txt" 2>&1
crash=$?
set -e
if test "$crash" -ne 5 || ! grep -q EXCEPTION_ACCESS_VIOLATION "$work/crash.txt"; then
    cat "$work/crash.txt"
    echo "hello32.exe plantage : code $crash, 5 et rapport de plantage attendus" >&2
    exit 1
fi
echo "hello32 : faute dans le code x86 -> rapport de plantage"
