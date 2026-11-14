#!/bin/sh
# Élévation des installateurs déroulés par Winlux (#2180) : un paquet qui
# demande les droits d'administrateur (MSI ALLUSERS=1, NSIS
# RequestExecutionLevel admin) passe par la règle UAC ; refusée, rien n'est
# installé (ERROR_CANCELLED) ; accordée (cas d'un compte administrateur, qui
# l'obtient sans question, #1739), l'installation se fait comme avant. Un
# installateur qui ne demande rien n'est pas concerné.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
chmod 755 "$work"
# Root est toujours élevé : on rejoue en compte ordinaire (nobody).
as_user=
if [ "$(id -u)" -eq 0 ]; then
    command -v setpriv >/dev/null || { echo "root sans setpriv : test sauté"; exit 0; }
    as_user="setpriv --reuid=65534 --regid=65534 --clear-groups"
fi
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
annule=$((1223 % 256))
cp "$root/runtime/src/msi/testdata/sample.msi" "$work/"
chmod 644 "$work/sample.msi"
run() {
    answer=$1; shift
    rm -rf "$work/c"; mkdir -p "$work/c"; chmod 777 "$work/c"
    set +e
    $as_user env -u DISPLAY -u WAYLAND_DISPLAY WINLUX_UAC="$answer" HOME="$work/c" WINLUX_HOME="$work/c" \
        WINLUX_DRIVE_C="$work/c" "$runtime" "$@" >"$work/out.txt" 2>&1
    rc=$?
    set -e
}
run deny --msiexec /i "$work/sample.msi" /qn
test "$rc" -eq "$annule" || fail "MSI ALLUSERS=1, élévation refusée : code $rc au lieu de 1223"
test ! -e "$work/c/Program Files" || fail "MSI ALLUSERS=1, élévation refusée : quelque chose a été installé"
run allow --msiexec /i "$work/sample.msi" /qn
test "$rc" -eq 0 || fail "MSI ALLUSERS=1, élévation accordée (administrateur) : code $rc"
test -f "$work/c/Program Files/Winlux Exemple/app.exe" || fail "MSI ALLUSERS=1, administrateur : non installé"
# Compte administrateur réel, sans réponse forcée ni écran : installé sans
# question, comme avant (#1739). Root est administrateur ; hors root, sauté.
if [ "$(id -u)" -eq 0 ]; then
    rm -rf "$work/c"; mkdir -p "$work/c"
    set +e
    env -u DISPLAY -u WAYLAND_DISPLAY -u WINLUX_UAC HOME="$work/c" WINLUX_HOME="$work/c" \
        WINLUX_DRIVE_C="$work/c" "$runtime" --msiexec /i "$work/sample.msi" /qn >"$work/out.txt" 2>&1
    rc=$?
    set -e
    test "$rc" -eq 0 || fail "MSI ALLUSERS=1 depuis un compte administrateur : code $rc"
    test -f "$work/c/Program Files/Winlux Exemple/app.exe" || fail "MSI ALLUSERS=1 depuis un compte administrateur : non installé"
fi

if command -v makensis >/dev/null; then
    echo bonjour >"$work/lisezmoi.txt"
    for lvl in admin user; do
        cat >"$work/$lvl.nsi" <<NSI
Unicode true
Name "Niveau $lvl"
OutFile "$work/$lvl.exe"
RequestExecutionLevel $lvl
InstallDir "\$PROGRAMFILES64\\Niveau $lvl"
Section
  SetOutPath \$INSTDIR
  File "$work/lisezmoi.txt"
SectionEnd
NSI
        makensis -V1 "$work/$lvl.nsi" >/dev/null || fail "makensis $lvl.nsi"
        chmod 644 "$work/$lvl.exe"
    done
    run deny --setup "$work/admin.exe"
    test "$rc" -eq "$annule" || fail "NSIS admin, élévation refusée : code $rc au lieu de 1223"
    test ! -e "$work/c/Program Files" || fail "NSIS admin, élévation refusée : quelque chose a été installé"
    run allow --setup "$work/admin.exe"
    test -f "$work/c/Program Files/Niveau admin/lisezmoi.txt" || fail "NSIS admin, élévation accordée : non installé"
    run deny --setup "$work/user.exe"
    test "$rc" -eq 0 || fail "NSIS user : élévation demandée à tort (code $rc)"
    test -f "$work/c/Program Files/Niveau user/lisezmoi.txt" || fail "NSIS user : non installé"
fi
echo "élévation des installateurs : MSI ALLUSERS=1 et NSIS admin refusés sans élévation, installés avec ; NSIS user sans question"
