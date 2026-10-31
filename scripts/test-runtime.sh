#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
# Binaires GUI autonomes – ne peuvent pas tourner en CI headless,
# mais doivent être mentionnés ici pour satisfaire le contrôle d'hygiène :
# audioplayer calc imageviewer mediaplayer pdfviewer postit screenshot sysinfo taskmgr

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

# Assistant de zone de notification factice : à la première icône, il répond
# par un clic droit (relâchement) en (5, 6), comme le ferait le panneau.
notify_helper="$power_root/notify-icon-helper"
cat >"$notify_helper" <<'HELPER'
#!/bin/sh
IFS= read -r line || exit 0
key=$(printf '%s\n' "$line" | cut -f2)
sleep 1  # Après NIM_SETVERSION, comme un vrai clic.
printf '%s\trup\t5\t6\n' "$key"
cat >/dev/null
HELPER
chmod 755 "$notify_helper"

for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo explorer_regression voicerecorder power_api audio_suite d3d_suite print_suite; do
    exe="$testdir/$name.exe"
    test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
    printf '\n== %s ==\n' "$name"
    if [ "$name" = exitcode ]; then expected=42; else expected=0; fi
    set +e
    # Sortie audio nulle cadencée : pas de carte son sur les runners CI.
    WINLUX_AUDIO=null WINLUX_TEST=salut WINLUX_GUI=headless WINLUX_POWER_ROOT="$power_root" \
        WINLUX_NOTIFY_ICON_HELPER="$notify_helper" "$runtime" "$exe"
    actual=$?
    set -e
    test "$actual" -eq "$expected" || {
        echo "$name : code $actual attendu $expected" >&2
        exit 1
    }
done
# Lancement croisé dans l'autre sens : un script Linux lit la sortie d'un .exe.
printf '\n== sortie d un .exe lue par un script Linux ==\n'
captured=$("$runtime" "$testdir/hello.exe" | head -n 1)
test "$captured" = "Bonjour depuis un .exe Windows, execute par Winlux !" || {
    echo "sortie de hello.exe non récupérée : $captured" >&2
    exit 1
}
echo "OK"
echo "Tous les tests runtime passent."
