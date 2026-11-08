#!/bin/sh
# Suspension de threads et contexte (#269), ce qu'il faut au ramasse-miettes
# de coreclr pour les applis .NET autonomes et WinForms : compile
# testc/msvc/threadctl_msvc.c avec clang (cible MSVC) si besoin, puis vérifie
# sous Winlux SuspendThread/ResumeThread, Get/SetThreadContext, la page de
# garde de la pile, la vue exécutable W^X de coreclr (#665), les messages
# enregistrés, GetObjectType sur un DC et WM_TIMER vu en PM_NOREMOVE.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/threadctl_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/threadctl_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

set +e
WINLUX_GUI=headless timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "threadctl_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "threadctl_msvc : suspension, contexte, détournement, page de garde, vue W^X, messages enregistrés"
