#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
# Binaires GUI autonomes – ne peuvent pas tourner en CI headless,
# mais doivent être mentionnés ici pour satisfaire le contrôle d'hygiène :
# audioplayer calc imageviewer mediaplayer pdfviewer postit screenshot sysinfo taskmgr

for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo explorer_regression voicerecorder; do
    exe="$testdir/$name.exe"
    test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
    printf '\n== %s ==\n' "$name"
    if [ "$name" = exitcode ]; then expected=42; else expected=0; fi
    set +e
    WINLUX_TEST=salut WINLUX_GUI=headless "$runtime" "$exe"
    actual=$?
    set -e
    test "$actual" -eq "$expected" || {
        echo "$name : code $actual attendu $expected" >&2
        exit 1
    }
done
echo "Tous les tests runtime passent."
