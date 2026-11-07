#!/bin/sh
# Programmes lancés par un installateur NSIS (#179) : de vrais installateurs
# fabriqués ici par makensis sont installés par Winlux.
# Vérifie :
#   - .onInit lit l'ancienne inscription (ReadRegStr) et lance le
#     désinstalleur de la version 1 (ExecWait … _?=$INSTDIR) ;
#   - un prérequis transporté (second installateur NSIS) est installé par
#     ExecWait et par nsExec::ExecToStack, code de sortie 0 rendu au script ;
#   - « msiexec /i paquet.msi /qn » passe par le msiexec intégré ;
#   - jamais lancés : un programme Linux déguisé en .exe, cmd.exe, une
#     adresse web (ExecShell) ;
#   - DeleteRegKey retire bien la clé visée ;
#   - « Applications installées » (--remove) désinstalle la version 2.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v makensis >/dev/null || { echo "makensis absent (paquet nsis) : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
cp "$root/runtime/src/msi/testdata/sample.msi" .
echo prerequis >prereq.txt; echo ancien >ancien.txt; echo commun >commun.txt; echo nouveau >nouveau.txt
printf '#!/bin/sh\ntouch "%s/linux-lance"\n' "$work" >linux.exe
chmod +x linux.exe

uk='Software\Microsoft\Windows\CurrentVersion\Uninstall'
cat >sub.nsi <<EOF
Unicode true
SetCompressor /SOLID lzma
Name "Prerequis Demo"
OutFile "sub.exe"
InstallDir "\$PROGRAMFILES64\\Prerequis Demo"
Section
  SetOutPath \$INSTDIR
  File prereq.txt
  WriteRegStr HKLM "$uk\\PrereqDemo" "DisplayName" "Prerequis Demo"
  WriteRegStr HKLM "$uk\\PrereqDemo" "UninstallString" "\$\\"\$INSTDIR\\uninst.exe\$\\""
  WriteUninstaller "\$INSTDIR\\uninst.exe"
SectionEnd
Section "Uninstall"
SectionEnd
EOF
header() {
    cat <<EOF
Unicode true
SetCompressor /SOLID lzma
Name "Demo App"
OutFile "demo-$1.exe"
InstallDir "\$PROGRAMFILES64\\Demo App"
EOF
}
tail_() {
    cat <<EOF
  WriteRegStr HKLM "$uk\\DemoApp" "DisplayName" "Demo App"
  WriteRegStr HKLM "$uk\\DemoApp" "DisplayVersion" "$1"
  WriteRegStr HKLM "$uk\\DemoApp" "UninstallString" "\$\\"\$INSTDIR\\uninst.exe\$\\""
  WriteUninstaller "\$INSTDIR\\uninst.exe"
SectionEnd
Section "Uninstall"
SectionEnd
EOF
}
{ header 1; cat <<'EOF'
Section
  SetOutPath $INSTDIR
  File ancien.txt
  File commun.txt
EOF
tail_ 1.0; } >v1.nsi
{ header 2; cat <<EOF
Function .onInit
  ReadRegStr \$R0 HKLM "$uk\\DemoApp" "UninstallString"
  StrCmp \$R0 "" fin
  ExecWait '\$R0 /S _?=\$INSTDIR' \$R1
  WriteRegStr HKCU "Software\\DemoApp" "ancien" "v1:\$R1"
  fin:
FunctionEnd
EOF
cat <<'EOF'
Section
  InitPluginsDir
  File /oname=$PLUGINSDIR\sub.exe sub.exe
  ExecWait '"$PLUGINSDIR\sub.exe" /S' $0
  WriteRegStr HKCU "Software\DemoApp" "sub" "$0"
  File /oname=$PLUGINSDIR\sample.msi sample.msi
  ExecWait 'msiexec /i "$PLUGINSDIR\sample.msi" /qn' $2
  WriteRegStr HKCU "Software\DemoApp" "msi" "$2"
  nsExec::ExecToStack '"$PLUGINSDIR\sub.exe" /S'
  Pop $5
  WriteRegStr HKCU "Software\DemoApp" "nsexec" "$5"
  SetOutPath $INSTDIR
  File linux.exe
  ExecWait '"$INSTDIR\linux.exe"' $3
  ExecWait 'cmd.exe /c echo bonjour' $4
  ExecShell open "https://example.org/merci"
  WriteRegStr HKCU "Software\DemoApp\Temporaire" "x" "1"
  DeleteRegKey HKCU "Software\DemoApp\Temporaire"
  File nouveau.txt
  File commun.txt
EOF
tail_ 2.0; } >v2.nsi
for n in sub v1 v2; do makensis -V1 "$n.nsi" >/dev/null; done

mkdir home
run() { HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN_TIMEOUT=120 "$runtime" "$@"; }
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
value() { run reg.exe query 'HKCU\Software\DemoApp' /v "$1" 2>/dev/null | awk -v n="$1" '$1 == n { print $3 }'; }

run --setup demo-1.exe >"$work/out.txt" 2>&1 || fail "installation de la version 1 en échec"
app="$work/home/Program Files/Demo App"
test -f "$app/ancien.txt" || fail "version 1 : ancien.txt absent"
run --setup demo-2.exe >"$work/out.txt" 2>&1 || fail "installation de la version 2 en échec"

test "$(value ancien)" = "v1:0" || fail "ReadRegStr ou désinstalleur de la version 1 : « $(value ancien) »"
grep -q 'programme lancé : .*uninst.exe" /S _?=' "$work/out.txt" || fail "le désinstalleur de la version 1 n'a pas été lancé"
test ! -e "$app/ancien.txt" || fail "ancien.txt resté : la version 1 n'a pas été désinstallée"
for f in nouveau.txt commun.txt uninst.exe; do test -f "$app/$f" || fail "version 2 : $f absent"; done
test -f "$work/home/Program Files/Prerequis Demo/prereq.txt" || fail "prérequis (ExecWait) non installé"
test "$(value sub)" = 0 || fail "code de sortie du prérequis : « $(value sub) »"
test "$(value nsexec)" = 0 || fail "nsExec::ExecToStack : « $(value nsexec) »"
test -f "$work/home/Program Files/Winlux Exemple/app.exe" || fail "msiexec /i : paquet non installé"
test "$(value msi)" = 0 || fail "code de sortie de msiexec : « $(value msi) »"
test ! -e "$work/linux-lance" || fail "un programme Linux déguisé en .exe a été lancé"
grep -q 'programme ignoré : cmd.exe' "$work/out.txt" || fail "cmd.exe n'est pas signalé comme ignoré"
grep -q 'adresse non ouverte' "$work/out.txt" || fail "ExecShell d'une adresse non signalé"
if run reg.exe query 'HKCU\Software\DemoApp\Temporaire' >/dev/null 2>&1; then fail "DeleteRegKey : clé restée"; fi

run --setup --list >"$work/list.txt"
grep -q "^demoapp	Demo App	2.0	nsis" "$work/list.txt" || fail "Demo App 2.0 absent de la liste"
grep -qi "^prereqdemo	" "$work/list.txt" || fail "prérequis absent de la liste"
run --setup --remove "HKLM\\$uk\\DemoApp" >"$work/out.txt" 2>&1 || fail "--remove en échec"
test ! -e "$app/nouveau.txt" || fail "désinstallation incomplète"
test -f "$work/home/Program Files/Prerequis Demo/prereq.txt" || fail "le prérequis est parti avec l'application"
echo "setup-run : ancien désinstalleur, prérequis NSIS, msiexec et nsExec lancés ; Linux, cmd et adresses ignorés ; désinstallation"
