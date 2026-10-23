#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}
timeout_seconds=${WINLUX_TEST_TIMEOUT:-30}
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-parallel.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
command -v timeout >/dev/null 2>&1 || { echo "timeout est requis" >&2; exit 1; }

names="hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo"
for name in $names; do
    exe="$testdir/$name.exe"
    test -f "$exe" || { echo "exécutable absent : $exe" >&2; exit 1; }
    mkdir -p "$work/$name/home" "$work/$name/data"
done

echo "Lancement parallèle des neuf exécutables PE..."
for name in $names; do
    (
        set +e
        HOME="$work/$name/home" \
        XDG_DATA_HOME="$work/$name/data" \
        WINLUX_TEST=salut \
        WINLUX_GUI=headless \
            timeout "$timeout_seconds" "$runtime" "$testdir/$name.exe" \
                >"$work/$name/output.log" 2>&1
        echo "$?" >"$work/$name/status"
    ) &
done
wait

result=0
for name in $names; do
    actual=$(cat "$work/$name/status")
    if [ "$name" = exitcode ]; then expected=42; else expected=0; fi
    if [ "$actual" -eq "$expected" ]; then
        echo "  OK     $name.exe (code $actual)"
    else
        echo "  ECHEC  $name.exe : code $actual, attendu $expected" >&2
        sed "s/^/    [$name] /" "$work/$name/output.log" >&2
        result=1
    fi
done

test "$result" -eq 0 || exit "$result"
echo "Tous les exécutables PE passent en parallèle."
