#!/bin/sh
# Définition d'écran des jeux (#271) : compile testc/msvc/display_msvc.c avec
# clang (cible MSVC) si besoin, puis le lance sous Winlux avec un faux xrandr
# (écran DP-1 en 2560x1440 à 144 Hz) qui note ses appels :
#   - EnumDisplaySettingsW : mode actuel et liste des modes ;
#   - ChangeDisplaySettingsExW : CDS_TEST, mode refusé, passage en 1280x720 ;
#   - après la fin du programme, le gardien remet 2560x1440 à 143.97 Hz.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/display_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/display_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/xrandr" <<SH
#!/bin/sh
if [ "\$1" = "--query" ]; then
cat <<'Q'
Screen 0: minimum 320 x 200, current 2560 x 1440, maximum 16384 x 16384
DP-1 connected primary 2560x1440+0+0 (normal left inverted right x axis y axis) 597mm x 336mm
   2560x1440    143.97*+ 119.88    59.95
   1920x1080    119.88    60.00    59.94
   1280x720      60.00
HDMI-1 disconnected (normal left inverted right x axis y axis)
Q
else
    echo "\$*" >>"$work/appels.txt"
fi
SH
chmod +x "$work/xrandr"
: >"$work/appels.txt"
cd "$work"
set +e
env -u DISPLAY WINLUX_GUI=headless WINLUX_XRANDR="$work/xrandr" timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "display_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
grep -qx -- "--output DP-1 --mode 1280x720 --rate 60.00" "$work/appels.txt" || {
    cat "$work/appels.txt"
    echo "display_msvc : 1280x720 n'a pas été demandé à xrandr" >&2
    exit 1
}
i=0
until grep -qx -- "--output DP-1 --mode 2560x1440 --rate 143.97" "$work/appels.txt"; do
    i=$((i + 1))
    test "$i" -lt 50 || {
        cat "$work/appels.txt"
        echo "display_msvc : l'écran n'a pas été remis après la fin du programme" >&2
        exit 1
    }
    sleep 0.1
done
test "$(wc -l <"$work/appels.txt")" -eq 2 || {
    cat "$work/appels.txt"
    echo "display_msvc : appels xrandr en trop (CDS_TEST ou mode refusé appliqué ?)" >&2
    exit 1
}
echo "display_msvc : modes énumérés, 1280x720 appliqué, écran remis à la sortie"
