#!/bin/sh
# Contrat de licence des installateurs (#2123 à #2126) :
#   - `winlux --setup --license` rend le contrat d'un .msi (licence.msi), d'un
#     installateur NSIS ANSI (LicenseData texte) et Unicode (MUI, RTF), et
#     rien (code 1) pour un paquet sans contrat ;
#   - lancé directement sans /S ni /qn, un paquet à contrat est refusé net
#     (1602, message) ; avec, il s'installe (l'option vaut acceptation) ;
#   - sous Xvfb, l'assistant présente le contrat : sans acceptation rien
#     n'est installé (code 1602, aucun fichier, raccourci ni registre),
#     après acceptation l'installation se fait.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
wizard=$root/scripts/winlux-setup-wizard.py
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v makensis >/dev/null || { echo "makensis absent (paquet nsis) : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
# 1602 (ERROR_INSTALL_USEREXIT) vu par le shell : code de sortie sur 8 bits.
refus=$((1602 % 256))
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }

# Contrat long (~110 Kio) : affiché en entier, sans troncature.
{ printf 'CONTRAT DE TEST NSIS\r\nArticle 1 : accepter avant d installer.\r\n'
  i=2; while [ "$i" -le 2000 ]; do printf 'Article %d : texte du contrat de test de Winlux.\r\n' "$i"; i=$((i + 1)); done
  printf 'FIN DU CONTRAT\r\n'; } >licence.txt
cat >licence.rtf <<'RTF'
{\rtf1\ansi\ansicpg1252{\fonttbl{\f0 Arial;}}\f0 CONTRAT RTF\par Caf\'e9 \ldblquote ok\rdblquote\par}
RTF
echo bonjour >lisezmoi.txt
cat >ansi.nsi <<'NSI'
Name "Licence Ansi"
OutFile "ansi.exe"
InstallDir "$PROGRAMFILES64\Licence Ansi"
LicenseData licence.txt
Page license
Page instfiles
Section
  SetOutPath $INSTDIR
  File lisezmoi.txt
SectionEnd
NSI
cat >mui.nsi <<'NSI'
Unicode true
!include "MUI2.nsh"
Name "Licence MUI"
OutFile "mui.exe"
InstallDir "$PROGRAMFILES64\Licence MUI"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "licence.rtf"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "French"
Section
  SetOutPath $INSTDIR
  File lisezmoi.txt
SectionEnd
NSI
cat >sans.nsi <<'NSI'
Unicode true
Name "Sans Licence"
OutFile "sans.exe"
InstallDir "$PROGRAMFILES64\Sans Licence"
Page directory
Page instfiles
Section
  SetOutPath $INSTDIR
  File lisezmoi.txt
SectionEnd
NSI
for f in ansi mui sans; do makensis -V1 "$f.nsi" >/dev/null || fail "makensis $f.nsi"; done
cp "$root/runtime/src/msi/testdata/licence.msi" "$root/runtime/src/msi/testdata/sample.msi" .

lic() { "$runtime" --setup --license "$1" 2>"$work/out.txt"; }
out=$(lic ansi.exe) || fail "NSIS ANSI : aucun contrat lu"
printf '%s\n' "$out" | grep -qx 'CONTRAT DE TEST NSIS' || fail "NSIS ANSI : $out"
printf '%s\n' "$out" | grep -q 'Article 1 :' || fail "NSIS ANSI : début absent"
test "$(printf '%s\n' "$out" | wc -l)" -eq 2002 || fail "NSIS ANSI : contrat tronqué ($(printf '%s\n' "$out" | wc -l) lignes)"
printf '%s\n' "$out" | tail -n 1 | grep -qx 'FIN DU CONTRAT' || fail "NSIS ANSI : fin du contrat absente"
out=$(lic mui.exe) || fail "NSIS MUI : aucun contrat lu"
test "$out" = "$(printf 'CONTRAT RTF\nCafé “ok”')" || fail "NSIS MUI (RTF) : $out"
out=$(lic licence.msi) || fail "MSI : aucun contrat lu"
printf '%s\n' "$out" | grep -qx 'CONTRAT DE LICENCE DE TEST' || fail "MSI : $out"
if lic sans.exe >/dev/null; then fail "NSIS sans contrat : un contrat a été inventé"; fi
if lic sample.msi >/dev/null; then fail "MSI sans contrat : un contrat a été inventé"; fi

# Lancement direct sans option silencieuse : refus net avec message, rien
# d'installé ; /S vaut acceptation (porte de sortie des tests en VM).
mkdir direct
run_direct() { HOME="$work/direct" WINLUX_HOME="$work/direct" WINLUX_DRIVE_C="$work/direct" \
    "$runtime" "$@" >"$work/out.txt" 2>&1; }
set +e
run_direct ansi.exe; rc=$?
set -e
test "$rc" -eq "$refus" || fail "lancement direct sans /S : code $rc au lieu de 1602"
grep -q "contrat de licence" "$work/out.txt" || fail "lancement direct sans /S : pas de message"
test ! -e "$work/direct/Program Files" || fail "lancement direct sans /S : quelque chose a été installé"
set +e
run_direct --msiexec /i licence.msi; rc=$?
set -e
test "$rc" -eq "$refus" || fail "msiexec sans /q : code $rc au lieu de 1602"
test ! -e "$work/direct/Program Files" || fail "msiexec sans /q : quelque chose a été installé"
run_direct ansi.exe /S || fail "lancement direct avec /S en échec"
test -f "$work/direct/Program Files/Licence Ansi/lisezmoi.txt" || fail "lancement direct avec /S : non installé"
run_direct --msiexec /i licence.msi /qn || fail "msiexec /qn en échec"
test -f "$work/direct/Program Files/Winlux Exemple/app.exe" || fail "msiexec /qn : non installé"

py=
for c in python3 python3.12; do
    if command -v "$c" >/dev/null && "$c" -c 'import gi; gi.require_version("Gtk", "3.0"); from gi.repository import Gtk' 2>/dev/null; then py=$c; break; fi
done
if [ -z "$py" ] || ! command -v xvfb-run >/dev/null; then
    echo "licence : contrats lus (python3-gi ou xvfb-run absent : assistant non testé)"
    exit 0
fi
mkdir home
wiz() {
    HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_LANG=fr \
    WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_SETUP_WIZARD_AUTO_LICENCE=${2:-} WINLUX_BIN="$runtime" \
        timeout 180 xvfb-run -a "$py" "$wizard" "$1" >"$work/out.txt" 2>&1
}
set +e
wiz ansi.exe; rc=$?
set -e
test "$rc" -eq "$refus" || fail "NSIS : contrat non accepté, code $rc au lieu de 1602"
test ! -e "$work/home/Program Files/Licence Ansi" || fail "NSIS : installé sans acceptation du contrat"
set +e
wiz licence.msi; rc=$?
set -e
test "$rc" -eq "$refus" || fail "MSI : contrat non accepté, code $rc au lieu de 1602"
test ! -e "$work/home/Program Files/Winlux Exemple" || fail "MSI : installé sans acceptation du contrat"
# Après les deux refus, rien d'autre que les caches de GTK : ni fichier, ni
# raccourci, ni registre, ni manifeste d'installation.
left=$(cd "$work/home" && find . -type f ! -path './.cache/*' ! -path './.config/dconf/*' \
    ! -name 'recently-used.xbel*' ! -path './.local/share/gvfs-metadata/*' | head -n 5)
test -z "$left" || fail "refus : il reste $left"
wiz ansi.exe accepte || fail "NSIS : installation après acceptation en échec"
test -f "$work/home/Program Files/Licence Ansi/lisezmoi.txt" || fail "NSIS : non installé après acceptation"
wiz licence.msi accepte || fail "MSI : installation après acceptation en échec"
test -f "$work/home/Program Files/Winlux Exemple/app.exe" || fail "MSI : non installé après acceptation"
wiz sans.exe || fail "NSIS sans contrat : l'assistant ne doit rien demander"
test -f "$work/home/Program Files/Sans Licence/lisezmoi.txt" || fail "NSIS sans contrat : non installé"
echo "licence : contrats .msi et NSIS lus, refus par défaut (1602), installation après acceptation"
