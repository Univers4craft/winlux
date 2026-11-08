#!/bin/sh
# #1019 : un .exe lancé depuis un dossier personnel (Bureau) qui charge une
# DLL système par son simple nom (LoadLibraryExW + LOAD_LIBRARY_SEARCH_SYSTEM32,
# comme la CRT de MSVC) ne doit rien demander. claude.exe restait bloqué sur la
# demande « fichiers personnels » avant même son main, puis sur la lecture de
# son dossier de lancement (Bureau), désormais accordé comme un argument.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}
exe="$testdir/loadlib_search.exe"
test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-loadlib-cwd.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home/Bureau" "$work/home/.config" "$work/app"
printf 'XDG_DESKTOP_DIR="$HOME/Bureau"\n' > "$work/home/.config/user-dirs.dirs"
cp "$exe" "$work/app/"
# Demande factice : note la question et refuse aussitôt.
cat > "$work/ask" <<HELPER
#!/bin/sh
echo "\$*" >> "$work/asked"
exit 1
HELPER
chmod 755 "$work/ask"
cd "$work/home/Bureau"
status=0
HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" DISPLAY="${DISPLAY:-:99}" \
    WINLUX_PERMISSION_HELPER="$work/ask" WINLUX_GUI=headless \
    timeout 30 "$runtime" "$work/app/loadlib_search.exe" > "$work/out" 2>&1 || status=$?
cat "$work/out"
if test -s "$work/asked"; then
    echo "ÉCHEC : demande d'autorisation pour une DLL système : $(cat "$work/asked")" >&2
    exit 1
fi
test "$status" = 0 || { echo "ÉCHEC : code $status" >&2; exit 1; }
grep -q '^OK' "$work/out"
echo "OK: DLL système chargée depuis le Bureau sans demande d'autorisation (#1019)"
