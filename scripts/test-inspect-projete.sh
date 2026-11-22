#!/bin/sh
# `winlux --inspect` projette le programme au lieu de le copier en mémoire
# (#2918) : un .exe de 300 Mo s'inspecte sous une limite de 150 Mo de données
# (RLIMIT_DATA ne compte pas une projection de fichier en lecture seule ;
# l'ancienne lecture entière échouait en « out of memory »).
# Prérequis : runtime compilé, prlimit, et MinGW-w64 ou un PE de test construit.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
command -v prlimit >/dev/null 2>&1 || { echo "prlimit absent : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    printf '#include <windows.h>\nint WINAPI WinMain(HINSTANCE a, HINSTANCE b, LPSTR c, int d) { return 0; }\n' >"$work/prog.c"
    x86_64-w64-mingw32-gcc -O2 -mwindows -o "$work/gros.exe" "$work/prog.c"
else
    pe=$(ls "$root"/target/x86_64-pc-windows-msvc/release/*.exe 2>/dev/null | head -n 1)
    test -n "$pe" || { echo "aucun PE de test : test sauté"; exit 0; }
    cp "$pe" "$work/gros.exe"
fi
attendu=$("$winlux" --inspect "$work/gros.exe")
# 300 Mo de données à la suite de l'image, comme un jeu ou une appli Electron.
head -c 300000000 /dev/zero | tr '\0' '\125' >>"$work/gros.exe"
code=0
obtenu=$(prlimit --data=$((150 * 1024 * 1024)) "$winlux" --inspect "$work/gros.exe" 2>"$work/err") || code=$?
if [ "$code" -ne 0 ] || [ "$obtenu" != "$attendu" ]; then
    echo "ÉCHEC : inspection d'un .exe de 300 Mo sous 150 Mo de données : code $code, « $obtenu » au lieu de « $attendu »" >&2
    cat "$work/err" >&2
    exit 1
fi
echo "--inspect : .exe de 300 Mo inspecté sous 150 Mo de données (« $obtenu ») (#2918)"
