#!/bin/sh
# Assistant d'installation au double-clic (#833) :
#   - quatre langues complètes, commande d'installation (MSI : msiexec /qn ;
#     NSIS/Inno : --setup avec /D= si le dossier change, /nodesktop) ;
#   - sous Xvfb, l'assistant enchaîne accueil, progression et fin sur un vrai
#     installateur NSIS (makensis) et sur sample.msi, code de sortie 0 ;
#   - winlux-open passe par l'assistant quand il est là.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
wizard=$root/scripts/winlux-setup-wizard.py
py=
for c in python3 python3.12; do
    if command -v "$c" >/dev/null && "$c" -c 'import gi; gi.require_version("Gtk", "3.0"); from gi.repository import Gtk' 2>/dev/null; then py=$c; break; fi
done
test -n "$py" || { echo "python3-gi absent : test sauté"; exit 0; }

"$py" - "$wizard" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_loader("wizard", loader=None)
w = importlib.util.module_from_spec(spec)
exec(open(sys.argv[1], encoding="utf-8").read().replace('if __name__ == "__main__":', "if False:"), w.__dict__)
for fr, row in w._TR.items():
    assert len(row) == 3 and all(row), fr
    for t in row:
        assert sorted(__import__("re").findall(r"\{\w+\}", t)) == sorted(__import__("re").findall(r"\{\w+\}", fr)), (fr, t)
msi = {"kind": "msi", "folder": ""}
assert w.command("w", "/a.msi", msi, "", True) == ["w", "--msiexec", "/i", "/a.msi", "/qn"]
nsis = {"kind": "nsis", "folder": "C:\\Program Files\\A"}
assert w.command("w", "/a.exe", nsis, "C:\\Program Files\\A", True) == ["w", "--setup", "/a.exe", "--desktop"]
assert w.command("w", "/a.exe", nsis, "D:\\A", False) == ["w", "--setup", "/a.exe", "/D=D:\\A", "/nodesktop"]
PY

test -x "$runtime" || { echo "runtime absent ($runtime) : test de l'interface sauté"; exit 0; }
command -v xvfb-run >/dev/null || { echo "xvfb-run absent : test de l'interface sauté"; exit 0; }
command -v makensis >/dev/null || { echo "makensis absent (paquet nsis) : test de l'interface sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
echo bonjour >lisezmoi.txt
cat >demo.nsi <<'NSI'
Unicode true
Name "Assistant Demo"
OutFile "demo.exe"
InstallDir "$PROGRAMFILES64\Assistant Demo"
Section
  SetOutPath $INSTDIR
  File lisezmoi.txt
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\AssistantDemo" "DisplayName" "Assistant Demo"
  WriteUninstaller "$INSTDIR\uninst.exe"
SectionEnd
Section "Uninstall"
SectionEnd
NSI
makensis -V1 demo.nsi >/dev/null
cp "$root/runtime/src/msi/testdata/sample.msi" .
mkdir home
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
wiz() {
    HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_LANG=${2:-fr} \
    WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_SETUP_WIZARD_TRACE="$work/argv.txt" WINLUX_BIN="$runtime" \
        timeout 180 xvfb-run -a "$py" "$wizard" "$1" >"$work/out.txt" 2>&1
}
wiz demo.exe de || fail "assistant NSIS en échec"
test -f "$work/home/Program Files/Assistant Demo/lisezmoi.txt" || fail "NSIS : fichier non installé"
grep -qx -- --setup "$work/argv.txt" || fail "NSIS : commande $(cat "$work/argv.txt")"
wiz sample.msi en || fail "assistant MSI en échec"
test -f "$work/home/Program Files/Winlux Exemple/app.exe" || fail "MSI : paquet non installé"
grep -qx /qn "$work/argv.txt" || fail "MSI : commande $(cat "$work/argv.txt")"
# winlux-open : le double-clic passe par l'assistant ; avec /S, non.
mkdir bin
printf '#!/bin/sh\necho "$1" >"%s/appel.txt"\nexit 0\n' "$work" >bin/winlux-setup-wizard
chmod +x bin/winlux-setup-wizard
open_() { PATH="$work/bin:$PATH" HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
    WINLUX_RUNTIME="$runtime" DISPLAY=:99 sh "$root/integration/winlux-open" "$@" >"$work/out.txt" 2>&1; }
open_ "$work/demo.exe" || fail "winlux-open avec l'assistant en échec"
test "$(cat "$work/appel.txt" 2>/dev/null)" = "$work/demo.exe" || fail "winlux-open n'a pas ouvert l'assistant (NSIS)"
rm -f appel.txt
open_ "$work/sample.msi" || fail "winlux-open (MSI) avec l'assistant en échec"
test -f appel.txt || fail "winlux-open n'a pas ouvert l'assistant (MSI)"
rm -f appel.txt
open_ "$work/demo.exe" /S || true
test ! -e appel.txt || fail "winlux-open a ouvert l'assistant malgré /S"
# Installateur tué par un signal (#2327) : l'assistant dit que le système l'a
# arrêté (SIGKILL : mémoire probablement insuffisante), jamais « code -9 ».
cat >"$work/tue.sh" <<'SH'
#!/bin/sh
case "$1 $2" in
    "--setup --info") printf 'nsis\tTué\t1.0\tDémo\tC:\\Program Files\\Tue\n' ;;
    "--setup --license") exit 1 ;;
    *) kill -KILL $$ ;;
esac
SH
chmod +x "$work/tue.sh"
for lang in fr en; do
    HOME="$work/home" WINLUX_LANG=$lang WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_BIN="$work/tue.sh" \
        timeout 180 xvfb-run -a "$py" "$wizard" demo.exe >"$work/out.txt" 2>&1 && fail "signal : succès annoncé ($lang)"
    grep -q "code -9" "$work/out.txt" && fail "signal : « code -9 » affiché ($lang)"
    grep -q "(signal SIGKILL)" "$work/out.txt" || fail "signal : SIGKILL non nommé ($lang)"
done
grep -q "Setup was stopped by the system (signal SIGKILL). This usually means there was not enough free memory." \
    "$work/out.txt" || fail "signal : cause absente (en)"
echo "setup-wizard : quatre langues, commandes, assistant NSIS et MSI sous Xvfb, installateur tué par un signal, winlux-open"
