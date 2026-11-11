#!/bin/sh
# Installateurs Squirrel.Windows (Setup.exe de Claude, Slack, Discord…) :
# un Setup.exe fabriqué ici (PE32, ressource DATA 131 → ZIP → .nupkg) est
# installé par Winlux, sans exécuter de code Windows, puis désinstallé.
# Vérifie : type reconnu, dossier %LocalAppData%\<Id>\app-<version>,
# Update.exe à la racine, raccourcis vers le programme principal (pas vers
# squirrel.exe), inscription et désinstallation propre.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
python3 -I - "$work/Demo-Setup.exe" <<'PY'
import io, struct, sys, zipfile

def gui64():
    b = bytearray(0x200)
    b[0:2] = b"MZ"; b[0x3C] = 0x80; b[0x80:0x84] = b"PE\0\0"
    b[0x84:0x86] = struct.pack("<H", 0x8664); b[0x80 + 24 + 68] = 2
    return bytes(b)

def zipped(files):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files:
            z.writestr(name, data)
    return out.getvalue()

spec = (b'<?xml version="1.0"?><package><metadata><id>DemoApp</id><version>2.0.1</version>'
        b'<title>Demo</title><authors>Demo SA</authors></metadata></package>')
nupkg = zipped([("DemoApp.nuspec", spec), ("lib/net45/Demo.exe", gui64() + b"x" * 5000),
                ("lib/net45/squirrel.exe", gui64() + b"y" * 90000),
                ("lib/net45/resources/app.asar", b"asar"), ("lib/net45/locales/fr.pak", b"fr")])
payload = zipped([("DemoApp-2.0.1-full.nupkg", nupkg), ("Update.exe", b"MZ update"), ("RELEASES", b"r")])

# PE32 : une section .rsrc, DATA (nommé) / 131 / 1033.
rva = 0x1000
r = bytearray()
r += b"\0" * 12 + struct.pack("<HH", 1, 0) + struct.pack("<II", 0x80000060, 0x80000018)
r += b"\0" * 12 + struct.pack("<HH", 0, 1) + struct.pack("<II", 131, 0x80000030)
r += b"\0" * 12 + struct.pack("<HH", 0, 1) + struct.pack("<II", 1033, 0x48)
r += struct.pack("<IIII", rva + 0x80, len(payload), 0, 0)
r += b"\0" * (0x60 - len(r)) + struct.pack("<H", 4) + "DATA".encode("utf-16-le")
r += b"\0" * (0x80 - len(r)) + payload
b = bytearray(0x400)
b[0:2] = b"MZ"; b[0x3C] = 0x80; b[0x80:0x84] = b"PE\0\0"
coff = 0x84
b[coff:coff + 4] = struct.pack("<HH", 0x14C, 1); b[coff + 16:coff + 18] = struct.pack("<H", 224)
opt = coff + 20
b[opt:opt + 2] = struct.pack("<H", 0x10B); b[opt + 112:opt + 116] = struct.pack("<I", rva)
sec = opt + 224
b[sec:sec + 5] = b".rsrc"
b[sec + 8:sec + 24] = struct.pack("<IIII", len(r), rva, len(r), 0x400)
open(sys.argv[1], "wb").write(bytes(b) + bytes(r))
PY

# WINLUX_SETUP_RUN=0 : le lancement de fin d'installation est journalisé
# sans démarrer le faux programme (#1730).
run() { HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN=0 "$runtime" "$@"; }
mkdir -p "$work/home"
info=$(run --setup --info "$work/Demo-Setup.exe")
case "$info" in squirrel*Demo*2.0.1*'Demo SA'*'AppData\Local\DemoApp'*) ;; *) echo "info inattendue : $info" >&2; exit 1 ;; esac
run --setup "$work/Demo-Setup.exe" >"$work/out.txt" 2>&1 || { cat "$work/out.txt"; exit 1; }
app="$work/home/AppData/Local/DemoApp"
grep -q 'Demo.exe --squirrel-firstrun' "$work/out.txt" || { echo "lancement de fin d'installation absent" >&2; cat "$work/out.txt" >&2; exit 1; }
for f in "$app/Update.exe" "$app/app-2.0.1/Demo.exe" "$app/app-2.0.1/resources/app.asar" "$app/app-2.0.1/locales/fr.pak"; do
    test -f "$f" || { echo "fichier absent : $f" >&2; exit 1; }
done
link="$work/home/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Demo SA/Demo.lnk"
test -f "$link" || { echo "raccourci absent : $link" >&2; exit 1; }
grep -qa 'Demo.exe' "$link" || LC_ALL=C tr -d '\000' <"$link" | grep -q 'Demo.exe' || { echo "le raccourci ne vise pas Demo.exe" >&2; exit 1; }
if LC_ALL=C tr -d '\000' <"$link" | grep -qi 'squirrel.exe'; then echo "le raccourci vise squirrel.exe" >&2; exit 1; fi
desk="$work/home/Desktop"
test -f "$desk/Demo.lnk" || { echo "raccourci du Bureau absent" >&2; ls -la "$work/home" >&2; exit 1; }
grep -qx 'Demo.lnk' "$desk/.hidden" || { echo "Demo.lnk non masqué sur le Bureau (.hidden)" >&2; exit 1; }
run --setup --list | grep -q "^DemoApp	Demo	2.0.1	squirrel" || { echo "absent de la liste" >&2; exit 1; }
run --setup --uninstall DemoApp >/dev/null 2>&1
test ! -e "$app/app-2.0.1/Demo.exe" || { echo "désinstallation incomplète" >&2; exit 1; }
test ! -e "$link" || { echo "raccourci resté après désinstallation" >&2; exit 1; }
echo "squirrel : Setup.exe lu, appli installée dans LocalAppData, raccourcis, désinstallation"
