#!/bin/sh
# Détection des installateurs au lancement d'un .exe (#3189) : elle ne lit
# plus tout le fichier, et elle reconnaît toujours les vrais installateurs.
#   - programme ordinaire (testc/msvc/sanslecture_msvc.c) suivi de 256 Mo
#     de données, lancé normalement : démarré (code 42) avec un pic de
#     mémoire sous 128 Mo et en moins de 2 s (avant : 265 Mo, tout le
#     fichier parcouru par les détecteurs Inno, NSIS et 7z) ;
#   - installateur NSIS bâti par makensis avec 32 Mo de contenu
#     incompressible : reconnu (« nsis ») ;
#   - installateur 7z bâti comme le décrit le SDK LZMA (module SFX, bloc
#     de configuration, archive 7z faite par 7z) : reconnu (« 7z ») ;
#   - installateur Inno : table « rDlPtS » en ressource 11111 (Inno 5.1.5 et
#     plus) ou désignée par le pointeur à 0x30 (plus ancien), suivie de
#     32 Mo : reconnu comme Inno Setup.
# Sauté (code 0) sans runtime ou sans clang ; NSIS et 7z sautés sans
# makensis ou sans 7z.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
src="$root/testc/msvc/sanslecture_msvc.c"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool llvm-rc python3; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
mkdir c
fail() { echo "ÉCHEC : $1" >&2; exit 1; }
info() {
    HOME="$work/c" WINLUX_HOME="$work/c" WINLUX_DRIVE_C="$work/c" "$runtime" --setup --info "$1" 2>&1 | head -3 || true
}

printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\n' >k.def
llvm-dlltool -m i386:x86-64 -d k.def -l k.lib
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -c "$src" -o p.obj
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib p.obj k.lib /out:stub.exe
python3 -I -c 'import os, sys
with open(sys.argv[1], "wb") as f: f.write(os.urandom(32 << 20))' contenu.bin

# 1. Programme ordinaire de 256 Mo, lancé normalement.
python3 -I -c 'import sys
block = bytes(range(256)) * 4096
with open(sys.argv[1], "ab") as f:
    for _ in range(256): f.write(block)' ordinaire.exe
cat stub.exe ordinaire.exe >ordinaire-gros.exe
python3 -I - "$runtime" "$work/ordinaire-gros.exe" "$work/c" <<'PY'
import os, subprocess, sys, time
runtime, exe, drive = sys.argv[1:4]
env = {k: v for k, v in os.environ.items()
       if k not in ("DISPLAY", "WAYLAND_DISPLAY", "WINLUX_UAC", "WINLUX_SETUP_NATIVE")}
env.update(HOME=drive, WINLUX_HOME=drive, WINLUX_DRIVE_C=drive, WINLUX_GUI="headless")
start = time.monotonic()
p = subprocess.Popen([runtime, exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
_, status, usage = os.wait4(p.pid, 0)
elapsed = time.monotonic() - start
code = os.waitstatus_to_exitcode(status)
peak = usage.ru_maxrss >> 10
size = os.path.getsize(exe) >> 20
print(f"programme ordinaire de {size} Mo : code {code}, pic {peak} Mo, {elapsed * 1000:.0f} ms")
if code != 42:
    sys.exit(f"ÉCHEC : le programme n'a pas démarré (code {code}) : {p.stderr.read().decode(errors='replace')[-1500:]}")
if peak >= 128:
    sys.exit(f"ÉCHEC : pic de {peak} Mo, le fichier a été parcouru avant le lancement")
if elapsed >= 2:
    sys.exit(f"ÉCHEC : {elapsed:.2f} s pour un programme qui sort aussitôt")
PY

# 2. NSIS réel (makensis).
if command -v makensis >/dev/null 2>&1; then
    cat >gros.nsi <<'NSI'
Unicode true
SetCompressor /SOLID lzma
Name "Gros NSIS"
OutFile "nsis-gros.exe"
InstallDir "$PROGRAMFILES64\Gros NSIS"
RequestExecutionLevel user
Section
  SetOutPath $INSTDIR
  File contenu.bin
SectionEnd
NSI
    makensis -V1 gros.nsi >/dev/null || fail "makensis gros.nsi"
    out=$(info nsis-gros.exe)
    echo "NSIS ($(($(wc -c <nsis-gros.exe) >> 20)) Mo) : $out"
    case "$out" in nsis*"Gros NSIS"*) ;; *) fail "installateur NSIS non reconnu" ;; esac
else
    echo "makensis absent : NSIS sauté"
fi

# 3. 7z auto-extractible : module SFX + configuration + archive (SDK LZMA,
#    DOC/installer.txt).
if command -v 7z >/dev/null 2>&1; then
    7z a -t7z -mx=1 archive.7z contenu.bin >/dev/null || fail "7z a"
    printf ';!@Install@!UTF-8!\r\nTitle="Gros 7z"\r\n;!@InstallEnd@!\r\n' >config.txt
    cat stub.exe config.txt archive.7z >sfx-gros.exe
    out=$(info sfx-gros.exe)
    echo "7z ($(($(wc -c <sfx-gros.exe) >> 20)) Mo) : $out"
    case "$out" in 7z*) ;; *) fail "installateur 7z non reconnu" ;; esac
else
    echo "7z absent : 7z sauté"
fi

# 4. Inno Setup : table en ressource 11111, puis désignée par 0x30.
python3 -I - <<'PY'
import struct, zlib
ident = b"rDlPtS\xcd\xe6\xd7\x7b\x0b\x2a"
body = ident + struct.pack("<I", 1) + b"\0" * 16 + struct.pack("<II", 4096, 8192)
body += struct.pack("<I", zlib.crc32(body))
open("table.bin", "wb").write(body)
open("inno.rc", "w").write('11111 RCDATA "table.bin"\n')
PY
llvm-rc -fo inno.res inno.rc
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib p.obj k.lib inno.res /out:inno-stub.exe
cat inno-stub.exe contenu.bin >inno-ressource.exe
python3 -I - <<'PY'
import struct
stub = bytearray(open("stub.exe", "rb").read())
body = open("contenu.bin", "rb").read()
table = open("table.bin", "rb").read()
at = len(stub) + len(body)
stub[0x30:0x3C] = struct.pack("<III", 0x6F6E6E49, at, at ^ 0xFFFFFFFF)
open("inno-pointeur.exe", "wb").write(bytes(stub) + body + table)
PY
for f in inno-ressource inno-pointeur; do
    out=$(info "$f.exe")
    echo "Inno ($f) : $out"
    case "$out" in *"Inno Setup"*) ;; *) fail "installateur Inno ($f) non reconnu" ;; esac
    case "$out" in *"pas un installateur"*) fail "installateur Inno ($f) pris pour un programme" ;; esac
done
echo "OK : détection des installateurs sans lire tout le fichier"
