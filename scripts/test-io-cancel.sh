#!/bin/sh
# Annulation des E/S et handles ou tampons invalides (#18) : compile testc/msvc/io_msvc.c
# avec clang (cible MSVC) si besoin, puis vérifie sous Winlux :
#   - une lecture déjà commencée sur un tube nommé, annulée par CancelIoEx
#     puis par CancelIo : ERROR_OPERATION_ABORTED, événement signalé, tube
#     encore utilisable, ERROR_NOT_FOUND quand il n'y a plus rien à annuler ;
#   - ReadFile, WriteFile, CancelIo(Ex), CloseHandle sur un handle invalide :
#     ERROR_INVALID_HANDLE ;
#   - tampon NULL ou en lecture seule : ERROR_NOACCESS, sans plantage.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/io_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/io_msvc.c" -nt "$exe"; then
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
    echo "io_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "io_msvc : E/S commencées annulées, handles et tampons invalides refusés proprement"
