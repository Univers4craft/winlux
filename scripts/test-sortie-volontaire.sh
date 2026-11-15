#!/bin/sh
# winlux-open et la sortie volontaire (#2272) : un programme qui se termine
# de lui-même (ExitProcess, exit) en code non nul n'affiche rien, comme sous
# Windows ; une exception non gérée, un composant manquant ou une mort sans
# code écrit restent signalés.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
open="$root/integration/winlux-open"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "$1" >&2; cat "$work/sortie.log" >&2 2>/dev/null || true; exit 1; }
# Faux runtime : écrit le code voulu, la marque de plantage si demandée, une
# ligne d'erreur si demandée, puis sort avec le statut voulu.
cat >"$work/winlux" <<FIN
#!/bin/sh
if [ "\$1" = --inspect ]; then echo gui; exit 0; fi
case "\$1" in --*) exit 1 ;; esac
code=\$(cat "$work/code")
[ -z "\$code" ] || printf '%s\n' "\$code" >"\$WINLUX_EXIT_CODE_FILE"
[ ! -s "$work/plantage" ] || printf 'exception\n' >"\$WINLUX_CRASH_MARK_FILE"
[ ! -s "$work/stderr" ] || cat "$work/stderr" >&2
exit "\$(cat "$work/statut")"
FIN
chmod +x "$work/winlux"
printf 'MZ' >"$work/prog.exe"
# lance CODE STATUT PLANTAGE STDERR
lance() {
    printf '%s' "$1" >"$work/code"; printf '%s' "$2" >"$work/statut"
    printf '%s' "$3" >"$work/plantage"; printf '%s' "$4" >"$work/stderr"
    set +e
    env -u DISPLAY -u WAYLAND_DISPLAY -u WINLUX_EXIT_CODE_FILE XDG_STATE_HOME="$work/state" \
        WINLUX_RUNTIME="$work/winlux" WINLUX_JOURNAL_DIR="$work/journal" WINLUX_NO_TERMINAL=1 \
        WINLUX_GUI=headless "$open" "$work/prog.exe" </dev/null >"$work/sortie.log" 2>&1
    rc=$?
    set -e
}
lance 0x00000001 1 "" ""
test "$rc" -eq 1 || fail "exit(1) : statut $rc au lieu de 1"
! grep -q 'inattendue' "$work/sortie.log" || fail "exit(1) présenté comme un plantage"
grep -q 'terminé de lui-même, code 0x00000001' "$work/state/winlux/last-launch.log" \
    || fail "exit(1) : code absent du journal de lancement"
lance 0x0000063A 58 "" ""
! grep -q 'inattendue' "$work/sortie.log" || fail "1602 présenté comme un plantage"
lance 0xC0000005 5 oui ""
grep -q 'inattendue (code 0xC0000005)' "$work/sortie.log" || fail "exception non gérée non signalée"
lance 0x00000001 1 "" "DLL introuvable : absente.dll"
grep -q 'absente.dll' "$work/sortie.log" || fail "DLL manquante non expliquée"
lance "" 3 "" ""
grep -q 'inattendue (code 3)' "$work/sortie.log" || fail "mort sans code écrit non signalée"
echo "winlux-open : sortie volontaire silencieuse ; plantage, composant manquant et mort sans code signalés"
