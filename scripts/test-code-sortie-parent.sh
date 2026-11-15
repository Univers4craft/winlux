#!/bin/sh
# Code de sortie complet transmis au parent Windows (#2267) : un programme
# qui lance un .exe par ShellExecuteEx passe par winlux-open ; celui-ci écrit
# le code Windows entier (1602, 3010) dans le WINLUX_EXIT_CODE_FILE de son
# parent, alors que son statut Linux n'en garde que 8 bits.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
open="$root/integration/winlux-open"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "$1" >&2; exit 1; }
cat >"$work/winlux" <<FIN
#!/bin/sh
if [ "\$1" = --inspect ]; then echo gui; exit 0; fi
case "\$1" in --*) exit 1 ;; esac
test -n "\${WINLUX_EXIT_CODE_FILE:-}" || exit 99
cat "$work/code-voulu" >"\$WINLUX_EXIT_CODE_FILE"
exit "\$(cat "$work/statut-voulu")"
FIN
chmod +x "$work/winlux"
printf 'MZ' >"$work/installeur.exe"
launch() {
    printf '%s\n' "$1" >"$work/code-voulu"; printf '%s' "$2" >"$work/statut-voulu"
    : >"$work/parent"
    set +e
    env -u DISPLAY -u WAYLAND_DISPLAY XDG_STATE_HOME="$work/state" WINLUX_RUNTIME="$work/winlux" \
        WINLUX_JOURNAL_DIR="$work/journal" WINLUX_EXIT_CODE_FILE="$work/parent" \
        WINLUX_NO_TERMINAL=1 "$open" "$work/installeur.exe" </dev/null >"$work/sortie.log" 2>&1
    rc=$?
    set -e
}
launch 0x0000063A 58
test "$(cat "$work/parent")" = 0x0000063A || fail "1602 : le parent lit « $(cat "$work/parent") » (statut $rc)"
launch 0x00000BC2 194
test "$(cat "$work/parent")" = 0x00000BC2 || fail "3010 : le parent lit « $(cat "$work/parent") » (statut $rc)"
launch 0x00000000 0
test "$(cat "$work/parent")" = 0x00000000 || fail "succès : le parent lit « $(cat "$work/parent") »"
test "$rc" -eq 0 || fail "succès : statut $rc"
# Sans parent Windows, rien n'est écrit nulle part et le statut reste.
printf '0x0000063A\n' >"$work/code-voulu"; printf 58 >"$work/statut-voulu"
set +e
env -u DISPLAY -u WAYLAND_DISPLAY -u WINLUX_EXIT_CODE_FILE XDG_STATE_HOME="$work/state" WINLUX_RUNTIME="$work/winlux" \
    WINLUX_JOURNAL_DIR="$work/journal" WINLUX_NO_TERMINAL=1 "$open" "$work/installeur.exe" </dev/null >/dev/null 2>&1
rc=$?
set -e
test "$rc" -eq 58 || fail "sans parent : statut $rc au lieu de 58"
# Contrat refusé faute d'assistant (#2123) : 1602 entier pour le parent.
cat >"$work/winlux-setup" <<FIN
#!/bin/sh
if [ "\$1" = --inspect ]; then echo setup; exit 0; fi
if [ "\$1 \$2" = "--setup --license" ]; then echo CONTRAT; exit 0; fi
exit 0
FIN
chmod +x "$work/winlux-setup"
: >"$work/parent"
set +e
env -u DISPLAY -u WAYLAND_DISPLAY XDG_STATE_HOME="$work/state" WINLUX_RUNTIME="$work/winlux-setup" \
    WINLUX_JOURNAL_DIR="$work/journal" WINLUX_EXIT_CODE_FILE="$work/parent" \
    WINLUX_NO_TERMINAL=1 "$open" "$work/installeur.exe" </dev/null >/dev/null 2>&1
set -e
test "$(cat "$work/parent")" = 0x0000063A || fail "contrat refusé : le parent lit « $(cat "$work/parent") »"
echo "code de sortie : 1602 et 3010 transmis entiers au parent Windows par winlux-open"
