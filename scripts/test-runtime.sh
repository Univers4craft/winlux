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

# winpaths : dossier personnel jetable, Documents XDG nommé « Mes documents »,
# et un fichier Linux au nom impossible sous Windows.
paths_home="$power_root/home"
mkdir -p "$paths_home/.config" "$paths_home/Mes documents"
printf 'XDG_DOCUMENTS_DIR="$HOME/Mes documents"\n' > "$paths_home/.config/user-dirs.dirs"
printf 'linux' > "$paths_home/Mes documents/réunion 10:30.txt"


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

for name in hello exitcode p1_suite dll_import delay_import findfiles winpaths uac_suite crt_suite gui_suite gui_demo explorer_regression voicerecorder power_api audio_suite d3d_suite print_suite; do
    exe="$testdir/$name.exe"
    test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
    printf '\n== %s ==\n' "$name"
    if [ "$name" = exitcode ]; then expected=42; else expected=0; fi
    set +e
    # Sortie audio nulle cadencée : pas de carte son sur les runners CI.
    if [ "$name" = winpaths ]; then
        WINLUX_HOME="$paths_home" WINLUX_DRIVE_C="$paths_home" XDG_CONFIG_HOME="$paths_home/.config" \
            WINLUX_GUI=headless "$runtime" "$exe"
    else
        WINLUX_UAC=allow WINLUX_AUDIO=null WINLUX_TEST=salut WINLUX_GUI=headless WINLUX_POWER_ROOT="$power_root" \
            WINLUX_NOTIFY_ICON_HELPER="$notify_helper" "$runtime" "$exe"
    fi
    actual=$?
    if [ "$name" = winpaths ] && [ "$actual" -eq 0 ]; then
        # Côté Linux : vrai nom accentué, dans le dossier XDG, sans « écriture pour tous ».
        report="$paths_home/Mes documents/Été 2026 — rapport.txt"
        test "$(cat "$report" 2>/dev/null)" = bonjour || { echo "winpaths : $report absent ou faux" >&2; actual=1; }
        mode=$(stat -c %a "$report" 2>/dev/null || echo 777)
        test $(( 0$mode & 2 )) -eq 0 || { echo "winpaths : $report inscriptible par tous ($mode)" >&2; actual=1; }
    fi
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
# UAC refusée : uac_suite (requireAdministrator) ne doit pas démarrer.
# 1223 (ERROR_CANCELLED) devient 199 en code de sortie Linux sur 8 bits.
# root est toujours administrateur : le refus ne se teste qu'en utilisateur.
if [ "$(id -u)" -ne 0 ]; then
    printf '\n== uac_suite (refus) ==\n'
    set +e
    WINLUX_UAC=deny WINLUX_GUI=headless "$runtime" "$testdir/uac_suite.exe" >/dev/null
    actual=$?
    set -e
    test "$actual" -eq 199 || { echo "uac_suite refusé : code $actual attendu 199" >&2; exit 1; }
    echo "  OK     programme non démarré après refus"
fi
echo "Tous les tests runtime passent."
