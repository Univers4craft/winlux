#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
# Binaires GUI autonomes – ne peuvent pas tourner en CI headless,
# mais doivent être mentionnés ici pour satisfaire le contrôle d'hygiène :
# audioplayer calc imageviewer pdfviewer postit screenshot sysinfo taskmgr

# Faux sysfs pour power_api : batterie à 75 %, 30 Wh restants à 10 W.
power_root="${TMPDIR:-/tmp}/winlux-power-test.$$"
battery="$power_root/sys/class/power_supply/BAT0"
mkdir -p "$battery" "$power_root/run/winlux" "$power_root/etc/winlux"
trap 'rm -rf "$power_root"' EXIT
printf 'Battery\n' > "$battery/type"
printf 'Discharging\n' > "$battery/status"
printf '75\n' > "$battery/capacity"
printf '30000000\n' > "$battery/energy_now"
printf '40000000\n' > "$battery/energy_full"
printf '10000000\n' > "$battery/power_now"

for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo explorer_regression voicerecorder power_api; do
    exe="$testdir/$name.exe"
    test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
    printf '\n== %s ==\n' "$name"
    if [ "$name" = exitcode ]; then expected=42; else expected=0; fi
    set +e
    WINLUX_TEST=salut WINLUX_GUI=headless WINLUX_POWER_ROOT="$power_root" "$runtime" "$exe"
    actual=$?
    set -e
    test "$actual" -eq "$expected" || {
        echo "$name : code $actual attendu $expected" >&2
        exit 1
    }
done
echo "Tous les tests runtime passent."
