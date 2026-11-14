#!/bin/sh
# Contrat de licence des installateurs (#2123 à #2126) :
#   - `winlux --setup --license` rend le contrat d'un .msi (licence.msi), d'un
#     installateur NSIS ANSI (LicenseData texte) et Unicode (MUI, RTF), et
#     rien (code 1) pour un paquet sans contrat ;
#   - lancé directement sans /S ni /qn (`winlux setup.exe`, `msiexec /i`),
#     un paquet à contrat ouvre la page du contrat : refus = 1602, rien
#     d'installé ; acceptation = installé. Sans affichage, refus net avec
#     message ; /S, /qn valent acceptation ;
#   - sous Xvfb (un seul serveur, lancé ici), l'assistant présente le contrat : sans acceptation rien
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

# Lancement direct sans option silencieuse et sans affichage : la page du
# contrat ne peut pas s'ouvrir, refus net avec message, rien d'installé ;
# /S vaut acceptation (porte de sortie des tests en VM).
mkdir direct
run_direct() { env -u DISPLAY -u WAYLAND_DISPLAY HOME="$work/direct" WINLUX_HOME="$work/direct" \
    WINLUX_DRIVE_C="$work/direct" "$runtime" "$@" >"$work/out.txt" 2>&1; }
set +e
run_direct ansi.exe; rc=$?
set -e
test "$rc" -eq "$refus" || fail "lancement direct sans /S : code $rc au lieu de 1602"
grep -q "aucun affichage" "$work/out.txt" || fail "lancement direct sans /S ni affichage : pas de message"
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
if [ -z "$py" ] || ! command -v Xvfb >/dev/null; then
    echo "licence : contrats lus (python3-gi ou Xvfb absent : assistant non testé)"
    exit 0
fi
# Un seul Xvfb pour tout le script (#2458) : xvfb-run rend 5 quand il ne
# parvient pas à effacer son dossier temporaire, et ce 5 remplaçait le code
# de l'assistant (« code 5 au lieu de 1602 », run 43639544421). Le code lu
# ci-dessous est donc toujours celui du programme testé. Numéro d'affichage
# libre choisi par Xvfb lui-même.
Xvfb -displayfd 3 -screen 0 1280x1024x24 -nolisten tcp 3>"$work/display" >"$work/xvfb.log" 2>&1 &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null || true; rm -rf "$work"' EXIT
n=0
while [ ! -s "$work/display" ]; do
    kill -0 "$xvfb" 2>/dev/null || { cat "$work/xvfb.log" >&2; echo "Xvfb n'a pas démarré" >&2; exit 1; }
    n=$((n + 1)); test "$n" -le 100 || { echo "Xvfb : pas d'affichage après 10 s" >&2; exit 1; }
    sleep 0.1
done
DISPLAY=:$(head -n 1 "$work/display")
export DISPLAY
mkdir home
wiz() {
    HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_LANG=fr \
    WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_SETUP_WIZARD_AUTO_LICENCE=${2:-} WINLUX_BIN="$runtime" \
        timeout 180 "$py" "$wizard" "$1" >"$work/out.txt" 2>&1
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
# Lancement direct avec affichage : la page du contrat s'ouvre ; refus =
# 1602 sans rien installer, acceptation = installation dans le même appel.
printf '#!/bin/sh\nexec %s %s "$@"\n' "$py" "$wizard" >"$work/assistant.sh"
chmod +x "$work/assistant.sh"
mkdir direct2
direct_gui() {
    HOME="$work/direct2" WINLUX_HOME="$work/direct2" WINLUX_DRIVE_C="$work/direct2" WINLUX_LANG=fr \
    WINLUX_SETUP_WIZARD="$work/assistant.sh" WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_SETUP_WIZARD_AUTO_LICENCE=${2:-} \
        timeout 180 "$runtime" "$1" >"$work/out.txt" 2>&1
}
direct_msi() {
    HOME="$work/direct2" WINLUX_HOME="$work/direct2" WINLUX_DRIVE_C="$work/direct2" WINLUX_LANG=fr \
    WINLUX_SETUP_WIZARD="$work/assistant.sh" WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_SETUP_WIZARD_AUTO_LICENCE=${2:-} \
        timeout 180 "$runtime" --msiexec /i "$1" >"$work/out.txt" 2>&1
}
set +e
direct_gui ansi.exe; rc=$?
set -e
test "$rc" -eq "$refus" || fail "lancement direct, contrat refusé : code $rc au lieu de 1602"
test ! -e "$work/direct2/Program Files" || fail "lancement direct, contrat refusé : quelque chose a été installé"
set +e
direct_msi licence.msi; rc=$?
set -e
test "$rc" -eq "$refus" || fail "msiexec /i, contrat refusé : code $rc au lieu de 1602"
test ! -e "$work/direct2/Program Files" || fail "msiexec /i, contrat refusé : quelque chose a été installé"
direct_gui ansi.exe accepte || fail "lancement direct, contrat accepté : installation en échec"
test -f "$work/direct2/Program Files/Licence Ansi/lisezmoi.txt" || fail "lancement direct, contrat accepté : non installé"
direct_msi licence.msi accepte || fail "msiexec /i, contrat accepté : installation en échec"
test -f "$work/direct2/Program Files/Winlux Exemple/app.exe" || fail "msiexec /i, contrat accepté : non installé"
wiz sans.exe || fail "NSIS sans contrat : l'assistant ne doit rien demander"
test -f "$work/home/Program Files/Sans Licence/lisezmoi.txt" || fail "NSIS sans contrat : non installé"
echo "licence : contrats .msi et NSIS lus, page du contrat à l'assistant et au lancement direct, refus = 1602, installation après acceptation"
