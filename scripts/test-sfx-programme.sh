#!/bin/sh
# Auto-extractible 7-Zip qui lance son propre programme (Zoom, #2390) :
# comme le module SFX de 7-Zip (aide, commutateur -sfx), l'archive est
# extraite dans le dossier temporaire du profil, le programme nommé par
# ExecuteFile ou RunProgram est lancé et attendu, son code rendu, et le
# dossier d'extraction effacé.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}
test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
test -f "$testdir/exitcode.exe" || { echo "exitcode.exe absent : $testdir" >&2; exit 1; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "ÉCHEC : $*" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
gen=$root/scripts/sfx-7zip-test.py
run() {
    rm -rf "$work/home"
    mkdir "$work/home"
    set +e
    HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
        timeout 120 "$runtime" --setup "$work/$1" >"$work/out.txt" 2>&1
    code=$?
    set -e
}
check() {
    test "$code" = 42 || fail "$1 : code $code au lieu de 42 (programme du SFX non lancé)"
    grep -q "cet auto-extractible lance son propre programme" "$work/out.txt" && fail "$1 : refus d'office"
    test -z "$(find "$work/home" -name 'winlux-sfx-*' 2>/dev/null)" || fail "$1 : dossier d'extraction laissé"
    grep -q 'AppData\\Local\\Temp\\winlux-sfx-' "$work/out.txt" || fail "$1 : extraction hors du dossier temporaire du profil"
}
python3 -I "$gen" "$work/execute.exe" 'Title="Demo"\nExecuteFile="setup.exe"' "setup.exe=$testdir/exitcode.exe"
run execute.exe
check ExecuteFile
python3 -I "$gen" "$work/runprogram.exe" 'RunProgram="\"%%T\\bin\\installe.exe\""' \
    "bin\\installe.exe=$testdir/exitcode.exe" "lisezmoi.txt=$root/README.md"
run runprogram.exe
check "RunProgram avec %%T"
python3 -I "$gen" "$work/directory.exe" 'Directory="bin"\nRunProgram="installe.exe"' \
    "bin\\installe.exe=$testdir/exitcode.exe"
run directory.exe
check "RunProgram avec Directory"
echo "auto-extractible 7-Zip : programme d'ExecuteFile et de RunProgram lancé, code rendu, dossier effacé (#2390)"
